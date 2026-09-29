use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use weft_core::render::{narrow, same_answer, ExprEval, ValueOrigin};
use weft_core::{AnswerId, AnswerKind, AnswerSet, Question, Value};

use crate::interact::Interaction;
use crate::secrets;
use crate::template::Template;

/// The layered pre-interactive answers, plus the multichoices the selected
/// presets constrain rather than answer.
pub struct Layered {
    pub answers: AnswerSet,
    pub constraints: PresetConstraints,
}

/// Multichoices the selected presets constrain: `fixed` choices are always
/// selected, `blocked` ones never offered.
#[derive(Default)]
pub struct PresetConstraints {
    /// id → (fixed, blocked).
    by_id: BTreeMap<AnswerId, (Vec<String>, Vec<String>)>,
    /// The constrained multichoices no input answered: the layered answers
    /// hold their fixed choices as a starting selection, which
    /// [`gather_reviewed`] offers rather than takes.
    prefilled: BTreeSet<AnswerId>,
}

impl PresetConstraints {
    /// `q` as a person is asked it: the presets' blocked choices hidden, and
    /// their fixed ones pinned like a template's.
    fn shown<'q>(&self, q: &'q Question) -> Cow<'q, Question> {
        let Some((fixed, blocked)) = self.by_id.get(&q.id) else {
            return Cow::Borrowed(q);
        };
        let mut shown = q.clone();
        if let AnswerKind::MultiChoice { choices } = &mut shown.kind {
            choices.retain(|c| !blocked.contains(c));
        }
        for f in fixed {
            if !shown.narrowing.fixed.contains(f) {
                shown.narrowing.fixed.push(f.clone());
            }
        }
        Cow::Owned(shown)
    }
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
    Ok(layered)
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
    Ok(walk(
        questions,
        provided,
        presolved_secrets,
        None,
        eval,
        interaction,
    )?
    .answers)
}

/// What [`gather_reviewed`] resolved, and which of it a person typed.
pub struct Gathered {
    /// Every question's answer, as [`gather`] resolves it.
    pub answers: AnswerSet,
    /// The answers typed at a prompt. A default accepted as offered is not
    /// among them, so it stays derived and keeps following the template.
    pub entered: AnswerSet,
}

