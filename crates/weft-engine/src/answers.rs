use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use weft_core::render::ExprEval;
use weft_core::{AnswerKind, AnswerSet, Question, Value};

use crate::interact::Interaction;
use crate::secrets;
use crate::template::Template;

/// Build the pre-interactive answer layers in precedence order (later wins):
/// presets in CLI order → answers file → `--answer` flags. Conflicts *within*
/// one layer (e.g. the same `--answer` twice with different values) are
/// errors, matching "conflicts at equal precedence are errors".
pub fn layered_answers(
    template: &Template,
    presets: &[String],
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
) -> Result<AnswerSet> {
    let mut layers: Vec<AnswerSet> = Vec::new();
    for preset in presets {
        layers.push(template.preset(preset)?);
    }
    if let Some(path) = answers_file {
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("reading answers file {path}"))?;
        let set: AnswerSet =
            toml::from_str(&src).with_context(|| format!("parsing answers file {path}"))?;
        layers.push(set);
    }
    if !answer_args.is_empty() {
        let mut set = AnswerSet::new();
        for arg in answer_args {
            let (id, value) = parse_answer_arg(&template.manifest.questions, arg)?;
            set.insert_strict(id, value)
                .map_err(|e| anyhow::anyhow!("--answer {arg}: {e}"))?;
        }
        layers.push(set);
    }
    let layered = weft_core::value::layer(&layers);
    for (id, _) in layered.iter() {
        if !template.manifest.questions.iter().any(|q| q.id == *id) {
            bail!(
                "answer `{id}` does not match any question in template `{}`",
                template.manifest.template.name
            );
        }
    }
    Ok(layered)
}

/// Parse `KEY=VALUE`, coercing VALUE according to the question's kind.
pub fn parse_answer_arg(questions: &[Question], arg: &str) -> Result<(weft_core::AnswerId, Value)> {
    let (key, raw) = arg
        .split_once('=')
        .with_context(|| format!("--answer {arg:?} is not KEY=VALUE"))?;
    let question = questions
        .iter()
        .find(|q| q.id.0 == key)
        .with_context(|| format!("no question with id `{key}`"))?;
    let value = coerce(&question.kind, raw).with_context(|| {
        format!(
            "--answer {key}: invalid {} value {raw:?}",
            question.kind.name()
        )
    })?;
    Ok((question.id.clone(), value))
}

fn coerce(kind: &AnswerKind, raw: &str) -> Result<Value> {
    Ok(match kind {
        AnswerKind::String => Value::String(raw.to_owned()),
        AnswerKind::Bool => match raw.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => bail!("expected true/false"),
        },
        AnswerKind::Int => Value::Int(raw.parse()?),
        AnswerKind::Choice { choices } => {
            if !choices.iter().any(|c| c == raw) {
                bail!("expected one of: {}", choices.join(", "));
            }
            Value::String(raw.to_owned())
        }
        AnswerKind::Secret { .. } => {
            bail!("secret answers are resolved from their declared source, not passed inline")
        }
    })
}

/// Placeholder values for every secret question, for previews (`weft graph`,
/// UIs) that must never resolve real secrets. Pass as `presolved_secrets` to
/// [`gather`]; the rendered output shows `<secret:id>` where the value would
/// go.
pub fn placeholder_secrets(questions: &[Question]) -> AnswerSet {
    questions
        .iter()
        .filter(|q| matches!(q.kind, AnswerKind::Secret { .. }))
        .map(|q| {
            (
                q.id.clone(),
                Value::Secret(weft_core::SecretValue::new(format!("<secret:{}>", q.id))),
            )
        })
        .collect()
}

/// Walk questions in declaration order and produce the complete answer set:
/// `when`-gated questions are skipped, provided answers win, secrets resolve
/// through their source, defaults evaluate, and anything left is prompted.
///
/// The final set is validated once more through `resolve_answers` in core.
pub fn gather(
    template: &Template,
    provided: &AnswerSet,
    presolved_secrets: &AnswerSet,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let questions = &template.manifest.questions;
    let mut resolved = AnswerSet::new();
    for q in questions {
        if let Some(when) = &q.when {
            let asked = eval
                .eval_bool(when, &resolved)
                .with_context(|| format!("evaluating when of question `{}`", q.id))?;
            if !asked {
                continue;
            }
        }
        if let AnswerKind::Secret { source } = &q.kind {
            if let Some(v) = presolved_secrets.get(&q.id) {
                // `weft update` re-resolves stored secret references before
                // gathering, so it never re-prompts for a resolvable ref.
                resolved.insert(q.id.clone(), v.clone());
                continue;
            }
            if provided.contains(&q.id) {
                bail!(
                    "secret `{}` cannot be answered via presets/answers files; \
                     it resolves through its declared source ({source})",
                    q.id
                );
            }
            let value = secrets::resolve(q, source, interaction)?;
            resolved.insert(q.id.clone(), Value::Secret(value));
            continue;
        }
        if let Some(v) = provided.get(&q.id) {
            resolved.insert(q.id.clone(), v.clone());
        } else if let Some(default) = &q.default {
            let value = eval
                .eval(default, &resolved)
                .with_context(|| format!("evaluating default of question `{}`", q.id))?;
            resolved.insert(q.id.clone(), value);
        } else {
            let value = interaction.ask(q, None)?;
            resolved.insert(q.id.clone(), value);
        }
    }
    let validated = weft_core::render::resolve_answers(questions, &resolved, eval)?;
    Ok(validated)
}
