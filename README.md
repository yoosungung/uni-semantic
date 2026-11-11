# uni-semantic

이종 BI/ROLAP 시맨틱 메타데이터를 **Universal Semantic IR** 허브로 변환하는 Hub-and-Spoke 컴파일러 프로젝트다. CLI는 **`slg`** (Semantic Layer Generator).

계약은 [ARCHITECTURE.md](ARCHITECTURE.md), 일정은 [ROADMAP.md](ROADMAP.md), 에이전트 워크플로는 [AGENTS.md](AGENTS.md). Phase 1 IR·Power BI↔Looker 스포크 MVP는 Cargo workspace(`frontends|backends/{powerbi,looker}`).

```bash
cargo test
cd ir && npm install && npm test
```

