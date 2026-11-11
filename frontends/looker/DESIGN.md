# frontends/looker — LookML → IR

LookML **최소 서브셋**을 IR 문서 묶음으로 파싱한다. 계약: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## MVP 입력

- `view:` + `sql_table_name:` + `dimension:` / `measure:`
- `explore:` + `join:` (`type` / `relationship` / `sql_on`)
- measure `type:` `sum` | `count` | `average` (+ `sql:`)
- **비범위:** NDT, derived_table, liquid, 복잡한 LookML 식.

## 출력

`Vec<Document>` — view당 `table`, explore join → `view`+`fk`, measure → `metric`.

## Commands

```bash
cargo test -p frontend_looker
cargo test -p mvp_roundtrip
```
