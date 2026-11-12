# frontends/tableau — `.tds` (XML) → IR

Tableau **단독 `.tds`** classic relation 조인 트리를 Universal Semantic IR 문서 묶음으로 파싱한다. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## MVP 입력

- 파일: standalone `.tds` XML (`datasource` > `connection` > `relation`).
- 테이블: `relation type="table"` (`name`, `table='[schema].[table]'`) + 선택 자식 `columns/column`(`name`·`datatype`).
- 조인: nested `relation type="join"` (`join`=`inner|left|right|full`) + equi `clause`/`expression op='='` with `[alias].[col]`.
- DB: `connection@dbname` (없으면 `dw`).
- **비범위:** `.tdsx`·워크북 embedded datasource, Object Model `relationships`, LOD/계산식, federated multi-connection 심화, Dialect Transpiler.

## 출력

`Vec<Document>` — `table`(+`fk` from join ON) · 조인 `view`(있을 때). 스키마 `$defs` 변경 없음(additive).

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_tableau
```
