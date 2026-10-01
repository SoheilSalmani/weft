//! Graph projection of a template: patches as nodes, dependencies as edges.
//!
//! This is the data structure behind `weft graph` and any UI that renders a
//! template visually. When answers are supplied, each node also reports
//! whether it is *active* (its `when` gate passes and no ancestor is
//! skipped), and [`node_diff`] computes exactly what one patch contributes
//! under those answers.

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use serde::Serialize;
use similar::TextDiff;
use weft_core::render::{ExprEval, Framed};
use weft_core::{AnswerSet, Op, Patch, PatchId, Question, Segment, TemplatePath};

use crate::compose;
use crate::template::{Node, NodeKind, Template};

#[derive(Debug, Serialize)]
pub struct GraphDoc {
    pub template: TemplateInfo,
    /// The base template this one extends, as declared (`[template] extends`).
    /// Its patches are root nodes here, like the template's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub questions: Vec<Question>,
    pub presets: Vec<String>,
    /// The composed graph: root-frame patches, every single include's
    /// patches (`<include>/<name>`, rendered under the include's mount), and
    /// one opaque node per repeat include.
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Included child templates as nested (structural) graphs — the feed for
    /// UI subflows. Empty for templates without includes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<IncludeGraph>,
}

/// An `[[include]]` with its child template's structural graph.
#[derive(Debug, Serialize)]
pub struct IncludeGraph {
    pub name: String,
    /// Child template path, relative to the parent root.
    pub template: String,
    /// Mount prefix (`{key}` substitutes the instance key).
    pub path: String,
    pub repeat: bool,
    /// Child answer id → Starlark bind source (seeded from parent answers).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub bind: std::collections::BTreeMap<String, String>,
    /// The child's own graph (structural — no answers applied).
    pub graph: GraphDoc,
}

#[derive(Debug, Serialize)]
pub struct TemplateInfo {
    pub name: String,
    pub weft_version: String,
}

#[derive(Debug, Serialize)]
pub struct GraphNode {
    /// Node id in the composed graph: the content id (blake3 hex) for a
    /// root patch, keyed by the include for an include node.
    pub id: PatchId,
    /// Human name: the patch's file stem, `<include>/<stem>` for an include
    /// node, the include name for an opaque repeat node.
    pub name: String,
    /// Display title (metadata) — UIs prefer this over `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub ops: Vec<OpSummary>,
    /// Integration patch: renders once per instance of this include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreach: Option<String>,
    /// The top-level include this node renders through (include nodes and
    /// opaque repeat nodes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<String>,
    /// Static mount prefix of an include node (`""` for a root mount).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount: Option<String>,
    /// A repeat include's instances, as one node: no ops, no gate; only
    /// `foreach` patches reach it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub opaque: bool,
    /// This patch's hooks (declaration order), for the UI's pre/post panels.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookSummary>,
    /// Whether the patch applies under the supplied answers; `None` when the
    /// graph was built without answers, for opaque nodes, and for include
    /// nodes whose instance answers could not be resolved.
    pub active: Option<bool>,
}

/// A patch hook, summarized for display.
#[derive(Debug, Serialize)]
pub struct HookSummary {
    pub id: String,
    /// `pre` | `post`.
    pub phase: &'static str,
    /// `check` | `setup` | `deploy`.
    pub effect: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Command source (interpolations shown as `${…}`).
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
}

fn hook_summary(hook: &weft_core::Hook) -> HookSummary {
    use weft_core::{HookEffect, HookPhase};
    HookSummary {
        id: hook.id.to_string(),
        phase: match hook.phase {
            HookPhase::Pre => "pre",
            HookPhase::Post => "post",
        },
        effect: match hook.effect {
            HookEffect::Check => "check",
            HookEffect::Setup => "setup",
            HookEffect::Deploy => "deploy",
        },
        label: hook.label.clone(),
        description: hook.description.clone(),
        action: hook.action.source(),
        when: hook.when.as_ref().map(|e| e.as_str().to_owned()),
        after: hook.after.iter().map(ToString::to_string).collect(),
        before: hook.before.iter().map(ToString::to_string).collect(),
    }
}

