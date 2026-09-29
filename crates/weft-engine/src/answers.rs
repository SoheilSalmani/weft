use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use weft_core::render::{narrow, ExprEval, ValueOrigin};
use weft_core::{AnswerKind, AnswerSet, Question, Value};

use crate::interact::Interaction;
use crate::secrets;
use crate::template::Template;

/// The layered pre-interactive answers plus the preset lock information —
/// what the wizard needs to skip locked rows and constrain multichoices.
pub struct Layered {
    pub answers: AnswerSet,
    /// Question ids locked by a selected preset (not editable, not
    /// overridable).
    pub locked: std::collections::BTreeSet<weft_core::AnswerId>,
    /// Multichoice constraints: id → (fixed, blocked).
    pub constraints: std::collections::BTreeMap<weft_core::AnswerId, (Vec<String>, Vec<String>)>,
}

/// Build the pre-interactive answers. Presets **lock** what they answer: a
/// later answers-file/`--answer` value for a locked id is an error, and
/// multichoice constraints merge as `(user ∪ fixed) − blocked`. Conflicts
/// *within* one layer (the same `--answer` twice with different values)
/// are also errors.
pub fn layered_answers(
    template: &Template,
    presets: &[String],
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
) -> Result<AnswerSet> {
    Ok(layered_full(template, presets, answers_file, answer_args)?.answers)
}

/// [`layered_answers`] with the lock/constraint info exposed.
pub fn layered_full(
    template: &Template,
    presets: &[String],
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
) -> Result<Layered> {
    let mut specs = Vec::new();
    for preset in presets {
        specs.push(template.preset_spec(preset)?);
    }
    let spec = crate::preset::PresetSpec::merge(specs)?;

    // User layers first (file → flags), then the preset spec enforces
    // locks and merges constraints over them.
    let user = layered_user(template, answers_file, answer_args)?;
    let layered = spec.apply(&user)?;
    for (id, _) in layered.iter() {
        if crate::compose::find_question(template, &id.0).is_none() {
            bail!(
                "answer `{id}` does not match any question in template `{}` or its includes",
                template.manifest.template.name
            );
        }
    }
    Ok(Layered {
        answers: layered,
        locked: spec.locked(),
        constraints: spec.constraints(),
    })
}

/// The user-provided layers only: answers file → `--answer` flags.
fn layered_user(
    template: &Template,
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
) -> Result<AnswerSet> {
    let mut layers: Vec<AnswerSet> = Vec::new();
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
            let (key, raw) = arg
                .split_once('=')
                .with_context(|| format!("--answer {arg:?} is not KEY=VALUE"))?;
            // Namespaced keys (`<include>.<child-id>`) resolve through the
            // includes; plain keys through the parent's questions.
            let question = crate::compose::find_question(template, key)
                .with_context(|| format!("no question with id `{key}`"))?;
            let value = coerce(question, raw).with_context(|| {
                format!(
                    "--answer {key}: invalid {} value {raw:?}",
                    question.kind.name()
                )
            })?;
            set.insert_strict(weft_core::AnswerId(key.to_owned()), value)
                .map_err(|e| anyhow::anyhow!("--answer {arg}: {e}"))?;
        }
        layers.push(set);
    }
    Ok(weft_core::value::layer(&layers))
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
    let value = coerce(question, raw).with_context(|| {
        format!(
            "--answer {key}: invalid {} value {raw:?}",
            question.kind.name()
        )
    })?;
    Ok((question.id.clone(), value))
}

