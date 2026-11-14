# ir/ — Universal Semantic IR

이 저장소의 **IR JSON Schema 정본**. Hub-and-Spoke 허브 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md) §1–§3. 스포크 파서/이미터는 여기 스키마만 읽고 쓴다.

## 레이아웃

| 경로 | 용도 |
|------|------|
| `document.schema.json` | IR 문서(`table`/`view`/`cube`/`metric`/`segment`) JSON Schema 정본 |
| `fixtures/valid/` | 스키마 통과 예시 |
| `fixtures/invalid/` | 스키마 거절 예시 |
| `test/validate.test.js` | Ajv(draft 2020-12) 검증 |
| `package.json` | 검증용 Node 의존성(`ajv`) — 런타임/`sql-mcp` 패키지 참조 없음 |
| `rust/` | 스포크용 serde 타입 헬퍼(`uni_semantic_ir`) — **스키마 정본은 계속 `document.schema.json`** |

## 출처·`$id`

- 출발 복사: `sql-mcp` `semantic/schema/document.schema.json` (`$id` `https://sql-mcp.local/semantic/document.schema.json`).
- 이 저장소 `$id`: `https://uni-semantic.local/ir/document.schema.json` (식별자일 뿐 HTTP 다운로드 주소 아님).
- 빌드·런타임에서 `sql-mcp` 경로/패키지를 참조하지 않는다.

## Phase 1 기본 요소 매핑

스포크·parity가 참조하는 Phase 1 표면(스키마 필드는 기존 `document.schema.json` 표현력만 사용; 필드 추가·required 승격은 `@eric` 승인 전 금지).

| 개념 | IR 위치 | Phase 1 픽스처 기대 |
|------|---------|---------------------|
| 테이블 · PK/FK | `table.pk` / `table.fk` | nonempty `pk`·`fk` ≥1 (`table-orders.json`) |
| 조인 타입 | `viewJoin.type` ∈ {`inner`,`left`} (스키마는 `right`/`full`도 허용) | `inner`·`left` 각 ≥1 |
| 카디널리티 | `viewJoin.relationship` ∈ {`many_to_one`,`one_to_one`,`one_to_many`} | Phase 1 픽스처에서는 **채움 필수**(스키마는 여전히 optional) |
| 단순 집계(measure) | `metric.expression`의 `sum` / `count` / `avg` | 각 함수 ≥1 fixture |
| GROUPING 축 | `metric.aggregation` ∈ {`group`,`rollup`,`cube`,`grouping_sets`} | measure 함수와 **별개** — `sum`/`count`/`avg`로 재해석 금지 |

검증: `test/validate.test.js`의 Phase 1 커버 테스트가 위 표를 고정한다.

## §3 확장 (`$defs`)

| `$defs` | 부착 | 비고 |
|---------|------|------|
| `dialect` | `unresolved_expression.dialect`만 (`$ref`) | 방언 태그 문자열. 멤버 선택 필드 없음 |
| `unresolved_expression` | 위 멤버의 `unresolved` | `dialect`+원본 식+`warning` |
| `hierarchy` | `cube.hierarchies[]` | `name` + `levels`(≥2, unique, 거친→세밀); optional `description` |
| `role_playing` | `cube.role_playing[]` | `base_dimension`+`role`. sql-mcp emit 대상에서는 **strip**(ARCHITECTURE §5.1; 부모 decision reject+strip). |
| `snowflake_path` | `viewJoin.snowflake_path` → `#/$defs/snowflake_path` | 중간 논리 테이블 name[] (`minItems` 1, `uniqueItems`). **속성 생략** = 직접 조인; **빈 배열 invalid** |

Phase 2에서 stub을 본구현으로 승격했다(additive: 기존 직접 조인 fixture는 `snowflake_path` 생략 유지). hierarchy/role_playing shape가 후속 스포크 AC와 어긋나면 티켓에서 옵션을 갱신한다(추정 대규모 rename 금지).

## Commands

```bash
cd ir
npm install
npm test

# 스포크 serde 헬퍼(워크스페이스 루트)
cargo test -p uni_semantic_ir
```
