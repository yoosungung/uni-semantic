//! IR → Power BI BIM (MVP subset).

use serde::Serialize;
use thiserror::Error;
use uni_semantic_ir::{Document, Metric, Table};

#[derive(Debug, Error)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
struct BimFile {
    #[serde(rename = "compatibilityLevel")]
    compatibility_level: i32,
    model: BimModel,
}

#[derive(Debug, Serialize)]
struct BimModel {
    tables: Vec<BimTable>,
    relationships: Vec<BimRelationship>,
}

#[derive(Debug, Serialize)]
struct BimTable {
    name: String,
    annotations: Vec<BimAnnotation>,
    columns: Vec<BimColumn>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    measures: Vec<BimMeasure>,
}

#[derive(Debug, Serialize)]
struct BimAnnotation {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct BimColumn {
    name: String,
    #[serde(rename = "dataType")]
    data_type: String,
    #[serde(rename = "sourceColumn")]
    source_column: String,
    #[serde(rename = "isKey")]
    is_key: bool,
    #[serde(rename = "isNullable")]
    is_nullable: bool,
}

#[derive(Debug, Serialize)]
struct BimMeasure {
    name: String,
    expression: String,
}

#[derive(Debug, Serialize)]
struct BimRelationship {
    name: String,
    #[serde(rename = "fromTable")]
    from_table: String,
    #[serde(rename = "fromColumn")]
    from_column: String,
    #[serde(rename = "toTable")]
    to_table: String,
    #[serde(rename = "toColumn")]
    to_column: String,
    #[serde(rename = "fromCardinality")]
    from_cardinality: String,
    #[serde(rename = "toCardinality")]
    to_cardinality: String,
}

fn ir_type_to_bim(t: &str) -> &'static str {
    match t {
        "bigint" | "int" => "int64",
        "numeric" | "double" => "double",
        "boolean" => "boolean",
        "date" => "date",
        "timestamp" => "dateTime",
        _ => "string",
    }
}

fn metric_to_dax(m: &Metric) -> Option<(String, String)> {
    // sum(col) → SUM(cube[col])
    let expr = m.expression.trim();
    let (agg, rest) = if let Some(r) = expr.strip_prefix("sum(") {
        ("SUM", r)
    } else if let Some(r) = expr.strip_prefix("count(") {
        ("COUNT", r)
    } else if let Some(r) = expr.strip_prefix("avg(") {
        ("AVERAGE", r)
    } else {
        return None;
    };
    let col = rest.trim_end_matches(')').trim();
    Some((
        m.name.clone(),
        format!("{agg}({cube}[{col}])", cube = m.cube, col = col),
    ))
}

pub fn emit_bim(docs: &[Document]) -> Result<String, Error> {
    let tables: Vec<&Table> = uni_semantic_ir::tables(docs);
    let metrics: Vec<&Metric> = uni_semantic_ir::metrics(docs);
    let mut bim_tables = Vec::new();
    let mut relationships = Vec::new();

    for t in &tables {
        let mut measures = Vec::new();
        for m in &metrics {
            if m.cube == t.name {
                if let Some((name, expression)) = metric_to_dax(m) {
                    measures.push(BimMeasure { name, expression });
                }
            }
        }
        bim_tables.push(BimTable {
            name: t.name.clone(),
            annotations: vec![
                BimAnnotation {
                    name: "uni_semantic.db".into(),
                    value: t.db.clone(),
                },
                BimAnnotation {
                    name: "uni_semantic.schema".into(),
                    value: t.schema.clone(),
                },
                BimAnnotation {
                    name: "uni_semantic.table".into(),
                    value: t.table.clone(),
                },
            ],
            columns: t
                .columns
                .iter()
                .map(|c| BimColumn {
                    name: c.name.clone(),
                    data_type: ir_type_to_bim(&c.col_type).into(),
                    source_column: c.name.clone(),
                    is_key: t.pk.iter().any(|p| p == &c.name),
                    is_nullable: c.nullable,
                })
                .collect(),
            measures,
        });

        for fk in &t.fk {
            let from_col = fk.columns.first().cloned().unwrap_or_default();
            let to_col = fk.references.columns.first().cloned().unwrap_or_default();
            relationships.push(BimRelationship {
                name: format!("{}_{}", t.name, fk.references.table),
                from_table: t.name.clone(),
                from_column: from_col,
                to_table: fk.references.table.clone(),
                to_column: to_col,
                from_cardinality: "many".into(),
                to_cardinality: "one".into(),
            });
        }
    }

    // Also emit relationships from IR views if table.fk empty
    if relationships.is_empty() {
        for v in uni_semantic_ir::views(docs) {
            for j in &v.joins {
                let on = j.on.first();
                relationships.push(BimRelationship {
                    name: format!("{}_{}", j.left, j.right),
                    from_table: j.left.clone(),
                    from_column: on.map(|o| o.left_column.clone()).unwrap_or_default(),
                    to_table: j.right.clone(),
                    to_column: on.map(|o| o.right_column.clone()).unwrap_or_default(),
                    from_cardinality: "many".into(),
                    to_cardinality: "one".into(),
                });
            }
        }
    }

    let file = BimFile {
        compatibility_level: 1567,
        model: BimModel {
            tables: bim_tables,
            relationships,
        },
    };
    Ok(serde_json::to_string_pretty(&file)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uni_semantic_ir::{ForeignKey, FkRef, TableColumn};

    #[test]
    fn emits_table_and_measure() {
        let docs = vec![
            Document::Table(Table {
                name: "orders".into(),
                target: "dw".into(),
                db: "dw".into(),
                schema: "sales".into(),
                table: "orders".into(),
                columns: vec![TableColumn {
                    name: "sale_amount".into(),
                    col_type: "numeric".into(),
                    nullable: false,
                }],
                pk: vec![],
                fk: vec![ForeignKey {
                    columns: vec!["customer_id".into()],
                    references: FkRef {
                        table: "customer".into(),
                        columns: vec!["customer_id".into()],
                    },
                }],
                permission_filters: vec![],
                scope_filter: None,
                default_filter: None,
                role: Some("all".into()),
            }),
            Document::Metric(Metric {
                name: "revenue".into(),
                description: String::new(),
                cube: "orders".into(),
                expression: "sum(sale_amount)".into(),
                aggregation: "group".into(),
                group_by: vec!["sale_amount".into()],
                filter: None,
                role: Some("all".into()),
            }),
        ];
        let out = emit_bim(&docs).unwrap();
        assert!(out.contains("SUM(orders[sale_amount])"));
        assert!(out.contains("\"fromTable\": \"orders\""));
    }
}