fn coerce(question: &Question, raw: &str) -> Result<Value> {
    // A blocked choice is gone from `choices`; say why, not just "not one of".
    let not_offered = |choice: &str, choices: &[String]| {
        if question.narrowing.blocked.iter().any(|b| b == choice) {
            anyhow::anyhow!("`{choice}` is blocked by {}", question.narrowing.refiners())
        } else {
            anyhow::anyhow!("`{choice}` is not one of: {}", choices.join(", "))
        }
    };
    Ok(match &question.kind {
        AnswerKind::String => Value::String(raw.to_owned()),
        AnswerKind::Bool => match raw.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => bail!("expected true/false"),
        },
        AnswerKind::Int => Value::Int(raw.parse()?),
        AnswerKind::Choice { choices } => {
            if !choices.iter().any(|c| c == raw) {
                return Err(not_offered(raw, choices));
            }
            Value::String(raw.to_owned())
        }
        AnswerKind::MultiChoice { choices } => {
            // Comma-separated on the CLI; empty string is the empty selection.
            let mut items = Vec::new();
            for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !choices.iter().any(|c| c == part) {
                    return Err(not_offered(part, choices));
                }
                items.push(Value::String(part.to_owned()));
            }
            Value::List(items)
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
/// Every value passes through its question's narrowing on the way in, so
/// later gates and defaults see what the render will.
///
/// The final set is validated once more through `resolve_answers` in core,
/// which also holds locked questions to their lock.
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
                // Not prompted, but keep the name defined via its default so
                // later Starlark gates/exprs don't hit an undefined name.
                let stand_in = match &q.default {
                    Some(default) => eval.eval(default, &resolved).ok(),
                    None if q.nothing_to_choose() => Some(Value::List(Vec::new())),
                    None => None,
                };
                if let Some(value) = stand_in.and_then(|v| narrow(q, v, ValueOrigin::Default).ok())
                {
                    resolved.insert(q.id.clone(), value);
                }
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
        let value = if let Some(v) = provided.get(&q.id) {
            narrow(q, v.clone(), ValueOrigin::Input)?
        } else if let Some(default) = &q.default {
            let value = eval
                .eval(default, &resolved)
                .with_context(|| format!("evaluating default of question `{}`", q.id))?;
            narrow(q, value, ValueOrigin::Default)?
        } else if q.nothing_to_choose() {
            narrow(q, Value::List(Vec::new()), ValueOrigin::Default)?
        } else {
            narrow(q, interaction.ask(q, None)?, ValueOrigin::Input)?
        };
        resolved.insert(q.id.clone(), value);
    }
    let validated = weft_core::render::resolve_answers(questions, &resolved, eval)?;
    Ok(validated)
}

/// Split one frame's resolved answers into what a project stores:
///
/// - `given` — the inputs: every `supplied` answer (any input layer: flags,
///   answers file, JSON, wizard, preset, or a stored given answer on update),
///   kept even while its question is gated off so it returns when the gate
///   opens; plus answers to open questions with no default and no `seeded`
///   value, which were prompted for.
/// - `derived` — every other resolved value: defaults, computed values,
///   `seeded` values (include binds), and the value standing in for a
///   gated-off question.
///
/// An old render reproduces from `given` overlaid with `derived` (see
/// [`crate::state::State::rendered_answers`]); a new render feeds back only
/// `given`, so derived values follow the template and the other answers.
/// Secrets are in neither: they persist as source references.
pub fn provenance(
    questions: &[Question],
    supplied: &AnswerSet,
    seeded: &std::collections::BTreeSet<weft_core::AnswerId>,
    resolved: &AnswerSet,
    eval: &dyn ExprEval,
) -> (AnswerSet, AnswerSet) {
    let mut given = AnswerSet::new();
    let mut derived = AnswerSet::new();
    for q in questions {
        if matches!(q.kind, AnswerKind::Secret { .. }) {
            continue;
        }
        // Gates reference earlier answers, so evaluating them over the final
        // set gives what `gather` saw.
        let open = q
            .when
            .as_ref()
            .is_none_or(|w| eval.eval_bool(w, resolved).unwrap_or(true));
        let value = resolved.get(&q.id);
        if let Some(input) = supplied.get(&q.id) {
            match (open, value) {
                (true, Some(v)) => {
                    given.insert(q.id.clone(), v.clone());
                }
                (_, v) => {
                    given.insert(q.id.clone(), input.clone());
                    if let Some(v) = v {
                        derived.insert(q.id.clone(), v.clone());
                    }
                }
            }
            continue;
        }
        let Some(v) = value else { continue };
        let prompted = open && q.is_promptable() && q.default.is_none() && !seeded.contains(&q.id);
        if prompted {
            given.insert(q.id.clone(), v.clone());
        } else {
            derived.insert(q.id.clone(), v.clone());
        }
    }
    (given, derived)
}