/// [`gather`] for a person choosing answers from scratch (`weft new`,
/// `weft session new`, `weft patch amend`): every open question no input
/// answered is asked, its default offered rather than taken. A multichoice
/// a preset constrains is asked with the blocked choices hidden and the
/// fixed ones pinned, starting from the preset's selection. `provided` is
/// the root frame's share of the layered answers.
///
/// [`NonInteractive`](crate::interact::NonInteractive) takes every offered
/// value, so an unattended run resolves exactly as [`gather`] does.
pub fn gather_reviewed(
    template: &Template,
    provided: &AnswerSet,
    constraints: &PresetConstraints,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Gathered> {
    let questions = &template.manifest.questions;
    let none = AnswerSet::new();
    walk(
        questions,
        provided,
        &none,
        Some(constraints),
        eval,
        interaction,
    )
}

/// The question walk behind [`gather`] and [`gather_reviewed`]. `review`
/// holds the presets' constraints when a person reviews every open question.
fn walk(
    questions: &[Question],
    provided: &AnswerSet,
    presolved_secrets: &AnswerSet,
    review: Option<&PresetConstraints>,
    eval: &dyn ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<Gathered> {
    let mut resolved = AnswerSet::new();
    let mut entered = AnswerSet::new();
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
        // A preset's starting selection answers the question for `gather`,
        // and is only an offer to a person reviewing.
        let prefilled = review.is_some_and(|r| r.prefilled.contains(&q.id));
        let value = match provided.get(&q.id) {
            Some(v) if !prefilled => narrow(q, v.clone(), ValueOrigin::Input)?,
            start => {
                let offered = if let Some(v) = start {
                    Some(narrow(q, v.clone(), ValueOrigin::Input)?)
                } else if let Some(default) = &q.default {
                    let value = eval
                        .eval(default, &resolved)
                        .with_context(|| format!("evaluating default of question `{}`", q.id))?;
                    Some(narrow(q, value, ValueOrigin::Default)?)
                } else if q.nothing_to_choose() {
                    Some(narrow(q, Value::List(Vec::new()), ValueOrigin::Default)?)
                } else {
                    None
                };
                let shown = review.map_or(Cow::Borrowed(q), |r| r.shown(q));
                match offered {
                    Some(v) if review.is_none() || !shown.is_promptable() => v,
                    offered => {
                        let answer = interaction.ask(&shown, offered.as_ref())?;
                        let answer = narrow(q, answer, ValueOrigin::Input)?;
                        if !offered.is_some_and(|o| same_answer(&o, &answer)) {
                            entered.insert(q.id.clone(), answer.clone());
                        }
                        answer
                    }
                }
            }
        };
        resolved.insert(q.id.clone(), value);
    }
    let answers = weft_core::render::resolve_answers(questions, &resolved, eval)?;
    Ok(Gathered { answers, entered })
}

/// Split one frame's resolved answers into what a project stores:
///
/// - `given` — the inputs: every `supplied` answer (any input layer: flags,
///   answers file, JSON, a preset, an answer typed at a prompt, or a stored
///   given answer on update), kept even while its question is gated off so
///   it returns when the gate opens; plus answers to open questions with no
///   default and no `seeded` value, which were prompted for.
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

/// [`layered_with_json`] with the presets' multichoice constraints exposed.
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
    let by_id = spec.constraints();
    // `apply` starts a constrained multichoice no input answered from its
    // fixed choices.
    let prefilled = by_id
        .iter()
        .filter(|(id, (fixed, _))| !fixed.is_empty() && !user.contains(id))
        .map(|(id, _)| id.clone())
        .collect();
    Ok(Layered {
        answers: spec.apply(&user)?,
        constraints: PresetConstraints { by_id, prefilled },
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

    /// Answers prompts in order and records each question as shown, with
    /// the value it offered.
    #[derive(Default)]
    struct Scripted {
        replies: Vec<Value>,
        asked: Vec<(Question, Option<Value>)>,
    }

    impl Interaction for Scripted {
        fn ask(&mut self, question: &Question, default: Option<&Value>) -> Result<Value> {
            self.asked.push((question.clone(), default.cloned()));
            Ok(self.replies.remove(0))
        }

        fn ask_secret(&mut self, _: &Question) -> Result<weft_core::SecretValue> {
            unreachable!("no secret questions here")
        }

        fn confirm(&mut self, _: &str, _: bool) -> Result<bool> {
            unreachable!("gathering confirms nothing")
        }
    }

    fn docker_questions() -> Vec<Question> {
        let mut use_docker = q("use_docker", Some("True"), None);
        use_docker.kind = AnswerKind::Bool;
        vec![
            q("name", None, None),
            q("slug", Some("name.lower()"), None),
            use_docker,
            q("registry", Some("'docker.io'"), Some("use_docker")),
        ]
    }

    #[test]
    fn review_offers_every_default_and_enters_only_changed_answers() {
        let mut person = Scripted {
            replies: vec![
                Value::String("Acme".into()),
                Value::String("acme".into()), // the offered slug, accepted
                Value::Bool(false),           // the default turned down
            ],
            ..Default::default()
        };
        let none = AnswerSet::new();
        let reviewed = walk(
            &docker_questions(),
            &none,
            &none,
            Some(&PresetConstraints::default()),
            &StarlarkEval,
            &mut person,
        )
        .unwrap();
        let offered: Vec<_> = person
            .asked
            .iter()
            .map(|(q, offered)| (q.id.0.as_str(), offered.clone()))
            .collect();
        // `registry` is never asked: turning Docker down closed its gate.
        assert_eq!(
            offered,
            vec![
                ("name", None),
                ("slug", Some(Value::String("acme".into()))),
                ("use_docker", Some(Value::Bool(true))),
            ]
        );
        assert_eq!(ids(&reviewed.entered), vec!["name", "use_docker"]);
    }

    #[test]
    fn an_unattended_review_resolves_like_gather() {
        let provided = set(&[("name", "Acme")]);
        let none = AnswerSet::new();
        let questions = docker_questions();
        let reviewed = walk(
            &questions,
            &provided,
            &none,
            Some(&PresetConstraints::default()),
            &StarlarkEval,
            &mut crate::interact::NonInteractive,
        )
        .unwrap();
        let gathered = walk(
            &questions,
            &provided,
            &none,
            None,
            &StarlarkEval,
            &mut crate::interact::NonInteractive,
        )
        .unwrap();
        assert_eq!(reviewed.answers, gathered.answers);
        assert!(reviewed.entered.is_empty());
    }

    #[test]
    fn review_asks_a_constrained_multichoice_from_the_preset_selection() {
        let list = |items: &[&str]| {
            Value::List(items.iter().map(|s| Value::String((*s).into())).collect())
        };
        let mut features = q("features", None, None);
        features.kind = AnswerKind::MultiChoice {
            choices: vec!["lint".into(), "docs".into(), "exp".into()],
        };
        let questions = vec![features];
        let id = AnswerId::from("features");
        // What `PresetSpec::apply` leaves for a constrained multichoice no
        // input answered: its fixed choices.
        let mut provided = AnswerSet::new();
        provided.insert(id.clone(), list(&["lint"]));
        let constraints = PresetConstraints {
            by_id: [(id.clone(), (vec!["lint".into()], vec!["exp".into()]))].into(),
            prefilled: [id.clone()].into(),
        };
        let none = AnswerSet::new();
        let mut person = Scripted {
            replies: vec![list(&["docs", "lint"])],
            ..Default::default()
        };
        let reviewed = walk(
            &questions,
            &provided,
            &none,
            Some(&constraints),
            &StarlarkEval,
            &mut person,
        )
        .unwrap();
        let (shown, offered) = &person.asked[0];
        assert_eq!(
            shown.kind,
            AnswerKind::MultiChoice {
                choices: vec!["lint".into(), "docs".into()]
            }
        );
        assert_eq!(shown.narrowing.fixed, vec!["lint".to_owned()]);
        assert_eq!(offered, &Some(list(&["lint"])));
        assert_eq!(reviewed.entered.get(&id), Some(&list(&["docs", "lint"])));

        // Unreviewed, the preset's selection is the answer.
        let gathered = walk(
            &questions,
            &provided,
            &none,
            None,
            &StarlarkEval,
            &mut crate::interact::NonInteractive,
        )
        .unwrap();
        assert_eq!(gathered.answers.get(&id), Some(&list(&["lint"])));
    }
}
