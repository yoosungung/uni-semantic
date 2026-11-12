//! Tableau `.tds` (classic relation tree) → Universal Semantic IR.

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
struct TdsTable {
    name: String,
    db: String,
    schema: String,
    physical: String,
    columns: Vec<TableColumn>,
    pk: Vec<String>,
}

#[derive(Debug, Clone)]
struct TdsJoin {
    join_type: String,
    left_alias: String,
    left_column: String,
    right_alias: String,
    right_column: String,
}

fn map_type(dt: &str) -> &'static str {
    match dt.to_ascii_lowercase().as_str() {
        "integer" | "int" | "bigint" | "smallint" => "bigint",
        "real" | "float" | "double" | "decimal" | "number" => "numeric",
        "boolean" | "bool" => "boolean",
        "date" => "date",
        "datetime" | "timestamp" => "timestamp",
        _ => "varchar",
    }
}

fn strip_brackets(s: &str) -> String {
    s.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .trim()
        .to_string()
}

/// Parse `[schema].[table]` / `[db].[schema].[table]` / bare name.
fn parse_table_ref(raw: &str) -> (String, String, String) {
    let parts: Vec<String> = raw
        .split('.')
        .map(strip_brackets)
        .filter(|p| !p.is_empty())
        .collect();
    match parts.as_slice() {
        [db, schema, table] => (db.clone(), schema.clone(), table.clone()),
        [schema, table] => ("dw".into(), schema.clone(), table.clone()),
        [table] => ("dw".into(), "public".into(), table.clone()),
        _ => ("dw".into(), "public".into(), normalize_ident(raw)),
    }
}

/// Parse `[alias].[col]` or `alias.col`.
fn parse_field_ref(raw: &str) -> Option<(String, String)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (alias, col) = if let Some((a, c)) = s.split_once("].[") {
        (strip_brackets(a), strip_brackets(c))
    } else if let Some((a, c)) = s.split_once('.') {
        (strip_brackets(a), strip_brackets(c))
    } else {
        return None;
    };
    if alias.is_empty() || col.is_empty() {
        return None;
    }
    Some((normalize_ident(&alias), normalize_ident(&col)))
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

fn collect_columns(table_node: roxmltree::Node<'_, '_>) -> Vec<TableColumn> {
    let mut cols = Vec::new();
    for columns in child_elems(table_node, "columns") {
        for col in child_elems(columns, "column") {
            let name = match attr(col, "name") {
                Some(n) if !n.is_empty() => normalize_ident(n),
                _ => continue,
            };
            let dt = attr(col, "datatype").unwrap_or("string");
            cols.push(TableColumn {
                name,
                col_type: map_type(dt).to_string(),
                nullable: true,
            });
        }
    }
    cols
}

