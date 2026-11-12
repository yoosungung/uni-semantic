# frontends/bi_matrix — `.xlsx` 정의서 → IR

BI Matrix / i-MATRIX Designer **테이블 정의서** `.xlsx` 템플릿을 Universal Semantic IR `table` 문서로 매핑한다. Hub-and-Spoke 계약은 [ARCHITECTURE.md](../../ARCHITECTURE.md). IR 정본은 [ir/DESIGN.md](../../ir/DESIGN.md).

## MVP 입력 (템플릿 SoR)

- 파일: Office Open XML `.xlsx`.
- 시트: 이름 `테이블정의서` (없으면 첫 시트 사용 금지 — 명시 시트만).
- 헤더(1행, 고정 라벨): `Table` · `Table 설명` · `Owner` · `순서` · `항목명` · `Type` · `Null 허용` · `Key` · `비고`.
- 데이터행: 같은 `Table`(+`Owner`) 그룹이 하나의 IR 테이블. `항목명`=컬럼, `Type`∈{문자형,숫자형,날짜형} → `varchar`/`numeric`/`date`, `Null 허용` Y/N, `Key`에 숫자(PK 순서) 또는 `PK`면 PK.
- 물리 위치: `db`=`dw`, `schema`=`Owner`(없으면 `public`), `table`/`name`=`Table`.
- **비범위:** V1/D1/P1 보고서 시트, i-META, FK/조인(정의서에 없음 → `fk=[]`), Dialect Transpiler, Python 사이드카(이 thin 템플릿은 Rust+`calamine`).

## 출력

`Vec<Document>` — `table`만 (`pk` 채움, `fk=[]`, 필터 빈값). 스키마 `$defs` 변경 없음.

## Commands

```bash
# 워크스페이스 루트
cargo test -p frontend_bi_matrix
```
