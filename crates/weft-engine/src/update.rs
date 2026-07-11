//! `weft update`: re-render with the pinned inputs, re-render with the new
//! template state, and 3-way merge the difference onto the user's tree.
//! Conflicts get markers plus a summary; nothing is silently clobbered.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::merge::merge3;
use weft_core::{AnswerId, AnswerSet, FileEntry, Question, Value};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::state::State;
use crate::tasks::{self, ChangeSet};
use crate::template::Template;
use crate::{answers, fsio, secrets};

pub struct UpdateOptions {
    pub dest: Utf8PathBuf,
    /// Print the plan without touching anything.
    pub dry_run: bool,
    /// Use this template path instead of the one recorded in state.
    pub template_override: Option<Utf8PathBuf>,
    pub skip_tasks: bool,
}

#[derive(Debug, Default)]
pub struct UpdateReport {
    pub conflicts: Vec<Utf8PathBuf>,
    pub notes: Vec<String>,
    pub written: usize,
}

pub fn run(opts: &UpdateOptions, interaction: &mut dyn Interaction) -> Result<UpdateReport> {
    let state = State::load(&opts.dest)?;
    let template_path = opts
        .template_override
        .clone()
        .unwrap_or_else(|| Utf8PathBuf::from(&state.state.template));
    let template = Template::load(&template_path)?;
    let eval = StarlarkEval;

    for id in &state.state.base {
        if !template.id_to_name.contains_key(id) {
            bail!(
                "pinned patch {} no longer exists in `{template_path}`; \
                 the template history was rewritten and this project can't be updated from it",
                id.short()
            );
        }
    }

    // Secrets resolve through the *stored* references — never re-prompted
    // while the reference still resolves.
    let presolved_secrets = resolve_stored_secrets(&template, &state, interaction)?;

    let mut old_answers = state.answers.clone();
    old_answers.overlay(&presolved_secrets);

    // New questions (added to the template since scaffold) get defaults or
    // prompts; existing answers are reused as-is.
    let new_answers = answers::gather(
        &template,
        &state.answers,
        &presolved_secrets,
        &eval,
        interaction,
    )?;

    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| state.state.base.contains(&p.id))
        .cloned()
        .collect();
    let old_render = weft_core::render::render(&base_patches, &old_answers, &eval)
        .context("re-rendering pinned inputs")?;
    let new_render = weft_core::render::render(&template.patches, &new_answers, &eval)
        .context("rendering new template state")?;

    let user_tree = fsio::read_tree(&opts.dest)?;

    let mut report = UpdateReport::default();
    let mut actions: Vec<(Utf8PathBuf, Action)> = Vec::new();

    let mut all_paths: BTreeSet<&Utf8PathBuf> = BTreeSet::new();
    all_paths.extend(old_render.paths());
    all_paths.extend(new_render.paths());
    all_paths.extend(user_tree.paths());

    for path in all_paths {
        let base = old_render.get(path);
        let ours = user_tree.get(path);
        let theirs = new_render.get(path);
        if let Some(action) = plan_file(base, ours, theirs) {
            actions.push((path.clone(), action));
        }
    }

    // Task planning happens against what changed between the two renders.
    let changed_paths: BTreeSet<Utf8PathBuf> = old_render
        .paths()
        .chain(new_render.paths())
        .filter(|p| old_render.get(p) != new_render.get(p))
        .cloned()
        .collect();
    let changed_answers: BTreeSet<AnswerId> = old_answers
        .iter()
        .map(|(id, _)| id)
        .chain(new_answers.iter().map(|(id, _)| id))
        .filter(|id| {
            // Secret values are opaque; only presence changes count.
            match (old_answers.get(id), new_answers.get(id)) {
                (Some(Value::Secret(_)), Some(Value::Secret(_))) => false,
                (a, b) => a != b,
            }
        })
        .cloned()
        .collect();
    let changes = ChangeSet {
        paths: changed_paths,
        answers: changed_answers,
    };
    let task_plan = tasks::plan(
        &template.manifest.tasks,
        &new_answers,
        &eval,
        Some(&changes),
    )?;

    if opts.dry_run {
        if actions.is_empty() {
            eprintln!("dry run: tree already up to date");
        }
        for (path, action) in &actions {
            eprintln!("dry run: {} {path}", action.verb());
        }
        for task in &task_plan {
            eprintln!("dry run: would run task `{}`", task.id);
        }
        return Ok(report);
    }

    for (path, action) in actions {
        match action {
            Action::Write(entry) => {
                fsio::write_file(&opts.dest, &path, &entry)?;
                report.written += 1;
            }
            Action::Delete => {
                std::fs::remove_file(opts.dest.join(&path))
                    .with_context(|| format!("deleting {path}"))?;
                report.notes.push(format!("deleted {path}"));
            }
            Action::Conflict(entry, n) => {
                fsio::write_file(&opts.dest, &path, &entry)?;
                report
                    .notes
                    .push(format!("{n} conflict(s) in {path} — resolve the markers"));
                report.conflicts.push(path);
            }
            Action::Note(msg) => report.notes.push(msg),
        }
    }

    // Pin the new template state.
    let secret_specs = collect_secret_specs(&template, &state, &new_answers);
    State::new(
        template_path
            .canonicalize_utf8()
            .unwrap_or(template_path)
            .to_string(),
        template.patches.iter().map(|p| p.id).collect(),
        new_render.hash(),
        &new_answers,
        &secret_specs,
    )
    .save(&opts.dest)?;

    if !opts.skip_tasks && report.conflicts.is_empty() {
        tasks::run(&task_plan, &opts.dest)?;
    } else if !task_plan.is_empty() && !report.conflicts.is_empty() {
        report.notes.push(format!(
            "skipped {} task(s) because of conflicts; re-run them after resolving",
            task_plan.len()
        ));
    }

    Ok(report)
}

