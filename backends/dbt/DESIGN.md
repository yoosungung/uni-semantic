# backends/dbt — IR → dbt Semantic Layer YAML

IR 문서 묶음을 MetricFlow **standalone YAML**(`semantic_models` + `metrics`) 최소 서브셋으로 emit한다. 파서·`slg`·MCP는 이 티켓 범위 밖.

## MVP 출력

- `table` → `semantic_models[]` (`model: ref('<name>')`, entities from pk/fk, dimensions from columns)
- `metric` `sum|count|avg(col)` → `measures` + top-level `metrics` (`type: simple`)
- `view` joins / `table.fk` → foreign entities (join keys)

Hub-only: Cube/Looker 타입을 직접 참조하지 않는다(IR만 입력).

## Commands

```bash
cargo test -p backend_dbt
```
