# transpiler/ — Dialect Transpiler (교·차집합 분류)

방언 수식 텍스트를 **교집합(공통 표준 집계)** 과 **차집합(방언 고유)** 으로 분류한다. Hub-and-Spoke·IR 스키마 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md). `dialect` 태그는 [ir/DESIGN.md](../ir/DESIGN.md) · wiki `Uni-Semantic-IR-Schema-Extensions`.

## PoC 범위

| 포함 | 비포함 |
|------|--------|
| `(dialect, expression)` → `intersection` \| `difference` | 타깃 방언 식 emit |
| 공통 표준 = `SUM` / `COUNT` / `AVG` (ARCHITECTURE §1.4) | Unresolved 격리·경고 리포트 (`385b033c`) |
| DAX · LookML · Tableau LOD · MSTR 최소 fixture | Phase 4 emitter/`slg` · 스포크 재작업 · `$defs` 확장 |

## 입력 SoR

- `dialect` ∈ {`dax`,`lookml`,`lod`,`mstr`} (소문자 태그; IR `$defs/dialect`와 동일 계열).
- `expression`: 방언 원문 한 줄(스포크가 뽑은 식 텍스트). LookML은 `type:<agg>` 또는 `sum|count|average|avg(...)` 한 줄 형태.

## 분류 규칙 (요약)

| 결과 | 조건 |
|------|------|
| `intersection` | 식이 표준 집계(`sum`/`count`/`avg`) **단일 호출**로 해석되고, 방언 고유 컨텍스트(DAX `CALCULATE`/`FILTER`, LOD `{FIXED\|INCLUDE\|EXCLUDE}`, MSTR 레벨 `{…}`, LookML non-sum/count/average type)가 없음 |
| `difference` | 그 외 — 이후 Unresolved 후보 |

성공 시 `aggregation` ∈ {`sum`,`count`,`avg`} 를 함께 반환한다(emit은 후속 티켓).

## 레이아웃

| 경로 | 용도 |
|------|------|
| `src/lib.rs` | `classify` API |
| `fixtures/cases.json` | 방언별 교·차집합 케이스 |
| `tests/classify.rs` | fixture 구동 단위 테스트 |

## Commands

```bash
# 워크스페이스 루트
cargo test -p dialect_transpiler
```
