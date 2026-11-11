//! LookML MVP subset → Universal Semantic IR.

use std::collections::BTreeMap;
use thiserror::Error;
use uni_semantic_ir::{
    normalize_ident, ColumnMapping, Document, ForeignKey, FkRef, JoinOn, Metric, Table,
    TableColumn, View, ViewColumn, ViewJoin, ViewSource,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Default)]
struct ViewAst {
    name: String,
    sql_table_name: Option<String>,
    dimensions: Vec<DimAst>,
    measures: Vec<MeasAst>,
}

#[derive(Debug)]
struct DimAst {
    name: String,
    sql: Option<String>,
    primary_key: bool,
    dim_type: Option<String>,
}

#[derive(Debug)]
struct MeasAst {
    name: String,
    meas_type: String,
    sql: Option<String>,
}

#[derive(Debug, Default)]
struct ExploreAst {
    name: String,
    view_name: Option<String>,
    joins: Vec<JoinAst>,
}

#[derive(Debug)]
struct JoinAst {
    name: String,
    join_type: String,
    sql_on: Option<String>,
}

fn strip_quotes(s: &str) -> String {
    let t = s.trim().trim_end_matches(';').trim();
    t.trim_matches('`').trim_matches('"').trim_matches('\'').to_string()
}

fn parse_sql_table_name(raw: &str) -> (String, String, String) {
    let s = strip_quotes(raw);
    let parts: Vec<&str> = s.split('.').map(|p| p.trim_matches('`')).collect();
    match parts.as_slice() {
        [db, schema, table] => (db.to_string(), schema.to_string(), table.to_string()),
        [schema, table] => ("dw".into(), schema.to_string(), table.to_string()),
        [table] => ("dw".into(), "public".into(), table.to_string()),
        _ => ("dw".into(), "public".into(), normalize_ident(&s)),
    }
}

fn field_sql_column(sql: &str) -> String {
    let s = sql.trim().trim_end_matches(';').trim();
    if let Some(idx) = s.rfind('.') {
        normalize_ident(&s[idx + 1..])
    } else {
        normalize_ident(s.trim_start_matches("${TABLE}").trim_start_matches('.'))
    }
}

