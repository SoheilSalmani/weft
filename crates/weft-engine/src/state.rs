use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerId, AnswerSet, Op, Patch, PatchId, StarlarkExpr, Value};

pub const STATE_DIR: &str = ".weft";
pub const STATE_FILE: &str = "state.toml";
/// The project's self-contained base snapshot (`.weft/base.json`): the patch
/// bodies the tree was last rendered from, so `weft update` reconstructs the
/// 3-way-merge base without depending on the template's *current* patch ids.
/// This is what lets update survive template history rewrites (amend, squash,
/// resync). Bodies, not the rendered tree — bodies keep secrets as `{answer}`
/// references, so no resolved secret value is ever written to `.weft/`.
pub const BASE_FILE: &str = "base.json";

/// One patch stored in the base snapshot: the hashed body only (metadata is
/// irrelevant to rendering). Reconstructs the exact same `PatchId`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPatch {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<PatchId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreach: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<Op>,
}

impl StoredPatch {
    pub fn from_patch(p: &Patch) -> Self {
        Self {
            depends_on: p.depends_on.clone(),
            when: p.when.clone(),
            foreach: p.foreach.clone(),
            ops: p.ops.clone(),
        }
    }

    /// Rebuild the `Patch` (recomputes its id from the stored body).
    pub fn into_patch(self) -> Patch {
        Patch::new_foreach(self.depends_on, self.when, self.foreach, self.ops)
    }
}

/// The project's pinned base bodies. Parent patches are the load-bearing
/// part; instance bodies are reserved for composed projects (not yet
/// reconstructed from here — instances still resolve against the template).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BaseSnapshot {
    #[serde(default)]
    pub parent: Vec<StoredPatch>,
}

impl BaseSnapshot {
    pub fn from_patches(patches: &[Patch]) -> Self {
        Self {
            parent: patches.iter().map(StoredPatch::from_patch).collect(),
        }
    }

    /// The reconstructed base patches (ids recomputed).
    pub fn parent_patches(&self) -> Vec<Patch> {
        self.parent
            .iter()
            .cloned()
            .map(StoredPatch::into_patch)
            .collect()
    }

    pub fn save(&self, dest: &Utf8Path) -> Result<()> {
        let dir = dest.join(STATE_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(BASE_FILE);
        let mut json = serde_json::to_string_pretty(self).context("serializing base snapshot")?;
        json.push('\n');
        std::fs::write(&path, json).with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    /// Load the base snapshot if present (absent for pre-feature projects).
    pub fn load(dest: &Utf8Path) -> Result<Option<Self>> {
        let path = dest.join(STATE_DIR).join(BASE_FILE);
        match std::fs::read_to_string(&path) {
            Ok(src) => Ok(Some(
                serde_json::from_str(&src).with_context(|| format!("parsing {path}"))?,
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {path}")),
        }
    }
}

/// `.weft/state.toml` in a scaffolded destination. Answers are stored as
/// concrete values *except* secrets, which are stored as references only
/// (`[secrets]` maps answer id to its source spec string).
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub state: StateMeta,
    #[serde(default)]
    pub answers: AnswerSet,
    #[serde(default)]
    pub secrets: BTreeMap<AnswerId, String>,
    /// Include instances (project-side data): which includes were
    /// instantiated, at which keys, with which answers and pinned child
    /// bases. Empty for templates without includes.
    #[serde(default, rename = "instance", skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<InstanceState>,
}

/// One include instance pinned in project state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceState {
    /// The `[[include]]` name in the parent template.
    pub include: String,
    /// Instance key (== include name for non-repeat includes).
    pub key: String,
    /// Mount prefix this instance was rendered under (stored so updates can
    /// reproduce the old tree even if the template later moves the mount).
    pub mount: String,
    /// Pinned child base: ids of the child patches rendered from.
    #[serde(default)]
    pub base: Vec<PatchId>,
    /// Child answers (secrets excluded — see `secrets`).
    #[serde(default)]
    pub answers: AnswerSet,
    /// Child secret references (answer id → source spec string).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<AnswerId, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateMeta {
    /// Template ref as given on the command line (path for MVP).
    pub template: String,
    /// Pinned base: ids of the patches this tree was rendered from.
    pub base: Vec<PatchId>,
    /// Hash of the rendered tree (pre user edits).
    pub tree_hash: String,
}

impl State {
    /// Split `answers` into persistable values and secret refs.
    pub fn new(
        template: String,
        base: Vec<PatchId>,
        tree_hash: String,
        answers: &AnswerSet,
        secret_specs: &BTreeMap<AnswerId, String>,
    ) -> Self {
        let mut plain = AnswerSet::new();
        for (id, value) in answers.iter() {
            match value {
                Value::Secret(_) => {}
                other => {
                    plain.insert(id.clone(), other.clone());
                }
            }
        }
        State {
            state: StateMeta {
                template,
                base,
                tree_hash,
            },
            answers: plain,
            secrets: secret_specs.clone(),
            instances: Vec::new(),
        }
    }

    /// Attach include instances (builder-style).
    pub fn with_instances(mut self, instances: Vec<InstanceState>) -> Self {
        self.instances = instances;
        self
    }

    /// Split a resolved answer set into (plain answers, nothing) dropping
    /// secrets — the storable projection used for instance states.
    pub fn plain_answers(answers: &AnswerSet) -> AnswerSet {
        answers
            .iter()
            .filter(|(_, v)| !matches!(v, Value::Secret(_)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn save(&self, dest: &Utf8Path) -> Result<()> {
        let dir = dest.join(STATE_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(STATE_FILE);
        let toml = toml::to_string_pretty(self).context("serializing state")?;
        std::fs::write(&path, toml).with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn load(dest: &Utf8Path) -> Result<Self> {
        let path = state_path(dest);
        let src = std::fs::read_to_string(&path).with_context(|| {
            format!("`{path}` not found: is `{dest}` a weft-scaffolded project?")
        })?;
        toml::from_str(&src).with_context(|| format!("parsing {path}"))
    }
}

pub fn state_path(dest: &Utf8Path) -> Utf8PathBuf {
    dest.join(STATE_DIR).join(STATE_FILE)
}
