# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 일정은 [ROADMAP.md](ROADMAP.md). 컴포넌트 내부 설계는 해당 폴더의 `DESIGN.md`(코드 도입 시).

§1은 규칙(왜). §2 이후는 형태(어떻게). 규칙이 바뀌면 §1을 먼저 고친다.

## 1. 계약사항 (불변 규칙)

1. **Hub-and-Spoke만.** 이종 ROLAP/BI 메타데이터 간 N×M 직접 변환을 두지 않는다. 모든 경로는 Frontend/Parser → **Universal Semantic IR** → Backend/Emitter 다.
2. **IR 정본은 이 저장소.** JSON Schema 기반 Universal Semantic IR을 허브로 둔다. 출발점은 `sql-mcp`의 `semantic/schema/document.schema.json`(`table` / `view` / `cube` / `metric` / `segment` 및 join relationship·grain)이며, BI 상호변환에 맞게 **확장·정련**한다. 런타임은 `sql-mcp`에 종속하지 않는다. OSI(Open Semantic Interchange) 등은 참고만 하고 허브로 채택하지 않는다.
3. **파이프라인.** Source Extractor/Parser → Common Semantic IR → Target Generator/Emitter. 스포크(프론트·백엔드)는 플러그인 모듈이며, 벤더·버전별 스키마 어댑터를 독립 유지한다.
4. **자동 변환 범위.** 물리 스키마, 조인(타입·카디널리티), 단순 집계 메트릭(`SUM` / `COUNT` / `AVG` 등 공통 표준)은 자동 컴파일 대상이다. 도구 고유 고급 수식의 무리한 100% 번역을 약속하지 않는다.
5. **Unresolved Expression.** 1:1 변환이 불가능한 방언 수식은 격리한다. 원본 식을 보존하고 경고 리포트를 발급한다. 우회는 SQL 푸시다운 뷰 또는 수동/리포트 처리로 둔다.
6. **조인·함정 정규화.** M:N, Role-Playing Dimension, Chasm/Fan Trap 해석 차이는 IR에 조인 방향성·역할·관계를 명시한 뒤, 타깃 엔진 권장 모델링 패턴(브릿지 테이블 등)으로 정규화해 emit한다.
7. **문서 우선 · TDD.** 계약·계획 변경은 문서를 먼저(또는 함께) 고친다. 코드 도입 시 스켈레톤 → 테스트 → 구현.

## 2. 토폴로지 (Hub-and-Spoke)

```
[Sources]
Power BI (TMDL/BIM) ──┐
Looker (LookML)     ──┼─▶ [Frontends / Parsers]
Tableau (TDS XML)   ──┤           │
SAP BO (UNX XML)    ──┤           ▼
Spotfire (DXP/XML)  ──┤   ┌───────────────────────────────┐
MSTR (Metadata JSON)──┤   │     Universal Semantic IR     │
BI Matrix (Excel)   ──┘   │ (table, view, cube, metric…)  │
                          └───────────────┬───────────────┘
                                          │
                                  [Backends / Emitters]
                                          ├─▶ Power BI TMDL
                                          ├─▶ Looker LookML
                                          ├─▶ Tableau TDS
                                          ├─▶ Cube.js / dbt YAML
                                          └─▶ sql-mcp / NL2SQL MCP
```

| 경계 (경로 계약) | 책임 |
|------------------|------|
| `ir/` | Universal Semantic IR JSON Schema 정본·예시 (코드 도입 시) |
| `frontends/<tool>/` | 소스 메타 → IR 파서/추출기 |
| `backends/<tool>/` | IR → 타깃 포맷 생성기 |
| `cli/` | **`slg`** (Semantic Layer Generator) 바이너리 |
| `mcp/` | IR 조회·NL2SQL 연동 MCP 표면 (후기 Phase) |

지금은 경로만 계약한다. 빈 stub 폴더는 만들지 않는다.

## 3. IR 형태 (sql-mcp 출발 매핑)

정본 스키마 파일은 Phase 1에서 `ir/`에 둔다. 출발 객체와 BI 변환 관점 대응:

| sql-mcp 출발 | 역할 |
|--------------|------|
| `table` | Physical Schema (물리 테이블·컬럼·PK/FK) |
| `view` + `viewJoin` | Relationships / Joins (타입·relationship·매핑) |
| `cube` + `dimensions` / `facts` | Logical Entities (차원·팩트·grain) |
| `metric` / `segment` | Measures / Aggregations · 필터 조각 |

**확장 예정 (이름·필드는 ROADMAP 미결정으로 둔다):** Hierarchy, Role-Playing Dimension, Unresolved Expression(방언·원본 식·경고), source dialect provenance, snowflake형 복합 조인 표현 보강.

검증: 변환 전후 메타에서 생성되는 물리 SQL의 동등성(Equivalence)을 단위 테스트로 검증한다.

## 4. 수식·리스크 대응

| 난제 | 위험 | 대응 (계약과의 연결) |
|------|------|----------------------|
| 조인 카디널리티·M:N | 엔진마다 Chasm/Fan 처리가 다름 | §1.6 — IR에 방향성·역할 명시 후 타깃 패턴으로 정규화 |
| 복합 컨텍스트 수식 (DAX vs LOD 등) | 1:1 수학 매핑이 까다로움 | §1.4–1.5 — 스키마/조인/기본 메트릭 자동, 고급 식은 Unresolved 또는 푸시다운 |
| 벤더 버전 파편화 (Legacy XML) | 구·신 XML 스키마 차이 | §1.3 — 파서 플러그인·버전 어댑터 분리 |

공통 표준 집계식은 타깃 수식으로 자동 컴파일한다. 교집합 밖 방언은 Dialect Transpiler(Phase 3)가 분류하고, 불가능하면 Unresolved로 남긴다.

## 5. CLI · 외부 연동 (형태만)

- CLI 이름: **`slg`** (Semantic Layer Generator). 예: `slg convert --from pbip --to lookml`.
- Phase 4에서 IR → dbt Semantic Layer YAML · Cube.js · `sql-mcp`/MCP emit을 둔다. MCP는 여러 BI 큐브/조인을 단일 IR 스키마로 조회하게 한다.
