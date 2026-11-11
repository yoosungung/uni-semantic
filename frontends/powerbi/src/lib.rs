//! Power BI BIM (MVP subset) → Universal Semantic IR.

use serde::Deserialize;
use thiserror::Error;
use uni_semantic_ir::{
    normalize_ident, Document, ForeignKey, FkRef, Metric, Table, TableColumn, View, ViewColumn,
    ViewJoin, ViewSource, JoinOn, ColumnMapping,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Deserialize)]
struct BimFile {
    model: BimModel,
}

#[derive(Debug, Deserialize)]
struct BimModel {
    #[serde(default)]
    tables: Vec<BimTable>,
    #[serde(default)]
    relationships: Vec<BimRelationship>,
}

#[derive(Debug, Deserialize)]
struct BimTable {
    name: String,
    #[serde(default)]
    columns: Vec<BimColumn>,
    #[serde(default)]
    measures: Vec<BimMeasure>,
    #[serde(default)]
    annotations: Vec<BimAnnotation>,
}

#[derive(Debug, Deserialize)]
struct BimColumn {
    name: String,
    #[serde(default, rename = "dataType")]
    data_type: Option<String>,
    #[serde(default, rename = "sourceColumn")]
    source_column: Option<String>,
    #[serde(default, rename = "isKey")]
    is_key: bool,
    #[serde(default, rename = "isNullable")]
    is_nullable: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct BimMeasure {
    name: String,
    #[serde(default)]
    expression: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BimAnnotation {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct BimRelationship {
    #[serde(rename = "fromTable")]
    from_table: String,
    #[serde(rename = "fromColumn")]
    from_column: String,
    #[serde(rename = "toTable")]
    to_table: String,
    #[serde(rename = "toColumn")]
    to_column: String,
    #[serde(default, rename = "fromCardinality")]
    from_cardinality: Option<String>,
}

fn map_type(dt: &str) -> &'static str {
    match dt.to_ascii_lowercase().as_str() {
        "int64" | "int32" | "int16" | "int" => "bigint",
        "double" | "decimal" | "currency" => "numeric",
        "boolean" | "bool" => "boolean",
        "date" => "date",
        "datetime" | "datetimezone" => "timestamp",
        _ => "varchar",
    }
}

fn measure_text(expr: &serde_json::Value) -> String {
    match expr {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Parse `SUM(Table[col])` / `COUNT(...)` / `AVERAGE|AVG(...)`.
fn parse_simple_agg(expr: &str) -> Option<(String, String, String)> {
    let e = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    let upper = e.to_ascii_uppercase();
    let (agg, rest) = if upper.starts_with("SUM(") {
        ("sum", &e[4..])
    } else if upper.starts_with("COUNT(") {
        ("count", &e[6..])
    } else if upper.starts_with("AVERAGE(") {
        ("avg", &e[8..])
    } else if upper.starts_with("AVG(") {
        ("avg", &e[4..])
    } else {
        return None;
    };
    let rest = rest.trim().trim_end_matches(')').trim();
    let (table, col) = rest.split_once('[')?;
    let col = col.trim_end_matches(']').trim();
    Some((
        agg.to_string(),
        normalize_ident(table.trim()),
        normalize_ident(col),
    ))
}

fn annotation(table: &BimTable, key: &str) -> Option<String> {
    table
        .annotations
        .iter()
        .find(|a| a.name == key)
        .map(|a| a.value.clone())
}

pub fn parse_bim(json: &str) -> Result<Vec<Document>, Error> {
    let bim: BimFile = serde_json::from_str(json)?;
    let mut docs = Vec::new();
    let mut table_names: Vec<String> = Vec::new();

    for t in &bim.model.tables {
        let name = normalize_ident(&t.name);
        table_names.push(name.clone());
        let db = annotation(t, "uni_semantic.db").unwrap_or_else(|| "dw".into());
        let schema = annotation(t, "uni_semantic.schema").unwrap_or_else(|| "public".into());
        let physical = annotation(t, "uni_semantic.table").unwrap_or_else(|| name.clone());

        let mut pk = Vec::new();
        let mut columns = Vec::new();
        for c in &t.columns {
            let col_name = normalize_ident(c.source_column.as_deref().unwrap_or(&c.name));
            if c.is_key {
                pk.push(col_name.clone());
            }
            columns.push(TableColumn {
                name: col_name,
                col_type: map_type(c.data_type.as_deref().unwrap_or("string")).to_string(),
                nullable: c.is_nullable.unwrap_or(true),
            });
        }
        if columns.is_empty() {
            return Err(Error::Msg(format!("table {name} has no columns")));
        }

        let mut fk = Vec::new();
        for r in &bim.model.relationships {
            if normalize_ident(&r.from_table) == name {
                fk.push(ForeignKey {
                    columns: vec![normalize_ident(&r.from_column)],
                    references: FkRef {
                        table: normalize_ident(&r.to_table),
                        columns: vec![normalize_ident(&r.to_column)],
                    },
                });
            }
        }

        docs.push(Document::Table(Table {
            name: name.clone(),
            target: "dw".into(),
            db,
            schema,
            table: physical,
            columns,
            pk: pk.clone(),
            fk,
            permission_filters: vec![],
            scope_filter: None,
            default_filter: None,
            role: Some("all".into()),
        }));

        for m in &t.measures {
            let text = measure_text(&m.expression);
            if let Some((agg, cube, col)) = parse_simple_agg(&text) {
                let group_col = pk.first().cloned().unwrap_or_else(|| col.clone());
                docs.push(Document::Metric(Metric {
                    name: normalize_ident(&m.name),
                    description: String::new(),
                    cube,
                    expression: format!("{agg}({col})"),
                    aggregation: "group".into(),
                    group_by: vec![group_col],
                    filter: None,
                    role: Some("all".into()),
                }));
            }
        }
    }

    if !bim.model.relationships.is_empty() {
        let mut sources = Vec::new();
        let mut joins = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for r in &bim.model.relationships {
            let left = normalize_ident(&r.from_table);
            let right = normalize_ident(&r.to_table);
            for n in [&left, &right] {
                if seen.insert(n.clone()) {
                    sources.push(ViewSource {
                        kind: "table".into(),
                        table: n.clone(),
                    });
                }
            }
            let join_type = match r.from_cardinality.as_deref() {
                Some(c) if c.eq_ignore_ascii_case("one") => "inner",
                _ => "left",
            };
            joins.push(ViewJoin {
                left: left.clone(),
                right: right.clone(),
                join_type: join_type.into(),
                on: vec![JoinOn {
                    left_column: normalize_ident(&r.from_column),
                    right_column: normalize_ident(&r.to_column),
                }],
            });
        }
        let view_name = if joins.len() == 1 {
            format!("{}_{}", joins[0].left, joins[0].right)
        } else {
            "model_joins".into()
        };
        let mut vcols = Vec::new();
        for t in uni_semantic_ir::tables(&docs) {
            for c in &t.columns {
                vcols.push(ViewColumn {
                    name: if t.name == table_names[0] {
                        c.name.clone()
                    } else {
                        format!("{}_{}", t.name, c.name)
                    },
                    mappings: vec![ColumnMapping {
                        source: t.name.clone(),
                        column: c.name.clone(),
                    }],
                });
            }
        }
        docs.push(Document::View(View {
            name: normalize_ident(&view_name),
            target: "dw".into(),
            combine: "join".into(),
            sources,
            joins,
            columns: vcols,
        }));
    }

    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/orders_customer.bim.json");

    #[test]
    fn parses_tables_fk_metric_and_view() {
        let docs = parse_bim(FIXTURE).expect("parse");
        let tables = uni_semantic_ir::tables(&docs);
        assert_eq!(tables.len(), 2);
        let orders = tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.pk, vec!["order_id".to_string()]);
        assert_eq!(orders.fk.len(), 1);
        assert_eq!(orders.fk[0].references.table, "customer");
        let metrics = uni_semantic_ir::metrics(&docs);
        assert!(metrics.iter().any(|m| m.expression == "sum(sale_amount)"));
        assert!(!uni_semantic_ir::views(&docs).is_empty());
    }
}
