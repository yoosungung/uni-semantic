# frontends/sap_bo — `.unx` PoC (Data Foundation · Business Layer) → IR

SAP BO Information Design Tool **`.unx`** 컨테이너에서 Data Foundation(`.dfx`)·Business Layer(`.blx`)를 풀어 Universal Semantic IR로 매핑하는 **PoC**. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## PoC 입력 SoR

- **공개 벤더 `.unx` 재배포 불가** → 저장소는 **합성 최소 fixture**만 둔다 (`fixtures/orders_customer.unx`).
- 패키징(공개 관측과 동일 계층): `.unx`(zip) → `.dfx`/`.blx`(zip) → 내부 XML.
- PoC XML은 벤더 스키마 **전량 호환이 아니라** 아래 최소 태그 계약이다. 실 IDT export는 Risks(포맷·라이선스).

### Data Foundation (`.dfx` 내부 XML)

- 루트: `dataFoundation`
- 테이블: `tables/table@name` (+`schema`/`db`, 기본 `public`/`dw`) · 자식 `column@name`/`dataType`/`nullable`/`key`
- 조인: `joins/join@type`(`inner|left|right|full`, 기본 `inner`) + `left|right@table`/`@column` (equi만)

### Business Layer (`.blx` 내부 XML)

- `businessItems` + `xsi:type` ∈ `business:Folder` | `business:Dimension` | `business:Measure` (공개 blx 파서 관측과 동일 축)
- Measure: `@businessName` · `@aggregation`(`SUM|COUNT|AVG|AVERAGE`) · `@bindingTable` · `@bindingColumn` → IR `metric`
- Dimension: 바인딩만 검증(물리 컬럼은 DF `table`에 이미 있으면 별도 IR 문서 없음)

## 출력

`Vec<Document>` — `table`(+`fk` from join) · `metric`. 스키마 `$defs` 변경 없음.

## Non-goals / Risks

- Non-goals: BO GUI·Contexts·Prompts·LOV·OLAP cube·digest 해시 검증·전량 BL 방언·`.unv`.
- Risks: 실 IDT XML 네임스페이스·태그 변형, SAP 라이선스/툴링 없이는 golden 벤더 fixture 확보 불가 → PoC 중단 기준은 **합성 smoke green + DESIGN 계약 문서화**.

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_sap_bo
```
