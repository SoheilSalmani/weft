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
    /// Display title, e.g. "Add Prisma support" (never hashed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Descriptive metadata (never hashed; see `weft_core::PatchMeta`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<StarlarkExpr>,
    /// Integration patch: render once per instance of the named include
    /// (`key` and `instance.<id>` in scope). Behavioral — part of the hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreach: Option<String>,
    /// Operations. Defaults to empty so **action patches** — patches that
    /// exist only to carry hooks (deploy, provision) — can omit it entirely.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<Op>,
    /// Pre/post-render side-effects owned by this patch (never hashed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
    /// The generator command this patch was recorded from, if any
    /// (`weft record --exec`; never hashed — see `weft_core::Generator`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<weft_core::Generator>,
}

/// A loaded template: manifest plus the patch DAG with resolved ids, plus
/// any included child templates (recursively loaded).
///
/// The template's **composed graph** ([`Self::nodes`]) is every patch that
/// renders when this template does, in the frame it renders in:
///
/// - root-frame patches (`patches`): its own `patches/*.json` plus those
///   inherited through `[template] extends` — same names, same ids;
/// - each single include's nodes, namespaced `<include>/<name>` and keyed
///   by the include ([`PatchId::keyed`]), rendered under its mount with the
///   instance's answers;
/// - one opaque node per `repeat` include, standing for every instance
///   (only `foreach` patches reach it).
///
/// `name_to_id` / `id_to_name` cover the whole graph, so a `depends_on`
/// entry, a `--base`, or a `--depends-on` may name any node.
#[derive(Debug)]
pub struct Template {
    pub root: Utf8PathBuf,
    /// Questions and includes are merged with the extended template's
    /// (base first); presets are this template's own.
    pub manifest: Manifest,
    /// Root-frame patches in name-resolution (topological) order.
    pub patches: Vec<Patch>,
    pub name_to_id: BTreeMap<String, PatchId>,
    pub id_to_name: BTreeMap<PatchId, String>,
    /// Loaded children: the extended template's includes first, then this
    /// template's own, in declaration order.
    pub includes: Vec<LoadedInclude>,
    /// The base template this one extends, if any.
    pub extends: Option<Extended>,
    /// The composed graph (see the type docs). Root nodes come first, in
    /// `patches` order.
    pub nodes: Vec<Node>,
    /// `.weftignore` rules (plus built-in defaults) — applied when reading
    /// a recording worktree back, never to rendered output.
    pub ignore: crate::weftignore::IgnoreRules,
}

/// The template a template extends: where it was loaded from, and which
/// root-frame patch names came from it (they are not files of this
/// template — amend/squash/resync them there).
#[derive(Debug)]
pub struct Extended {
    pub root: Utf8PathBuf,
    pub decl: crate::manifest::ExtendsDecl,
    pub patches: std::collections::BTreeSet<String>,
}

/// One node of a template's composed graph.
#[derive(Debug, Clone)]
pub struct Node {
    /// Identity in this graph: the patch id for root nodes, a keyed id for
    /// include nodes, a structural hash for repeat (opaque) nodes.
    pub id: PatchId,
    /// `next-config`, `web/next-config`, `web/svc/base`, or `connector`.
    pub name: String,
    /// Dependencies as ids in this graph.
    pub depends_on: Vec<PatchId>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    /// A root-frame patch: index into `Template::patches`.
    Root(usize),
    /// A single include's patch, rendered in the include's frame.
    Child {
        /// Include names from this template down to the owning child
        /// (`["web"]`, or `["web", "svc"]` for a nested include).
        path: Vec<String>,
        /// Static mount prefix (nested mounts joined).
        mount: Utf8PathBuf,
        patch: Box<Patch>,
    },
    /// Every instance of a repeat include, opaque: no patch, no frame.
    Instances { include: String },
}

impl Node {
    /// The top-level include this node renders through, if any.
    pub fn include(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Root(_) => None,
            NodeKind::Child { path, .. } => path.first().map(String::as_str),
            NodeKind::Instances { include } => Some(include),
        }
    }
}