fn infer_pk(name: &str, columns: &[TableColumn]) -> Vec<String> {
    let candidate = format!("{name}_id");
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

fn parse_equi_clause(join_node: roxmltree::Node<'_, '_>) -> Option<(String, String, String, String)> {
    for clause in child_elems(join_node, "clause") {
        for expr in child_elems(clause, "expression") {
            if attr(expr, "op") != Some("=") {
                continue;
            }
            let mut fields = Vec::new();
            for child in child_elems(expr, "expression") {
                if let Some(op) = attr(child, "op") {
                    if let Some(pair) = parse_field_ref(op) {
                        fields.push(pair);
                    }
                }
            }
            if fields.len() >= 2 {
                return Some((
                    fields[0].0.clone(),
                    fields[0].1.clone(),
                    fields[1].0.clone(),
                    fields[1].1.clone(),
                ));
            }
        }
    }
    None
}

fn walk_relation(
    node: roxmltree::Node<'_, '_>,
    db_default: &str,
    tables: &mut BTreeMap<String, TdsTable>,
    joins: &mut Vec<TdsJoin>,
) -> Result<(), Error> {
    let ty = attr(node, "type").unwrap_or("");
    match ty {
        "table" => {
            let name = normalize_ident(attr(node, "name").unwrap_or("unnamed"));
            let table_attr = attr(node, "table").unwrap_or("");
            let (mut db, schema, physical) = parse_table_ref(table_attr);
            if db == "dw" && !db_default.is_empty() {
                db = db_default.to_string();
            }
            let columns = collect_columns(node);
            if columns.is_empty() {
                return Err(Error::Msg(format!("table {name} has no columns")));
            }
            let pk = infer_pk(&name, &columns);
            tables.insert(
                name.clone(),
                TdsTable {
                    name,
                    db,
                    schema: normalize_ident(&schema),
                    physical: normalize_ident(&physical),
                    columns,
                    pk,
                },
            );
            Ok(())
        }
        "join" => {
            let join_type = match attr(node, "join").unwrap_or("left").to_ascii_lowercase().as_str() {
                "inner" => "inner",
                "right" => "right",
                "full" | "fullouter" => "full",
                _ => "left",
            }
            .to_string();
            let equi = parse_equi_clause(node).ok_or_else(|| {
                Error::Msg("join relation missing equi clause [alias].[col]".into())
            })?;
            // Children: nested join/table relations (skip clause).
            for child in node.children().filter(|c| c.is_element()) {
                if child.tag_name().name() == "relation" {
                    walk_relation(child, db_default, tables, joins)?;
                }
            }
            joins.push(TdsJoin {
                join_type,
                left_alias: equi.0,
                left_column: equi.1,
                right_alias: equi.2,
                right_column: equi.3,
            });
            Ok(())
        }
        "collection" => {
            for child in child_elems(node, "relation") {
                walk_relation(child, db_default, tables, joins)?;
            }
            Ok(())
        }
        other => Err(Error::Msg(format!("unsupported relation type: {other}"))),
    }
}

/// Parse classic Tableau `.tds` XML into IR documents.
pub fn parse_tds(xml: &str) -> Result<Vec<Document>, Error> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "datasource" {
        return Err(Error::Msg(format!(
            "expected root <datasource>, got <{}>",
            root.tag_name().name()
        )));
    }

    let connection = child_elems(root, "connection")
        .into_iter()
        .next()
        .ok_or_else(|| Error::Msg("datasource missing <connection>".into()))?;
    let db_default = attr(connection, "dbname").unwrap_or("dw");

    let relation = child_elems(connection, "relation")
        .into_iter()
        .next()
        .ok_or_else(|| Error::Msg("connection missing <relation>".into()))?;

    let mut tables = BTreeMap::new();
    let mut joins = Vec::new();
    walk_relation(relation, db_default, &mut tables, &mut joins)?;

    if tables.is_empty() {
        return Err(Error::Msg("no tables found in relation tree".into()));
    }

    let mut docs = Vec::new();
    let table_order: Vec<String> = tables.keys().cloned().collect();

    for t in tables.values() {
        let mut fk = Vec::new();
        for j in &joins {
            if j.left_alias == t.name {
                fk.push(ForeignKey {
                    columns: vec![j.left_column.clone()],
                    references: FkRef {
                        table: j.right_alias.clone(),
                        columns: vec![j.right_column.clone()],
                    },
                });
            }
        }
        docs.push(Document::Table(Table {
            name: t.name.clone(),
            target: "dw".into(),
            db: t.db.clone(),
            schema: t.schema.clone(),
            table: t.physical.clone(),
            columns: t.columns.clone(),
            pk: t.pk.clone(),
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
            for n in [&j.left_alias, &j.right_alias] {
                if seen.insert(n.clone()) {
                    sources.push(ViewSource {
                        kind: "table".into(),
                        table: n.clone(),
                    });
                }
            }
            view_joins.push(ViewJoin {
                left: j.left_alias.clone(),
                right: j.right_alias.clone(),
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

    const FIXTURE: &str = include_str!("../fixtures/orders_customer.tds");

    #[test]
    fn parses_tables_fk_and_join_view() {
        let docs = parse_tds(FIXTURE).expect("parse");
        let tables = uni_semantic_ir::tables(&docs);
        assert_eq!(tables.len(), 2);

        let orders = tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.schema, "sales");
        assert_eq!(orders.table, "orders");
        assert_eq!(orders.pk, vec!["order_id".to_string()]);
        assert_eq!(orders.fk.len(), 1);
        assert_eq!(orders.fk[0].columns, vec!["customer_id".to_string()]);
        assert_eq!(orders.fk[0].references.table, "customer");
        assert_eq!(orders.fk[0].references.columns, vec!["customer_id".to_string()]);

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
    fn rejects_non_datasource_root() {
        let err = parse_tds("<workbook/>").expect_err("must fail");
        assert!(err.to_string().contains("datasource"));
    }
}
