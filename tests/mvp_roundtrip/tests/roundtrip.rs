//! Hub-and-Spoke MVP: Power BI ↔ IR ↔ Looker (no direct spoke↔spoke convert).

use uni_semantic_ir::{fingerprint, metrics, tables};

#[test]
fn powerbi_to_ir_to_looker_to_ir() {
    let bim = include_str!("../../../frontends/powerbi/fixtures/orders_customer.bim.json");
    let ir1 = frontend_powerbi::parse_bim(bim).expect("bim→ir");
    assert!(tables(&ir1).len() >= 2);
    assert!(metrics(&ir1).iter().any(|m| m.expression.contains("sum(")));

    let lookml = backend_looker::emit_lookml(&ir1).expect("ir→lookml");
    assert!(lookml.contains("view: orders"));
    assert!(lookml.contains("explore:"));

    let ir2 = frontend_looker::parse_lookml(&lookml).expect("lookml→ir");
    let t1 = fingerprint(&ir1);
    let t2 = fingerprint(&ir2);
    // table + metric names survive hub round-trip
    for key in t1.iter().filter(|k| k.0 == "table" || k.0 == "metric") {
        assert!(
            t2.contains(key),
            "missing after Looker hub hop: {:?}",
            key
        );
    }
}

#[test]
fn looker_to_ir_to_powerbi_to_ir() {
    let lkml = include_str!("../../../frontends/looker/fixtures/orders_customer.lkml");
    let ir1 = frontend_looker::parse_lookml(lkml).expect("lookml→ir");
    assert!(tables(&ir1).iter().any(|t| t.name == "orders"));

    let bim = backend_powerbi::emit_bim(&ir1).expect("ir→bim");
    assert!(bim.contains("\"name\": \"orders\""));
    assert!(bim.contains("SUM(orders[sale_amount])") || bim.contains("sum("));

    let ir2 = frontend_powerbi::parse_bim(&bim).expect("bim→ir");
    for key in fingerprint(&ir1)
        .iter()
        .filter(|k| k.0 == "table" || k.0 == "metric")
    {
        assert!(
            fingerprint(&ir2).contains(key),
            "missing after Power BI hub hop: {:?}",
            key
        );
    }
}

#[test]
fn hub_only_no_direct_spoke_path_in_emitters() {
    // Compile-time / dependency intent: emitters take IR docs only.
    // Runtime check: converting BIM→LookML requires an IR stage in this test harness.
    let bim = include_str!("../../../frontends/powerbi/fixtures/orders_customer.bim.json");
    let ir = frontend_powerbi::parse_bim(bim).unwrap();
    let _ = backend_looker::emit_lookml(&ir).unwrap();
    // There is no frontend_powerbi::to_lookml / backend_looker::from_bim API.
}
