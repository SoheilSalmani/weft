use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerSet, Hook, Op, Patch, PatchId, StarlarkExpr};

use crate::manifest::Manifest;

pub const MANIFEST_FILE: &str = "weft.toml";
pub const PATCHES_DIR: &str = "patches";

/// On-disk patch file (`patches/<name>.json`). Dependencies are referenced by
/// *name* (another patch's file stem) so fixture patches stay hand-writable;
/// content ids are recomputed on load, which is also what validates them.
#[derive(Debug, Serialize, Deserialize)]
pub struct PatchFile {
    /// Descriptive metadata (never hashed; see `weft_core::PatchMeta`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
    /// Operations. Defaults to empty so **action patches** — patches that
    /// exist only to carry hooks (deploy, provision) — can omit it entirely.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<Op>,
    /// Pre/post-render side-effects owned by this patch (never hashed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
}

/// A loaded template: manifest plus the patch DAG with resolved ids, plus
/// any included child templates (recursively loaded).
#[derive(Debug)]
pub struct Template {
    pub root: Utf8PathBuf,
    pub manifest: Manifest,
    /// In name-resolution (topological) order.
    pub patches: Vec<Patch>,
    pub name_to_id: BTreeMap<String, PatchId>,
    pub id_to_name: BTreeMap<PatchId, String>,
    /// Loaded children, in declaration order (parallel to
    /// `manifest.includes`).
    pub includes: Vec<LoadedInclude>,
}

/// An `[[include]]` declaration together with its loaded child template.
#[derive(Debug)]
pub struct LoadedInclude {
    pub decl: crate::manifest::IncludeDecl,
    pub template: Template,
}

/// Maximum include nesting depth (defensive; cycles are detected separately).
const MAX_INCLUDE_DEPTH: usize = 8;

impl Template {
    pub fn load(root: &Utf8Path) -> Result<Self> {
        let mut seen = Vec::new();
        Self::load_inner(root, &mut seen, 0)
    }

    fn load_inner(root: &Utf8Path, seen: &mut Vec<Utf8PathBuf>, depth: usize) -> Result<Self> {
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest_src = std::fs::read_to_string(&manifest_path).with_context(|| {
            format!("`{manifest_path}` not found: is `{root}` a weft template?")
        })?;
        let manifest: Manifest =
            toml::from_str(&manifest_src).with_context(|| format!("parsing {manifest_path}"))?;

        let mut files: BTreeMap<String, PatchFile> = BTreeMap::new();
        let patches_dir = root.join(PATCHES_DIR);
        if patches_dir.is_dir() {
            for entry in patches_dir
                .read_dir_utf8()
                .with_context(|| format!("reading {patches_dir}"))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension() != Some("json") {
                    continue;
                }
                let stem = path
                    .file_stem()
                    .context("patch file has no name")?
                    .to_owned();
                let src =
                    std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
                let file: PatchFile =
                    serde_json::from_str(&src).with_context(|| format!("parsing patch {path}"))?;
                files.insert(stem, file);
            }
        }

        let (patches, name_to_id) = resolve_patches(files)?;
        let id_to_name = name_to_id.iter().map(|(n, id)| (*id, n.clone())).collect();

        // Recursively load included child templates, guarding against cycles
        // (via canonicalized roots) and runaway nesting.
        let mut includes = Vec::with_capacity(manifest.includes.len());
        if !manifest.includes.is_empty() {
            if depth >= MAX_INCLUDE_DEPTH {
                bail!(
                    "template includes nested deeper than {MAX_INCLUDE_DEPTH} levels at `{root}`"
                );
            }
            let canonical = root.canonicalize_utf8().unwrap_or_else(|_| root.to_owned());
            seen.push(canonical);
            let mut names = std::collections::BTreeSet::new();
            for decl in &manifest.includes {
                if !names.insert(&decl.name) {
                    bail!("duplicate include name `{}` in `{root}`", decl.name);
                }
                let child_root = root.join(&decl.template);
                let child_canonical = child_root.canonicalize_utf8().with_context(|| {
                    format!("include `{}`: template `{child_root}` not found", decl.name)
                })?;
                if seen.contains(&child_canonical) {
                    bail!(
                        "include cycle: `{child_canonical}` is already being loaded (via include `{}`)",
                        decl.name
                    );
                }
                let template =
                    Self::load_inner(&child_root, seen, depth + 1).with_context(|| {
                        format!("loading include `{}` from `{child_root}`", decl.name)
                    })?;
                includes.push(LoadedInclude {
                    decl: decl.clone(),
                    template,
                });
            }
            seen.pop();
        }

