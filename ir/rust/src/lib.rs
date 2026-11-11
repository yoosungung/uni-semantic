//! Universal Semantic IR document types (MVP subset).
//! Schema SoR: `ir/document.schema.json` — this crate is serde only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Document {
    #[serde(rename = "table")]
    Table(Table),
    #[serde(rename = "view")]
    View(View),
    #[serde(rename = "metric")]
    Metric(Metric),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub target: String,
    pub db: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<TableColumn>,
    pub pk: Vec<String>,
    pub fk: Vec<ForeignKey>,
    pub permission_filters: Vec<PermissionFilter>,
    pub scope_filter: Option<Filter>,
    pub default_filter: Option<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub references: FkRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FkRef {
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionFilter {
    pub role: String,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub target: String,
    pub combine: String,
    pub sources: Vec<ViewSource>,
    pub joins: Vec<ViewJoin>,
    pub columns: Vec<ViewColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewSource {
    pub kind: String,
    pub table: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewJoin {
    pub left: String,
    pub right: String,
    #[serde(rename = "type")]
    pub join_type: String,
    pub on: Vec<JoinOn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinOn {
    pub left_column: String,
    pub right_column: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewColumn {
    pub name: String,
    pub mappings: Vec<ColumnMapping>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnMapping {
    pub source: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub description: String,
    pub cube: String,
    pub expression: String,
    pub aggregation: String,
    pub group_by: Vec<String>,
    pub filter: Option<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

impl Document {
    pub fn name(&self) -> &str {
        match self {
            Document::Table(t) => &t.name,
            Document::View(v) => &v.name,
            Document::Metric(m) => &m.name,
        }
    }
}

/// Stable fingerprint for round-trip compares (order-insensitive by kind+name).
pub fn fingerprint(docs: &[Document]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = docs
        .iter()
        .map(|d| {
            let kind = match d {
                Document::Table(_) => "table",
                Document::View(_) => "view",
                Document::Metric(_) => "metric",
            };
            (kind.to_string(), d.name().to_string())
        })
        .collect();
    out.sort();
    out
}

pub fn tables(docs: &[Document]) -> Vec<&Table> {
    docs.iter()
        .filter_map(|d| match d {
            Document::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

pub fn metrics(docs: &[Document]) -> Vec<&Metric> {
    docs.iter()
        .filter_map(|d| match d {
            Document::Metric(m) => Some(m),
            _ => None,
        })
        .collect()
}

pub fn views(docs: &[Document]) -> Vec<&View> {
    docs.iter()
        .filter_map(|d| match d {
            Document::View(v) => Some(v),
            _ => None,
        })
        .collect()
}

/// Normalize IR identifier: non [A-Za-z0-9_] → `_`, must start with letter/underscore.
pub fn normalize_ident(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if s.is_empty() {
        return "_unnamed".into();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}