/// The full pre-interactive layering: answers file → `--answer` flags →
/// answers JSON (inline, `@file`, `-` for stdin; later wins) — then the
/// selected presets' locks and constraints are enforced over the result.
pub fn layered_with_json(
    template: &Template,
    presets: &[String],
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
    answers_json: Option<&str>,
) -> Result<AnswerSet> {
    Ok(layered_with_json_full(template, presets, answers_file, answer_args, answers_json)?.answers)
}

/// [`layered_with_json`] with the lock/constraint info exposed.
pub fn layered_with_json_full(
    template: &Template,
    presets: &[String],
    answers_file: Option<&Utf8Path>,
    answer_args: &[String],
    answers_json: Option<&str>,
) -> Result<Layered> {
    // JSON is the highest user layer; the spec still applies over it.
    let mut specs = Vec::new();
    for preset in presets {
        specs.push(template.preset_spec(preset)?);
    }
    let spec = crate::preset::PresetSpec::merge(specs)?;

    let mut user = layered_user(template, answers_file, answer_args)?;
    if let Some(json_spec) = answers_json {
        let json = match json_spec {
            "-" => std::io::read_to_string(std::io::stdin())?,
            s if s.starts_with('@') => std::fs::read_to_string(&s[1..])
                .with_context(|| format!("reading answers JSON file {}", &s[1..]))?,
            s => s.to_owned(),
        };
        // Template-aware parse: keys may be namespaced (`<include>.<id>`).
        let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&json)
            .context("answers JSON must be an object of question id -> value")?;
        let mut set = AnswerSet::new();
        for (key, raw) in &object {
            let question = crate::compose::find_question(template, key)
                .with_context(|| format!("no question with id `{key}`"))?;
            set.insert(
                weft_core::AnswerId(key.clone()),
                json_to_value(&question.kind, key, raw)?,
            );
        }
        user.overlay(&set);
    }
    Ok(Layered {
        answers: spec.apply(&user)?,
        locked: spec.locked(),
        constraints: spec.constraints(),
    })
}

/// Kind-appropriate stand-in values for trial-evaluating expressions
/// (previews of defaults, validating gates) without real user input.
pub fn dummy_answers(questions: &[Question]) -> AnswerSet {
    let mut set = AnswerSet::new();
    for q in questions {
        let value = match &q.kind {
            AnswerKind::String => Value::String("example".into()),
            AnswerKind::Bool => Value::Bool(true),
            AnswerKind::Int => Value::Int(1),
            AnswerKind::Choice { choices } => {
                Value::String(choices.first().cloned().unwrap_or_default())
            }
            // All choices selected, so `x in components` membership tests and
            // joins exercise their truthy branch during trial evaluation.
            AnswerKind::MultiChoice { choices } => {
                Value::List(choices.iter().cloned().map(Value::String).collect())
            }
            AnswerKind::Secret { .. } => continue, // not visible to expressions
        };
        set.insert(q.id.clone(), value);
    }
    set
}

/// Coerce a JSON value into a weft value according to the question's kind.
/// The agent-facing counterpart of [`parse_answer_arg`].
pub fn json_to_value(kind: &AnswerKind, key: &str, raw: &serde_json::Value) -> Result<Value> {
    let bad = |expected: &str| anyhow::anyhow!("answer `{key}` expects {expected}, got {raw}");
    Ok(match kind {
        AnswerKind::String | AnswerKind::Choice { .. } => {
            Value::String(raw.as_str().ok_or_else(|| bad("a string"))?.to_owned())
        }
        AnswerKind::Bool => Value::Bool(raw.as_bool().ok_or_else(|| bad("a boolean"))?),
        AnswerKind::Int => Value::Int(raw.as_i64().ok_or_else(|| bad("an integer"))?),
        AnswerKind::MultiChoice { .. } => {
            let arr = raw.as_array().ok_or_else(|| bad("an array of strings"))?;
            let mut items = Vec::with_capacity(arr.len());
            for el in arr {
                let s = el
                    .as_str()
                    .ok_or_else(|| bad("an array of strings"))?
                    .to_owned();
                items.push(Value::String(s));
            }
            Value::List(items)
        }
        AnswerKind::Secret { .. } => {
            bail!("secret `{key}` cannot be supplied; it resolves through its declared source")
        }
    })
}

