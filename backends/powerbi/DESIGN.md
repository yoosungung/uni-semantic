# backends/powerbi — IR → BIM

IR 문서 묶음을 Power BI **BIM JSON 서브셋**으로 emit한다. 파서 대칭은 [frontends/powerbi/DESIGN.md](../../frontends/powerbi/DESIGN.md).

## MVP 출력

- `table` → BIM table(+columns, isKey from pk, annotations for db/schema/table)
- `view` joins / `table.fk` → `relationships`
- `metric` `sum|count|avg(col)` → DAX `SUM|COUNT|AVERAGE(Table[col])`

Hub-only: Looker 타입을 직접 참조하지 않는다(IR JSON만 입력).

## Commands

```bash
cargo test -p backend_powerbi
cargo test -p mvp_roundtrip
```
