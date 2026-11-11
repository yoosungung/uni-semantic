//! Power BI → IR → Looker → IR path: physical SQL before/after hub hop must match.

use sql_parity::{compare_sql, emit_physical_sql, phase1_shape, sql_equivalent, CompareResult};

const BIM: &str = include_str!("../../frontends/powerbi/fixtures/orders_customer.bim.json");

#[test]
fn powerbi_looker_hub_physical_sql_parity_pass() {
    let ir_before = frontend_powerbi::parse_bim(BIM).expect("bim→ir");
    let sql_before = emit_physical_sql(&ir_before).expect("sql before");

    let lookml = backend_looker::emit_lookml(&ir_before).expect("ir→lookml");
    let ir_after = frontend_looker::parse_lookml(&lookml).expect("lookml→ir");
    let sql_after = emit_physical_sql(&ir_after).expect("sql after");

    assert!(
        sql_equivalent(&sql_before, &sql_after),
        "physical SQL diverged across Power BI→Looker hub:\nBEFORE:\n{sql_before}\nAFTER:\n{sql_after}\nCMP={:?}",
        compare_sql(&sql_before, &sql_after)
    );

    let s0 = phase1_shape(&ir_before);
    let s1 = phase1_shape(&ir_after);
    assert_eq!(s0.tables, s1.tables, "Phase1 tables shape");
    assert_eq!(s0.aggs, s1.aggs, "Phase1 aggs shape");
    // join_type may normalize left/left_outer; compare endpoints + keyword family
    assert_eq!(s0.joins.len(), s1.joins.len());
    for (a, b) in s0.joins.iter().zip(s1.joins.iter()) {
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
        let norm = |t: &str| -> String {
            match t {
                "left_outer" => "left".into(),
                other => other.to_ascii_lowercase(),
            }
        };
        assert_eq!(norm(&a.2), norm(&b.2));
    }
}

#[test]
fn intentional_cross_emit_agg_mismatch_detected() {
    let ir = frontend_powerbi::parse_bim(BIM).expect("bim→ir");
    let left = emit_physical_sql(&ir).expect("left");

    let mut mutated = ir.clone();
    for d in &mut mutated {
        if let uni_semantic_ir::Document::Metric(m) = d {
            if m.expression.starts_with("sum(") {
                m.expression = m.expression.replacen("sum(", "count(", 1);
            }
        }
    }
    let right = emit_physical_sql(&mutated).expect("right");
    assert!(matches!(
        compare_sql(&left, &right),
        CompareResult::Differ { .. }
    ));
}
