use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use weft_core::Question;

/// The parsed `weft.toml` at a template root.
///
/// Side-effects (`hooks`) are **not** declared here — they live on the patches
/// that own them (`patches/<name>.json`), collected at render time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub template: TemplateMeta,
    #[serde(default, rename = "question", skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    #[serde(default, rename = "preset", skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<PresetDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateMeta {
    pub name: String,
    #[serde(rename = "weft-version")]
    pub weft_version: String,
    /// What this template scaffolds — the first thing agents read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A named, layered partial answer-set. The file is a plain TOML map of
/// answer id to value, relative to the template root.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetDecl {
    pub name: String,
    pub file: Utf8PathBuf,
}
