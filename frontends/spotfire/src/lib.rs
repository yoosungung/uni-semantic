//! Spotfire Information Link XML → Universal Semantic IR.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use uni_semantic_ir::{
    normalize_ident, ColumnMapping, Document, ForeignKey, FkRef, JoinOn, Table, TableColumn, View,
    ViewColumn, ViewJoin, ViewSource,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("xml: {0}")]
    Xml(String),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone)]
struct IlJoin {
    join_type: String,
    left_table: String,
    left_column: String,
    right_table: String,
    right_column: String,
}

fn map_type(dt: &str) -> &'static str {
    match dt.to_ascii_lowercase().as_str() {
        "integer" | "int" | "bigint" | "smallint" => "bigint",
        "real" | "float" | "double" | "decimal" | "number" | "numeric" => "numeric",
        "boolean" | "bool" => "boolean",
        "date" => "date",
        "datetime" | "timestamp" => "timestamp",
        _ => "varchar",
    }
}

fn attr<'a>(n: roxmltree::Node<'a, 'a>, key: &str) -> Option<&'a str> {
    n.attribute(key)
}

fn child_elems<'a, 'input: 'a>(
    n: roxmltree::Node<'a, 'input>,
    tag: &str,
) -> Vec<roxmltree::Node<'a, 'input>> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().name() == tag)
        .collect()
}

fn is_key(raw: Option<&str>) -> bool {
    matches!(
        raw.unwrap_or("").to_ascii_lowercase().as_str(),
        "true" | "y" | "1" | "pk"
    )
}

fn infer_pk(table: &str, columns: &[TableColumn], marked: &[String]) -> Vec<String> {
    if !marked.is_empty() {
        return marked.to_vec();
    }
    let candidate = format!("{table}_id");
    if let Some(c) = columns.iter().find(|c| c.name == candidate) {
        return vec![c.name.clone()];
    }
    if let Some(c) = columns.iter().find(|c| c.name == "id") {
        return vec![c.name.clone()];
    }
    columns
        .first()
        .map(|c| vec![c.name.clone()])
        .unwrap_or_default()
}

