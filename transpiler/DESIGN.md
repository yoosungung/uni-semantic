# transpiler/ — Dialect Transpiler (교·차집합 분류 · 컴파일 · Unresolved)

방언 수식 텍스트를 **교집합(공통 표준 집계)** 과 **차집합(방언 고유)** 으로 분류하고, 교집합은 IR 표준 집계로 **자동 컴파일**, 차집합은 **Unresolved Expression 격리·경고 리포트**로 남긴다. Hub-and-Spoke·IR 스키마 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md). `dialect` / `unresolved_expression` shape는 [ir/DESIGN.md](../ir/DESIGN.md) · wiki `Uni-Semantic-IR-Schema-Extensions`.

## PoC 범위

| 포함 | 비포함 |
|------|--------|
| `(dialect, expression)` → `intersection` \| `difference` | 타깃 방언 식 emit (Phase 4) |
| 공통 표준 = `SUM` / `COUNT` / `AVG` (ARCHITECTURE §1.4) | 스포크 재작업 · `slg` · MCP |
| intersection → IR-canonical `sum\|count\|avg(operand)` (operand 추출 가능할 때) | `$defs` 확장(기존 `unresolved_expression` 재사용) |
| difference → `unresolved_expression` 격리 + `WarningReport` | 무단 `$defs` 필드 추가 |

## 입력 SoR

- `dialect` ∈ {`dax`,`lookml`,`lod`,`mstr`} (소문자 태그; IR `$defs/dialect`와 동일 계열).
- `expression`: 방언 원문 한 줄(스포크가 뽑은 식 텍스트). LookML은 `type:<agg>` 또는 `sum|count|average|avg(...)` 한 줄 형태.

## 분류 규칙 (요약)

| 결과 | 조건 |
|------|------|
| `intersection` | 식이 표준 집계(`sum`/`count`/`avg`) **단일 호출**로 해석되고, 방언 고유 컨텍스트(DAX `CALCULATE`/`FILTER`, LOD `{FIXED\|INCLUDE\|EXCLUDE}`, MSTR 레벨 `{…}`, LookML non-sum/count/average type)가 없음 |
| `difference` | 그 외 — Unresolved 후보 |

## 컴파일 · Unresolved (본 티켓)

| 입력 분류 | 출력 |
|-----------|------|
| `intersection` | `CompiledExpression` — `aggregation` + (가능 시) IR `metric.expression` 형태 `sum(col)` / `count(col)` / `avg(col)` |
| `difference` | `UnresolvedExpression` — IR `$defs/unresolved_expression`과 동일 required: `dialect`+`expression`+`warning` |
| 배치 | `WarningReport` — Unresolved 항목만 모은 경고 리포트(+ compiled_count) |

LookML `type:sum` 등 operand 없는 교집합은 `aggregation`만 확정하고 `ir_expression`은 `None`(sql 필드는 스포크 책임).

## 레이아웃

| 경로 | 용도 |
|------|------|
| `src/lib.rs` | `classify` · `compile` · `compile_all` / `WarningReport` |
| `fixtures/cases.json` | 방언별 교·차집합 케이스 |
| `fixtures/compile_cases.json` | 컴파일·Unresolved 기대값 |
| `tests/classify.rs` | 분류 fixture 단위 테스트 |
| `tests/compile_report.rs` | 컴파일·경고 리포트 단위 테스트 |

## Commands

```bash
# 워크스페이스 루트
cargo test -p dialect_transpiler
```