/// Very small LookML subset parser (line-oriented).
pub fn parse_lookml(src: &str) -> Result<Vec<Document>, Error> {
    let mut views: BTreeMap<String, ViewAst> = BTreeMap::new();
    let mut explores: Vec<ExploreAst> = Vec::new();

    let mut cur_view: Option<String> = None;
    let mut cur_explore: Option<usize> = None;
    let mut cur_dim: Option<DimAst> = None;
    let mut cur_meas: Option<MeasAst> = None;
    let mut cur_join: Option<JoinAst> = None;
    let mut brace_depth = 0i32;

    let flush_dim = |v: &mut ViewAst, d: Option<DimAst>| {
        if let Some(d) = d {
            v.dimensions.push(d);
        }
    };
    let flush_meas = |v: &mut ViewAst, m: Option<MeasAst>| {
        if let Some(m) = m {
            v.measures.push(m);
        }
    };

    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let opens = line.chars().filter(|c| *c == '{').count() as i32;
        let closes = line.chars().filter(|c| *c == '}').count() as i32;

        if let Some(rest) = line.strip_prefix("view:") {
            let name = normalize_ident(rest.split('{').next().unwrap_or("").trim());
            views.entry(name.clone()).or_insert(ViewAst {
                name: name.clone(),
                ..Default::default()
            });
            cur_view = Some(name);
            cur_explore = None;
            cur_dim = None;
            cur_meas = None;
            cur_join = None;
        } else if let Some(rest) = line.strip_prefix("explore:") {
            let name = normalize_ident(rest.split('{').next().unwrap_or("").trim());
            explores.push(ExploreAst {
                name,
                ..Default::default()
            });
            cur_explore = Some(explores.len() - 1);
            cur_view = None;
            cur_dim = None;
            cur_meas = None;
            cur_join = None;
        } else if line.starts_with("dimension:") {
            if let Some(ref vn) = cur_view {
                if let Some(v) = views.get_mut(vn) {
                    flush_dim(v, cur_dim.take());
                    flush_meas(v, cur_meas.take());
                }
            }
            let name = normalize_ident(
                line.trim_start_matches("dimension:")
                    .split('{')
                    .next()
                    .unwrap_or("")
                    .trim(),
            );
            cur_dim = Some(DimAst {
                name,
                sql: None,
                primary_key: false,
                dim_type: None,
            });
            cur_meas = None;
        } else if line.starts_with("measure:") {
            if let Some(ref vn) = cur_view {
                if let Some(v) = views.get_mut(vn) {
                    flush_dim(v, cur_dim.take());
                    flush_meas(v, cur_meas.take());
                }
            }
            let name = normalize_ident(
                line.trim_start_matches("measure:")
                    .split('{')
                    .next()
                    .unwrap_or("")
                    .trim(),
            );
            cur_meas = Some(MeasAst {
                name,
                meas_type: "sum".into(),
                sql: None,
            });
            cur_dim = None;
        } else if line.starts_with("join:") {
            if let Some(ei) = cur_explore {
                if let Some(j) = cur_join.take() {
                    explores[ei].joins.push(j);
                }
                let name = normalize_ident(
                    line.trim_start_matches("join:")
                        .split('{')
                        .next()
                        .unwrap_or("")
                        .trim(),
                );
                cur_join = Some(JoinAst {
                    name,
                    join_type: "left_outer".into(),
                    sql_on: None,
                });
            }
        } else if let Some(rest) = line.strip_prefix("sql_table_name:") {
            if let Some(ref vn) = cur_view {
                if let Some(v) = views.get_mut(vn) {
                    v.sql_table_name = Some(strip_quotes(rest));
                }
            }
        } else if let Some(rest) = line.strip_prefix("view_name:") {
            if let Some(ei) = cur_explore {
                explores[ei].view_name = Some(normalize_ident(&strip_quotes(rest)));
            }
        } else if let Some(rest) = line.strip_prefix("sql:") {
            let val = strip_quotes(rest);
            if let Some(ref mut d) = cur_dim {
                d.sql = Some(val);
            } else if let Some(ref mut m) = cur_meas {
                m.sql = Some(val);
            }
        } else if let Some(rest) = line.strip_prefix("sql_on:") {
            if let Some(ref mut j) = cur_join {
                j.sql_on = Some(strip_quotes(rest));
            }
        } else if let Some(rest) = line.strip_prefix("type:") {
            let t = strip_quotes(rest);
            if let Some(ref mut d) = cur_dim {
                d.dim_type = Some(t);
            } else if let Some(ref mut m) = cur_meas {
                m.meas_type = t;
            } else if let Some(ref mut j) = cur_join {
                j.join_type = t;
            }
        } else if let Some(rest) = line.strip_prefix("primary_key:") {
            if let Some(ref mut d) = cur_dim {
                d.primary_key = strip_quotes(rest).eq_ignore_ascii_case("yes");
            }
        }

        brace_depth += opens - closes;
        if closes > 0 && brace_depth >= 0 {
            // end of nested block heuristics
            if cur_dim.is_some() || cur_meas.is_some() {
                if let Some(ref vn) = cur_view {
                    if let Some(v) = views.get_mut(vn) {
                        if line.contains('}') && opens == 0 {
                            flush_dim(v, cur_dim.take());
                            flush_meas(v, cur_meas.take());
                        }
                    }
                }
            }
            if cur_join.is_some() && line.contains('}') && opens == 0 {
                if let Some(ei) = cur_explore {
                    if let Some(j) = cur_join.take() {
                        explores[ei].joins.push(j);
                    }
                }
            }
        }
    }

    if let Some(ref vn) = cur_view {
        if let Some(v) = views.get_mut(vn) {
            flush_dim(v, cur_dim.take());
            flush_meas(v, cur_meas.take());
        }
    }
    if let Some(ei) = cur_explore {
        if let Some(j) = cur_join.take() {
            explores[ei].joins.push(j);
        }
    }

    let mut docs = Vec::new();
    for v in views.values() {
        let (db, schema, table) = v
            .sql_table_name
            .as_deref()
            .map(parse_sql_table_name)
            .unwrap_or_else(|| ("dw".into(), "public".into(), v.name.clone()));
        let mut pk = Vec::new();
        let mut columns = Vec::new();
        for d in &v.dimensions {
            let col = d
                .sql
                .as_deref()
                .map(field_sql_column)
                .unwrap_or_else(|| d.name.clone());
            if d.primary_key {
                pk.push(col.clone());
            }
            let col_type = match d.dim_type.as_deref() {
                Some("number") => "bigint",
                Some("yesno") => "boolean",
                Some("date") | Some("time") => "date",
                _ => "varchar",
            };
            columns.push(TableColumn {
                name: col,
                col_type: col_type.into(),
                nullable: !d.primary_key,
            });
        }
        if columns.is_empty() {
            return Err(Error::Msg(format!("view {} has no dimensions", v.name)));
        }
        docs.push(Document::Table(Table {
            name: v.name.clone(),
            target: "dw".into(),
            db,
            schema,
            table,
            columns,
            pk: pk.clone(),
            fk: vec![],
            permission_filters: vec![],
            scope_filter: None,
            default_filter: None,
            role: Some("all".into()),
        }));

        for m in &v.measures {
            let agg = match m.meas_type.to_ascii_lowercase().as_str() {
                "sum" => "sum",
                "count" => "count",
                "average" | "avg" => "avg",
                _ => continue,
            };
            let col = m
                .sql
                .as_deref()
                .map(field_sql_column)
                .unwrap_or_else(|| m.name.clone());
            let group = pk.first().cloned().unwrap_or_else(|| col.clone());
            docs.push(Document::Metric(Metric {
                name: m.name.clone(),
                description: String::new(),
                cube: v.name.clone(),
                expression: format!("{agg}({col})"),
                aggregation: "group".into(),
                group_by: vec![group],
                filter: None,
                role: Some("all".into()),
            }));
        }
    }

    for ex in &explores {
        if ex.joins.is_empty() {
            continue;
        }
        let base = ex
            .view_name
            .clone()
            .unwrap_or_else(|| ex.name.clone());
        let mut sources = vec![ViewSource {
            kind: "table".into(),
            table: base.clone(),
        }];
        let mut joins = Vec::new();
        let mut fk_updates: Vec<(String, ForeignKey)> = Vec::new();

        for j in &ex.joins {
            sources.push(ViewSource {
                kind: "table".into(),
                table: j.name.clone(),
            });
            let join_type = match j.join_type.as_str() {
                "inner" => "inner",
                _ => "left",
            };
            let (left_col, right_col) = parse_sql_on(j.sql_on.as_deref().unwrap_or(""));
            joins.push(ViewJoin {
                left: base.clone(),
                right: j.name.clone(),
                join_type: join_type.into(),
                on: vec![JoinOn {
                    left_column: left_col.clone(),
                    right_column: right_col.clone(),
                }],
            });
            fk_updates.push((
                base.clone(),
                ForeignKey {
                    columns: vec![left_col],
                    references: FkRef {
                        table: j.name.clone(),
                        columns: vec![right_col],
                    },
                },
            ));
        }

        for (table_name, fk) in fk_updates {
            for d in &mut docs {
                if let Document::Table(t) = d {
                    if t.name == table_name {
                        t.fk.push(fk.clone());
                    }
                }
            }
        }

        let mut vcols = Vec::new();
        for t in uni_semantic_ir::tables(&docs) {
            for c in &t.columns {
                vcols.push(ViewColumn {
                    name: if t.name == base {
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
            name: ex.name.clone(),
            target: "dw".into(),
            combine: "join".into(),
            sources,
            joins,
            columns: vcols,
        }));
    }

    Ok(docs)
}

fn parse_sql_on(sql: &str) -> (String, String) {
    // ${orders.customer_id} = ${customer.customer_id}
    let parts: Vec<&str> = sql.split('=').collect();
    if parts.len() != 2 {
        return ("id".into(), "id".into());
    }
    let left = parts[0]
        .trim()
        .trim_start_matches("${")
        .trim_end_matches('}');
    let right = parts[1]
        .trim()
        .trim_start_matches("${")
        .trim_end_matches(';')
        .trim_end_matches('}');
    let lc = left.rsplit('.').next().unwrap_or("id");
    let rc = right.rsplit('.').next().unwrap_or("id");
    (normalize_ident(lc), normalize_ident(rc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/orders_customer.lkml");

    #[test]
    fn parses_views_explore_metric() {
        let docs = parse_lookml(FIXTURE).expect("parse");
        assert!(uni_semantic_ir::tables(&docs).iter().any(|t| t.name == "orders"));
        assert!(uni_semantic_ir::metrics(&docs)
            .iter()
            .any(|m| m.expression == "sum(sale_amount)"));
        assert!(!uni_semantic_ir::views(&docs).is_empty());
        let orders = uni_semantic_ir::tables(&docs)
            .into_iter()
            .find(|t| t.name == "orders")
            .unwrap();
        assert_eq!(orders.fk.len(), 1);
    }
}
