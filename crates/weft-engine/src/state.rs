use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerId, AnswerSet, Op, Patch, PatchId, StarlarkExpr};

use crate::compose::ComposedPart;

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

/// One include instance's pinned bodies: the child patches it rendered from
/// and, recursively, its own nested instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSnapshot {
    pub include: String,
    pub key: String,
    #[serde(default)]
    pub patches: Vec<StoredPatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<InstanceSnapshot>,
}

impl InstanceSnapshot {
    fn from_part(part: &ComposedPart<'_>) -> Self {
        Self {
            include: part.instance.include.clone(),
            key: part.instance.key.clone(),
            patches: part.patches.iter().map(StoredPatch::from_patch).collect(),
            children: part.children.iter().map(Self::from_part).collect(),
        }
    }

    /// The reconstructed child patches (ids recomputed).
    pub fn patches(&self) -> Vec<Patch> {
        self.patches
            .iter()
            .cloned()
            .map(StoredPatch::into_patch)
            .collect()
    }

    /// The nested instance snapshot for `(include, key)`, if stored.
    pub fn child(&self, include: &str, key: &str) -> Option<&InstanceSnapshot> {
        self.children
            .iter()
            .find(|c| c.include == include && c.key == key)
    }
}

/// The project's pinned base bodies: the root-frame patches and, per
/// include instance, the child bodies it rendered from (recursively). This
/// is the whole composed render's input, so `weft update` reconstructs the
/// merge base without consulting any template's *current* patch ids.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BaseSnapshot {
    #[serde(default)]
    pub parent: Vec<StoredPatch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<InstanceSnapshot>,
}

