//! Dialect Transpiler — classify, auto-compile common aggs, isolate Unresolved.
//!
//! ARCHITECTURE §1.4–1.5 · §4. IR `$defs/unresolved_expression` shape (no schema expand).

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Dax,
    Lookml,
    Lod,
    Mstr,
}

impl Dialect {
    pub fn as_str(self) -> &'static str {
        match self {
            Dialect::Dax => "dax",
            Dialect::Lookml => "lookml",
            Dialect::Lod => "lod",
            Dialect::Mstr => "mstr",
        }
    }
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
    /// 교집합 — ARCHITECTURE §1.4 공통 표준 집계 (자동 컴파일 대상).
    Intersection { aggregation: CommonAgg },
    /// 차집합 — 방언 고유 → Unresolved 격리.
    Difference { reason: String },
}

/// IR-canonical compiled measure (ARCHITECTURE §1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExpression {
    pub aggregation: CommonAgg,
    /// `sum(col)` / `count(col)` / `avg(col)` when operand extractable; LookML `type:*` → `None`.
    pub ir_expression: Option<String>,
}

/// IR `$defs/unresolved_expression` — dialect + expression + warning (ARCHITECTURE §1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedExpression {
    pub dialect: String,
    pub expression: String,
    pub warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileResult {
    Compiled(CompiledExpression),
    Unresolved(UnresolvedExpression),
}

/// Warning report collecting Unresolved isolations from a batch compile.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WarningReport {
    pub unresolved: Vec<UnresolvedExpression>,
    pub compiled_count: usize,
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

/// Classify then auto-compile intersection, or isolate difference as Unresolved.
pub fn compile(dialect: Dialect, expression: &str) -> Result<CompileResult, Error> {
    let expr = collapse_ws(expression);
    if expr.is_empty() {
        return Err(Error::EmptyExpression);
    }

    match classify(dialect, &expr)? {
        Classification::Intersection { aggregation } => Ok(CompileResult::Compiled(
            CompiledExpression {
                aggregation,
                ir_expression: try_ir_expression(dialect, &expr, aggregation),
            },
        )),
        Classification::Difference { reason } => Ok(CompileResult::Unresolved(
            UnresolvedExpression {
                dialect: dialect.as_str().to_string(),
                expression: expr,
                warning: format!("unresolved: {reason}"),
            },
        )),
    }
}

/// Batch compile; Unresolved items accumulate into `WarningReport`.
pub fn compile_all(
    inputs: &[(Dialect, &str)],
) -> Result<(Vec<CompileResult>, WarningReport), Error> {
    let mut results = Vec::with_capacity(inputs.len());
    let mut report = WarningReport::default();
    for &(dialect, expression) in inputs {
        let outcome = compile(dialect, expression)?;
        match &outcome {
            CompileResult::Compiled(_) => report.compiled_count += 1,
            CompileResult::Unresolved(u) => report.unresolved.push(u.clone()),
        }
        results.push(outcome);
    }
    Ok((results, report))
}

/// Build IR-canonical `agg(operand)` when the dialect expression yields an operand.
fn try_ir_expression(dialect: Dialect, expr: &str, aggregation: CommonAgg) -> Option<String> {
    let operand = extract_operand(dialect, expr)?;
    if operand.is_empty() {
        return None;
    }
    Some(format!("{}({})", aggregation.as_str(), operand))
}

fn extract_operand(dialect: Dialect, expr: &str) -> Option<String> {
    match dialect {
        Dialect::Lookml => {
            let lower = expr.to_ascii_lowercase();
            if lower.starts_with("type:") {
                return None;
            }
            let (_, inner) = split_call(expr)?;
            Some(strip_lookml_table_ref(inner))
        }
        Dialect::Dax => {
            let (_, inner) = split_call(expr)?;
            dax_column_name(inner)
        }
        Dialect::Lod => {
            let (_, inner) = split_call(expr)?;
            tableau_field_name(inner)
        }
        Dialect::Mstr => {
            let (_, inner) = split_call(expr)?;
            let name = inner.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        }
    }
}

fn strip_lookml_table_ref(inner: &str) -> String {
    let s = inner.trim();
    // ${TABLE}.col / ${view_name}.col → col
    if let Some(rest) = s.strip_prefix("${") {
        if let Some(dot) = rest.find("}.") {
            return rest[dot + 2..].trim().to_string();
        }
    }
    s.to_string()
}

fn dax_column_name(inner: &str) -> Option<String> {
    let (_, col_part) = inner.split_once('[')?;
    if !col_part.ends_with(']') {
        return None;
    }
    let col = &col_part[..col_part.len() - 1];
    let col = col.trim();
    if col.is_empty() {
        None
    } else {
        Some(col.to_string())
    }
}

fn tableau_field_name(inner: &str) -> Option<String> {
    let s = inner.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return None;
    }
    let name = s[1..s.len() - 1].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
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
