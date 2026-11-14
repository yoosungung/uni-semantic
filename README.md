# uni-semantic

이종 BI/ROLAP 시맨틱 메타데이터를 **Universal Semantic IR** 허브로 변환하는 Hub-and-Spoke 컴파일러 프로젝트다. CLI는 **`slg`** (Semantic Layer Generator).

계약은 [ARCHITECTURE.md](ARCHITECTURE.md), 일정은 [ROADMAP.md](ROADMAP.md), 에이전트 워크플로는 [AGENTS.md](AGENTS.md). Phase 1 IR·Power BI↔Looker 스포크 MVP·SQL parity 하네스 + Phase 2 Tableau `.tds` · BI Matrix `.xlsx` · SAP BO `.unx` PoC 파서는 Cargo workspace(`frontends|backends/{powerbi,looker}`, `frontends/tableau`, `frontends/bi_matrix`, `frontends/sap_bo`, `parity/`).

```bash
cargo test
cargo test -p slg
cargo run -p slg -- convert --from pbip --to lookml frontends/powerbi/fixtures/orders_customer.bim.json
cargo run -p slg -- schema-diff frontends/powerbi/fixtures/orders_customer.bim.json frontends/looker/fixtures/orders_customer.lkml
cargo test -p sql_parity
cargo test -p frontend_tableau
cargo test -p frontend_bi_matrix
cargo test -p frontend_sap_bo
cd ir && npm install && npm test
```


