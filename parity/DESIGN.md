# parity/ — 물리 SQL Equivalence 하네스

ARCHITECTURE.md §3 검증 문장(변환 전후 물리 SQL 동등성)용 **단위 테스트 헬퍼**. Hub-and-Spoke·IR 스키마 계약은 변경하지 않는다.

## 동등성 정책 (MVP)

| 층 | 역할 |
|----|------|
| `normalize_sql` / `compare_sql` | 공백·대소문자·백틱 정규화 후 문자열 비교 |
| `phase1_shape` | 테이블 물리명 · 조인(left/right/type) · 집계(sum/count/avg) 보조 assert |
| `emit_physical_sql` | IR → 결정적 Phase-1 SELECT(+JOIN) 유도 (방언 트랜스파일러 아님) |

SQL AST 비교·실창고 live 쿼리는 Non-goals (Phase 3 / 별 게이트).

## 레이아웃

| 경로 | 용도 |
|------|------|
| `src/lib.rs` | 비교 API · IR→물리 SQL 유도 · Phase1 shape |
| `tests/cross_emit.rs` | Power BI BIM → IR → Looker → IR 경로 SQL parity + 의도적 불일치 |

## Commands

```bash
# 워크스페이스 루트
cargo test -p sql_parity
cargo test
```