/// Parse a JSON object of answers (`{"project_name": "X", "use_db": true}`)
/// into a typed answer set, validating keys against the declared questions.
pub fn answers_from_json(questions: &[Question], json: &str) -> Result<AnswerSet> {
    let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json)
        .context("answers JSON must be an object of question id -> value")?;
    let mut set = AnswerSet::new();
    for (key, raw) in &object {
        let question = questions
            .iter()
            .find(|q| q.id.0 == *key)
            .with_context(|| format!("no question with id `{key}`"))?;
        set.insert(
            question.id.clone(),
            json_to_value(&question.kind, key, raw)?,
        );
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use weft_core::{AnswerId, StarlarkExpr};
    use weft_lang::StarlarkEval;

    use super::*;

    fn q(id: &str, default: Option<&str>, when: Option<&str>) -> Question {
        Question {
            id: AnswerId::from(id),
            kind: AnswerKind::String,
            prompt: None,
            description: None,
            example: None,
            default: default.map(|d| StarlarkExpr(d.to_owned())),
            when: when.map(|w| StarlarkExpr(w.to_owned())),
            computed: false,
            section: None,
            narrowing: Default::default(),
        }
    }

    fn set(pairs: &[(&str, &str)]) -> AnswerSet {
        pairs
            .iter()
            .map(|(k, v)| (AnswerId::from(*k), Value::String((*v).to_owned())))
            .collect()
    }

    fn ids(s: &AnswerSet) -> Vec<&str> {
        s.iter().map(|(k, _)| k.0.as_str()).collect()
    }

    #[test]
    fn supplied_and_prompted_answers_are_given_defaults_and_binds_derived() {
        let questions = vec![
            q("name", None, None),                 // prompted
            q("slug", Some("name.lower()"), None), // default
            q("owner", None, None),                // seeded by a bind
            q("title", Some("'x'"), None),         // supplied though defaulted
        ];
        let resolved = set(&[
            ("name", "Acme"),
            ("slug", "acme"),
            ("owner", "ops"),
            ("title", "Custom"),
        ]);
        let seeded: BTreeSet<AnswerId> = [AnswerId::from("owner")].into();
        let (given, derived) = provenance(
            &questions,
            &set(&[("title", "Custom")]),
            &seeded,
            &resolved,
            &StarlarkEval,
        );
        assert_eq!(ids(&given), vec!["name", "title"]);
        assert_eq!(ids(&derived), vec!["owner", "slug"]);
    }

    #[test]
    fn a_supplied_answer_behind_a_closed_gate_is_kept_with_its_stand_in() {
        let mut docker = q("use_docker", None, None);
        docker.kind = AnswerKind::Bool;
        let questions = vec![docker, q("image", Some("'python'"), Some("use_docker"))];
        let mut resolved = set(&[("image", "python")]);
        resolved.insert(AnswerId::from("use_docker"), Value::Bool(false));
        let mut supplied = set(&[("image", "custom")]);
        supplied.insert(AnswerId::from("use_docker"), Value::Bool(false));
        let (given, derived) = provenance(
            &questions,
            &supplied,
            &BTreeSet::new(),
            &resolved,
            &StarlarkEval,
        );
        // The input survives for when the gate opens again; the render used
        // the default standing in for it.
        assert_eq!(
            given.get(&AnswerId::from("image")),
            Some(&Value::String("custom".into()))
        );
        assert_eq!(
            derived.get(&AnswerId::from("image")),
            Some(&Value::String("python".into()))
        );
        let mut rendered = given.clone();
        rendered.overlay(&derived);
        assert_eq!(rendered, resolved);
    }
}