/// An `[[include]]` declaration together with its loaded child template.
#[derive(Debug)]
pub struct LoadedInclude {
    pub decl: crate::manifest::IncludeDecl,
    pub template: Template,
}

/// Maximum include nesting depth (defensive; cycles are detected separately).
const MAX_INCLUDE_DEPTH: usize = 8;

/// Resolves an `[[include]]` to a local template directory. Path includes
/// are trivial; `hub:` and git includes need a resolver that consults the
/// lockfile, fetches, and verifies (the CLI provides one) — the engine
/// itself never touches the network.
pub trait IncludeResolver {
    /// The local directory holding the child template for `decl`, declared
    /// in the template rooted at `parent_root`.
    fn resolve(
        &mut self,
        parent_root: &Utf8Path,
        decl: &crate::manifest::IncludeDecl,
    ) -> Result<Utf8PathBuf>;
}

/// The default resolver: relative-path includes only. A remote (`hub:` or
/// git) include is an error — there is nothing to fetch with from the pure
/// engine.
pub struct PathResolver;

impl IncludeResolver for PathResolver {
    fn resolve(
        &mut self,
        parent_root: &Utf8Path,
        decl: &crate::manifest::IncludeDecl,
    ) -> Result<Utf8PathBuf> {
        if decl.kind() != crate::source::Kind::Path {
            bail!(
                "include `{}` references `{}` (a remote template); \
                 resolve it through the CLI (`weft new`/`weft lock`)",
                decl.name,
                decl.template
            );
        }
        Ok(parent_root.join(&decl.template))
    }
}

impl Template {
    /// Load a template resolving path includes only. Errors on remote
    /// includes — use [`Self::load_with`] with the CLI's resolver.
    pub fn load(root: &Utf8Path) -> Result<Self> {
        Self::load_with(root, &mut PathResolver)
    }

    /// Load a template, resolving each include through `resolver`.
    pub fn load_with(root: &Utf8Path, resolver: &mut dyn IncludeResolver) -> Result<Self> {
        let mut seen = Vec::new();
        Self::load_inner(root, &mut seen, 0, resolver)
    }

