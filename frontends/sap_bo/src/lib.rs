//! SAP BO `.unx` PoC (Data Foundation · Business Layer) → Universal Semantic IR.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use thiserror::Error;
use uni_semantic_ir::{
    normalize_ident, Document, ForeignKey, FkRef, Metric, Table, TableColumn,
};
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum Error {
    #[error("zip: {0}")]
    Zip(String),
    #[error("xml: {0}")]
    Xml(String),
    #[error("{0}")]
    Msg(String),
}

const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

fn map_type(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "integer" | "int" | "bigint" | "smallint" | "numeric" | "number" | "decimal" | "float"
        | "double" => "numeric",
        "boolean" | "bool" => "boolean",
        "date" => "date",
        "datetime" | "timestamp" => "timestamp",
        _ => "varchar",
    }
}

fn map_agg(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "SUM" => Some("sum"),
        "COUNT" => Some("count"),
        "AVG" | "AVERAGE" => Some("avg"),
        _ => None,
    }
}

fn attr<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

fn xsi_type<'a>(node: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    node.attribute((XSI_NS, "type"))
        .or_else(|| node.attribute("type"))
}

fn child_elems<'a, 'b>(
    node: roxmltree::Node<'a, 'b>,
    tag: &str,
) -> Vec<roxmltree::Node<'a, 'b>> {
    node.children()
        .filter(|c| c.is_element() && c.tag_name().name() == tag)
        .collect()
}

fn read_zip_entry(bytes: &[u8], prefer_ext: &str) -> Result<Vec<u8>, Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
    let mut chosen: Option<usize> = None;
    for i in 0..archive.len() {
        let name = archive
            .by_index(i)
            .map_err(|e| Error::Zip(e.to_string()))?
            .name()
            .to_string();
        if name.ends_with('/') {
            continue;
        }
        if name.ends_with(prefer_ext) {
            chosen = Some(i);
            break;
        }
        if chosen.is_none() {
            chosen = Some(i);
        }
    }
    let i = chosen.ok_or_else(|| Error::Zip("empty archive".into()))?;
    let mut file = archive
        .by_index(i)
        .map_err(|e| Error::Zip(e.to_string()))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)
        .map_err(|e| Error::Zip(e.to_string()))?;
    Ok(out)
}

fn unzip_nested_xml(container: &[u8], prefer_ext: &str) -> Result<String, Error> {
    // Outer may already be XML, or a zip of XML.
    let head = container.iter().find(|b| !b.is_ascii_whitespace());
    if head == Some(&b'<') {
        return String::from_utf8(container.to_vec()).map_err(|e| Error::Xml(e.to_string()));
    }
    let inner = read_zip_entry(container, prefer_ext)?;
    let head2 = inner.iter().find(|b| !b.is_ascii_whitespace());
    if head2 == Some(&b'<') {
        return String::from_utf8(inner).map_err(|e| Error::Xml(e.to_string()));
    }
    // Nested zip (dfx/blx as zip-of-xml).
    let xml_bytes = read_zip_entry(&inner, prefer_ext)?;
    String::from_utf8(xml_bytes).map_err(|e| Error::Xml(e.to_string()))
}

