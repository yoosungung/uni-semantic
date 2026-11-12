//! BI Matrix 테이블 정의서 `.xlsx` → Universal Semantic IR.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

use calamine::{Data, Reader, Xlsx};
use thiserror::Error;
use uni_semantic_ir::{normalize_ident, Document, Table, TableColumn};

#[derive(Debug, Error)]
pub enum Error {
    #[error("xlsx: {0}")]
    Xlsx(String),
    #[error("{0}")]
    Msg(String),
}

const SHEET_NAME: &str = "테이블정의서";

const HDR_TABLE: &str = "Table";
const HDR_DESC: &str = "Table 설명";
const HDR_OWNER: &str = "Owner";
const HDR_COL: &str = "항목명";
const HDR_TYPE: &str = "Type";
const HDR_NULL: &str = "Null 허용";
const HDR_KEY: &str = "Key";

fn cell_str(v: &Data) -> String {
    match v {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "Y" } else { "N" }.into(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Error(e) => e.to_string(),
    }
}

fn map_type(raw: &str) -> &'static str {
    match raw.trim() {
        "문자형" => "varchar",
        "숫자형" => "numeric",
        "날짜형" => "date",
        other => {
            let l = other.to_ascii_lowercase();
            match l.as_str() {
                "varchar" | "string" | "text" | "char" => "varchar",
                "numeric" | "number" | "int" | "integer" | "bigint" | "decimal" | "float" => {
                    "numeric"
                }
                "date" | "datetime" | "timestamp" => "date",
                _ => "varchar",
            }
        }
    }
}

fn parse_nullable(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_uppercase().as_str(),
        "Y" | "YES" | "TRUE" | "1"
    )
}

fn parse_pk_rank(raw: &str) -> Option<u32> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("pk") || s.eq_ignore_ascii_case("primary") {
        return Some(1);
    }
    s.parse::<u32>().ok().filter(|n| *n > 0)
}

fn header_index(header: &[String]) -> Result<BTreeMap<&'static str, usize>, Error> {
    let mut map = BTreeMap::new();
    for (i, h) in header.iter().enumerate() {
        let key = match h.as_str() {
            HDR_TABLE => HDR_TABLE,
            HDR_DESC => HDR_DESC,
            HDR_OWNER => HDR_OWNER,
            HDR_COL => HDR_COL,
            HDR_TYPE => HDR_TYPE,
            HDR_NULL => HDR_NULL,
            HDR_KEY => HDR_KEY,
            _ => continue,
        };
        map.insert(key, i);
    }
    for req in [HDR_TABLE, HDR_COL, HDR_TYPE, HDR_NULL, HDR_KEY] {
        if !map.contains_key(req) {
            return Err(Error::Msg(format!("missing header column: {req}")));
        }
    }
    Ok(map)
}

fn get<'a>(row: &'a [Data], idx: &BTreeMap<&str, usize>, key: &str) -> String {
    idx.get(key)
        .and_then(|i| row.get(*i))
        .map(cell_str)
        .unwrap_or_default()
}

struct ColDraft {
    column: TableColumn,
    pk_rank: Option<u32>,
    order: u32,
}

struct TableDraft {
    description: String,
    owner: String,
    cols: Vec<ColDraft>,
}

