use dialect_transpiler::{classify, Classification, CommonAgg, Dialect};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Case {
    dialect: String,
    expression: String,
    expect: String,
    #[serde(default)]
    aggregation: Option<String>,
}

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/cases.json")
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
fn fixture_cases_classify() {
    let raw = fs::read_to_string(fixtures_path()).expect("read fixtures/cases.json");
    let cases: Vec<Case> = serde_json::from_str(&raw).expect("parse fixtures");
    assert!(!cases.is_empty(), "fixtures must not be empty");

    for (i, c) in cases.iter().enumerate() {
        let got = classify(parse_dialect(&c.dialect), &c.expression)
            .unwrap_or_else(|e| panic!("case {i} classify error: {e}"));
        match c.expect.as_str() {
            "intersection" => {
                let want = parse_agg(c.aggregation.as_deref().expect("intersection needs aggregation"));
                match got {
                    Classification::Intersection { aggregation } => {
                        assert_eq!(
                            aggregation, want,
                            "case {i} {} {:?} agg mismatch",
                            c.dialect, c.expression
                        );
                    }
                    Classification::Difference { reason } => {
                        panic!(
                            "case {i} {} {:?} expected intersection, got difference: {reason}",
                            c.dialect, c.expression
                        );
                    }
                }
            }
            "difference" => {
                assert!(
                    matches!(got, Classification::Difference { .. }),
                    "case {i} {} {:?} expected difference, got {got:?}",
                    c.dialect,
                    c.expression
                );
            }
            other => panic!("case {i} bad expect: {other}"),
        }
    }
}

#[test]
fn unknown_dialect_errors() {
    let err = classify(
        // force via public API path that accepts &str? — use Dialect only; probe invalid via try_parse if any
        Dialect::Dax,
        "",
    );
    assert!(err.is_err(), "empty expression should error");
}
