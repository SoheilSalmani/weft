//! `weft describe`: the machine-readable template contract for agents (and
//! the AGENTS.md generator built on it). Everything an agent needs to
//! scaffold correctly: each question's type, constraints, evaluated default
//! preview, gate, and whether it must be supplied; preset contents; the
//! patch DAG with descriptions; exact usage commands.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use weft_core::render::ExprEval;
use weft_core::{AnswerKind, AnswerSet, Value};

use crate::graph;
use crate::template::Template;

#[derive(Serialize)]
pub struct DescribeDoc {
    pub template: TemplateDescription,
    pub questions: Vec<QuestionDescription>,
    pub presets: Vec<PresetDescription>,
    pub patches: Vec<PatchDescription>,
    /// All hooks across all patches, ordered `pre` then `post`, each in patch
    /// render order then declaration order (execution order).
    pub hooks: Vec<HookDescription>,
    /// Included child templates (composition). Answers for a child are
    /// namespaced `<name>.<id>` (non-repeat) / `<name>.<key>.<id>` (repeat);
    /// repeat instances are declared with `--instance <name>=<key>`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<IncludeDescription>,
    pub usage: Usage,
}

/// One `[[include]]`: the declaration plus the child's question schema, so
/// an agent knows exactly which namespaced answers each instance accepts.
#[derive(Serialize)]
pub struct IncludeDescription {
    pub name: String,
    /// The child template path, relative to this template's root.
    pub template: String,
    /// Mount prefix; `{key}` substitutes the instance key.
    pub path: String,
    /// Whether the project may instantiate this include 0..N times.
    pub repeat: bool,
    /// Child answer id → Starlark source seeded from parent answers (+ `key`).
    /// Explicit per-instance answers override these.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bind: BTreeMap<String, String>,
    /// The child template's questions (answered per instance, namespaced).
    pub questions: Vec<QuestionDescription>,
}

