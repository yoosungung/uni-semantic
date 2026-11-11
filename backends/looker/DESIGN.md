# backends/looker — IR → LookML

IR 문서 묶음을 LookML **최소 서브셋**으로 emit한다. 파서 대칭은 [frontends/looker/DESIGN.md](../../frontends/looker/DESIGN.md).

## MVP 출력

- `table` → `view:` + dimensions (+ primary_key)
- `view` joins → `explore:` + `join:`
- `metric` → `measure:` (`sum`/`count`/`average`)

Hub-only: Power BI 타입을 직접 참조하지 않는다.

## Commands

```bash
cargo test -p backend_looker
cargo test -p mvp_roundtrip
```