/// Parse standalone Spotfire Information Link XML into IR documents.
pub fn parse_information_link(xml: &str) -> Result<Vec<Document>, Error> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "informationLink" {
        return Err(Error::Msg(format!(
            "expected root <informationLink>, got <{}>",
            root.tag_name().name()
        )));
    }

    let db_default = attr(root, "db").unwrap_or("dw");

    // table_name -> (schema, physical, columns, pk_marked)
    let mut tables: BTreeMap<String, (String, String, Vec<TableColumn>, Vec<String>)> =
        BTreeMap::new();

    for col in child_elems(root, "columns")
        .into_iter()
        .flat_map(|c| child_elems(c, "column"))
    {
        let table_raw = attr(col, "table").ok_or_else(|| Error::Msg("column missing @table".into()))?;
        let table_name = normalize_ident(table_raw);
        let schema = normalize_ident(attr(col, "schema").unwrap_or("public"));
        let physical_col = normalize_ident(
            attr(col, "column")
                .or_else(|| attr(col, "name"))
                .ok_or_else(|| Error::Msg("column missing @column/@name".into()))?,
        );
        let display = normalize_ident(attr(col, "name").unwrap_or(physical_col.as_str()));
        let col_name = if display.is_empty() {
            physical_col.clone()
        } else {
            display
        };
        let col_type = map_type(attr(col, "dataType").unwrap_or("string")).to_string();
        let entry = tables.entry(table_name.clone()).or_insert_with(|| {
            (schema.clone(), table_name.clone(), Vec::new(), Vec::new())
        });
        // First schema wins; keep physical table name as table_name.
        if entry.2.iter().all(|c| c.name != col_name) {
            entry.2.push(TableColumn {
                name: col_name.clone(),
                col_type,
                nullable: !is_key(attr(col, "key")),
            });
        }
        if is_key(attr(col, "key")) {
            entry.3.push(col_name);
        }
    }

    if tables.is_empty() {
        return Err(Error::Msg("informationLink has no columns".into()));
    }

    let mut joins = Vec::new();
    for join in child_elems(root, "joins")
        .into_iter()
        .flat_map(|j| child_elems(j, "join"))
    {
        let join_type = match attr(join, "type").unwrap_or("inner").to_ascii_lowercase().as_str() {
            "left" | "leftouter" => "left",
            "right" | "rightouter" => "right",
            "full" | "fullouter" => "full",
            _ => "inner",
        }
        .to_string();
        let left = child_elems(join, "left")
            .into_iter()
            .next()
            .ok_or_else(|| Error::Msg("join missing <left>".into()))?;
        let right = child_elems(join, "right")
            .into_iter()
            .next()
            .ok_or_else(|| Error::Msg("join missing <right>".into()))?;
        joins.push(IlJoin {
            join_type,
            left_table: normalize_ident(
                attr(left, "table").ok_or_else(|| Error::Msg("join left missing @table".into()))?,
            ),
            left_column: normalize_ident(
                attr(left, "column")
                    .ok_or_else(|| Error::Msg("join left missing @column".into()))?,
            ),
            right_table: normalize_ident(
                attr(right, "table")
                    .ok_or_else(|| Error::Msg("join right missing @table".into()))?,
            ),
            right_column: normalize_ident(
                attr(right, "column")
                    .ok_or_else(|| Error::Msg("join right missing @column".into()))?,
            ),
        });
    }

    let mut docs = Vec::new();
    let table_order: Vec<String> = tables.keys().cloned().collect();

    for (name, (schema, physical, columns, pk_marked)) in &tables {
        let pk = infer_pk(name, columns, pk_marked);
        let mut fk = Vec::new();
        for j in &joins {
            if &j.left_table == name {
                fk.push(ForeignKey {
                    columns: vec![j.left_column.clone()],
                    references: FkRef {
                        table: j.right_table.clone(),
                        columns: vec![j.right_column.clone()],
                    },
                });
            }
        }
        docs.push(Document::Table(Table {
            name: name.clone(),
            target: "dw".into(),
            db: db_default.to_string(),
            schema: schema.clone(),
            table: physical.clone(),
            columns: columns.clone(),
            pk,
            fk,
            permission_filters: vec![],
            scope_filter: None,
            default_filter: None,
            role: Some("all".into()),
        }));
    }

    if !joins.is_empty() {
        let mut sources = Vec::new();
        let mut seen = BTreeSet::new();
        let mut view_joins = Vec::new();
        for j in &joins {
            for n in [&j.left_table, &j.right_table] {
                if seen.insert(n.clone()) {
                    sources.push(ViewSource {
                        kind: "table".into(),
                        table: n.clone(),
                    });
                }
            }
            view_joins.push(ViewJoin {
                left: j.left_table.clone(),
                right: j.right_table.clone(),
                join_type: j.join_type.clone(),
                on: vec![JoinOn {
                    left_column: j.left_column.clone(),
                    right_column: j.right_column.clone(),
                }],
            });
        }
        let view_name = if view_joins.len() == 1 {
            format!("{}_{}", view_joins[0].left, view_joins[0].right)
        } else {
            "model_joins".into()
        };
        let mut vcols = Vec::new();
        let first = table_order.first().cloned().unwrap_or_default();
        for t in uni_semantic_ir::tables(&docs) {
            for c in &t.columns {
                vcols.push(ViewColumn {
                    name: if t.name == first {
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
            joins: view_joins,
            columns: vcols,
        }));
    }

    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/orders_customer.il.xml");

    #[test]
    fn parses_tables_fk_and_join_view() {
        let docs = parse_information_link(FIXTURE).expect("parse");
        let tables = uni_semantic_ir::tables(&docs);
        assert_eq!(tables.len(), 2);

        let orders = tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.schema, "sales");
        assert_eq!(orders.table, "orders");
        assert_eq!(orders.db, "dw");
        assert_eq!(orders.pk, vec!["order_id".to_string()]);
        assert_eq!(orders.fk.len(), 1);
        assert_eq!(orders.fk[0].columns, vec!["customer_id".to_string()]);
        assert_eq!(orders.fk[0].references.table, "customer");
        assert_eq!(
            orders.fk[0].references.columns,
            vec!["customer_id".to_string()]
        );

        let customer = tables.iter().find(|t| t.name == "customer").unwrap();
        assert_eq!(customer.pk, vec!["customer_id".to_string()]);
        assert!(customer.columns.iter().any(|c| c.name == "region_code"));

        let views = uni_semantic_ir::views(&docs);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].joins.len(), 1);
        assert_eq!(views[0].joins[0].join_type, "left");
        assert_eq!(views[0].joins[0].left, "orders");
        assert_eq!(views[0].joins[0].right, "customer");
    }

    #[test]
    fn rejects_non_information_link_root() {
        let err = parse_information_link("<dxp/>").expect_err("must fail");
        assert!(err.to_string().contains("informationLink"));
    }
}
