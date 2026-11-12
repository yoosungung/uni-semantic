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

    match dialect {
        Dialect::Dax => Ok(classify_dax(&expr)),
        Dialect::Lookml => Ok(classify_lookml(&expr)),
        Dialect::Lod => Ok(classify_lod(&expr)),
        Dialect::Mstr => Ok(classify_mstr(&expr)),
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn difference(reason: impl Into<String>) -> Classification {
    Classification::Difference {
        reason: reason.into(),
    }
}

fn intersection(agg: CommonAgg) -> Classification {
    Classification::Intersection { aggregation: agg }
}

/// Strip a single outer `NAME(...)` call; returns `(name_upper, inner)`.
fn split_call(expr: &str) -> Option<(&str, &str)> {
    let open = expr.find('(')?;
    let name = expr[..open].trim();
    if name.is_empty() {
        return None;
    }
    let rest = &expr[open + 1..];
    if !rest.ends_with(')') {
        return None;
    }
    let inner = rest[..rest.len() - 1].trim();
    // Reject nested top-level commas / extra paren pairs beyond a single call.
    if !balanced_parens(inner) {
        return None;
    }
    Some((name, inner))
}

fn balanced_parens(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn parse_common_agg_name(name: &str) -> Option<CommonAgg> {
    match name.to_ascii_uppercase().as_str() {
        "SUM" => Some(CommonAgg::Sum),
        "COUNT" => Some(CommonAgg::Count),
        "AVG" | "AVERAGE" => Some(CommonAgg::Avg),
        _ => None,
    }
}

fn classify_dax(expr: &str) -> Classification {
    let upper = expr.to_ascii_uppercase();
    // Dialect-specific context markers (CALCULATE / FILTER / ALL / …).
    for marker in [
        "CALCULATE(",
        "FILTER(",
        "ALL(",
        "ALLEXCEPT(",
        "RELATED(",
        "RELATEDTABLE(",
        "VALUES(",
        "DISTINCT(",
        "USERELATIONSHIP(",
    ] {
        if upper.contains(marker) {
            return difference(format!("dax context marker `{marker}`"));
        }
    }

    let Some((name, inner)) = split_call(expr) else {
        return difference("dax: not a single function call");
    };
    let Some(agg) = parse_common_agg_name(name) else {
        return difference(format!("dax: unsupported function `{name}`"));
    };
    // Simple column ref: Table[col] (no nested calls).
    if !is_dax_column_ref(inner) {
        return difference("dax: argument is not Table[column]");
    }
    if upper.matches('(').count() != 1 {
        return difference("dax: nested call");
    }
    intersection(agg)
}

fn is_dax_column_ref(inner: &str) -> bool {
    // Table[column] — identifiers without nested '(' / ','.
    if inner.contains('(') || inner.contains(',') {
        return false;
    }
    let Some((table, col_part)) = inner.split_once('[') else {
        return false;
    };
    if table.trim().is_empty() || !col_part.ends_with(']') {
        return false;
    }
    let col = &col_part[..col_part.len() - 1];
    !col.trim().is_empty() && !col.contains('[')
}

fn classify_lookml(expr: &str) -> Classification {
    let lower = expr.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("type:") {
        let t = rest.trim();
        return match t {
            "sum" => intersection(CommonAgg::Sum),
            "count" => intersection(CommonAgg::Count),
            "average" | "avg" => intersection(CommonAgg::Avg),
            other => difference(format!("lookml: non-common measure type `{other}`")),
        };
    }

    let Some((name, inner)) = split_call(expr) else {
        return difference("lookml: not type:agg or single call");
    };
    let Some(agg) = parse_common_agg_name(name) else {
        return difference(format!("lookml: unsupported function `{name}`"));
    };
    if inner.is_empty() || !balanced_parens(inner) {
        return difference("lookml: empty or unbalanced argument");
    }
    // Reject nested top-level function calls beyond the outer agg.
    if expr.matches('(').count() != 1 {
        return difference("lookml: nested call");
    }
    intersection(agg)
}

fn classify_lod(expr: &str) -> Classification {
    // Tableau LOD expressions wrap in {FIXED|INCLUDE|EXCLUDE …}.
    let trimmed = expr.trim();
    if trimmed.starts_with('{') {
        return difference("lod: FIXED/INCLUDE/EXCLUDE expression");
    }

    let Some((name, inner)) = split_call(expr) else {
        return difference("lod: not a single function call");
    };
    let Some(agg) = parse_common_agg_name(name) else {
        return difference(format!("lod: unsupported function `{name}`"));
    };
    if !is_tableau_field_ref(inner) {
        return difference("lod: argument is not [field]");
    }
    if expr.matches('(').count() != 1 {
        return difference("lod: nested call");
    }
    intersection(agg)
}

fn is_tableau_field_ref(inner: &str) -> bool {
    let s = inner.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return false;
    }
    let name = &s[1..s.len() - 1];
    !name.trim().is_empty() && !name.contains('[') && !name.contains('(')
}

fn classify_mstr(expr: &str) -> Classification {
    // Level/dimensionality modifiers: Sum(X){#Region}, Sum(X){~+}, …
    if expr.contains('{') || expr.contains('}') {
        return difference("mstr: level / dimensionality modifier");
    }

    let Some((name, inner)) = split_call(expr) else {
        return difference("mstr: not a single function call");
    };
    let Some(agg) = parse_common_agg_name(name) else {
        return difference(format!("mstr: unsupported function `{name}`"));
    };
    if inner.is_empty() || inner.contains('(') || inner.contains(',') {
        return difference("mstr: argument is not a simple metric/fact name");
    }
    if expr.matches('(').count() != 1 {
        return difference("mstr: nested call");
    }
    intersection(agg)
}
