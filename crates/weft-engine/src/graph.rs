//! Graph projection of a template: patches as nodes, dependencies as edges.
//!
//! This is the data structure behind `weft graph` and any UI that renders a
//! template visually. When answers are supplied, each node also reports
//! whether it is *active* (its `when` gate passes and no ancestor is
//! skipped), and [`node_diff`] computes exactly what one patch contributes
//! under those answers.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::Serialize;
use similar::TextDiff;
use weft_core::render::ExprEval;
use weft_core::{AnswerSet, Op, Patch, PatchId, Question, Segment, TemplatePath};

use crate::template::Template;

#[derive(Debug, Serialize)]
pub struct GraphDoc {
    pub template: TemplateInfo,
    pub questions: Vec<Question>,
    pub presets: Vec<String>,
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
    /// Content id (blake3 hex).
    pub id: PatchId,
    /// Human name (the patch's file stem).
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
    /// This patch's hooks (declaration order), for the UI's pre/post panels.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookSummary>,
    /// Whether the patch applies under the supplied answers; `None` when the
    /// graph was built without answers.
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
    }
}

#[derive(Debug, Serialize)]
pub struct OpSummary {
    /// `create_file` | `modify_file` | `delete_file` | `rename_path` | `set_mode`
    pub kind: &'static str,
    /// Display form of the target path; answer references render as `{id}`,
    /// expressions as `{=expr}`, renames as `from → to`.
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
    let skipped = match answers {
        Some(answers) => Some(skipped_patches(&template.patches, answers, eval)?),
        None => None,
    };

    let mut nodes = Vec::with_capacity(template.patches.len());
    let mut edges = Vec::new();
    for patch in &template.patches {
        let name = template.id_to_name[&patch.id].clone();
        nodes.push(GraphNode {
            id: patch.id,
            name,
            title: patch.meta.title.clone(),
            when: patch.when.as_ref().map(|w| w.as_str().to_owned()),
            description: patch.meta.description.clone(),
            tags: patch.meta.tags.clone(),
            ops: patch.ops.iter().map(op_summary).collect(),
            foreach: patch.foreach.clone(),
            hooks: patch.meta.hooks.iter().map(hook_summary).collect(),
            active: skipped.as_ref().map(|s| !s.contains(&patch.id)),
        });
        for dep in &patch.depends_on {
            edges.push(GraphEdge {
                source: *dep,
                target: patch.id,
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

/// Mirror of the skip logic in `weft_core::render::render`: a patch is
/// skipped when its `when` gate is false or any dependency is skipped.
fn skipped_patches(
    patches: &[Patch],
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<BTreeSet<PatchId>> {
    let order = weft_core::render::patch_order(patches)?;
    let mut skipped = BTreeSet::new();
    for patch in order {
        if patch.depends_on.iter().any(|d| skipped.contains(d)) {
            skipped.insert(patch.id);
            continue;
        }
        // Foreach patches gate per instance (`key`/`instance_*` in scope) —
        // their activity isn't answerable from parent answers alone, so they
        // are shown as active whenever their dependencies are.
        if patch.foreach.is_some() {
            continue;
        }
        if let Some(when) = &patch.when {
            let active = eval
                .eval_bool(when, answers)
                .with_context(|| format!("evaluating when of patch {}", patch.id.short()))?;
            if !active {
                skipped.insert(patch.id);
            }
        }
    }
    Ok(skipped)
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
    }
}

/// Symbolic display of a possibly-abstracted path.
pub fn display_path(path: &TemplatePath) -> String {
    path.0
        .iter()
        .map(|seg| match seg {
            Segment::Literal(s) => s.clone(),
            Segment::Answer(id) => format!("{{{id}}}"),
            Segment::Expr(e) => format!("{{={}}}", e.as_str()),
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

/// What `patch_name` contributes under `answers`: render its ancestor
/// closure without and with the patch and diff the trees. Requires a
/// complete answer set (same requirement as rendering).
pub fn node_diff(
    template: &Template,
    patch_name: &str,
    answers: &AnswerSet,
    eval: &dyn ExprEval,
) -> Result<NodeDiff> {
    let id = *template.name_to_id.get(patch_name).with_context(|| {
        format!(
            "unknown patch `{patch_name}` (known: {})",
            template
                .name_to_id
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let closure = template.ancestor_closure(id);
    let with: Vec<Patch> = template
        .patches
        .iter()
        .filter(|p| closure.contains(&p.id))
        .cloned()
        .collect();
    let without: Vec<Patch> = with.iter().filter(|p| p.id != id).cloned().collect();

    // Force the node's own gate open so the diff shows what it *would* do;
    // report the real gate state separately.
    let skipped = skipped_patches(&template.patches, answers, eval)?;
    let active = !skipped.contains(&id);
    let with: Vec<Patch> = with
        .into_iter()
        .map(|mut p| {
            if p.id == id {
                p.when = None;
            }
            p
        })
        .collect();

    let before = weft_core::render::render(&without, answers, eval)
        .context("rendering the patch's base (ancestors only)")?;
    let after = weft_core::render::render(&with, answers, eval)
        .context("rendering the patch's base plus the patch")?;

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
        name: patch_name.to_owned(),
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
