# frontends/mstr — Metadata REST Schema JSON → IR

MicroStrategy **Modeling REST** 응답을 묶은 Schema JSON에서 테이블·속성·관계를 추출해 Universal Semantic IR로 매핑한다. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## PoC 입력 SoR

- **실테넌트 REST·시크릿·튜토리얼 덤프 재배포 불가** → 저장소는 **합성 최소 fixture**만 둔다 (`fixtures/orders_customer.schema.json`).
- 루트: `{ db?, tables[], relationships? }` — `tables[]`는 `GET /api/model/tables/{id}` subset.
- 테이블: `information.name` · `physicalTable.{tableName,namespace,columns[]}` · `tableKey[]` · `attributes[].forms[].expression.text`(ID 폼→물리 컬럼).
- 컬럼: `columnName` + `dataType.type` (`integer`/`double`/`string`/`date`/`datetime`/`boolean` …).
- 관계: `relationships[]` — `parent`/`child`(attribute name) + `relationshipTable.name` + 선택 `joinType`(`inner|left`, 기본 `left`).
- **비범위:** 라이브 HTTP 클라이언트, changeset, freeform SQL table, fact/metric 식·Dialect Transpiler, partition/HANA prompt.

## 출력

`Vec<Document>` — `table`(물리 컬럼·`tableKey`→`pk`·relationship→`fk`) · 조인 `view`(있을 때). 스키마 `$defs` 변경 없음(additive).

## Risks

실 Modeling 응답은 form 표현이 tokens/tree일 수 있다. PoC는 `expression.text` 컬럼 식별만. 중단 기준은 **합성 smoke green + DESIGN 계약 문서화**.

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_mstr
```
