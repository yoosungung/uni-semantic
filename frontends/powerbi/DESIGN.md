# frontends/powerbi — BIM → IR

Power BI **단독 BIM JSON 서브셋**을 Universal Semantic IR 문서 묶음으로 파싱한다. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## MVP 입력

- 파일: `model.bim` 형태 JSON (`compatibilityLevel` + `model.tables` / `model.relationships`).
- 테이블: `columns`(`name`·`dataType`·`sourceColumn`·`isKey`), 선택 `measures`.
- 물리 위치: 테이블 annotation `uni_semantic.db` / `uni_semantic.schema` / `uni_semantic.table` (없으면 `dw` / `public` / 논리명).
- 관계: `fromTable`/`fromColumn`/`toTable`/`toColumn` (+ 선택 cardinality) → IR `table.fk` + `view` join.
- 메트릭: DAX `SUM|COUNT|AVERAGE(Table[col])` / `AVG(...)`만 → IR `metric`(`sum|count|avg`).
- **비범위:** PBIP 패키지, 전체 TMDL 텍스트, M partition 해석, 고급 DAX.

## 출력

`Vec<Document>` — `table`(+`fk`) · 조인 `view`(있을 때) · `metric`.

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_powerbi
cargo test -p mvp_roundtrip
```