fn parse_df(xml: &str) -> Result<Vec<Document>, Error> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "dataFoundation" {
        return Err(Error::Msg(format!(
            "expected root <dataFoundation>, got <{}>",
            root.tag_name().name()
        )));
    }

    let mut tables: BTreeMap<String, Table> = BTreeMap::new();
    for table_node in child_elems(root, "tables")
        .into_iter()
        .flat_map(|t| child_elems(t, "table"))
    {
        let raw_name = attr(table_node, "name")
            .ok_or_else(|| Error::Msg("table missing @name".into()))?;
        let name = normalize_ident(raw_name);
        let schema = attr(table_node, "schema").unwrap_or("public").to_string();
        let db = attr(table_node, "db").unwrap_or("dw").to_string();
        let mut columns = Vec::new();
        let mut pk = Vec::new();
        for col in child_elems(table_node, "column") {
            let cname = normalize_ident(
                attr(col, "name").ok_or_else(|| Error::Msg("column missing @name".into()))?,
            );
            let col_type = map_type(attr(col, "dataType").unwrap_or("varchar"));
            let nullable = !matches!(
                attr(col, "nullable").unwrap_or("true").to_ascii_lowercase().as_str(),
                "false" | "n" | "0"
            );
            if matches!(
                attr(col, "key").unwrap_or("").to_ascii_lowercase().as_str(),
                "true" | "y" | "1" | "pk"
            ) {
                pk.push(cname.clone());
            }
            columns.push(TableColumn {
                name: cname,
                col_type: col_type.into(),
                nullable,
            });
        }
        tables.insert(
            name.clone(),
            Table {
                name: name.clone(),
                target: "dw".into(),
                db,
                schema,
                table: name,
                columns,
                pk,
                fk: vec![],
                permission_filters: vec![],
                scope_filter: None,
                default_filter: None,
                role: Some("all".into()),
            },
        );
    }

    if tables.is_empty() {
        return Err(Error::Msg("dataFoundation has no tables".into()));
    }

    for join in child_elems(root, "joins")
        .into_iter()
        .flat_map(|j| child_elems(j, "join"))
    {
        let join_type = match attr(join, "type").unwrap_or("inner").to_ascii_lowercase().as_str()
        {
            "left" | "leftouter" => "left",
            "right" | "rightouter" => "right",
            "full" | "fullouter" => "full",
            _ => "inner",
        };
        let _ = join_type; // PoC maps equi joins to FK only (additive table.fk).
        let left = child_elems(join, "left")
            .into_iter()
            .next()
            .ok_or_else(|| Error::Msg("join missing <left>".into()))?;
        let right = child_elems(join, "right")
            .into_iter()
            .next()
            .ok_or_else(|| Error::Msg("join missing <right>".into()))?;
        let left_table = normalize_ident(
            attr(left, "table").ok_or_else(|| Error::Msg("join left missing @table".into()))?,
        );
        let left_col = normalize_ident(
            attr(left, "column").ok_or_else(|| Error::Msg("join left missing @column".into()))?,
        );
        let right_table = normalize_ident(
            attr(right, "table").ok_or_else(|| Error::Msg("join right missing @table".into()))?,
        );
        let right_col = normalize_ident(
            attr(right, "column")
                .ok_or_else(|| Error::Msg("join right missing @column".into()))?,
        );
        if !tables.contains_key(&left_table) {
            return Err(Error::Msg(format!(
                "join references unknown left table `{left_table}`"
            )));
        }
        if !tables.contains_key(&right_table) {
            return Err(Error::Msg(format!(
                "join references unknown right table `{right_table}`"
            )));
        }
        tables.get_mut(&left_table).unwrap().fk.push(ForeignKey {
            columns: vec![left_col],
            references: FkRef {
                table: right_table,
                columns: vec![right_col],
            },
        });
    }

    Ok(tables.into_values().map(Document::Table).collect())
}

fn walk_business_items(
    node: roxmltree::Node<'_, '_>,
    tables: &BTreeMap<String, &Table>,
    metrics: &mut Vec<Document>,
) -> Result<(), Error> {
    let ty = xsi_type(node).unwrap_or("");
    if ty.ends_with("Folder") || ty == "business:Folder" {
        for child in child_elems(node, "businessItems") {
            walk_business_items(child, tables, metrics)?;
        }
        return Ok(());
    }
    if ty.ends_with("Dimension") || ty == "business:Dimension" {
        let table = attr(node, "bindingTable").unwrap_or("");
        let col = attr(node, "bindingColumn").unwrap_or("");
        if !table.is_empty() && !col.is_empty() {
            let tname = normalize_ident(table);
            let cname = normalize_ident(col);
            let Some(t) = tables.get(&tname) else {
                return Err(Error::Msg(format!(
                    "dimension binds unknown table `{tname}`"
                )));
            };
            if !t.columns.iter().any(|c| c.name == cname) {
                return Err(Error::Msg(format!(
                    "dimension binds unknown column `{tname}.{cname}`"
                )));
            }
        }
        return Ok(());
    }
    if ty.ends_with("Measure") || ty == "business:Measure" {
        let business_name = attr(node, "businessName")
            .ok_or_else(|| Error::Msg("measure missing @businessName".into()))?;
        let agg_raw = attr(node, "aggregation").unwrap_or("");
        let agg = map_agg(agg_raw).ok_or_else(|| {
            Error::Msg(format!(
                "measure `{business_name}` unsupported aggregation `{agg_raw}` (PoC: SUM|COUNT|AVG)"
            ))
        })?;
        let table = normalize_ident(
            attr(node, "bindingTable")
                .ok_or_else(|| Error::Msg("measure missing @bindingTable".into()))?,
        );
        let column = normalize_ident(
            attr(node, "bindingColumn")
                .ok_or_else(|| Error::Msg("measure missing @bindingColumn".into()))?,
        );
        if !tables.contains_key(&table) {
            return Err(Error::Msg(format!(
                "measure binds unknown table `{table}`"
            )));
        }
        metrics.push(Document::Metric(Metric {
            name: normalize_ident(business_name),
            description: business_name.to_string(),
            cube: table.clone(),
            expression: format!("{table}.{column}"),
            aggregation: agg.into(),
            group_by: vec![],
            filter: None,
            role: Some("all".into()),
        }));
        return Ok(());
    }
    // Unknown businessItems types: ignore for PoC.
    Ok(())
}

