//! IR → dbt Semantic Layer YAML (MetricFlow standalone subset).

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

fn dim_kind(col_type: &str) -> (&'static str, bool) {
    match col_type {
        "date" | "timestamp" | "timestamptz" | "datetime" => ("time", true),
        _ => ("categorical", false),
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

fn emit_semantic_model(t: &Table, metrics: &[&Metric]) -> String {
    let mut out = String::new();
    out.push_str("  - name: ");
    out.push_str(&yaml_str(&t.name));
    out.push('\n');
    out.push_str(&format!("    model: ref('{}')\n", t.name.replace('\'', "''")));
    out.push_str("    entities:\n");
    if t.pk.is_empty() {
        out.push_str(&format!(
            "      - name: {}\n        type: primary\n        expr: id\n",
            yaml_str(&t.name)
        ));
    } else {
        for pk in &t.pk {
            out.push_str(&format!(
                "      - name: {}\n        type: primary\n        expr: {}\n",
                yaml_str(pk),
                yaml_str(pk)
            ));
        }
    }
    for fk in &t.fk {
        let expr = fk.columns.first().map(|s| s.as_str()).unwrap_or("id");
        out.push_str(&format!(
            "      - name: {}\n        type: foreign\n        expr: {}\n",
            yaml_str(&fk.references.table),
            yaml_str(expr)
        ));
    }
    out.push_str("    dimensions:\n");
    for c in &t.columns {
        let (dtype, is_time) = dim_kind(&c.col_type);
        out.push_str(&format!(
            "      - name: {}\n        type: {}\n        expr: {}\n",
            yaml_str(&c.name),
            dtype,
            yaml_str(&c.name)
        ));
        if is_time {
            out.push_str("        type_params:\n          time_granularity: day\n");
        }
    }
    let table_metrics: Vec<&&Metric> = metrics.iter().filter(|m| m.cube == t.name).collect();
    if !table_metrics.is_empty() {
        out.push_str("    measures:\n");
        for m in table_metrics {
            if let Some((agg, col)) = parse_agg(&m.expression) {
                out.push_str(&format!(
                    "      - name: {}\n        agg: {}\n        expr: {}\n",
                    yaml_str(&m.name),
                    agg,
                    yaml_str(col)
                ));
                if !m.description.is_empty() {
                    out.push_str(&format!(
                        "        description: {}\n",
                        yaml_str(&m.description)
                    ));
                }
            }
        }
    }
    out
}

/// Emit MetricFlow-compatible Semantic Layer YAML from IR documents.
pub fn emit_dbt_yaml(docs: &[Document]) -> Result<String, Error> {
    let tables: Vec<&Table> = uni_semantic_ir::tables(docs);
    if tables.is_empty() {
        return Err(Error::Msg("no tables to emit".into()));
    }
    let metrics = uni_semantic_ir::metrics(docs);
    let mut out = String::from("semantic_models:\n");
    for t in &tables {
        out.push_str(&emit_semantic_model(t, &metrics));
    }

    let emitted: Vec<&Metric> = metrics
        .iter()
        .copied()
        .filter(|m| tables.iter().any(|t| t.name == m.cube) && parse_agg(&m.expression).is_some())
        .collect();
    if !emitted.is_empty() {
        out.push_str("\nmetrics:\n");
        for m in emitted {
            out.push_str(&format!("  - name: {}\n", yaml_str(&m.name)));
            if !m.description.is_empty() {
                out.push_str(&format!("    description: {}\n", yaml_str(&m.description)));
            }
            out.push_str("    type: simple\n");
            out.push_str("    type_params:\n");
            out.push_str(&format!("      measure: {}\n", yaml_str(&m.name)));
        }
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
                        name: "order_date".into(),
                        col_type: "date".into(),
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
    fn emits_semantic_model_entities_dims_and_metric() {
        let out = emit_dbt_yaml(&sample_docs()).unwrap();
        assert!(out.contains("semantic_models:"));
        assert!(out.contains("name: orders"));
        assert!(out.contains("model: ref('orders')"));
        assert!(out.contains("type: primary"));
        assert!(out.contains("type: foreign"));
        assert!(out.contains("name: customer"));
        assert!(out.contains("type: time"));
        assert!(out.contains("time_granularity: day"));
        assert!(out.contains("agg: sum"));
        assert!(out.contains("expr: sale_amount"));
        assert!(out.contains("metrics:"));
        assert!(out.contains("type: simple"));
        assert!(out.contains("measure: revenue"));
    }

    #[test]
    fn empty_tables_errors() {
        let err = emit_dbt_yaml(&[]).unwrap_err();
        assert!(err.to_string().contains("no tables"));
    }
}