    fn load_inner(
        root: &Utf8Path,
        seen: &mut Vec<Utf8PathBuf>,
        depth: usize,
        resolver: &mut dyn IncludeResolver,
    ) -> Result<Self> {
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest_src = std::fs::read_to_string(&manifest_path).with_context(|| {
            format!("`{manifest_path}` not found: is `{root}` a weft template?")
        })?;
        let mut manifest: Manifest =
            toml::from_str(&manifest_src).with_context(|| format!("parsing {manifest_path}"))?;

        let composed = manifest.template.extends.is_some() || !manifest.includes.is_empty();
        if composed && depth >= MAX_INCLUDE_DEPTH {
            bail!("template includes nested deeper than {MAX_INCLUDE_DEPTH} levels at `{root}`");
        }
        let canonical = root.canonicalize_utf8().unwrap_or_else(|_| root.to_owned());
        seen.push(canonical);

        // `extends`: import the base template's graph as-is. Its questions
        // come first (the extender's may reference them), its includes keep
        // their names, and its root-frame patches keep their names and ids.
        let mut patches: Vec<Patch> = Vec::new();
        let mut names: BTreeMap<String, PatchId> = BTreeMap::new();
        let mut includes: Vec<LoadedInclude> = Vec::new();
        let mut extends = None;
        if let Some(decl) = manifest.template.extends.clone() {
            let base_root = resolver.resolve(root, &decl.as_include())?;
            let base_canonical = base_root
                .canonicalize_utf8()
                .with_context(|| format!("extends: template `{base_root}` not found"))?;
            if seen.contains(&base_canonical) {
                bail!("extends cycle: `{base_canonical}` is already being loaded");
            }
            let base = Self::load_inner(&base_root, seen, depth + 1, resolver)
                .with_context(|| format!("loading extended template from `{base_root}`"))?;
            let mut questions = base.manifest.questions;
            for q in &manifest.questions {
                if questions.iter().any(|b| b.id == q.id) {
                    bail!(
                        "question `{}` is already declared by the extended template `{base_root}`",
                        q.id
                    );
                }
            }
            questions.append(&mut manifest.questions);
            manifest.questions = questions;
            let inherited: std::collections::BTreeSet<String> = base
                .patches
                .iter()
                .map(|p| base.id_to_name[&p.id].clone())
                .collect();
            names = base
                .name_to_id
                .iter()
                .filter(|(n, _)| inherited.contains(*n))
                .map(|(n, id)| (n.clone(), *id))
                .collect();
            extends = Some(Extended {
                root: base_root,
                decl,
                patches: inherited,
            });
            patches = base.patches;
            includes = base.includes;
        }

        // Own includes, recursively loaded; cycles are detected through the
        // canonicalized roots on the stack.
        let mut include_names: std::collections::BTreeSet<String> =
            includes.iter().map(|i| i.decl.name.clone()).collect();
        for decl in &manifest.includes {
            if !include_names.insert(decl.name.clone()) {
                bail!("duplicate include name `{}` in `{root}`", decl.name);
            }
            let child_root = resolver.resolve(root, decl)?;
            let child_canonical = child_root.canonicalize_utf8().with_context(|| {
                format!("include `{}`: template `{child_root}` not found", decl.name)
            })?;
            if seen.contains(&child_canonical) {
                bail!(
                    "include cycle: `{child_canonical}` is already being loaded (via include `{}`)",
                    decl.name
                );
            }
            let template = Self::load_inner(&child_root, seen, depth + 1, resolver)
                .with_context(|| format!("loading include `{}` from `{child_root}`", decl.name))?;
            includes.push(LoadedInclude {
                decl: decl.clone(),
                template,
            });
        }
        seen.pop();

        // Include nodes: a single include's whole graph, namespaced and keyed;
        // a repeat include as one opaque node.
        let mut nodes: Vec<Node> = Vec::new();
        let mut node_names: BTreeMap<String, PatchId> = BTreeMap::new();
        for inc in &includes {
            // An invalid mount is a `weft check` issue, not a load error.
            let mount =
                crate::compose::mount_path(&inc.decl.path, &inc.decl.name).unwrap_or_default();
            for node in include_nodes(inc, &mount) {
                node_names.insert(node.name.clone(), node.id);
                nodes.push(node);
            }
        }

        // Own patch files, resolved against the inherited names and the
        // include nodes.
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
                if let Some(ext) = &extends {
                    if names.contains_key(&stem) {
                        bail!(
                            "patch `{stem}` is already defined by the extended template `{}`",
                            ext.root
                        );
                    }
                }
                let src =
                    std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
                let file: PatchFile =
                    serde_json::from_str(&src).with_context(|| format!("parsing patch {path}"))?;
                files.insert(stem, file);
            }
        }
        let repeat_names: std::collections::BTreeSet<&str> = includes
            .iter()
            .filter(|i| i.decl.repeat)
            .map(|i| i.decl.name.as_str())
            .collect();
        let mut external = names.clone();
        external.extend(node_names.iter().map(|(n, id)| (n.clone(), *id)));
        let (own, own_names) = resolve_patches(files, &external, &repeat_names)?;
        patches.extend(own);
        names.extend(own_names);

        let mut all_nodes: Vec<Node> = patches
            .iter()
            .enumerate()
            .map(|(i, p)| Node {
                id: p.id,
                name: names
                    .iter()
                    .find(|(_, id)| **id == p.id)
                    .map(|(n, _)| n.clone())
                    .expect("every root patch is named"),
                depends_on: p.depends_on.clone(),
                kind: NodeKind::Root(i),
            })
            .collect();
        all_nodes.append(&mut nodes);
        let name_to_id: BTreeMap<String, PatchId> =
            all_nodes.iter().map(|n| (n.name.clone(), n.id)).collect();
        let id_to_name = name_to_id.iter().map(|(n, id)| (*id, n.clone())).collect();

        Ok(Template {
            root: root.to_owned(),
            manifest,
            patches,
            name_to_id,
            id_to_name,
            includes,
            extends,
            nodes: all_nodes,
            ignore: crate::weftignore::IgnoreRules::load(root)?,
        })
    }

    /// The node with this graph id.
    pub fn node(&self, id: PatchId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The patch a node renders (none for a repeat include's opaque node).
    pub fn node_patch<'a>(&'a self, node: &'a Node) -> Option<&'a Patch> {
        match &node.kind {
            NodeKind::Root(i) => Some(&self.patches[*i]),
            NodeKind::Child { patch, .. } => Some(patch),
            NodeKind::Instances { .. } => None,
        }
    }

    /// Is this root-frame patch name inherited through `extends`?
    pub fn is_inherited(&self, name: &str) -> bool {
        self.extends
            .as_ref()
            .is_some_and(|e| e.patches.contains(name))
    }

    /// Nodes that directly depend on `id` (by name), anywhere in the graph.
    pub fn direct_dependents(&self, id: PatchId) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|n| n.depends_on.contains(&id))
            .map(|n| n.name.clone())
            .collect()
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

    /// Load one preset's full spec (locks + multichoice constraints).
    pub fn preset_spec(&self, name: &str) -> Result<crate::preset::PresetSpec> {
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
        crate::preset::PresetSpec::parse(name, &src)
    }

    /// Load one preset's *locked* answers — the locks-only projection.
    /// Use [`Self::preset_spec`] where constraints matter.
    pub fn preset(&self, name: &str) -> Result<AnswerSet> {
        let answers = self.preset_spec(name)?.lock_answers();
        Ok(answers)
    }

    /// A node's ancestor closure (the node itself plus everything it
    /// transitively depends on), over the composed graph.
    pub fn ancestor_closure(&self, id: PatchId) -> std::collections::BTreeSet<PatchId> {
        let by_id: BTreeMap<PatchId, &Node> = self.nodes.iter().map(|n| (n.id, n)).collect();
        let mut closure = std::collections::BTreeSet::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if closure.insert(cur) {
                if let Some(node) = by_id.get(&cur) {
                    stack.extend(&node.depends_on);
                }
            }
        }
        closure
    }

    /// Read one patch's on-disk file by name (for read-modify-write edits).
    pub fn patch_file(&self, name: &str) -> Result<PatchFile> {
        if let Some(ext) = self.extends.as_ref().filter(|e| e.patches.contains(name)) {
            bail!(
                "patch `{name}` is inherited from `{}` — edit it there",
                ext.root
            );
        }
        let path = self.patch_path(name);
        let src = std::fs::read_to_string(&path)
            .with_context(|| format!("no patch `{name}` in this template ({path})"))?;
        serde_json::from_str(&src).with_context(|| format!("parsing patch {path}"))
    }

    /// Write a patch's on-disk file back (must already exist). Callers are
    /// responsible for keeping edits metadata-only unless they intend to
    /// change the patch's content id.
    pub fn save_patch_file(&self, name: &str, file: &PatchFile) -> Result<()> {
        let path = self.patch_path(name);
        if !path.is_file() {
            bail!("no patch `{name}` in this template");
        }
        let json = serde_json::to_string_pretty(file)?;
        std::fs::write(&path, json + "\n").with_context(|| format!("writing {path}"))
    }

    /// The on-disk path of a patch by name.
    pub fn patch_path(&self, name: &str) -> Utf8PathBuf {
        self.root.join(PATCHES_DIR).join(format!("{name}.json"))
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
        self.write_patch_full(name, depends_on, when, None, ops, meta)
    }

    /// [`Self::write_patch`] with a `foreach` include (integration patches).
    pub fn write_patch_full(
        &self,
        name: &str,
        depends_on: Vec<String>,
        when: Option<StarlarkExpr>,
        foreach: Option<String>,
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
        let patch = Patch::new_foreach(dep_ids, when.clone(), foreach.clone(), ops.clone());
        let file = PatchFile {
            title: meta.title,
            description: meta.description,
            tags: meta.tags,
            depends_on,
            when,
            foreach,
            ops,
            hooks: meta.hooks,
            generator: meta.generator,
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

/// The nodes one include contributes to its parent's graph.
fn include_nodes(inc: &LoadedInclude, mount: &Utf8Path) -> Vec<Node> {
    let name = &inc.decl.name;
    if inc.decl.repeat {
        // Opaque: identified by what an instance renders from.
        let ids: Vec<String> = inc.template.nodes.iter().map(|n| n.id.to_hex()).collect();
        let preimage = serde_json::json!({
            "include": name,
            "mount": inc.decl.path,
            "bind": inc.decl.bind.iter().map(|(k, e)| (k.clone(), e.as_str().to_owned())).collect::<BTreeMap<_, _>>(),
            "nodes": ids,
        });
        return vec![Node {
            id: PatchId::from_canonical_bytes(preimage.to_string().as_bytes()),
            name: name.clone(),
            depends_on: Vec::new(),
            kind: NodeKind::Instances {
                include: name.clone(),
            },
        }];
    }
    inc.template
        .nodes
        .iter()
        .map(|child| Node {
            id: PatchId::keyed(name, child.id),
            name: format!("{name}/{}", child.name),
            depends_on: child
                .depends_on
                .iter()
                .map(|d| PatchId::keyed(name, *d))
                .collect(),
            kind: match &child.kind {
                NodeKind::Root(i) => NodeKind::Child {
                    path: vec![name.clone()],
                    mount: mount.to_owned(),
                    patch: Box::new(inc.template.patches[*i].clone()),
                },
                NodeKind::Child {
                    path,
                    mount: inner,
                    patch,
                } => NodeKind::Child {
                    path: std::iter::once(name.clone())
                        .chain(path.iter().cloned())
                        .collect(),
                    mount: mount.join(inner),
                    patch: patch.clone(),
                },
                NodeKind::Instances { include } => NodeKind::Instances {
                    include: format!("{name}/{include}"),
                },
            },
        })
        .collect()
}

/// Resolve name-based dependencies into content-addressed patches, in
/// topological order. Deterministic: ready names are processed in sorted
/// order. `external` names (inherited patches, include nodes) resolve
/// directly; `repeat_includes` are opaque nodes no patch may depend on.
fn resolve_patches(
    files: BTreeMap<String, PatchFile>,
    external: &BTreeMap<String, PatchId>,
    repeat_includes: &std::collections::BTreeSet<&str>,
) -> Result<(Vec<Patch>, BTreeMap<String, PatchId>)> {
    for (name, file) in &files {
        for dep in &file.depends_on {
            if repeat_includes.contains(dep.as_str()) {
                bail!(
                    "patch `{name}` depends on `{dep}`, a repeat include; reach its instances \
                     with `\"foreach\": \"{dep}\"` instead"
                );
            }
            if !files.contains_key(dep) && !external.contains_key(dep) {
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
            .filter(|(_, f)| {
                f.depends_on
                    .iter()
                    .all(|d| resolved.contains_key(d) || external.contains_key(d))
            })
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
                .map(|d| resolved.get(d).or_else(|| external.get(d)).copied())
                .collect::<Option<Vec<_>>>()
                .expect("ready deps resolve");
            let patch = Patch::new_foreach(dep_ids, file.when, file.foreach, file.ops).with_meta(
                weft_core::PatchMeta {
                    title: file.title,
                    description: file.description,
                    tags: file.tags,
                    hooks: file.hooks,
                    generator: file.generator,
                },
            );
            resolved.insert(name, patch.id);
            patches.push(patch);
        }
    }
    Ok((patches, resolved))
}