fn parse_bl(xml: &str, table_docs: &[Document]) -> Result<Vec<Document>, Error> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "businessLayer" {
        return Err(Error::Msg(format!(
            "expected root <businessLayer>, got <{}>",
            root.tag_name().name()
        )));
    }
    let table_map: BTreeMap<String, &Table> = table_docs
        .iter()
        .filter_map(|d| match d {
            Document::Table(t) => Some((t.name.clone(), t)),
            _ => None,
        })
        .collect();

    let mut metrics = Vec::new();
    for root_folder in child_elems(root, "root") {
        for item in child_elems(root_folder, "businessItems") {
            walk_business_items(item, &table_map, &mut metrics)?;
        }
    }
    // Also allow businessItems directly under businessLayer.
    for item in child_elems(root, "businessItems") {
        walk_business_items(item, &table_map, &mut metrics)?;
    }
    Ok(metrics)
}

/// Parse SAP BO `.unx` (zip of `.dfx`/`.blx` zips) bytes → IR documents.
pub fn parse_unx_bytes(bytes: &[u8]) -> Result<Vec<Document>, Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;

    let mut dfx_bytes: Option<Vec<u8>> = None;
    let mut blx_bytes: Option<Vec<u8>> = None;
    let names: Vec<String> = (0..archive.len())
        .map(|i| {
            archive
                .by_index(i)
                .map(|f| f.name().to_string())
                .map_err(|e| Error::Zip(e.to_string()))
        })
        .collect::<Result<_, _>>()?;

    for (i, name) in names.iter().enumerate() {
        if name.ends_with('/') {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::Zip(e.to_string()))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| Error::Zip(e.to_string()))?;
        if lower.ends_with(".dfx") {
            dfx_bytes = Some(buf);
        } else if lower.ends_with(".blx") {
            blx_bytes = Some(buf);
        }
    }

    let dfx = dfx_bytes.ok_or_else(|| Error::Msg(".unx missing .dfx entry".into()))?;
    let blx = blx_bytes.ok_or_else(|| Error::Msg(".unx missing .blx entry".into()))?;

    let df_xml = unzip_nested_xml(&dfx, ".dfx")?;
    let bl_xml = unzip_nested_xml(&blx, ".blx")?;

    let mut docs = parse_df(&df_xml)?;
    let metrics = parse_bl(&bl_xml, &docs)?;
    docs.extend(metrics);
    Ok(docs)
}

pub fn parse_unx_path(path: impl AsRef<std::path::Path>) -> Result<Vec<Document>, Error> {
    let bytes = std::fs::read(path.as_ref()).map_err(|e| Error::Zip(e.to_string()))?;
    parse_unx_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uni_semantic_ir::{metrics, tables};

    const FIXTURE: &[u8] = include_bytes!("../fixtures/orders_customer.unx");

    #[test]
    fn parses_df_tables_fk_and_bl_measures() {
        let docs = parse_unx_bytes(FIXTURE).expect("parse");
        let ts = tables(&docs);
        assert_eq!(ts.len(), 2);
        let orders = ts.iter().find(|t| t.name == "orders").expect("orders");
        assert_eq!(orders.schema, "sales");
        assert_eq!(orders.pk, vec!["order_id".to_string()]);
        assert_eq!(orders.fk.len(), 1);
        assert_eq!(orders.fk[0].columns, vec!["customer_id".to_string()]);
        assert_eq!(orders.fk[0].references.table, "customer");
        assert_eq!(orders.fk[0].references.columns, vec!["customer_id".to_string()]);

        let ms = metrics(&docs);
        assert_eq!(ms.len(), 3);
        assert!(ms.iter().any(|m| m.name == "Sale_Amount" && m.aggregation == "sum"));
        assert!(ms.iter().any(|m| m.name == "Order_Count" && m.aggregation == "count"));
        assert!(ms.iter().any(|m| m.name == "Avg_Amount" && m.aggregation == "avg"));
        let sale = ms.iter().find(|m| m.name == "Sale_Amount").unwrap();
        assert_eq!(sale.cube, "orders");
        assert_eq!(sale.expression, "orders.amount");
    }

    #[test]
    fn rejects_non_zip() {
        let err = parse_unx_bytes(b"not-a-zip").expect_err("must fail");
        let s = err.to_string();
        assert!(s.contains("zip") || s.contains("Zip") || s.contains("invalid"), "{s}");
    }
}
