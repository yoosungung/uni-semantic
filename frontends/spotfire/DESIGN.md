# frontends/spotfire — Information Link XML → IR

Spotfire **standalone Information Link XML**에서 테이블·컬럼·equi 조인을 추출해 Universal Semantic IR로 매핑한다. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## PoC 입력 SoR

- **공개 벤더 IL / Library export 재배포 불가** → 저장소는 **합성 최소 fixture**만 둔다 (`fixtures/orders_customer.il.xml`).
- 루트: `informationLink` (`@name`, 선택 `@db` 기본 `dw`)
- 컬럼: `columns/column` — `@schema`(기본 `public`) · `@table` · `@column`(물리, 기본=`@name`) · `@name`(표시/IR 컬럼명) · `@dataType` · 선택 `@key`
- 조인: `joins/join@type`(`inner|left|right|full`, 기본 `inner`) + `left|right@table`/`@column` (equi만)
- **비범위:** `.dxp` 임베드, Library zip(`part0`/`metadata.xml`), Filter/Procedure/Prompt, Freehand SQL IL, Information Services 전량 스키마.

## 출력

`Vec<Document>` — `table`(컬럼을 schema+table로 묶음, join→`fk`) · 조인 `view`(있을 때). 스키마 `$defs` 변경 없음(additive).

## Risks

실 Library export는 네임스페이스·GUID 참조·속성명이 다를 수 있다. PoC 중단 기준은 **합성 smoke green + DESIGN 계약 문서화**.

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_spotfire
```
