# backends/cube — IR → Cube.js YAML

IR 문서 묶음을 Cube **YAML data model**(`cubes:`) 최소 서브셋으로 emit한다. 파서·`slg`·MCP는 이 티켓 범위 밖.

## MVP 출력

- `table` → `cubes[]` (`sql_table: db.schema.table`, dimensions from columns, pk → `primary_key`)
- `metric` `sum|count|avg(col)` → `measures`
- `view` joins / `table.fk` → `joins` (`many_to_one`)

Hub-only: dbt/Looker 타입을 직접 참조하지 않는다(IR만 입력).

## Commands

```bash
cargo test -p backend_cube
```