#[derive(Serialize)]
pub struct TemplateDescription {
    pub name: String,
    pub weft_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct QuestionDescription {
    pub id: String,
    /// `string` | `bool` | `int` | `choice` | `secret`
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// The raw Starlark default expression, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_expr: Option<String>,
    /// Best-effort evaluation of the default with stand-in earlier answers
    /// (display form). Absent when the default can't be previewed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_preview: Option<String>,
    /// Asked only when this Starlark gate is truthy (earlier answers in scope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Must be supplied by the caller: no default and not a secret.
    pub required: bool,
    /// A derived value: computed from other answers, never prompted or
    /// supplied. Agents should not provide it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub computed: bool,
    /// Display grouping (sidebar sections); presentation only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Secrets resolve through this source, never through supplied answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_source: Option<String>,
}

#[derive(Serialize)]
pub struct PresetDescription {
    pub name: String,
    pub answers: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
pub struct PatchDescription {
    pub name: String,
    /// Display title (metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Integration patch: renders once per instance of this include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreach: Option<String>,
    pub depends_on: Vec<String>,
    pub ops: Vec<graph::OpSummary>,
}

/// A patch-scoped side-effect, in execution order. Everything an agent needs
/// to understand what runs, when, and its blast radius.
#[derive(Serialize)]
pub struct HookDescription {
    pub id: String,
    /// The patch that owns this hook.
    pub patch: String,
    /// `pre` (guard, before write) | `post` (after write).
    pub phase: &'static str,
    /// `check` (read-only) | `setup` (idempotent local) | `deploy` (external).
    pub effect: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Command source (interpolations shown as `${…}`; secrets never resolved).
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
}

#[derive(Serialize)]
pub struct Usage {
    /// Ready-to-run scaffold command with required answers filled from
    /// examples/placeholders.
    pub scaffold: String,
    /// One scaffold command per preset.
    pub scaffold_with_preset: Vec<String>,
    /// Answers-as-JSON variant for agents.
    pub scaffold_json: String,
    /// How to evolve the template (record → edit → commit).
    pub author: Vec<String>,
}

/// Build question descriptions for one question list (parent or child).
fn question_descriptions(
    questions: &[weft_core::Question],
    eval: &dyn ExprEval,
) -> Vec<QuestionDescription> {
    questions
        .iter()
        .map(|q| {
            let default_preview = q.default.as_ref().and_then(|expr| {
                // Scope: earlier questions only, mirroring resolution order.
                let idx = questions.iter().position(|o| o.id == q.id).unwrap_or(0);
                let scope: AnswerSet = crate::answers::dummy_answers(&questions[..idx]);
                eval.eval(expr, &scope).ok().map(|v| v.render_text())
            });
            let (kind, choices, secret_source) = match &q.kind {
                AnswerKind::String => ("string", None, None),
                AnswerKind::Bool => ("bool", None, None),
                AnswerKind::Int => ("int", None, None),
                AnswerKind::Choice { choices } => ("choice", Some(choices.clone()), None),
                AnswerKind::MultiChoice { choices } => ("multichoice", Some(choices.clone()), None),
                AnswerKind::Secret { source } => ("secret", None, Some(source.to_string())),
            };
            QuestionDescription {
                id: q.id.0.clone(),
                kind,
                choices,
                prompt: q.prompt.clone(),
                description: q.description.clone(),
                example: q.example.clone(),
                default_expr: q.default.as_ref().map(|e| e.as_str().to_owned()),
                default_preview,
                when: q.when.as_ref().map(|e| e.as_str().to_owned()),
                required: q.default.is_none()
                    && !q.computed
                    && !matches!(q.kind, AnswerKind::Secret { .. }),
                computed: q.computed,
                section: q.section.clone(),
                secret_source,
            }
        })
        .collect()
}

pub fn describe(template: &Template, eval: &dyn ExprEval) -> Result<DescribeDoc> {
    let questions = &template.manifest.questions;
    let dummies = crate::answers::dummy_answers(questions);

    let question_docs = question_descriptions(questions, eval);

    let presets = template
        .manifest
        .presets
        .iter()
        .filter_map(|decl| {
            template
                .preset(&decl.name)
                .ok()
                .map(|set| PresetDescription {
                    name: decl.name.clone(),
                    answers: answer_set_to_json(&set),
                })
        })
        .collect();

    let patches = template
        .patches
        .iter()
        .map(|p| PatchDescription {
            name: template.id_to_name[&p.id].clone(),
            title: p.meta.title.clone(),
            description: p.meta.description.clone(),
            tags: p.meta.tags.clone(),
            when: p.when.as_ref().map(|e| e.as_str().to_owned()),
            foreach: p.foreach.clone(),
            depends_on: p
                .depends_on
                .iter()
                .filter_map(|d| template.id_to_name.get(d).cloned())
                .collect(),
            ops: p.ops.iter().map(graph::op_summary).collect(),
        })
        .collect();

    let hooks = hook_descriptions(template);

    let includes = template
        .includes
        .iter()
        .map(|inc| IncludeDescription {
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
            questions: question_descriptions(&inc.template.manifest.questions, eval),
        })
        .collect();

    let usage = build_usage(template, &question_docs, &dummies);

    Ok(DescribeDoc {
        template: TemplateDescription {
            name: template.manifest.template.name.clone(),
            weft_version: template.manifest.template.weft_version.clone(),
            description: template.manifest.template.description.clone(),
        },
        questions: question_docs,
        presets,
        patches,
        hooks,
        includes,
        usage,
    })
}

/// Flatten every patch's hooks into execution order: `pre` before `post`,
/// then patch render order (`template.patches` is topological), then
/// declaration order. This is the full side-effect contract for agents.
fn hook_descriptions(template: &Template) -> Vec<HookDescription> {
    use weft_core::{HookEffect, HookPhase};
    let mut rows: Vec<(u8, usize, usize, HookDescription)> = Vec::new();
    for (pi, patch) in template.patches.iter().enumerate() {
        let patch_name = template.id_to_name[&patch.id].clone();
        for (di, hook) in patch.meta.hooks.iter().enumerate() {
            let phase_rank = match hook.phase {
                HookPhase::Pre => 0,
                HookPhase::Post => 1,
            };
            rows.push((
                phase_rank,
                pi,
                di,
                HookDescription {
                    id: hook.id.to_string(),
                    patch: patch_name.clone(),
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
                    inputs: hook.inputs.iter().map(ToString::to_string).collect(),
                },
            ));
        }
    }
    rows.sort_by_key(|(phase, pi, di, _)| (*phase, *pi, *di));
    // Present each phase in true execution order: topo-sort over `after`,
    // with the base (declaration) order as the stable tie-break.
    let mut out = Vec::with_capacity(rows.len());
    for phase in [0u8, 1] {
        let group: Vec<HookDescription> = rows
            .iter()
            .filter(|(p, _, _, _)| *p == phase)
            .map(|(_, _, _, d)| clone_desc(d))
            .collect();
        out.extend(topo_by_after(group));
    }
    out
}

fn clone_desc(d: &HookDescription) -> HookDescription {
    HookDescription {
        id: d.id.clone(),
        patch: d.patch.clone(),
        phase: d.phase,
        effect: d.effect,
        label: d.label.clone(),
        description: d.description.clone(),
        action: d.action.clone(),
        when: d.when.clone(),
        after: d.after.clone(),
        inputs: d.inputs.clone(),
    }
}

/// Stable topological order of one phase's hooks over their `after` edges;
/// input order is the base tie-break. `after` refs outside the set are ignored.
fn topo_by_after(hooks: Vec<HookDescription>) -> Vec<HookDescription> {
    let index: BTreeMap<&str, usize> = hooks
        .iter()
        .enumerate()
        .map(|(i, h)| (h.id.as_str(), i))
        .collect();
    let mut indegree = vec![0usize; hooks.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); hooks.len()];
    for (i, h) in hooks.iter().enumerate() {
        for a in &h.after {
            if let Some(&j) = index.get(a.as_str()) {
                indegree[i] += 1;
                dependents[j].push(i);
            }
        }
    }
    let mut ready: std::collections::BTreeSet<usize> =
        (0..hooks.len()).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(hooks.len());
    while let Some(&i) = ready.iter().next() {
        ready.remove(&i);
        order.push(i);
        for &d in &dependents[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                ready.insert(d);
            }
        }
    }
    // A cycle (rejected by `weft check`) leaves some hooks unplaced; append
    // them in base order so describe still lists everything.
    for i in 0..hooks.len() {
        if !order.contains(&i) {
            order.push(i);
        }
    }
    let mut hooks: Vec<Option<HookDescription>> = hooks.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| hooks[i].take().unwrap())
        .collect()
}

