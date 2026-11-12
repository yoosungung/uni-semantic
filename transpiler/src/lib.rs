//! Dialect Transpiler — classify dialect expressions into common vs dialect-specific.
//!
//! PoC: classifier only (ARCHITECTURE §1.4–1.5 · §4). Emit / Unresolved isolation = later tickets.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Dax,
    Lookml,
    Lod,
    Mstr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonAgg {
    Sum,
    Count,
    Avg,
}

impl CommonAgg {
    pub fn as_str(self) -> &'static str {
        match self {
            CommonAgg::Sum => "sum",
            CommonAgg::Count => "count",
            CommonAgg::Avg => "avg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// 교집합 — ARCHITECTURE §1.4 공통 표준 집계 (자동 컴파일 후보).
    Intersection { aggregation: CommonAgg },
    /// 차집합 — 방언 고유 (Unresolved 후보; 격리는 후속 티켓).
    Difference { reason: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("empty expression")]
    EmptyExpression,
}

/// Classify a dialect-native expression into intersection (common agg) or difference.
pub fn classify(dialect: Dialect, expression: &str) -> Result<Classification, Error> {
    let expr = collapse_ws(expression);
    if expr.is_empty() {
        return Err(Error::EmptyExpression);
    }

    Ok(match dialect {
        Dialect::Dax => classify_dax(&expr),
        Dialect::Lookml => classify_lookml(&expr),
        Dialect::Lod => classify_lod(&expr),
        Dialect::Mstr => classify_mstr(&expr),
    })
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn intersection(agg: CommonAgg) -> Classification {
    Classification::Intersection { aggregation: agg }
}

fn difference(reason: impl Into<String>) -> Classification {
    Classification::Difference {
        reason: reason.into(),
    }
}

/// Strip a balanced outer `NAME(...)` and return `(agg, inner)` if the whole expr is that call.
fn take_fn_call<'a>(expr: &'a str, names: &[&str]) -> Option<(CommonAgg, &'a str)> {
    let upper = expr.to_ascii_uppercase();
    for name in names {
        let prefix = format!("{name}(");
        if upper.starts_with(&prefix) && expr.ends_with(')') {
            let inner = &expr[prefix.len()..expr.len() - 1];
            // Reject nested top-level calls that imply context (simple heuristic: another FN( after start)
            let agg = match *name {
                "SUM" => CommonAgg::Sum,
                "COUNT" => CommonAgg::Count,
                "AVG" | "AVERAGE" => CommonAgg::Avg,
                _ => continue,
            };
            return Some((agg, inner.trim()));
        }
    }
    None
}

fn classify_dax(expr: &str) -> Classification {
    let upper = expr.to_ascii_uppercase();
    // Dialect-specific context first.
    for marker in [
        "CALCULATE(",
        "FILTER(",
        "ALL(",
        "ALLEXCEPT(",
        "RELATED(",
        "RELATEDTABLE(",
        "EARLIER(",
        "USERELATIONSHIP(",
    ] {
        if upper.contains(marker) {
            return difference(format!("dax context `{marker}`"));
        }
    }
    match take_fn_call(expr, &["SUM", "COUNT", "AVERAGE", "AVG"]) {
        Some((agg, _)) => intersection(agg),
        None => difference("dax: not a lone SUM|COUNT|AVERAGE|AVG call"),
    }
}

fn classify_lookml(expr: &str) -> Classification {
    let lower = expr.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("type:") {
        let t = rest.trim();
        return match t {
            "sum" => intersection(CommonAgg::Sum),
            "count" => intersection(CommonAgg::Count),
            "average" | "avg" => intersection(CommonAgg::Avg),
            other => difference(format!("lookml type `{other}` not in sum|count|average")),
        };
    }
    match take_fn_call(expr, &["SUM", "COUNT", "AVERAGE", "AVG"]) {
        Some((agg, _)) => intersection(agg),
        None => difference("lookml: not type:sum|count|average nor lone agg call"),
    }
}

fn classify_lod(expr: &str) -> Classification {
    let upper = expr.to_ascii_uppercase();
    // Tableau LOD braces
    if expr.contains('{') {
        for marker in ["FIXED", "INCLUDE", "EXCLUDE"] {
            if upper.contains(marker) {
                return difference(format!("lod `{marker}` expression"));
            }
        }
        return difference("lod brace expression");
    }
    match take_fn_call(expr, &["SUM", "COUNT", "AVG", "AVERAGE"]) {
        Some((agg, _)) => intersection(agg),
        None => difference("lod: not a lone SUM|COUNT|AVG call"),
    }
}

fn classify_mstr(expr: &str) -> Classification {
    // Level / dimensionality markers: {...} after metric, e.g. Sum(X){#Region} or Sum(X){~+}
    if let Some(idx) = expr.find('{') {
        if idx > 0 {
            return difference("mstr level/dimensionality `{…}`");
        }
    }
    // MSTR often capitalizes Sum/Count/Avg (not all-caps).
    let upper = expr.to_ascii_uppercase();
    match take_fn_call(&upper, &["SUM", "COUNT", "AVG", "AVERAGE"]) {
        Some((agg, _)) => {
            // Re-check original ends with ) and is a single call (no trailing junk beyond ws).
            let trimmed = expr.trim();
            if !trimmed.ends_with(')') {
                return difference("mstr: malformed agg call");
            }
            intersection(agg)
        }
        None => difference("mstr: not a lone Sum|Count|Avg call"),
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn dax_sum_intersection() {
        let c = classify(Dialect::Dax, "SUM(orders[sale_amount])").unwrap();
        assert_eq!(
            c,
            Classification::Intersection {
                aggregation: CommonAgg::Sum
            }
        );
    }

    #[test]
    fn lod_fixed_difference() {
        let c = classify(Dialect::Lod, "{FIXED [region]: SUM([sale_amount])}").unwrap();
        assert!(matches!(c, Classification::Difference { .. }));
    }
}