impl BaseSnapshot {
    /// Snapshot a composed render's inputs: the root-frame patches and every
    /// part's bodies, recursively.
    pub fn from_render(parent_patches: &[Patch], parts: &[ComposedPart<'_>]) -> Self {
        Self {
            parent: parent_patches.iter().map(StoredPatch::from_patch).collect(),
            instances: parts.iter().map(InstanceSnapshot::from_part).collect(),
        }
    }

    /// The reconstructed root-frame patches (ids recomputed).
    pub fn parent_patches(&self) -> Vec<Patch> {
        self.parent
            .iter()
            .cloned()
            .map(StoredPatch::into_patch)
            .collect()
    }

    /// The stored instance snapshot for `(include, key)`, if any (absent for
    /// instances added since the snapshot, or pre-feature projects).
    pub fn instance(&self, include: &str, key: &str) -> Option<&InstanceSnapshot> {
        self.instances
            .iter()
            .find(|i| i.include == include && i.key == key)
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
///
/// Answers carry provenance (see [`crate::answers::provenance`]):
/// `[answers]` holds what you gave — the inputs every re-render starts from —
/// and `[derived]` what the template computed from them at the last render.
/// A project scaffolded before provenance was recorded has no `[derived]`:
/// every stored answer counts as given (it stays put until `weft update
/// --unset ID` hands it back to its default).
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub state: StateMeta,
    #[serde(default)]
    pub answers: AnswerSet,
    #[serde(default, skip_serializing_if = "AnswerSet::is_empty")]
    pub derived: AnswerSet,
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
    /// Child answers you gave (`<include>.<id>` answers; secrets excluded —
    /// see `secrets`).
    #[serde(default)]
    pub answers: AnswerSet,
    /// Child answers derived at the last render: binds, defaults, computed.
    #[serde(default, skip_serializing_if = "AnswerSet::is_empty")]
    pub derived: AnswerSet,
    /// Child secret references (answer id → source spec string).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<AnswerId, String>,
}

impl InstanceState {
    /// The child answers the last render used (secrets excluded).
    pub fn rendered_answers(&self) -> AnswerSet {
        let mut all = self.answers.clone();
        all.overlay(&self.derived);
        all
    }

    /// The flat id of one of this instance's child answers, as `--answer`
    /// takes it: `<include>.<id>`, or `<include>.<key>.<id>` for an instance
    /// of a repeat include.
    pub fn flat_id(&self, repeat: bool, id: &AnswerId) -> String {
        if repeat {
            format!("{}.{}.{id}", self.include, self.key)
        } else {
            format!("{}.{id}", self.include)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateMeta {
    /// Template source as given on the command line: a local path
    /// (absolutized), a `hub:owner/name@version` ref, or a git source
    /// (`gh:owner/repo//dir@rev` — see [`crate::source`]).
    pub template: String,
    /// For git sources: the commit the tree was rendered from. `template`
    /// says what to track (a branch or tag); this is where it resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Pinned base: ids of the patches this tree was rendered from.
    pub base: Vec<PatchId>,
    /// Hash of the rendered tree (pre user edits).
    pub tree_hash: String,
    /// Files the last update left conflict markers in. The next update
    /// refuses to run while any of them still has markers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Utf8PathBuf>,
}

/// What `state.toml` records as the template source: the ref to track plus,
/// for git, the resolved commit. Produced by whoever fetched the template
/// (the CLI); the engine only sees a local directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSource {
    pub template: String,
    pub commit: Option<String>,
}

impl StoredSource {
    /// A local path source: absolutized so `weft update` works from any cwd.
    pub fn path(template: &Utf8Path) -> Self {
        StoredSource {
            template: template
                .canonicalize_utf8()
                .unwrap_or_else(|_| template.to_owned())
                .to_string(),
            commit: None,
        }
    }
}

/// Where a scaffolded project's template is right now: a local directory
/// (the state's own path, or the checkout of a remote source the CLI
/// fetched) plus what to record as the source afterwards (`None` = the
/// directory itself).
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    pub dir: Utf8PathBuf,
    pub stored: Option<StoredSource>,
}

impl State {
    /// Pin a render: its source, base, tree hash, and the root frame's
    /// answers split into given and derived (secrets excluded; see
    /// [`crate::answers::provenance`]).
    pub fn new(
        source: StoredSource,
        base: Vec<PatchId>,
        tree_hash: String,
        (given, derived): (AnswerSet, AnswerSet),
        secret_specs: &BTreeMap<AnswerId, String>,
    ) -> Self {
        State {
            state: StateMeta {
                template: source.template,
                commit: source.commit,
                base,
                tree_hash,
                conflicts: Vec::new(),
            },
            answers: given,
            derived,
            secrets: secret_specs.clone(),
            instances: Vec::new(),
        }
    }

    /// Attach include instances (builder-style).
    pub fn with_instances(mut self, instances: Vec<InstanceState>) -> Self {
        self.instances = instances;
        self
    }

    /// The root answers the last render used (secrets excluded): given
    /// overlaid with derived.
    pub fn rendered_answers(&self) -> AnswerSet {
        let mut all = self.answers.clone();
        all.overlay(&self.derived);
        all
    }

    /// Every stored answer — the root's, then each instance's — with where
    /// it came from (what `weft answers` lists).
    pub fn answer_rows(&self) -> Vec<AnswerRow> {
        let mut rows = frame_rows(&self.answers, &self.derived, &self.secrets, |id| {
            id.to_string()
        });
        for inst in &self.instances {
            // State doesn't record repeat-ness: a key equal to its include's
            // name is a single include's instance (as hook namespaces read it).
            let repeat = inst.key != inst.include;
            rows.extend(frame_rows(
                &inst.answers,
                &inst.derived,
                &inst.secrets,
                |id| inst.flat_id(repeat, id),
            ));
        }
        rows
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

/// One stored answer as `weft answers` lists it.
#[derive(Debug, Serialize)]
pub struct AnswerRow {
    /// Flat id: `id`, `<include>.<id>`, or `<include>.<key>.<id>`.
    pub id: String,
    /// The value (absent for secrets, which are never stored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<weft_core::Value>,
    pub origin: AnswerOrigin,
    /// A secret's source reference (`env:API_TOKEN`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Where a stored answer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerOrigin {
    /// You gave it; every re-render starts from it.
    Given,
    /// You gave it, but its question is off under the other answers; it
    /// applies again when the question does.
    Off,
    /// The template computed it: a default, a computed value, a bind.
    Derived,
    /// Resolved from its source on every render; only the reference is
    /// stored.
    Secret,
}

impl AnswerOrigin {
    pub fn label(self) -> &'static str {
        match self {
            AnswerOrigin::Given => "given",
            AnswerOrigin::Off => "given (question off)",
            AnswerOrigin::Derived => "derived",
            AnswerOrigin::Secret => "secret",
        }
    }
}

fn frame_rows(
    given: &AnswerSet,
    derived: &AnswerSet,
    secrets: &BTreeMap<AnswerId, String>,
    flat_id: impl Fn(&AnswerId) -> String,
) -> Vec<AnswerRow> {
    let ids: std::collections::BTreeSet<&AnswerId> = given
        .iter()
        .chain(derived.iter())
        .map(|(id, _)| id)
        .chain(secrets.keys())
        .collect();
    ids.into_iter()
        .map(|id| {
            let (value, origin, source) = match (secrets.get(id), given.get(id), derived.get(id)) {
                (Some(src), _, _) => (None, AnswerOrigin::Secret, Some(src.clone())),
                (None, Some(g), Some(_)) => (Some(g.clone()), AnswerOrigin::Off, None),
                (None, Some(g), None) => (Some(g.clone()), AnswerOrigin::Given, None),
                (None, None, d) => (d.cloned(), AnswerOrigin::Derived, None),
            };
            AnswerRow {
                id: flat_id(id),
                value,
                origin,
                source,
            }
        })
        .collect()
}

pub fn state_path(dest: &Utf8Path) -> Utf8PathBuf {
    dest.join(STATE_DIR).join(STATE_FILE)
}
