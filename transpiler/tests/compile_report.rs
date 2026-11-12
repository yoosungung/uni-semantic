use dialect_transpiler::{
    compile, compile_all, Classification, CommonAgg, CompileResult, Dialect, UnresolvedExpression,
};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Case {
    dialect: String,
    expression: String,
    expect: String,
    #[serde(default)]
    aggregation: Option<String>,
    /// missing = skip check; JSON null = expect `None`; string = expect that IR form.
    #[serde(default)]
    ir_expression: Option<Value>,
    #[serde(default)]
    warning_contains: Option<String>,
}

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/compile_cases.json")
}

fn parse_dialect(s: &str) -> Dialect {
    match s {
        "dax" => Dialect::Dax,
        "lookml" => Dialect::Lookml,
        "lod" => Dialect::Lod,
        "mstr" => Dialect::Mstr,
        other => panic!("unknown dialect in fixture: {other}"),
    }
}

fn parse_agg(s: &str) -> CommonAgg {
    match s {
        "sum" => CommonAgg::Sum,
        "count" => CommonAgg::Count,
        "avg" => CommonAgg::Avg,
        other => panic!("unknown aggregation in fixture: {other}"),
    }
}

#[test]
fn fixture_cases_compile() {
    let raw = fs::read_to_string(fixtures_path()).expect("read fixtures/compile_cases.json");
    let cases: Vec<Case> = serde_json::from_str(&raw).expect("parse fixtures");
    assert!(!cases.is_empty(), "fixtures must not be empty");

    for (i, c) in cases.iter().enumerate() {
        let got = compile(parse_dialect(&c.dialect), &c.expression)
            .unwrap_or_else(|e| panic!("case {i} compile error: {e}"));
        match c.expect.as_str() {
            "compiled" => {
                let want_agg =
                    parse_agg(c.aggregation.as_deref().expect("compiled needs aggregation"));
                match got {
                    CompileResult::Compiled(ce) => {
                        assert_eq!(
                            ce.aggregation, want_agg,
                            "case {i} {} {:?} agg mismatch",
                            c.dialect, c.expression
                        );
                        if let Some(want_ir) = &c.ir_expression {
                            let expected = match want_ir {
                                Value::Null => None,
                                Value::String(s) => Some(s.clone()),
                                other => panic!("case {i} bad ir_expression JSON: {other}"),
                            };
                            assert_eq!(
                                ce.ir_expression, expected,
                                "case {i} {} {:?} ir_expression mismatch",
                                c.dialect, c.expression
                            );
                        }
                    }
                    CompileResult::Unresolved(u) => {
                        panic!(
                            "case {i} {} {:?} expected compiled, got unresolved: {}",
                            c.dialect, c.expression, u.warning
                        );
                    }
                }
            }
            "unresolved" => match got {
                CompileResult::Unresolved(u) => {
                    assert_eq!(u.dialect, c.dialect, "case {i} dialect tag");
                    assert!(
                        !u.expression.trim().is_empty(),
                        "case {i} expression preserved"
                    );
                    assert!(!u.warning.is_empty(), "case {i} warning required");
                    if let Some(needle) = &c.warning_contains {
                        assert!(
                            u.warning.contains(needle),
                            "case {i} warning {:?} missing {:?}",
                            u.warning,
                            needle
                        );
                    }
                }
                CompileResult::Compiled(ce) => {
                    panic!(
                        "case {i} {} {:?} expected unresolved, got compiled {:?}",
                        c.dialect, c.expression, ce
                    );
                }
            },
            other => panic!("case {i} bad expect: {other}"),
        }
    }
}

#[test]
fn compile_all_builds_warning_report() {
    let inputs = [
        (Dialect::Dax, "SUM(orders[sale_amount])"),
        (Dialect::Dax, "CALCULATE(SUM(orders[sale_amount]), ALL(orders))"),
        (Dialect::Lod, "{FIXED [region]: SUM([sale_amount])}"),
        (Dialect::Lookml, "type:sum"),
    ];
    let (results, report) = compile_all(&inputs).expect("compile_all");
    assert_eq!(results.len(), 4);
    assert_eq!(report.compiled_count, 2);
    assert_eq!(report.unresolved.len(), 2);
    for u in &report.unresolved {
        assert!(!u.dialect.is_empty());
        assert!(!u.expression.is_empty());
        assert!(!u.warning.is_empty());
    }
    // IR $defs shape: dialect + expression + warning
    let sample: &UnresolvedExpression = &report.unresolved[0];
    let _ = (sample.dialect.as_str(), sample.expression.as_str(), sample.warning.as_str());
}

#[test]
fn difference_fixture_isolates_unresolved() {
    // Explicit difference path from classifier → Unresolved (ticket AC).
    let r = compile(
        Dialect::Dax,
        "CALCULATE(SUM(orders[sale_amount]), FILTER(orders, orders[x] > 1))",
    )
    .expect("compile");
    match r {
        CompileResult::Unresolved(u) => {
            assert_eq!(u.dialect, "dax");
            assert!(u.expression.contains("CALCULATE"));
            assert!(u.warning.contains("unresolved") || u.warning.contains("context"));
        }
        CompileResult::Compiled(_) => panic!("expected unresolved isolation"),
    }
}

#[test]
fn intersection_compiles_common_agg() {
    let r = compile(Dialect::Lod, "AVG([sale_amount])").expect("compile");
    match r {
        CompileResult::Compiled(ce) => {
            assert_eq!(ce.aggregation, CommonAgg::Avg);
            assert_eq!(ce.ir_expression.as_deref(), Some("avg(sale_amount)"));
        }
        CompileResult::Unresolved(_) => panic!("expected compiled"),
    }
    // classify still SoR for intersection
    let c = dialect_transpiler::classify(Dialect::Lod, "AVG([sale_amount])").unwrap();
    assert!(matches!(
        c,
        Classification::Intersection {
            aggregation: CommonAgg::Avg
        }
    ));
}