#[derive(Debug, Serialize)]
pub struct OpSummary {
    /// `create_file` | `create_binary_file` | `modify_file` | `delete_file`
    /// | `rename_path` | `set_mode` | `fill_slot`
    pub kind: &'static str,
    /// Display form of the target path; answer references render as `{id}`,
    /// expressions as `{=expr}`, renames as `from → to`, and a fill as
    /// `path#slot`.
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct GraphEdge {
    /// The dependency (rendered first).
    pub source: PatchId,
    /// The dependent patch.
    pub target: PatchId,
}

/// Build the graph document. `answers` enables active/inactive computation;
/// pass `None` for a purely structural graph.
pub fn graph_doc(
    template: &Template,
    answers: Option<&AnswerSet>,
    eval: &dyn ExprEval,
) -> Result<GraphDoc> {
    let activity = match answers {
        Some(answers) => Some(Activity::compute(template, answers, eval)?),
        None => None,
    };

    let mut nodes = Vec::with_capacity(template.nodes.len());
    let mut edges = Vec::new();
    for node in &template.nodes {
        let patch = template.node_patch(node);
        let active = activity.as_ref().and_then(|a| a.of(node, patch));
        let (mount, opaque) = match &node.kind {
            NodeKind::Root(_) => (None, false),
            NodeKind::Child { mount, .. } => (Some(mount.to_string()), false),
            NodeKind::Instances { .. } => (None, true),
        };
        nodes.push(GraphNode {
            id: node.id,
            name: node.name.clone(),
            title: patch.and_then(|p| p.meta.title.clone()),
            when: patch.and_then(|p| p.when.as_ref().map(|w| w.as_str().to_owned())),
            description: patch.and_then(|p| p.meta.description.clone()),
            tags: patch.map(|p| p.meta.tags.clone()).unwrap_or_default(),
            ops: patch
                .map(|p| p.ops.iter().map(op_summary).collect())
                .unwrap_or_default(),
            foreach: patch.and_then(|p| p.foreach.clone()),
            include: node.include().map(str::to_owned),
            mount,
            opaque,
            hooks: patch
                .map(|p| p.meta.hooks.iter().map(hook_summary).collect())
                .unwrap_or_default(),
            active,
        });
        for dep in &node.depends_on {
            edges.push(GraphEdge {
                source: *dep,
                target: node.id,
            });
        }
        // A foreach patch reads every instance of its include: an edge from
        // the include's opaque node. A single include has no such node.
        if let Some(source) = patch
            .and_then(|p| p.foreach.as_deref())
            .and_then(|inc| foreach_source(template, node, inc))
        {
            edges.push(GraphEdge {
                source,
                target: node.id,
            });
        }
    }
    // Stable output: nodes by name, edges by (source, target).
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    edges.sort_by_key(|e| (e.source, e.target));

    // Nested child graphs (structural), for UI subflows.
    let includes = template
        .includes
        .iter()
        .map(|inc| {
            Ok(IncludeGraph {
                name: inc.decl.name.clone(),
                template: inc.decl.template.to_string(),
                path: inc.decl.path.clone(),
                repeat: inc.decl.repeat,
                bind: inc
                    .decl
                    .bind
                    .iter()
                    .map(|(k, e)| (k.clone(), e.as_str().to_owned()))
                    .collect(),
                graph: graph_doc(&inc.template, None, eval)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(GraphDoc {
        template: TemplateInfo {
            name: template.manifest.template.name.clone(),
            weft_version: template.manifest.template.weft_version.clone(),
        },
        extends: template
            .extends
            .as_ref()
            .map(|e| e.decl.template().to_string()),
        questions: template.manifest.questions.clone(),
        presets: template
            .manifest
            .presets
            .iter()
            .map(|p| p.name.clone())
            .collect(),
        nodes,
        edges,
        includes,
    })
}

/// The opaque node a foreach patch iterates: the repeat include `inc` of the
/// frame `node` renders in (`web/conn` for a child node under `web`).
fn foreach_source(template: &Template, node: &Node, inc: &str) -> Option<PatchId> {
    let name = match &node.kind {
        NodeKind::Child { path, .. } => format!("{}/{inc}", path.join("/")),
        _ => inc.to_owned(),
    };
    let id = *template.name_to_id.get(&name)?;
    matches!(template.node(id)?.kind, NodeKind::Instances { .. }).then_some(id)
}

/// Node activity under one answer set, over the composed graph — the skip
/// logic of `weft_core::render::render_framed` without rendering.
struct Activity {
    /// Gate open and no skipped dependency. Foreach patches and opaque
    /// nodes are never here.
    active: BTreeSet<PatchId>,
    /// Nodes whose state cannot be decided: opaque repeat nodes, include
    /// nodes whose instance answers did not resolve, and whatever depends on
    /// those.
    unknown: BTreeSet<PatchId>,
}

impl Activity {
    fn compute(template: &Template, answers: &AnswerSet, eval: &dyn ExprEval) -> Result<Self> {
        let mut unknown: BTreeSet<PatchId> = template
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Instances { .. }))
            .map(|n| n.id)
            .collect();
        // Include nodes need their instance's answers (binds, child
        // defaults, placeholder secrets). When those cannot be resolved
        // non-interactively, the child frames are unknown and the root frame
        // is computed alone.
        if let Ok(parts) = compose::preview_parts(
            template,
            answers,
            &Default::default(),
            &Default::default(),
            eval,
        ) {
            let active = compose::active_nodes(&template.patches, answers, &parts, eval)?;
            return Ok(Activity { active, unknown });
        }
        unknown.extend(
            template
                .nodes
                .iter()
                .filter(|n| matches!(n.kind, NodeKind::Child { .. }))
                .map(|n| n.id),
        );
        let mut active = BTreeSet::new();
        // `patches` is topological: dependencies are decided first.
        for patch in &template.patches {
            if patch.depends_on.iter().any(|d| unknown.contains(d)) {
                unknown.insert(patch.id);
                continue;
            }
            if patch.foreach.is_some() || !patch.depends_on.iter().all(|d| active.contains(d)) {
                continue;
            }
            let open = match &patch.when {
                None => true,
                Some(when) => eval
                    .eval_bool(when, answers)
                    .with_context(|| format!("evaluating when of patch {}", patch.id.short()))?,
            };
            if open {
                active.insert(patch.id);
            }
        }
        Ok(Activity { active, unknown })
    }

    /// `Some(state)` for a decidable node. Foreach patches gate per instance
    /// (`key`/`instance_*` in scope) — not answerable from parent answers
    /// alone, so they show as active whenever their dependencies are.
    fn of(&self, node: &Node, patch: Option<&Patch>) -> Option<bool> {
        let patch = patch?;
        if patch.foreach.is_some() {
            if node.depends_on.iter().any(|d| self.unknown.contains(d)) {
                return None;
            }
            return Some(node.depends_on.iter().all(|d| self.active.contains(d)));
        }
        (!self.unknown.contains(&node.id)).then(|| self.active.contains(&node.id))
    }
}

pub(crate) fn op_summary(op: &Op) -> OpSummary {
    match op {
        Op::CreateFile { path, .. } => OpSummary {
            kind: "create_file",
            path: display_path(path),
        },
        Op::CreateBinaryFile { path, .. } => OpSummary {
            kind: "create_binary_file",
            path: display_path(path),
        },
        Op::ModifyFile { path, .. } => OpSummary {
            kind: "modify_file",
            path: display_path(path),
        },
        Op::DeleteFile { path } => OpSummary {
            kind: "delete_file",
            path: display_path(path),
        },
        Op::RenamePath { from, to } => OpSummary {
            kind: "rename_path",
            path: format!("{} → {}", display_path(from), display_path(to)),
        },
        Op::SetMode { path, .. } => OpSummary {
            kind: "set_mode",
            path: display_path(path),
        },
        Op::FillSlot { path, slot, .. } => OpSummary {
            kind: "fill_slot",
            path: format!("{}#{slot}", display_path(path)),
        },
    }
}

/// Symbolic display of a possibly-abstracted path.
pub fn display_path(path: &TemplatePath) -> String {
    display_segments(&path.0)
}

/// Symbolic display of segments: literal text as is, an answer reference as
/// `{id}`, an expression as `{=source}`.
pub fn display_segments(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|seg| match seg {
            Segment::Literal(s) => s.clone(),
            Segment::Answer(id) => format!("{{{id}}}"),
            Segment::Expr(e) => format!("{{={}}}", e.as_str()),
            Segment::Slot(decl) => format!("{{slot {}}}", decl.slot),
        })
        .collect()
}

// ---- per-node diff ----------------------------------------------------

#[derive(Debug, Serialize)]
pub struct NodeDiff {
    pub id: PatchId,
    pub name: String,
    /// `false` when the patch's `when` gate is closed under these answers
    /// (the diff is then what it *would* contribute if enabled).
    pub active: bool,
    pub files: Vec<FileDiff>,
}

#[derive(Debug, Serialize)]
pub struct FileDiff {
    pub path: Utf8PathBuf,
    /// `created` | `modified` | `deleted`
    pub change: &'static str,
    /// Unified diff of the file (against empty for created/deleted).
    pub diff: String,
    /// Full file content on each side (empty string for the missing side).
    /// UIs render these with real diff widgets instead of the text diff.
    pub before: String,
    pub after: String,
    /// Either side is binary: before/after are empty, diff is a stub.
    #[serde(default)]
    pub binary: bool,
}

/// What node `name` contributes under `answers`: render its ancestor
/// closure (over the composed graph, each node in its frame) without and
/// with the node and diff the trees. Requires a complete answer set (same
/// requirement as rendering); include nodes also need their instance's
/// answers to resolve non-interactively.
pub fn node_diff(
    template: &Template,
    name: &str,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<NodeDiff> {
    let id = *template.name_to_id.get(name).with_context(|| {
        format!(
            "unknown patch `{name}` (known: {})",
            template
                .name_to_id
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let node = template.node(id).expect("every name is a node");
    let Some(patch) = template.node_patch(node) else {
        bail!(
            "`{name}` is a repeat include: its instances are opaque to this graph \
             (diff a `foreach = \"{name}\"` patch instead)"
        );
    };
    let parts = compose::preview_parts(
        template,
        answers,
        &Default::default(),
        &Default::default(),
        eval,
    )
    .context("resolving the template's includes")?;
    let graph = compose::graph_nodes(&template.patches, answers, &parts);
    let closure = template.ancestor_closure(id);

    // Force the node's own gate open so the diff shows what it *would* do;
    // report the real gate state separately.
    let mut forced = patch.clone();
    forced.when = None;
    let active = Activity::compute(template, answers, eval)?
        .of(node, Some(patch))
        .unwrap_or(false);
    let with: Vec<Framed> = graph
        .iter()
        .filter(|n| closure.contains(&n.id))
        .map(|n| Framed {
            id: n.id,
            depends_on: &n.depends_on,
            patch: if n.id == id { &forced } else { n.patch },
            answers: n.answers,
            mount: &n.mount,
        })
        .collect();
    let without: Vec<Framed> = with.iter().filter(|n| n.id != id).copied().collect();

    let render = |nodes: &[Framed], what: &str| -> Result<weft_core::Tree> {
        let order =
            weft_core::render::framed_order(nodes).with_context(|| format!("ordering {what}"))?;
        let (tree, _) = weft_core::render::render_framed(&order, eval)
            .with_context(|| format!("rendering {what}"))?;
        Ok(tree)
    };
    let before = render(&without, "the node's base (ancestors only)")?;
    let after = render(&with, "the node's base plus the node")?;

    let mut files = Vec::new();
    let mut paths: BTreeSet<&Utf8PathBuf> = BTreeSet::new();
    paths.extend(before.paths());
    paths.extend(after.paths());
    for path in paths {
        let before_entry = before.get(path);
        let after_entry = after.get(path);
        if before_entry.map(|e| &e.content) == after_entry.map(|e| &e.content) {
            continue;
        }
        let change = match (before_entry, after_entry) {
            (None, Some(_)) => "created",
            (Some(_), None) => "deleted",
            _ => "modified",
        };
        // Binary sides are opaque: no unified diff, empty before/after.
        let binary = before_entry.is_some_and(|e| e.content.is_binary())
            || after_entry.is_some_and(|e| e.content.is_binary());
        let old = before_entry.and_then(|e| e.content.text()).unwrap_or("");
        let new = after_entry.and_then(|e| e.content.text()).unwrap_or("");
        let diff = if binary {
            "Binary files differ\n".to_owned()
        } else {
            TextDiff::from_lines(old, new)
                .unified_diff()
                .context_radius(3)
                .header(&format!("a/{path}"), &format!("b/{path}"))
                .to_string()
        };
        files.push(FileDiff {
            path: path.clone(),
            change,
            diff,
            before: if binary {
                String::new()
            } else {
                old.to_owned()
            },
            after: if binary {
                String::new()
            } else {
                new.to_owned()
            },
            binary,
        });
    }

    Ok(NodeDiff {
        id,
        name: name.to_owned(),
        active,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use weft_core::{AnswerId, Value};
    use weft_lang::StarlarkEval;

    fn hello_template() -> Template {
        let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/templates/hello");
        Template::load(&root).unwrap()
    }

    fn answers(use_docker: bool) -> AnswerSet {
        [
            (
                AnswerId::from("project_name"),
                Value::String("My Demo".into()),
            ),
            (
                AnswerId::from("package_name"),
                Value::String("my-demo".into()),
            ),
            (AnswerId::from("use_docker"), Value::Bool(use_docker)),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn structural_graph_without_answers() {
        let doc = graph_doc(&hello_template(), None, &StarlarkEval).unwrap();
        assert_eq!(doc.nodes.len(), 3);
        assert_eq!(doc.edges.len(), 2);
        assert!(doc.nodes.iter().all(|n| n.active.is_none()));
        let docker = doc.nodes.iter().find(|n| n.name == "docker").unwrap();
        assert_eq!(docker.when.as_deref(), Some("use_docker"));
        assert_eq!(docker.ops[0].kind, "create_file");
    }

    #[test]
    fn active_state_follows_when_gate() {
        let template = hello_template();
        let on = graph_doc(&template, Some(&answers(true)), &StarlarkEval).unwrap();
        assert!(on.nodes.iter().all(|n| n.active == Some(true)));

        let off = graph_doc(&template, Some(&answers(false)), &StarlarkEval).unwrap();
        let docker = off.nodes.iter().find(|n| n.name == "docker").unwrap();
        let base = off.nodes.iter().find(|n| n.name == "base").unwrap();
        assert_eq!(docker.active, Some(false));
        assert_eq!(base.active, Some(true));
    }

    fn workspace_template() -> Template {
        let root = camino::Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/templates/workspace");
        Template::load(&root).unwrap()
    }

    #[test]
    fn composed_graph_lists_include_nodes_and_opaque_repeat() {
        let template = workspace_template();
        let answers: AnswerSet = [(
            AnswerId::from("workspace_name"),
            Value::String("Acme".into()),
        )]
        .into_iter()
        .collect();
        let doc = graph_doc(&template, Some(&answers), &StarlarkEval).unwrap();
        let names: Vec<&str> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "base",
                "connector",
                "registry",
                "svc/base",
                "svc/deploy",
                "svc/docker"
            ]
        );

        let by_name = |n: &str| doc.nodes.iter().find(|x| x.name == n).unwrap();
        let svc_docker = by_name("svc/docker");
        assert_eq!(svc_docker.include.as_deref(), Some("svc"));
        assert_eq!(svc_docker.mount.as_deref(), Some("services/hello"));
        assert_eq!(svc_docker.id, template.name_to_id["svc/docker"]);
        // Instance answers resolve through the bind (`use_docker` defaults on).
        assert_eq!(svc_docker.active, Some(true));

        let connector = by_name("connector");
        assert!(connector.opaque);
        assert!(connector.ops.is_empty());
        assert_eq!(connector.active, None);

        // Child dependencies stay inside the include; the foreach patch is
        // fed by the opaque node.
        let edge = |s: &str, t: &str| {
            doc.edges
                .iter()
                .any(|e| e.source == by_name(s).id && e.target == by_name(t).id)
        };
        assert!(edge("svc/base", "svc/docker"));
        assert!(edge("connector", "registry"));
        assert!(edge("base", "registry"));
        assert_eq!(doc.edges.len(), 4);
    }

    #[test]
    fn node_diff_of_include_node_renders_under_its_mount() {
        let template = workspace_template();
        let answers: AnswerSet = [(
            AnswerId::from("workspace_name"),
            Value::String("Acme".into()),
        )]
        .into_iter()
        .collect();
        let diff = node_diff(&template, "svc/docker", &answers, &StarlarkEval).unwrap();
        assert!(diff.active);
        assert!(diff
            .files
            .iter()
            .any(|f| f.path == "services/hello/Dockerfile" && f.change == "created"));
        assert!(node_diff(&template, "connector", &answers, &StarlarkEval).is_err());
    }

    #[test]
    fn node_diff_shows_contribution() {
        let template = hello_template();
        let diff = node_diff(&template, "docker", &answers(true), &StarlarkEval).unwrap();
        assert!(diff.active);
        assert_eq!(diff.files.len(), 2, "{diff:?}");
        let dockerfile = diff.files.iter().find(|f| f.path == "Dockerfile").unwrap();
        assert_eq!(dockerfile.change, "created");
        assert!(dockerfile.diff.contains("+FROM python:3.12-slim"));
        let readme = diff.files.iter().find(|f| f.path == "README.md").unwrap();
        assert_eq!(readme.change, "modified");
        assert!(readme.diff.contains("+Ships with Docker."));
    }

    #[test]
    fn node_diff_of_gated_off_patch_still_shows_would_be_changes() {
        let template = hello_template();
        let diff = node_diff(&template, "docker", &answers(false), &StarlarkEval).unwrap();
        assert!(!diff.active);
        assert!(diff.files.iter().any(|f| f.path == "Dockerfile"));
    }
}
