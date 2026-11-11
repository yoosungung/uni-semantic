//! IR → LookML (MVP subset).

use thiserror::Error;
use uni_semantic_ir::{Document, Metric, Table};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
}

fn dim_type(ir: &str) -> &'static str {
    match ir {
        "bigint" | "int" | "numeric" | "double" => "number",
        "boolean" => "yesno",
        "date" | "timestamp" => "date",
        _ => "string",
    }
}

fn emit_view(t: &Table, metrics: &[&Metric]) -> String {
    let mut out = String::new();
    out.push_str(&format!("view: {} {{\n", t.name));
    out.push_str(&format!(
        "  sql_table_name: `{}`.`{}`.`{}` ;;\n\n",
        t.db, t.schema, t.table
    ));
    for c in &t.columns {
        out.push_str(&format!("  dimension: {} {{\n", c.name));
        out.push_str(&format!("    type: {} \n", dim_type(&c.col_type)));
        if t.pk.iter().any(|p| p == &c.name) {
            out.push_str("    primary_key: yes\n");
        }
        out.push_str(&format!("    sql: ${{TABLE}}.{} ;;\n", c.name));
        out.push_str("  }\n\n");
    }
    for m in metrics.iter().filter(|m| m.cube == t.name) {
        let expr = m.expression.trim();
        let (mtype, col) = if let Some(r) = expr.strip_prefix("sum(") {
            ("sum", r.trim_end_matches(')'))
        } else if let Some(r) = expr.strip_prefix("count(") {
            ("count", r.trim_end_matches(')'))
        } else if let Some(r) = expr.strip_prefix("avg(") {
            ("average", r.trim_end_matches(')'))
        } else {
            continue;
        };
        out.push_str(&format!("  measure: {} {{\n", m.name));
        out.push_str(&format!("    type: {}\n", mtype));
        out.push_str(&format!("    sql: ${{TABLE}}.{} ;;\n", col.trim()));
        out.push_str("  }\n\n");
    }
    out.push_str("}\n\n");
    out
}

pub fn emit_lookml(docs: &[Document]) -> Result<String, Error> {
    let tables: Vec<&Table> = uni_semantic_ir::tables(docs);
    if tables.is_empty() {
        return Err(Error::Msg("no tables to emit".into()));
    }
    let metrics = uni_semantic_ir::metrics(docs);
    let mut out = String::new();
    for t in &tables {
        out.push_str(&emit_view(t, &metrics));
    }

    let views = uni_semantic_ir::views(docs);
    if let Some(v) = views.first() {
        let base = v
            .sources
            .first()
            .map(|s| s.table.clone())
            .unwrap_or_else(|| v.name.clone());
        out.push_str(&format!("explore: {} {{\n", v.name));
        out.push_str(&format!("  view_name: {}\n\n", base));
        for j in &v.joins {
            let on = j.on.first();
            let left_col = on.map(|o| o.left_column.as_str()).unwrap_or("id");
            let right_col = on.map(|o| o.right_column.as_str()).unwrap_or("id");
            let jtype = if j.join_type == "inner" {
                "inner"
            } else {
                "left_outer"
            };
            out.push_str(&format!("  join: {} {{\n", j.right));
            out.push_str(&format!("    type: {}\n", jtype));
            out.push_str("    relationship: many_to_one\n");
            out.push_str(&format!(
                "    sql_on: ${{{left}.{lc}}} = ${{{right}.{rc}}} ;;\n",
                left = j.left,
                lc = left_col,
                right = j.right,
                rc = right_col
            ));
            out.push_str("  }\n");
        }
        out.push_str("}\n");
    } else {
        // synthesize explore from first table FKs
        if let Some(t) = tables.first() {
            if !t.fk.is_empty() {
                let name = format!("{}_joins", t.name);
                out.push_str(&format!("explore: {} {{\n", name));
                out.push_str(&format!("  view_name: {}\n\n", t.name));
                for fk in &t.fk {
                    let lc = fk.columns.first().map(|s| s.as_str()).unwrap_or("id");
                    let rc = fk
                        .references
                        .columns
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("id");
                    out.push_str(&format!("  join: {} {{\n", fk.references.table));
                    out.push_str("    type: left_outer\n");
                    out.push_str("    relationship: many_to_one\n");
                    out.push_str(&format!(
                        "    sql_on: ${{{left}.{lc}}} = ${{{right}.{rc}}} ;;\n",
                        left = t.name,
                        right = fk.references.table
                    ));
                    out.push_str("  }\n");
                }
                out.push_str("}\n");
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uni_semantic_ir::{ForeignKey, FkRef, TableColumn};

    #[test]
    fn emits_view_and_explore() {
        let docs = vec![Document::Table(Table {
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
        })];
        let out = emit_lookml(&docs).unwrap();
        assert!(out.contains("view: orders"));
        assert!(out.contains("join: customer"));
        assert!(out.contains("primary_key: yes"));
    }
}
