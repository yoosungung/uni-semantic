# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — Phase와 의존성으로만 나눈다. 계약 본문은 [ARCHITECTURE.md](ARCHITECTURE.md).

factory `pm-roadmap-sync`는 `##` 아래 **`- [ ]` / `- [x]`** 만 enqueue한다 (plain bullet·`###` alone → no-op).

## 목표

이종 ROLAP/BI 메타데이터를 **Universal Semantic IR** 허브로 흡수·방출하는 Hub-and-Spoke 컴파일러. CLI는 **`slg`** (Semantic Layer Generator). 후기 Phase에서 dbt / Cube.js / `sql-mcp`(MCP·NL2SQL)까지 emit한다.

## 완료

- [x] 기획 문서: Hub-and-Spoke 전략, IR 출발점(`sql-mcp` `document.schema.json` 확장·정련), Phase 1–4, CLI=`slg` — [ARCHITECTURE.md](ARCHITECTURE.md) · 이 파일 · [README.md](README.md).
- [x] 구현 스택: 코어·`slg` = **Rust**(Cargo); IR = JSON Schema; Excel/레거시 XML 스포크는 필요 시 **Python 어댑터**(IR JSON만 교환).
- [x] IR↔`sql-mcp`: 스키마 **복사** 후 이 저장소에서 확장. 안정화 후 `sql-mcp`를 이 스키마에 맞추는 방향을 검토.
- [x] IR 확장 식별자: `dialect`, `unresolved_expression`, `hierarchy`, `role_playing` — [ARCHITECTURE.md](ARCHITECTURE.md) §3.
- [x] Schema Diff: **CLI만** (GUI 없음).

## Phase 1 — IR 스펙 · Power BI ↔ Looker MVP

의존: 없음(문서 계약 확정 후).

- [x] IR JSON Schema: `sql-mcp` document 스키마를 출발으로 확장·정련 (`ir/`).
- [x] Frontend/Backend: **Power BI (TMDL/BIM)** ↔ **Looker (LookML)**.
- [x] 기본 요소: 테이블, PK/FK 조인(`Inner` / `Left` / cardinality), 단순 집계(`SUM` / `COUNT` / `AVG`).
- [x] 검증: 변환 전후 물리 SQL parity 단위 테스트 하네스.

## Phase 2 — XML·국산 엔터프라이즈 스포크

의존: Phase 1 IR·MVP 파이프라인.

- [x] Tableau `.tds` (XML) 파서 · 조인 트리 추출.
- [x] BI Matrix `.xlsx` 정의서 템플릿 파싱 → IR 매핑.
- [ ] SAP BO `.unx` PoC: Data Foundation · Business Layer 추출.
- [ ] IR 보강: snowflake형 복합 조인 · Hierarchy 표현.

## Phase 3 — 리포지토리 연동 · 수식 방언

의존: Phase 2 스포크·IR 확장.

- [ ] Spotfire Information Link XML 파서.
- [ ] MicroStrategy Metadata REST → Schema JSON 추출.
- [ ] Dialect Transpiler: DAX / LookML expression / Tableau LOD / MSTR 식의 교·차집합 분류.
- [ ] 공통 집계는 자동 컴파일; 불가 식은 Unresolved Expression 격리·경고 리포트.

## Phase 4 — 에코시스템 · MCP · `slg` 배포

의존: Phase 3 수식 레이어·주요 스포크.

- [ ] Emitter: dbt Semantic Layer YAML · Cube.js 데이터 모델.
- [ ] IR 기반 MCP 연동(`sql-mcp`/NL2SQL 에이전트가 단일 스키마로 조회).
- [ ] **`slg`** CLI 패키징 (`slg convert --from pbip --to lookml`) · Schema Diff(`slg` 하위 명령, CLI만).

## 미결정

- (없음 — 기획 단계 미결정은 위 「완료」로 해소. Phase 구현 중 새 이슈만 여기에 추가.)

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · IR → `ir/` · 스포크 내부 → `frontends|backends/<tool>/DESIGN.md` · CLI → `cli/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
