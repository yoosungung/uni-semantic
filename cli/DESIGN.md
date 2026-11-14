# cli/ — `slg` (Semantic Layer Generator)

Rust 바이너리 CLI. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md) §5. 스포크 내부는 각 `DESIGN.md`.

## 범위 (이 티켓)

- `slg convert --from pbip --to lookml <input> [-o <out>]`
  - Hub-only: `frontend_powerbi::parse_bim` → IR → `backend_looker::emit_lookml`
  - `--from pbip`: BIM JSON 파일, 또는 디렉터리 내 `model.bim` / `*.bim.json`
  - 풀 PBIP 패키지·TMDL·M partition은 비범위 ([frontends/powerbi/DESIGN.md](../frontends/powerbi/DESIGN.md))
- `slg schema-diff <left> <right>`
  - 양쪽을 IR로 올린 뒤 `fingerprint`(kind+name) 기준 added/removed
  - 포맷: `.lkml` → Looker, 그 외(`.json`/`.bim`/dir) → BIM/pbip
- Non-goals: MCP, GUI Schema Diff, 다른 Phase 4 확장

## Commands

```bash
# 워크스페이스 루트
cargo test -p slg
cargo run -p slg -- convert --from pbip --to lookml frontends/powerbi/fixtures/orders_customer.bim.json
cargo run -p slg -- schema-diff frontends/powerbi/fixtures/orders_customer.bim.json frontends/looker/fixtures/orders_customer.lkml
```