fn answer_set_to_json(set: &AnswerSet) -> BTreeMap<String, serde_json::Value> {
    set.iter()
        .filter_map(|(id, value)| Some((id.0.clone(), value_to_json(value)?)))
        .collect()
}

/// JSON projection of an answer value; `None` for secrets (and lists that
/// transitively contain one), which must never be serialized.
fn value_to_json(value: &Value) -> Option<serde_json::Value> {
    Some(match value {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect::<Option<_>>()?)
        }
        Value::Secret(_) => return None,
    })
}

fn build_usage(
    template: &Template,
    questions: &[QuestionDescription],
    _dummies: &AnswerSet,
) -> Usage {
    let dir = &template.root;
    let required: Vec<&QuestionDescription> = questions.iter().filter(|q| q.required).collect();
    let flag_answers: String = required
        .iter()
        .map(|q| {
            let value = q.example.clone().unwrap_or_else(|| match q.kind {
                "bool" => "true".into(),
                "int" => "1".into(),
                "choice" => q
                    .choices
                    .as_ref()
                    .and_then(|c| c.first().cloned())
                    .unwrap_or_default(),
                _ => format!("<{}>", q.id),
            });
            format!(" --answer \"{}={}\"", q.id, value)
        })
        .collect();
    let json_answers: String = {
        let map: BTreeMap<&str, serde_json::Value> = required
            .iter()
            .map(|q| {
                let value = match q.kind {
                    "bool" => serde_json::Value::Bool(true),
                    "int" => serde_json::Value::Number(1.into()),
                    "choice" => serde_json::Value::String(
                        q.choices
                            .as_ref()
                            .and_then(|c| c.first().cloned())
                            .unwrap_or_default(),
                    ),
                    _ => serde_json::Value::String(
                        q.example.clone().unwrap_or_else(|| format!("<{}>", q.id)),
                    ),
                };
                (q.id.as_str(), value)
            })
            .collect();
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
    };
    Usage {
        scaffold: format!("weft new {dir} <dest>{flag_answers} --non-interactive"),
        scaffold_with_preset: template
            .manifest
            .presets
            .iter()
            .map(|p| {
                format!(
                    "weft new {dir} <dest> --preset {}{flag_answers} --non-interactive",
                    p.name
                )
            })
            .collect(),
        scaffold_json: format!("weft new {dir} <dest> --answers-json '{json_answers}'"),
        author: vec![
            format!("weft record --template {dir} <answers…>   # base worktree is printed"),
            "… edit the worktree files with concrete values …".into(),
            format!(
                "weft commit --template {dir} --name <patch> --describe \"what it does\" --yes"
            ),
        ],
    }
}

