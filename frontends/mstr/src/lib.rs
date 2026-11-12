//! MicroStrategy Modeling REST Schema JSON → Universal Semantic IR.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Deserialize;
use thiserror::Error;
use uni_semantic_ir::{
    normalize_ident, ColumnMapping, Document, ForeignKey, FkRef, JoinOn, Table, TableColumn, View,
    ViewColumn, ViewJoin, ViewSource,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Deserialize)]
struct SchemaRoot {
    #[serde(default)]
    db: Option<String>,
    #[serde(default)]
    tables: Vec<LogicalTable>,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct LogicalTable {
    information: ObjectInfo,
    #[serde(rename = "physicalTable")]
    physical_table: PhysicalTable,
    #[serde(default, rename = "tableKey")]
    table_key: Vec<ObjectRef>,
    #[serde(default)]
    attributes: Vec<AttributeDef>,
}

#[derive(Debug, Deserialize)]
struct PhysicalTable {
    #[serde(rename = "tableName")]
    table_name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    columns: Vec<PhysicalColumn>,
}

#[derive(Debug, Deserialize)]
struct PhysicalColumn {
    #[serde(rename = "columnName")]
    column_name: String,
    #[serde(default, rename = "dataType")]
    data_type: Option<DataType>,
}

#[derive(Debug, Deserialize)]
struct DataType {
    #[serde(default, rename = "type")]
    type_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ObjectInfo {
    name: String,
    #[serde(default, rename = "objectId")]
    object_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ObjectRef {
    name: String,
    #[serde(default, rename = "objectId")]
    #[allow(dead_code)]
    object_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AttributeDef {
    information: ObjectInfo,
    #[serde(default)]
    forms: Vec<AttributeForm>,
}

#[derive(Debug, Deserialize)]
struct AttributeForm {
    name: String,
    #[serde(default)]
    expression: Option<FormExpression>,
}

#[derive(Debug, Deserialize)]
struct FormExpression {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    parent: ObjectRef,
    /// Child attribute (Modeling REST); FK column comes from parent form on relationshipTable.
    #[allow(dead_code)]
    child: ObjectRef,
    #[serde(rename = "relationshipTable")]
    relationship_table: ObjectRef,
    #[serde(default, rename = "joinType")]
    join_type: Option<String>,
}

fn map_type(dt: &str) -> &'static str {
    match dt.to_ascii_lowercase().as_str() {
        "integer" | "int" | "bigint" | "smallint" | "unsigned" => "bigint",
        "double" | "float" | "real" | "decimal" | "number" | "numeric" | "bigdecimal" => "numeric",
        "boolean" | "bool" => "boolean",
        "date" => "date",
        "datetime" | "timestamp" | "time" => "timestamp",
        _ => "varchar",
    }
}

fn id_form_column(attr: &AttributeDef) -> Option<String> {
    attr.forms
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("ID"))
        .or_else(|| attr.forms.first())
        .and_then(|f| f.expression.as_ref())
        .and_then(|e| e.text.as_deref())
        .map(|t| normalize_ident(t.trim()))
        .filter(|s| !s.is_empty())
}

fn join_type(raw: Option<&str>) -> String {
    match raw.unwrap_or("left").to_ascii_lowercase().as_str() {
        "inner" => "inner".into(),
        "right" | "rightouter" => "right".into(),
        "full" | "fullouter" => "full".into(),
        _ => "left".into(),
    }
}

/// Parse MicroStrategy Modeling REST Schema JSON into IR documents.
pub fn parse_schema_json(json: &str) -> Result<Vec<Document>, Error> {
    let root: SchemaRoot = serde_json::from_str(json)?;
    if root.tables.is_empty() {
        return Err(Error::Msg("schema has no tables".into()));
    }

    let db = root.db.as_deref().unwrap_or("dw");

    // logical name (information.name) → physical IR table name
    let mut logical_to_ir: HashMap<String, String> = HashMap::new();
    // attribute name → (lookup_ir_table, id_column)
    let mut attr_lookup: HashMap<String, (String, String)> = HashMap::new();
    // attribute name → id column as seen on a given logical table
    let mut attr_on_table: HashMap<(String, String), String> = HashMap::new();

    struct BuiltTable {
        ir_name: String,
        schema: String,
        physical: String,
        columns: Vec<TableColumn>,
        pk: Vec<String>,
    }

    let mut built: Vec<BuiltTable> = Vec::new();

    for t in &root.tables {
        let logical_name = t.information.name.clone();
        let ir_name = normalize_ident(&t.physical_table.table_name);
        let schema = normalize_ident(t.physical_table.namespace.as_deref().unwrap_or("public"));
        let physical = normalize_ident(&t.physical_table.table_name);

        let mut columns = Vec::new();
        for c in &t.physical_table.columns {
            let name = normalize_ident(&c.column_name);
            if columns.iter().any(|x: &TableColumn| x.name == name) {
                continue;
            }
            let col_type = c
                .data_type
                .as_ref()
                .and_then(|d| d.type_name.as_deref())
                .map(map_type)
                .unwrap_or("varchar")
                .to_string();
            columns.push(TableColumn {
                name,
                col_type,
                nullable: true,
            });
        }

        let mut pk = Vec::new();
        for key_ref in &t.table_key {
            if let Some(attr) = t
                .attributes
                .iter()
                .find(|a| a.information.name == key_ref.name)
            {
                if let Some(col) = id_form_column(attr) {
                    if columns.iter().any(|c| c.name == col) && !pk.contains(&col) {
                        pk.push(col);
                    }
                }
            }
        }
        if pk.is_empty() {
            if let Some(c) = columns.first() {
                pk.push(c.name.clone());
            }
        }
        for col in &mut columns {
            if pk.contains(&col.name) {
                col.nullable = false;
            }
        }

        for attr in &t.attributes {
            if let Some(col) = id_form_column(attr) {
                attr_on_table.insert(
                    (logical_name.clone(), attr.information.name.clone()),
                    col.clone(),
                );
                // Prefer lookup table that owns this attribute as tableKey.
                if t.table_key
                    .iter()
                    .any(|k| k.name == attr.information.name)
                {
                    attr_lookup.insert(attr.information.name.clone(), (ir_name.clone(), col));
                } else {
                    attr_lookup
                        .entry(attr.information.name.clone())
                        .or_insert_with(|| (ir_name.clone(), col));
                }
            }
        }

        logical_to_ir.insert(logical_name.clone(), ir_name.clone());
        if let Some(oid) = &t.information.object_id {
            logical_to_ir.insert(oid.clone(), ir_name.clone());
        }

        built.push(BuiltTable {
            ir_name,
            schema,
            physical,
            columns,
            pk,
        });
    }

    let mut fks: BTreeMap<String, Vec<ForeignKey>> = BTreeMap::new();
    let mut joins = Vec::new();

    for rel in &root.relationships {
        let rel_logical = &rel.relationship_table.name;
        let child_ir = logical_to_ir
            .get(rel_logical)
            .cloned()
            .ok_or_else(|| Error::Msg(format!("relationshipTable unknown: {rel_logical}")))?;

        let parent_key = attr_lookup.get(&rel.parent.name).ok_or_else(|| {
            Error::Msg(format!(
                "parent attribute lookup missing: {}",
                rel.parent.name
            ))
        })?;
        // FK column on relationship table = parent attribute ID form mapped there.
        let child_col = attr_on_table
            .get(&(rel_logical.clone(), rel.parent.name.clone()))
            .cloned()
            .ok_or_else(|| {
                Error::Msg(format!(
                    "parent key column missing on relationship table {}: {}",
                    rel_logical, rel.parent.name
                ))
            })?;

        let parent_ir = parent_key.0.clone();
        let parent_col = parent_key.1.clone();

        if child_ir == parent_ir {
            continue;
        }

        fks.entry(child_ir.clone()).or_default().push(ForeignKey {
            columns: vec![child_col.clone()],
            references: FkRef {
                table: parent_ir.clone(),
                columns: vec![parent_col.clone()],
            },
        });

        joins.push((
            child_ir,
            parent_ir,
            child_col,
            parent_col,
            join_type(rel.join_type.as_deref()),
        ));
    }

    let mut docs = Vec::new();
    let table_order: Vec<String> = built.iter().map(|t| t.ir_name.clone()).collect();

    for t in &built {
        docs.push(Document::Table(Table {
            name: t.ir_name.clone(),
            target: "dw".into(),
            db: db.to_string(),
            schema: t.schema.clone(),
            table: t.physical.clone(),
            columns: t.columns.clone(),
            pk: t.pk.clone(),
            fk: fks.get(&t.ir_name).cloned().unwrap_or_default(),
            permission_filters: vec![],
            scope_filter: None,
            default_filter: None,
            role: Some("all".into()),
        }));
    }

    if !joins.is_empty() {
        let mut sources = Vec::new();
        let mut seen = BTreeSet::new();
        let mut view_joins = Vec::new();
        for (left, right, left_col, right_col, jt) in &joins {
            for n in [left, right] {
                if seen.insert(n.clone()) {
                    sources.push(ViewSource {
                        kind: "table".into(),
                        table: n.clone(),
                    });
                }
            }
            view_joins.push(ViewJoin {
                left: left.clone(),
                right: right.clone(),
                join_type: jt.clone(),
                on: vec![JoinOn {
                    left_column: left_col.clone(),
                    right_column: right_col.clone(),
                }],
            });
        }
        let view_name = if view_joins.len() == 1 {
            format!("{}_{}", view_joins[0].left, view_joins[0].right)
        } else {
            "model_joins".into()
        };
        let mut vcols = Vec::new();
        let first = table_order.first().cloned().unwrap_or_default();
        for t in uni_semantic_ir::tables(&docs) {
            for c in &t.columns {
                vcols.push(ViewColumn {
                    name: if t.name == first {
                        c.name.clone()
                    } else {
                        format!("{}_{}", t.name, c.name)
                    },
                    mappings: vec![ColumnMapping {
                        source: t.name.clone(),
                        column: c.name.clone(),
                    }],
                });
            }
        }
        docs.push(Document::View(View {
            name: normalize_ident(&view_name),
            target: "dw".into(),
            combine: "join".into(),
            sources,
            joins: view_joins,
            columns: vcols,
        }));
    }

    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/orders_customer.schema.json");

    #[test]
    fn parses_tables_fk_and_join_view() {
        let docs = parse_schema_json(FIXTURE).expect("parse");
        let tables = uni_semantic_ir::tables(&docs);
        assert_eq!(tables.len(), 2);

        let orders = tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.schema, "sales");
        assert_eq!(orders.table, "orders");
        assert_eq!(orders.db, "dw");
        assert_eq!(orders.pk, vec!["ORDER_ID".to_string()]);
        assert_eq!(orders.fk.len(), 1);
        assert_eq!(orders.fk[0].columns, vec!["CUSTOMER_ID".to_string()]);
        assert_eq!(orders.fk[0].references.table, "customer");
        assert_eq!(
            orders.fk[0].references.columns,
            vec!["CUSTOMER_ID".to_string()]
        );
        assert!(orders
            .columns
            .iter()
            .any(|c| c.name == "SALE_AMOUNT" && c.col_type == "numeric"));

        let customer = tables.iter().find(|t| t.name == "customer").unwrap();
        assert_eq!(customer.pk, vec!["CUSTOMER_ID".to_string()]);
        assert!(customer.columns.iter().any(|c| c.name == "REGION_CODE"));

        let views = uni_semantic_ir::views(&docs);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].joins.len(), 1);
        assert_eq!(views[0].joins[0].join_type, "left");
        assert_eq!(views[0].joins[0].left, "orders");
        assert_eq!(views[0].joins[0].right, "customer");
    }

    #[test]
    fn rejects_empty_tables() {
        let err = parse_schema_json(r#"{"tables":[]}"#).expect_err("must fail");
        assert!(err.to_string().contains("no tables"));
    }
}