enum Action {
    Write(FileEntry),
    Delete,
    Conflict(FileEntry, usize),
    Note(String),
}

impl Action {
    fn verb(&self) -> &'static str {
        match self {
            Action::Write(_) => "update",
            Action::Delete => "delete",
            Action::Conflict(..) => "conflict in",
            Action::Note(_) => "note:",
        }
    }
}

/// Decide what to do with one path given (base, ours, theirs).
fn plan_file(
    base: Option<&FileEntry>,
    ours: Option<&FileEntry>,
    theirs: Option<&FileEntry>,
) -> Option<Action> {
    if base == theirs || ours == theirs {
        return None; // template unchanged, or both already agree
    }
    if ours == base {
        // User untouched: take the template side wholesale.
        return Some(match theirs {
            Some(entry) => Action::Write(entry.clone()),
            None => Action::Delete,
        });
    }
    // Both sides diverged from base.
    match (ours, theirs) {
        (None, Some(_)) => Some(Action::Note(
            "template modified a file you deleted; kept it deleted".to_owned(),
        )),
        (Some(_), None) => Some(Action::Note(
            "template deleted a file you modified; kept your version".to_owned(),
        )),
        (Some(o), Some(t)) => {
            let base_text = base.map(|e| e.content.as_str()).unwrap_or("");
            let outcome = merge3(base_text, &o.content, &t.content);
            let mode = if base.map(|e| e.mode) == Some(o.mode) {
                t.mode
            } else {
                o.mode
            };
            let entry = FileEntry {
                content: outcome.text,
                mode,
            };
            if outcome.conflicts == 0 {
                Some(Action::Write(entry))
            } else {
                Some(Action::Conflict(entry, outcome.conflicts))
            }
        }
        (None, None) => unreachable!("path came from some tree"),
    }
}

fn resolve_stored_secrets(
    template: &Template,
    state: &State,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let mut resolved = AnswerSet::new();
    for (id, spec_str) in &state.secrets {
        let spec: weft_core::SecretSpec = spec_str
            .parse()
            .with_context(|| format!("invalid stored secret reference for `{id}`"))?;
        let question = template
            .manifest
            .questions
            .iter()
            .find(|q| q.id == *id)
            .cloned()
            .unwrap_or_else(|| synthetic_secret_question(id, &spec));
        let value = secrets::resolve(&question, &spec, interaction)?;
        resolved.insert(id.clone(), Value::Secret(value));
    }
    Ok(resolved)
}

/// A question may have been removed from the manifest while the old render
/// still needs its stored secret; fabricate enough of a question to resolve.
fn synthetic_secret_question(id: &AnswerId, spec: &weft_core::SecretSpec) -> Question {
    Question {
        id: id.clone(),
        kind: weft_core::AnswerKind::Secret {
            source: spec.clone(),
        },
        prompt: None,
        description: None,
        example: None,
        default: None,
        when: None,
    }
}

fn collect_secret_specs(
    template: &Template,
    old_state: &State,
    new_answers: &AnswerSet,
) -> BTreeMap<AnswerId, String> {
    let mut specs = old_state.secrets.clone();
    for q in &template.manifest.questions {
        if let weft_core::AnswerKind::Secret { source } = &q.kind {
            if new_answers.contains(&q.id) {
                specs
                    .entry(q.id.clone())
                    .or_insert_with(|| source.to_string());
            }
        }
    }
    specs
}

/// CLI-facing summary printer. Returns an error when conflicts remain so the
/// process exits nonzero.
pub fn finish(report: &UpdateReport) -> Result<()> {
    for note in &report.notes {
        eprintln!("{note}");
    }
    if report.conflicts.is_empty() {
        eprintln!("update complete ({} file(s) written)", report.written);
        Ok(())
    } else {
        bail!(
            "update finished with conflicts in {} file(s): {}",
            report.conflicts.len(),
            report
                .conflicts
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Small helper for tests/e2e: does `dir` need an update at all?
pub fn is_up_to_date(dest: &Utf8Path) -> Result<bool> {
    let state = State::load(dest)?;
    let template = Template::load(Utf8Path::new(&state.state.template))?;
    let pinned: BTreeSet<_> = state.state.base.iter().copied().collect();
    let current: BTreeSet<_> = template.patches.iter().map(|p| p.id).collect();
    Ok(pinned == current)
}
