use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use weft_core::{Question, StarlarkExpr};

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
    #[serde(default, rename = "include", skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<IncludeDecl>,
}

/// A child template mounted at a path prefix. The child is an ordinary,
/// self-contained weft template; it knows nothing of the parent.
///
/// Child answers are addressed from the parent as `<name>.<id>` (and
/// `<name>.<key>.<id>` once `repeat` instances land); `bind` seeds child
/// answers from parent answers.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeDecl {
    /// Include slug; unique per template. Namespaces the child's answers and
    /// (for non-repeat includes) doubles as the implicit instance key.
    pub name: String,
    /// The child template. Either a directory path relative to this template
    /// root (`../go-service`) or a registry ref (`hub:owner/name`); the
    /// latter must carry a `version` requirement and resolves through the
    /// lockfile.
    pub template: Utf8PathBuf,
    /// Semver requirement for a `hub:` template (e.g. `^1.0`). Required for
    /// hub refs, forbidden for path refs. Resolved to an exact version in
    /// `weft.lock`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Mount prefix inside the rendered tree. May contain `{key}`, which is
    /// substituted with the instance key (required when `repeat = true`).
    pub path: String,
    /// Whether the project may instantiate this include 0..N times.
    #[serde(default)]
    pub repeat: bool,
    /// Child answer id → Starlark expression over *parent* answers (plus
    /// `key`, the instance key). Explicit per-instance answers override these.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bind: BTreeMap<String, StarlarkExpr>,
}

impl IncludeDecl {
    /// Is this a registry (`hub:`) include rather than a relative path?
    pub fn is_hub(&self) -> bool {
        self.template.as_str().starts_with("hub:")
    }

    /// The `hub:owner/name` ref string, if this is a hub include.
    pub fn hub_ref(&self) -> Option<&str> {
        self.is_hub().then(|| self.template.as_str())
    }
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