/// Parse BI Matrix 테이블정의서 workbook bytes → IR `table` documents.
pub fn parse_xlsx_bytes(bytes: &[u8]) -> Result<Vec<Document>, Error> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> =
        Xlsx::new(cursor).map_err(|e| Error::Xlsx(e.to_string()))?;
    let range = workbook
        .worksheet_range(SHEET_NAME)
        .map_err(|e| Error::Xlsx(format!("sheet `{SHEET_NAME}`: {e}")))?;

    let mut rows = range.rows();
    let header_row = rows
        .next()
        .ok_or_else(|| Error::Msg("empty sheet".into()))?;
    let header: Vec<String> = header_row.iter().map(cell_str).collect();
    let idx = header_index(&header)?;

    let mut tables: BTreeMap<String, TableDraft> = BTreeMap::new();
    let mut table_order: Vec<String> = Vec::new();
    let mut row_ord: u32 = 0;

    for row in rows {
        row_ord += 1;
        let table_raw = get(row, &idx, HDR_TABLE);
        let col_raw = get(row, &idx, HDR_COL);
        if table_raw.is_empty() && col_raw.is_empty() {
            continue;
        }
        if table_raw.is_empty() || col_raw.is_empty() {
            return Err(Error::Msg(format!(
                "row {row_ord}: Table and 항목명 are required"
            )));
        }
        let name = normalize_ident(&table_raw);
        let owner = {
            let o = get(row, &idx, HDR_OWNER);
            if o.is_empty() {
                "public".into()
            } else {
                normalize_ident(&o)
            }
        };
        let desc = get(row, &idx, HDR_DESC);
        let col_type = map_type(&get(row, &idx, HDR_TYPE));
        let nullable = parse_nullable(&get(row, &idx, HDR_NULL));
        let pk_rank = parse_pk_rank(&get(row, &idx, HDR_KEY));
        let col_name = normalize_ident(&col_raw);

        let entry = tables.entry(name.clone()).or_insert_with(|| {
            table_order.push(name.clone());
            TableDraft {
                description: desc.clone(),
                owner: owner.clone(),
                cols: Vec::new(),
            }
        });
        if entry.description.is_empty() && !desc.is_empty() {
            entry.description = desc;
        }
        entry.cols.push(ColDraft {
            column: TableColumn {
                name: col_name,
                col_type: col_type.into(),
                nullable,
            },
            pk_rank,
            order: row_ord,
        });
    }

    if tables.is_empty() {
        return Err(Error::Msg("no data rows".into()));
    }

    let mut docs = Vec::new();
    for name in table_order {
        let draft = tables.remove(&name).expect("ordered table");
        let mut pk_pairs: Vec<(u32, String)> = draft
            .cols
            .iter()
            .filter_map(|c| c.pk_rank.map(|r| (r, c.column.name.clone())))
            .collect();
        pk_pairs.sort_by_key(|(r, _)| *r);
        let pk: Vec<String> = pk_pairs.into_iter().map(|(_, n)| n).collect();

        let mut cols = draft.cols;
        cols.sort_by_key(|c| c.order);
        let columns: Vec<TableColumn> = cols.into_iter().map(|c| c.column).collect();

        docs.push(Document::Table(Table {
            name: name.clone(),
            target: "dw".into(),
            db: "dw".into(),
            schema: draft.owner,
            table: name,
            columns,
            pk,
            fk: vec![],
            permission_filters: vec![],
            scope_filter: None,
            default_filter: None,
            role: None,
        }));
    }
    Ok(docs)
}

/// Convenience: read path then parse.
pub fn parse_xlsx_path(path: impl AsRef<Path>) -> Result<Vec<Document>, Error> {
    let bytes = std::fs::read(path.as_ref()).map_err(|e| Error::Xlsx(e.to_string()))?;
    parse_xlsx_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../fixtures/orders_customer.xlsx");

    #[test]
    fn parses_tables_and_pk_from_definition_sheet() {
        let docs = parse_xlsx_bytes(FIXTURE).expect("parse");
        let tables = uni_semantic_ir::tables(&docs);
        assert_eq!(tables.len(), 2);

        let orders = tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.schema, "sales");
        assert_eq!(orders.table, "orders");
        assert_eq!(orders.pk, vec!["order_id".to_string()]);
        assert!(orders.fk.is_empty());
        assert_eq!(
            orders
                .columns
                .iter()
                .map(|c| (c.name.as_str(), c.col_type.as_str(), c.nullable))
                .collect::<Vec<_>>(),
            vec![
                ("order_id", "numeric", false),
                ("customer_id", "numeric", false),
                ("region_code", "varchar", false),
                ("order_date", "date", false),
                ("sale_amount", "numeric", true),
            ]
        );

        let customer = tables.iter().find(|t| t.name == "customer").unwrap();
        assert_eq!(customer.pk, vec!["customer_id".to_string()]);
        assert!(customer.columns.iter().any(|c| c.name == "name" && c.nullable));
    }

    #[test]
    fn rejects_garbage_bytes() {
        let err = parse_xlsx_bytes(b"not-an-xlsx").expect_err("must fail");
        assert!(err.to_string().contains("xlsx"), "{err}");
    }
}
