# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — Phase와 의존성으로만 나눈다. 계약 본문은 [ARCHITECTURE.md](ARCHITECTURE.md).

## 목표

이종 ROLAP/BI 메타데이터를 **Universal Semantic IR** 허브로 흡수·방출하는 Hub-and-Spoke 컴파일러. CLI는 **`slg`** (Semantic Layer Generator). 후기 Phase에서 dbt / Cube.js / `sql-mcp`(MCP·NL2SQL)까지 emit한다.

## 완료

- 기획 문서: Hub-and-Spoke 전략, IR 출발점(`sql-mcp` `document.schema.json` 확장·정련), Phase 1–4, CLI=`slg` — [ARCHITECTURE.md](ARCHITECTURE.md) · 이 파일 · [README.md](README.md).

## Phase 1 — IR 스펙 · Power BI ↔ Looker MVP

의존: 없음(문서 계약 확정 후).

- IR JSON Schema: `sql-mcp` document 스키마를 출발으로 확장·정련 (`ir/`).
- Frontend/Backend: **Power BI (TMDL/BIM)** ↔ **Looker (LookML)**.
- 기본 요소: 테이블, PK/FK 조인(`Inner` / `Left` / cardinality), 단순 집계(`SUM` / `COUNT` / `AVG`).
- 검증: 변환 전후 물리 SQL equivalence 단위 테스트 하네스.

## Phase 2 — XML·국산 엔터프라이즈 스포크

의존: Phase 1 IR·MVP 파이프라인.

- Tableau `.tds` (XML) 파서 · 조인 트리 추출.
- BI Matrix `.xlsx` 정의서 템플릿 파싱 → IR 매핑.
- SAP BO `.unx` PoC: Data Foundation · Business Layer 추출.
- IR 보강: snowflake형 복합 조인 · Hierarchy 표현.

## Phase 3 — 리포지토리 연동 · 수식 방언

의존: Phase 2 스포크·IR 확장.

- Spotfire Information Link XML 파서.
- MicroStrategy Metadata REST → Schema JSON 추출.
- Dialect Transpiler: DAX / LookML expression / Tableau LOD / MSTR 식의 교·차집합 분류.
- 공통 집계는 자동 컴파일; 불가 식은 Unresolved Expression 격리·경고 리포트.

## Phase 4 — 에코시스템 · MCP · `slg` 배포

의존: Phase 3 수식 레이어·주요 스포크.

- Emitter: dbt Semantic Layer YAML · Cube.js 데이터 모델.
- IR 기반 MCP 연동(`sql-mcp`/NL2SQL 에이전트가 단일 스키마로 조회).
- **`slg`** CLI 패키징 (`slg convert --from pbip --to lookml`) · Schema Diff 도구.

## 미결정

- IR 확장 필드 최종 이름(Hierarchy, Role-Playing, Unresolved Expression, provenance 등).
- `sql-mcp` 스키마와의 동기화 방식(복사 vs 패키지 참조) 및 버전 정책.
- 구현 언어·빌드 툴체인.
- Schema Diff의 UI 형태(CLI 전용 vs GUI).

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · IR → `ir/` · 스포크 내부 → `frontends|backends/<tool>/DESIGN.md` · CLI → `cli/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
