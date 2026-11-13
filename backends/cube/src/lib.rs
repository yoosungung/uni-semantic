//! IR → Cube.js YAML data model (MVP subset).

use thiserror::Error;
use uni_semantic_ir::{Document, Metric, Table};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
}

fn yaml_str(s: &str) -> String {
    if s.is_empty()
        || s.contains(':')
        || s.contains('#')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('\n')
        || s.starts_with([' ', '-', '?', '|', '>', '*', '&', '!', '%', '@', '`'])
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn dim_type(col_type: &str) -> &'static str {
    match col_type {
        "bigint" | "int" | "integer" | "numeric" | "double" | "float" | "decimal" => "number",
        "boolean" | "bool" => "boolean",
        "date" | "timestamp" | "timestamptz" | "datetime" => "time",
        _ => "string",
    }
}

fn parse_agg(expr: &str) -> Option<(&'static str, &str)> {
    let expr = expr.trim();
    for (prefix, agg) in [("sum(", "sum"), ("count(", "count"), ("avg(", "avg")] {
        if let Some(rest) = expr.strip_prefix(prefix) {
            let col = rest.trim_end_matches(')').trim();
            if !col.is_empty() {
                return Some((agg, col));
            }
        }
    }
    None
}

fn emit_cube(t: &Table, metrics: &[&Metric], docs: &[Document]) -> String {
    let mut out = String::new();
    out.push_str("  - name: ");
    out.push_str(&yaml_str(&t.name));
    out.push('\n');
    out.push_str(&format!(
        "    sql_table: {}.{}.{}\n",
        t.db, t.schema, t.table
    ));

    // joins from matching view or table.fk
    let mut joins: Vec<(String, String, String, String)> = Vec::new();
    for v in uni_semantic_ir::views(docs) {
        let base = v
            .sources
            .first()
            .map(|s| s.table.as_str())
            .unwrap_or(v.name.as_str());
        if base != t.name {
            continue;
        }
        for j in &v.joins {
            if j.left != t.name {
                continue;
            }
            let on = j.on.first();
            let lc = on
                .map(|o| o.left_column.clone())
                .unwrap_or_else(|| "id".into());
            let rc = on
                .map(|o| o.right_column.clone())
                .unwrap_or_else(|| "id".into());
            joins.push((j.right.clone(), lc, rc, "many_to_one".into()));
        }
    }
    if joins.is_empty() {
        for fk in &t.fk {
            let lc = fk
                .columns
                .first()
                .cloned()
                .unwrap_or_else(|| "id".into());
            let rc = fk
                .references
                .columns
                .first()
                .cloned()
                .unwrap_or_else(|| "id".into());
            joins.push((fk.references.table.clone(), lc, rc, "many_to_one".into()));
        }
    }
    if !joins.is_empty() {
        out.push_str("    joins:\n");
        for (right, lc, rc, rel) in &joins {
            out.push_str(&format!("      - name: {}\n", yaml_str(right)));
            out.push_str(&format!("        relationship: {}\n", rel));
            out.push_str(&format!(
                "        sql: \"{{CUBE.{lc}}} = {{{right}.{rc}}}\"\n"
            ));
        }
    }

    out.push_str("    dimensions:\n");
    for c in &t.columns {
        out.push_str(&format!("      - name: {}\n", yaml_str(&c.name)));
        out.push_str(&format!("        sql: {}\n", yaml_str(&c.name)));
        out.push_str(&format!("        type: {}\n", dim_type(&c.col_type)));
        if t.pk.iter().any(|p| p == &c.name) {
            out.push_str("        primary_key: true\n");
        }
    }

    let table_metrics: Vec<&&Metric> = metrics.iter().filter(|m| m.cube == t.name).collect();
    if !table_metrics.is_empty() {
        out.push_str("    measures:\n");
        for m in table_metrics {
            if let Some((agg, col)) = parse_agg(&m.expression) {
                out.push_str(&format!("      - name: {}\n", yaml_str(&m.name)));
                out.push_str(&format!("        sql: {}\n", yaml_str(col)));
                out.push_str(&format!("        type: {}\n", agg));
            }
        }
    }
    out
}

/// Emit Cube.js YAML data model from IR documents.
pub fn emit_cube_yaml(docs: &[Document]) -> Result<String, Error> {
    let tables: Vec<&Table> = uni_semantic_ir::tables(docs);
    if tables.is_empty() {
        return Err(Error::Msg("no tables to emit".into()));
    }
    let metrics = uni_semantic_ir::metrics(docs);
    let mut out = String::from("cubes:\n");
    for t in &tables {
        out.push_str(&emit_cube(t, &metrics, docs));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uni_semantic_ir::{ForeignKey, FkRef, Metric, TableColumn};

    fn sample_docs() -> Vec<Document> {
        vec![
            Document::Table(Table {
                name: "orders".into(),
                target: "dw".into(),
                db: "dw".into(),
                schema: "sales".into(),
                table: "orders".into(),
                columns: vec![
                    TableColumn {
                        name: "order_id".into(),
                        col_type: "bigint".into(),
                        nullable: false,
                    },
                    TableColumn {
                        name: "customer_id".into(),
                        col_type: "bigint".into(),
                        nullable: false,
                    },
                    TableColumn {
                        name: "sale_amount".into(),
                        col_type: "numeric".into(),
                        nullable: false,
                    },
                ],
                pk: vec!["order_id".into()],
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
                description: "판매 합계".into(),
                cube: "orders".into(),
                expression: "sum(sale_amount)".into(),
                aggregation: "group".into(),
                group_by: vec![],
                filter: None,
                role: Some("all".into()),
            }),
        ]
    }

    #[test]
    fn emits_cube_dims_joins_measures() {
        let out = emit_cube_yaml(&sample_docs()).unwrap();
        assert!(out.contains("cubes:"));
        assert!(out.contains("name: orders"));
        assert!(out.contains("sql_table: dw.sales.orders"));
        assert!(out.contains("primary_key: true"));
        assert!(out.contains("join: customer") || out.contains("name: customer"));
        assert!(out.contains("relationship: many_to_one"));
        assert!(out.contains("type: sum"));
        assert!(out.contains("sql: sale_amount"));
        assert!(out.contains("name: revenue"));
    }

    #[test]
    fn empty_tables_errors() {
        let err = emit_cube_yaml(&[]).unwrap_err();
        assert!(err.to_string().contains("no tables"));
    }
}
