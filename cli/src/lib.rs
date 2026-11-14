//! `slg` library: convert + schema-diff over Hub-and-Spoke IR.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use uni_semantic_ir::{fingerprint, Document};

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
    #[error("powerbi: {0}")]
    PowerBi(#[from] frontend_powerbi::Error),
    #[error("looker parse: {0}")]
    LookerParse(#[from] frontend_looker::Error),
    #[error("looker emit: {0}")]
    LookerEmit(#[from] backend_looker::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Pbip,
    Lookml,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiff {
    pub added: Vec<(String, String)>,
    pub removed: Vec<(String, String)>,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.is_empty() {
            out.push_str("schema-diff: identical (fingerprint)\n");
            return out;
        }
        for (k, n) in &self.removed {
            out.push_str(&format!("- {k}:{n}\n"));
        }
        for (k, n) in &self.added {
            out.push_str(&format!("+ {k}:{n}\n"));
        }
        out
    }
}

pub fn infer_format(path: &Path) -> SourceFormat {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".lkml") || name.ends_with(".lookml") {
        SourceFormat::Lookml
    } else {
        SourceFormat::Pbip
    }
}

/// Resolve BIM JSON path: file as-is, or first `model.bim` / `*.bim.json` under dir.
pub fn resolve_pbip_input(path: &Path) -> Result<PathBuf, Error> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if !path.is_dir() {
        return Err(Error::Msg(format!(
            "pbip input not found: {}",
            path.display()
        )));
    }
    let model = path.join("model.bim");
    if model.is_file() {
        return Ok(model);
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let n = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if n.ends_with(".bim.json") || n.ends_with(".bim") {
            candidates.push(p);
        }
    }
    candidates.sort();
    candidates.into_iter().next().ok_or_else(|| {
        Error::Msg(format!(
            "no model.bim / *.bim.json under {}",
            path.display()
        ))
    })
}

pub fn load_documents(path: &Path, format: SourceFormat) -> Result<Vec<Document>, Error> {
    match format {
        SourceFormat::Pbip => {
            let file = resolve_pbip_input(path)?;
            let text = fs::read_to_string(&file)?;
            Ok(frontend_powerbi::parse_bim(&text)?)
        }
        SourceFormat::Lookml => {
            if !path.is_file() {
                return Err(Error::Msg(format!(
                    "lookml input must be a file: {}",
                    path.display()
                )));
            }
            let text = fs::read_to_string(path)?;
            Ok(frontend_looker::parse_lookml(&text)?)
        }
    }
}

pub fn convert_pbip_to_lookml(input: &Path) -> Result<String, Error> {
    let docs = load_documents(input, SourceFormat::Pbip)?;
    Ok(backend_looker::emit_lookml(&docs)?)
}

pub fn schema_diff_docs(left: &[Document], right: &[Document]) -> SchemaDiff {
    let l = fingerprint(left);
    let r = fingerprint(right);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for key in &l {
        if !r.contains(key) {
            removed.push(key.clone());
        }
    }
    for key in &r {
        if !l.contains(key) {
            added.push(key.clone());
        }
    }
    SchemaDiff { added, removed }
}

pub fn schema_diff_paths(left: &Path, right: &Path) -> Result<SchemaDiff, Error> {
    let left_docs = load_documents(left, infer_format(left))?;
    let right_docs = load_documents(right, infer_format(right))?;
    Ok(schema_diff_docs(&left_docs, &right_docs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_bim() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../frontends/powerbi/fixtures/orders_customer.bim.json")
    }

    fn fixture_lkml() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../frontends/looker/fixtures/orders_customer.lkml")
    }

    #[test]
    fn convert_pbip_fixture_emits_lookml_views() {
        let lookml = convert_pbip_to_lookml(&fixture_bim()).expect("convert");
        assert!(lookml.contains("view: orders"), "{lookml}");
        assert!(lookml.contains("explore:"), "{lookml}");
    }

    #[test]
    fn convert_pbip_from_directory_resolves_bim_json() {
        let dir = fixture_bim().parent().unwrap().to_path_buf();
        let lookml = convert_pbip_to_lookml(&dir).expect("dir convert");
        assert!(lookml.contains("view: orders"));
    }

    #[test]
    fn schema_diff_same_hub_shapes_empty_or_symmetric() {
        let bim = load_documents(&fixture_bim(), SourceFormat::Pbip).unwrap();
        let lkml = load_documents(&fixture_lkml(), SourceFormat::Lookml).unwrap();
        let diff = schema_diff_docs(&bim, &lkml);
        // Round-trip fixtures share table/metric names; views may differ by spoke.
        assert!(
            !diff.removed.iter().any(|(k, n)| k == "table" && n == "orders"),
            "orders table should survive: {diff:?}"
        );
        let _ = diff.render();
    }

    #[test]
    fn schema_diff_detects_removed_table() {
        let left = load_documents(&fixture_bim(), SourceFormat::Pbip).unwrap();
        let right: Vec<Document> = left
            .iter()
            .filter(|d| d.name() != "orders")
            .cloned()
            .collect();
        let diff = schema_diff_docs(&left, &right);
        assert!(
            diff.removed.iter().any(|(k, n)| k == "table" && n == "orders"),
            "{diff:?}"
        );
        assert!(diff.render().contains("- table:orders"));
    }

    #[test]
    fn infer_format_by_extension() {
        assert_eq!(
            infer_format(Path::new("a.lkml")),
            SourceFormat::Lookml
        );
        assert_eq!(
            infer_format(Path::new("model.bim")),
            SourceFormat::Pbip
        );
    }
}
