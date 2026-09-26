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
/// Child answers are addressed from the parent as `<name>.<id>` (or
/// `<name>.<key>.<id>` for `repeat` instances); `bind` seeds child answers
/// from parent answers. A single (non-repeat) include's patches become nodes
/// of the parent's graph, named `<name>/<patch>`, so parent patches can
/// depend on them and edit the child's files.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeDecl {
    /// Include slug; unique per template. Namespaces the child's answers and
    /// (for non-repeat includes) doubles as the implicit instance key.
    pub name: String,
    /// The child template: a directory path relative to this template root
    /// (`../go-service`), a registry ref (`hub:owner/name`, which must carry
    /// a `version` requirement), or a git source (`gh:owner/repo//dir@rev`,
    /// any git URL — see [`crate::source`]). Remote refs resolve through the
    /// lockfile.
    pub template: Utf8PathBuf,
    /// Semver requirement for a `hub:` template (e.g. `^1.0`). Required for
    /// hub refs, forbidden otherwise (git includes pin with `@rev` in the
    /// ref). Resolved to an exact version in `weft.lock`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Mount prefix inside the rendered tree. May contain `{key}`, which is
    /// substituted with the instance key (required when `repeat = true`).
    /// Empty (or `.`) mounts the child at the root — its files merge with
    /// the parent's, and a path both create is a render error.
    #[serde(default)]
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
    pub fn kind(&self) -> crate::source::Kind {
        crate::source::kind(self.template.as_str())
    }

    /// Is this a registry (`hub:`) include?
    pub fn is_hub(&self) -> bool {
        self.kind() == crate::source::Kind::Hub
    }

    /// Is this a git include (`gh:…`, a git URL)?
    pub fn is_git(&self) -> bool {
        self.kind() == crate::source::Kind::Git
    }

    /// The `hub:owner/name` ref string, if this is a hub include.
    pub fn hub_ref(&self) -> Option<&str> {
        self.is_hub().then(|| self.template.as_str())
    }
}

/// What a template extends: a base template whose patches, questions, and
/// includes are imported *as-is* (same names, same ids, no mount) — the
/// extender's own patches build on them like any other ancestor. Either a
/// path relative to this template root or a `hub:` ref with a version.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExtendsDecl {
    Path(Utf8PathBuf),
    Full {
        template: Utf8PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
}

impl ExtendsDecl {
    /// The synthetic include declaration the resolver sees (path/hub
    /// resolution is the same as for an include; nothing else applies).
    pub fn as_include(&self) -> IncludeDecl {
        let (template, version) = match self {
            ExtendsDecl::Path(p) => (p.clone(), None),
            ExtendsDecl::Full { template, version } => (template.clone(), version.clone()),
        };
        IncludeDecl {
            name: "extends".into(),
            template,
            version,
            path: String::new(),
            repeat: false,
            bind: BTreeMap::new(),
        }
    }

    pub fn template(&self) -> &Utf8PathBuf {
        match self {
            ExtendsDecl::Path(p) => p,
            ExtendsDecl::Full { template, .. } => template,
        }
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
    /// The base template this one builds on (see [`ExtendsDecl`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<ExtendsDecl>,
}

/// A named, layered partial answer-set. The file is a plain TOML map of
/// answer id to value, relative to the template root.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetDecl {
    pub name: String,
    pub file: Utf8PathBuf,
}
