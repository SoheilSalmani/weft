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
    pub tasks: Vec<TaskDescription>,
    pub usage: Usage,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub depends_on: Vec<String>,
    pub ops: Vec<graph::OpSummary>,
}

#[derive(Serialize)]
pub struct TaskDescription {
    pub id: String,
    pub inputs: Vec<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
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

pub fn describe(template: &Template, eval: &dyn ExprEval) -> Result<DescribeDoc> {
    let questions = &template.manifest.questions;
    let dummies = crate::answers::dummy_answers(questions);

    let question_docs: Vec<QuestionDescription> = questions
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
                secret_source,
            }
        })
        .collect();

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
            description: p.meta.description.clone(),
            tags: p.meta.tags.clone(),
            when: p.when.as_ref().map(|e| e.as_str().to_owned()),
            depends_on: p
                .depends_on
                .iter()
                .filter_map(|d| template.id_to_name.get(d).cloned())
                .collect(),
            ops: p.ops.iter().map(graph::op_summary).collect(),
        })
        .collect();

    let tasks = template
        .manifest
        .tasks
        .iter()
        .map(|t| TaskDescription {
            id: t.id.to_string(),
            inputs: t.inputs.iter().map(ToString::to_string).collect(),
            action: t.action.source(),
            when: t.when.as_ref().map(|e| e.as_str().to_owned()),
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
        tasks,
        usage,
    })
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