        Ok(Template {
            root: root.to_owned(),
            manifest,
            patches,
            name_to_id,
            id_to_name,
            includes,
        })
    }

    /// The loaded include with the given name.
    pub fn include(&self, name: &str) -> Option<&LoadedInclude> {
        self.includes.iter().find(|i| i.decl.name == name)
    }

    pub fn preset_names(&self) -> Vec<&str> {
        self.manifest
            .presets
            .iter()
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Load one preset's partial answer set.
    pub fn preset(&self, name: &str) -> Result<AnswerSet> {
        let decl = self
            .manifest
            .presets
            .iter()
            .find(|p| p.name == name)
            .with_context(|| {
                format!(
                    "template `{}` has no preset `{name}` (available: {})",
                    self.manifest.template.name,
                    self.preset_names().join(", ")
                )
            })?;
        let path = self.root.join(&decl.file);
        let src = std::fs::read_to_string(&path)
            .with_context(|| format!("reading preset file {path}"))?;
        let answers: AnswerSet =
            toml::from_str(&src).with_context(|| format!("parsing preset file {path}"))?;
        Ok(answers)
    }

    /// A patch's ancestor closure (the patch itself plus everything it
    /// transitively depends on). Ids must exist in this template.
    pub fn ancestor_closure(&self, id: PatchId) -> std::collections::BTreeSet<PatchId> {
        let by_id: BTreeMap<PatchId, &Patch> = self.patches.iter().map(|p| (p.id, p)).collect();
        let mut closure = std::collections::BTreeSet::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if closure.insert(cur) {
                if let Some(patch) = by_id.get(&cur) {
                    stack.extend(&patch.depends_on);
                }
            }
        }
        closure
    }

    /// Write a new patch file and return its resolved id. `depends_on` are
    /// patch names that must already exist.
    pub fn write_patch(
        &self,
        name: &str,
        depends_on: Vec<String>,
        when: Option<StarlarkExpr>,
        ops: Vec<Op>,
        meta: weft_core::PatchMeta,
    ) -> Result<PatchId> {
        if self.name_to_id.contains_key(name) {
            bail!("patch `{name}` already exists in this template");
        }
        let dep_ids: Vec<PatchId> = depends_on
            .iter()
            .map(|n| {
                self.name_to_id
                    .get(n)
                    .copied()
                    .with_context(|| format!("unknown patch dependency `{n}`"))
            })
            .collect::<Result<_>>()?;
        let patch = Patch::new(dep_ids, when.clone(), ops.clone());
        let file = PatchFile {
            description: meta.description,
            tags: meta.tags,
            depends_on,
            when,
            ops,
            hooks: meta.hooks,
        };
        let dir = self.root.join(PATCHES_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{name}.json"));
        let mut json = serde_json::to_string_pretty(&file)?;
        json.push('\n');
        std::fs::write(&path, json).with_context(|| format!("writing {path}"))?;
        Ok(patch.id)
    }
}

/// Resolve name-based dependencies into content-addressed patches, in
/// topological order. Deterministic: ready names are processed in sorted
/// order.
fn resolve_patches(
    files: BTreeMap<String, PatchFile>,
) -> Result<(Vec<Patch>, BTreeMap<String, PatchId>)> {
    for (name, file) in &files {
        for dep in &file.depends_on {
            if !files.contains_key(dep) {
                bail!("patch `{name}` depends on unknown patch `{dep}`");
            }
        }
    }
    let mut resolved: BTreeMap<String, PatchId> = BTreeMap::new();
    let mut patches = Vec::with_capacity(files.len());
    let mut remaining = files;
    while !remaining.is_empty() {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|(_, f)| f.depends_on.iter().all(|d| resolved.contains_key(d)))
            .map(|(n, _)| n.clone())
            .collect();
        if ready.is_empty() {
            let names: Vec<_> = remaining.keys().cloned().collect();
            bail!("patch dependency cycle among: {}", names.join(", "));
        }
        for name in ready {
            let file = remaining.remove(&name).expect("ready name present");
            let dep_ids = file
                .depends_on
                .iter()
                .map(|d| resolved[d])
                .collect::<Vec<_>>();
            let patch = Patch::new(dep_ids, file.when, file.ops).with_meta(weft_core::PatchMeta {
                description: file.description,
                tags: file.tags,
                hooks: file.hooks,
            });
            resolved.insert(name, patch.id);
            patches.push(patch);
        }
    }
    Ok((patches, resolved))
}
