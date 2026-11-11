//! Physical SQL equivalence helpers for Phase 1 (tables · joins · SUM/COUNT/AVG).
//!
//! Equivalence policy (ticket MVP): **normalized string** compare, with optional
//! Phase-1 shape (tables/joins/aggs) as a secondary assert. Full SQL AST is out of scope.

use std::collections::BTreeMap;
use thiserror::Error;
use uni_semantic_ir::{Document, Metric, Table, View};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("no tables in IR document set")]
    NoTables,
    #[error("no metrics in IR document set")]
    NoMetrics,
    #[error("metric cube `{0}` has no matching table")]
    MissingCube(String),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareResult {
    Equal,
    Differ {
        left_norm: String,
        right_norm: String,
    },
}

/// Lowercase, collapse whitespace, strip trailing `;`, drop `` ` `` / `"` around idents.
pub fn normalize_sql(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut prev_space = true;
    for ch in sql.chars() {
        let c = match ch {
            '`' | '"' => continue,
            c if c.is_whitespace() => ' ',
            c => c.to_ascii_lowercase(),
        };
        if c == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
            out.push(' ');
        } else {
            prev_space = false;
            out.push(c);
        }
    }
    out.trim().trim_end_matches(';').trim().to_string()
}

pub fn compare_sql(left: &str, right: &str) -> CompareResult {
    let left_norm = normalize_sql(left);
    let right_norm = normalize_sql(right);
    if left_norm == right_norm {
        CompareResult::Equal
    } else {
        CompareResult::Differ {
            left_norm,
            right_norm,
        }
    }
}

pub fn sql_equivalent(left: &str, right: &str) -> bool {
    matches!(compare_sql(left, right), CompareResult::Equal)
}

/// Canonical Phase-1 surface extracted from IR (order-insensitive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase1Shape {
    pub tables: Vec<String>,
    /// `(left, right, join_type)`
    pub joins: Vec<(String, String, String)>,
    /// `(agg, cube, column)` from `sum|count|avg(col)`
    pub aggs: Vec<(String, String, String)>,
}

pub fn phase1_shape(docs: &[Document]) -> Phase1Shape {
    let mut tables: Vec<String> = uni_semantic_ir::tables(docs)
        .iter()
        .map(|t| format!("{}.{}.{}", t.db, t.schema, t.table))
        .collect();
    tables.sort();
    tables.dedup();

    let mut joins: Vec<(String, String, String)> = Vec::new();
    for v in uni_semantic_ir::views(docs) {
        for j in &v.joins {
            joins.push((
                j.left.clone(),
                j.right.clone(),
                j.join_type.to_ascii_lowercase(),
            ));
        }
    }
    // FK fallback when views absent
    if joins.is_empty() {
        for t in uni_semantic_ir::tables(docs) {
            for fk in &t.fk {
                joins.push((
                    t.name.clone(),
                    fk.references.table.clone(),
                    "left".into(),
                ));
            }
        }
    }
    joins.sort();
    joins.dedup();

    let mut aggs: Vec<(String, String, String)> = Vec::new();
    for m in uni_semantic_ir::metrics(docs) {
        if let Some((agg, col)) = parse_agg_expr(&m.expression) {
            aggs.push((agg, m.cube.clone(), col));
        }
    }
    aggs.sort();
    aggs.dedup();

    Phase1Shape {
        tables,
        joins,
        aggs,
    }
}

fn parse_agg_expr(expr: &str) -> Option<(String, String)> {
    let e = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = e.to_ascii_lowercase();
    let (agg, rest) = if let Some(r) = lower.strip_prefix("sum(") {
        ("sum", r)
    } else if let Some(r) = lower.strip_prefix("count(") {
        ("count", r)
    } else if let Some(r) = lower.strip_prefix("avg(") {
        ("avg", r)
    } else {
        return None;
    };
    let col = rest.trim_end_matches(')').trim().to_string();
    if col.is_empty() {
        None
    } else {
        Some((agg.into(), col))
    }
}

fn qual_table(t: &Table) -> String {
    format!("`{}`.`{}`.`{}`", t.db, t.schema, t.table)
}

fn join_keyword(join_type: &str) -> &'static str {
    match join_type.to_ascii_lowercase().as_str() {
        "inner" => "INNER JOIN",
        "left" | "left_outer" => "LEFT JOIN",
        "right" | "right_outer" => "RIGHT JOIN",
        "full" | "full_outer" => "FULL JOIN",
        _ => "LEFT JOIN",
    }
}

/// Derive a deterministic Phase-1 physical SQL string from IR (fixture / unit use).
///
/// Shape: `SELECT <agg>(<cube>.<col>) AS <metric> FROM … [JOIN …]`.
/// One statement per metric, joined with newline. Not a warehouse dialect transpiler.
pub fn emit_physical_sql(docs: &[Document]) -> Result<String, Error> {
    let table_list = uni_semantic_ir::tables(docs);
    if table_list.is_empty() {
        return Err(Error::NoTables);
    }
    let by_name: BTreeMap<&str, &Table> = table_list.iter().map(|t| (t.name.as_str(), *t)).collect();
    let metrics = uni_semantic_ir::metrics(docs);
    if metrics.is_empty() {
        return Err(Error::NoMetrics);
    }
    let views = uni_semantic_ir::views(docs);

    let mut stmts: Vec<String> = Vec::new();
    for m in &metrics {
        stmts.push(emit_metric_sql(m, &by_name, &views)?);
    }
    stmts.sort();
    Ok(stmts.join("\n"))
}