/// Render the contract as human/agent-readable markdown (AGENTS.md).
pub fn agents_md(doc: &DescribeDoc) -> String {
    let mut out = String::new();
    let mut w = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };

    w("<!-- generated by `weft describe --agents-md`; regenerate rather than edit -->");
    w(&format!("# {} — template guide", doc.template.name));
    w("");
    if let Some(desc) = &doc.template.description {
        w(desc);
        w("");
    }
    w("This directory is a [weft](https://github.com/SoheilSalmani/weft) template.");
    w("For the full machine-readable contract run `weft describe --json` here.");
    w("");

    w("## Questions");
    w("");
    w("| id | kind | required | default | description |");
    w("| --- | --- | --- | --- | --- |");
    for q in &doc.questions {
        let kind = match (&q.choices, &q.secret_source) {
            (Some(choices), _) => format!("choice: {}", choices.join(" / ")),
            (_, Some(source)) => format!("secret ({source})"),
            _ => q.kind.to_owned(),
        };
        let default = q
            .default_preview
            .clone()
            .or_else(|| q.default_expr.clone())
            .unwrap_or_else(|| "—".into());
        let mut desc = q
            .description
            .clone()
            .or_else(|| q.prompt.clone())
            .unwrap_or_default();
        if let Some(example) = &q.example {
            desc = format!("{desc} (e.g. `{example}`)");
        }
        if let Some(when) = &q.when {
            desc = format!("{desc} — only asked when `{when}`");
        }
        w(&format!(
            "| `{}` | {} | {} | {} | {} |",
            q.id,
            kind,
            if q.required { "**yes**" } else { "no" },
            default,
            desc.trim()
        ));
    }
    w("");
    w("Secrets are never passed as answers — they resolve from their source");
    w("(`env:`/`cmd:`/`prompt`) at render time.");
    w("");

    if !doc.presets.is_empty() {
        w("## Presets");
        w("");
        for p in &doc.presets {
            let pairs: Vec<String> = p
                .answers
                .iter()
                .map(|(k, v)| format!("`{k}={v}`"))
                .collect();
            w(&format!("- **{}**: {}", p.name, pairs.join(", ")));
        }
        w("");
    }

    w("## Patches");
    w("");
    for p in &doc.patches {
        let mut line = format!("- **{}**", p.name);
        if let Some(when) = &p.when {
            line = format!("{line} _(when `{when}`)_");
        }
        if let Some(desc) = &p.description {
            line = format!("{line}: {desc}");
        }
        w(&line);
    }
    w("");

    if !doc.hooks.is_empty() {
        w("## Hooks (side-effects, in run order)");
        w("");
        w("`pre` hooks run before any file is written (a failure aborts); `post`");
        w("hooks run after. Effects: **check** = read-only/safe, **setup** =");
        w("idempotent local, **deploy** = external/irreversible (confirm first).");
        w("");
        w("| phase | effect | label | patch | command | when |");
        w("| --- | --- | --- | --- | --- | --- |");
        for h in &doc.hooks {
            w(&format!(
                "| {} | {} | {} | `{}` | `{}` | {} |",
                h.phase,
                h.effect,
                h.label,
                h.patch,
                h.action,
                h.when.as_deref().unwrap_or("—"),
            ));
        }
        w("");
    }

    if !doc.includes.is_empty() {
        w("## Includes (composition)");
        w("");
        w("This template mounts child templates. Answer a child's questions with");
        w("namespaced ids: `<include>.<id>` (single) or `<include>.<key>.<id>`");
        w("(repeatable; declare instances with `--instance <include>=<key>`).");
        w("");
        w("| include | template | mount | repeat | bound answers |");
        w("| --- | --- | --- | --- | --- |");
        for inc in &doc.includes {
            let bound: Vec<String> = inc.bind.keys().map(|k| format!("`{k}`")).collect();
            w(&format!(
                "| `{}` | `{}` | `{}` | {} | {} |",
                inc.name,
                inc.template,
                inc.path,
                if inc.repeat { "0..N" } else { "1" },
                if bound.is_empty() {
                    "—".to_owned()
                } else {
                    bound.join(", ")
                },
            ));
        }
        w("");
    }

    w("## Scaffold");
    w("");
    w("```sh");
    w(&doc.usage.scaffold);
    for cmd in &doc.usage.scaffold_with_preset {
        w(cmd);
    }
    w("```");
    w("");
    w("Agents can pass answers as JSON instead of flags:");
    w("");
    w("```sh");
    w(&doc.usage.scaffold_json);
    w("```");
    w("");

    w("## Evolve this template");
    w("");
    w("```sh");
    for cmd in &doc.usage.author {
        w(cmd);
    }
    w("```");
    w("");
    w("Run `weft check --json` after any change; independent patches must");
    w("commute and every expression must evaluate.");
    out
}