fn emit_metric_sql(
    m: &Metric,
    by_name: &BTreeMap<&str, &Table>,
    views: &[&View],
) -> Result<String, Error> {
    let (agg, col) = parse_agg_expr(&m.expression)
        .ok_or_else(|| Error::Msg(format!("unparsed agg in {}", m.expression)))?;
    let cube = by_name
        .get(m.cube.as_str())
        .copied()
        .ok_or_else(|| Error::MissingCube(m.cube.clone()))?;

    let mut sql = String::new();
    sql.push_str(&format!(
        "SELECT {agg}({cube}.{col}) AS {name}\nFROM {qual} AS {cube}",
        agg = agg.to_ascii_uppercase(),
        cube = cube.name,
        col = col,
        name = m.name,
        qual = qual_table(cube),
    ));

    let mut emitted_joins = Vec::new();
    for v in views {
        for j in &v.joins {
            if j.left != cube.name && j.right != cube.name {
                // still include joins that attach the graph of the cube's view
                if !v.sources.iter().any(|s| s.table == cube.name) {
                    continue;
                }
            }
            let right = by_name
                .get(j.right.as_str())
                .copied()
                .ok_or_else(|| Error::MissingCube(j.right.clone()))?;
            let on = j.on.first();
            let lc = on.map(|o| o.left_column.as_str()).unwrap_or("id");
            let rc = on.map(|o| o.right_column.as_str()).unwrap_or("id");
            let key = (j.left.clone(), j.right.clone(), j.join_type.clone());
            if emitted_joins.contains(&key) {
                continue;
            }
            emitted_joins.push(key);
            sql.push_str(&format!(
                "\n{kw} {qual} AS {alias} ON {left}.{lc} = {alias}.{rc}",
                kw = join_keyword(&j.join_type),
                qual = qual_table(right),
                alias = j.right,
                left = j.left,
                lc = lc,
                rc = rc,
            ));
        }
    }

    if emitted_joins.is_empty() {
        for fk in &cube.fk {
            let right = by_name
                .get(fk.references.table.as_str())
                .copied()
                .ok_or_else(|| Error::MissingCube(fk.references.table.clone()))?;
            let lc = fk.columns.first().map(|s| s.as_str()).unwrap_or("id");
            let rc = fk
                .references
                .columns
                .first()
                .map(|s| s.as_str())
                .unwrap_or("id");
            sql.push_str(&format!(
                "\nLEFT JOIN {qual} AS {alias} ON {left}.{lc} = {alias}.{rc}",
                qual = qual_table(right),
                alias = fk.references.table,
                left = cube.name,
                lc = lc,
                rc = rc,
            ));
        }
    }

    Ok(sql)
}

#[cfg(test)]
mod unit {
    use super::*;
    use uni_semantic_ir::{
        ForeignKey, FkRef, JoinOn, TableColumn, ViewJoin, ViewSource,
    };

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
            Document::Table(Table {
                name: "customer".into(),
                target: "dw".into(),
                db: "dw".into(),
                schema: "sales".into(),
                table: "customer".into(),
                columns: vec![TableColumn {
                    name: "customer_id".into(),
                    col_type: "bigint".into(),
                    nullable: false,
                }],
                pk: vec!["customer_id".into()],
                fk: vec![],
                permission_filters: vec![],
                scope_filter: None,
                default_filter: None,
                role: Some("all".into()),
            }),
            Document::View(View {
                name: "orders_customer".into(),
                target: "dw".into(),
                combine: "join".into(),
                sources: vec![
                    ViewSource {
                        kind: "table".into(),
                        table: "orders".into(),
                    },
                    ViewSource {
                        kind: "table".into(),
                        table: "customer".into(),
                    },
                ],
                joins: vec![ViewJoin {
                    left: "orders".into(),
                    right: "customer".into(),
                    join_type: "inner".into(),
                    on: vec![JoinOn {
                        left_column: "customer_id".into(),
                        right_column: "customer_id".into(),
                    }],
                }],
                columns: vec![],
            }),
            Document::Metric(Metric {
                name: "revenue".into(),
                description: String::new(),
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
    fn normalize_collapses_whitespace_and_case() {
        let a = "SELECT  Sum(orders.sale_amount)  AS revenue ;";
        let b = "select sum(orders.sale_amount) as revenue";
        assert!(sql_equivalent(a, b));
    }

    #[test]
    fn emit_pass_fixture_inner_sum() {
        let sql = emit_physical_sql(&sample_docs()).unwrap();
        assert!(sql.to_ascii_uppercase().contains("SUM(ORDERS.SALE_AMOUNT)"));
        assert!(sql.to_ascii_uppercase().contains("INNER JOIN"));
        let shape = phase1_shape(&sample_docs());
        assert!(shape.tables.iter().any(|t| t.contains("orders")));
        assert_eq!(shape.joins[0].2, "inner");
        assert_eq!(shape.aggs[0].0, "sum");
    }

    #[test]
    fn intentional_mismatch_fails_compare() {
        let left = emit_physical_sql(&sample_docs()).unwrap();
        let mut right_docs = sample_docs();
        if let Document::Metric(m) = &mut right_docs[3] {
            m.expression = "count(sale_amount)".into();
        }
        let right = emit_physical_sql(&right_docs).unwrap();
        assert!(
            !sql_equivalent(&left, &right),
            "sum vs count must differ:\n{left}\n---\n{right}"
        );
        assert!(matches!(
            compare_sql(&left, &right),
            CompareResult::Differ { .. }
        ));
    }
}
