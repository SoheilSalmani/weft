//! `weft update`: re-render with the pinned inputs, re-render with the new
//! template state, and 3-way merge the difference onto the user's tree.
//! Conflicts get markers plus a summary; nothing is silently clobbered.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::merge::merge3;
use weft_core::{AnswerId, AnswerSet, FileEntry, Question, Value};
use weft_lang::StarlarkEval;

use crate::hooks::{self, ChangeSet};
use crate::interact::Interaction;
use crate::state::State;
use crate::template::Template;
use crate::{answers, compose, fsio, secrets};

pub struct UpdateOptions {
    pub dest: Utf8PathBuf,
    /// Print the plan without touching anything.
    pub dry_run: bool,
    /// Use this template path instead of the one recorded in state.
    pub template_override: Option<Utf8PathBuf>,
    pub skip_tasks: bool,
    /// Instances `(include, key)` excluded from the *new* side — the
    /// `weft instance remove` path: their files get template-deleted
    /// semantics and they are unpinned from state.
    pub drop_instances: Vec<(String, String)>,
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
    for inst in &state.instances {
        let inc = template.include(&inst.include).with_context(|| {
            format!(
                "instance `{}` was scaffolded from include `{}`, which no longer exists in \
                 `{template_path}`",
                inst.key, inst.include
            )
        })?;
        for id in &inst.base {
            if !inc.template.id_to_name.contains_key(id) {
                bail!(
                    "instance `{}`: pinned child patch {} no longer exists in include `{}`; \
                     the child template history was rewritten",
                    inst.key,
                    id.short(),
                    inst.include
                );
            }
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

    // Reconstruct the old composed tree: pinned parent base + per-instance
    // pinned child bases, all with the stored answers. Then produce the new
    // composed tree from the current template state.
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| state.state.base.contains(&p.id))
        .cloned()
        .collect();

    let mut old_parts: Vec<compose::ComposedPart> = Vec::new();
    let mut new_parts: Vec<compose::ComposedPart> = Vec::new();
    for inst in &state.instances {
        let inc = template.include(&inst.include).expect("checked above");
        // Old side: stored answers + re-resolved stored child secrets.
        let child_secrets = resolve_instance_secrets(&inc.template, inst, interaction)?;
        let mut old_child_answers = inst.answers.clone();
        old_child_answers.overlay(&child_secrets);
        old_parts.push(compose::ComposedPart {
            template: &inc.template,
            patches: inc
                .template
                .patches
                .iter()
                .filter(|p| inst.base.contains(&p.id))
                .cloned()
                .collect(),
            instance: compose::ResolvedInstance {
                include: inst.include.clone(),
                key: inst.key.clone(),
                mount: Utf8PathBuf::from(&inst.mount),
                answers: old_child_answers,
            },
        });
        // New side: binds re-seed, stored answers win, new child questions
        // get defaults/prompts (same policy as the parent's answers). Dropped
        // instances (`weft instance remove`) have no new side at all.
        if opts
            .drop_instances
            .iter()
            .any(|(i, k)| *i == inst.include && *k == inst.key)
        {
            continue;
        }
        let new_child_answers = compose::resolve_instance_answers(
            inc,
            &inst.key,
            &new_answers,
            &inst.answers,
            &eval,
            interaction,
        )?;
        new_parts.push(compose::ComposedPart {
            template: &inc.template,
            patches: inc.template.patches.clone(),
            instance: compose::ResolvedInstance {
                include: inst.include.clone(),
                key: inst.key.clone(),
                mount: compose::mount_path(&inc.decl.path, &inst.key)?,
                answers: new_child_answers,
            },
        });
    }

    let old_render = compose::render_composed(&base_patches, &old_answers, &old_parts, &eval)
        .context("re-rendering pinned inputs")?;
    let new_render = compose::render_composed(&template.patches, &new_answers, &new_parts, &eval)
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
    // Pre-hooks always run on update (they're guards); post-hooks re-fire only
    // when one of their inputs changed.
    let collected = hooks::collect(&template, &new_answers, &eval)?;
    let post_plan = hooks::fire_on_update(&collected.post, Some(&changes), &eval, &new_answers)?;

    // Child hook plans, one per instance: inputs are evaluated against the
    // child-relative change set (paths under the mount, stripped; the child
    // answers that changed).
    struct ChildPlan<'t> {
        mount: Utf8PathBuf,
        answers: AnswerSet,
        pre: Vec<&'t weft_core::Hook>,
        post: Vec<&'t weft_core::Hook>,
        include: String,
    }
    let mut child_plans: Vec<ChildPlan> = Vec::new();
    for new in &new_parts {
        // Dropped instances make the two sides diverge, so match by identity.
        let old = old_parts
            .iter()
            .find(|o| {
                o.instance.include == new.instance.include && o.instance.key == new.instance.key
            })
            .expect("every new part has an old counterpart");
        let child_collected = hooks::collect(new.template, &new.instance.answers, &eval)?;
        let changed_child_answers: BTreeSet<AnswerId> = old
            .instance
            .answers
            .iter()
            .map(|(id, _)| id)
            .chain(new.instance.answers.iter().map(|(id, _)| id))
            .filter(
                |id| match (old.instance.answers.get(id), new.instance.answers.get(id)) {
                    (Some(Value::Secret(_)), Some(Value::Secret(_))) => false,
                    (a, b) => a != b,
                },
            )
            .cloned()
            .collect();
        let child_changes =
            compose::child_changes(&changes, &new.instance.mount, changed_child_answers);
        let post = hooks::fire_on_update(
            &child_collected.post,
            Some(&child_changes),
            &eval,
            &new.instance.answers,
        )?;
        child_plans.push(ChildPlan {
            mount: new.instance.mount.clone(),
            answers: new.instance.answers.clone(),
            pre: child_collected.pre,
            post,
            include: new.instance.include.clone(),
        });
    }

    if opts.dry_run {
        if actions.is_empty() {
            eprintln!("dry run: tree already up to date");
        }
        for (path, action) in &actions {
            eprintln!("dry run: {} {path}", action.verb());
        }
        for hook in &collected.pre {
            eprintln!("dry run: would run pre-hook `{}` ({})", hook.id, hook.label);
        }
        for plan in &child_plans {
            for hook in &plan.pre {
                eprintln!(
                    "dry run: would run pre-hook `{}` ({}) [include {}]",
                    hook.id, hook.label, plan.include
                );
            }
            for hook in &plan.post {
                eprintln!(
                    "dry run: would run post-hook `{}` ({}) [include {}]",
                    hook.id, hook.label, plan.include
                );
            }
        }
        for hook in &post_plan {
            eprintln!(
                "dry run: would run post-hook `{}` ({})",
                hook.id, hook.label
            );
        }
        return Ok(report);
    }

    // Pre-hooks run before touching files.
    if !opts.skip_tasks {
        hooks::run(&collected.pre, &opts.dest, &new_answers, &eval)
            .context("a pre-render hook failed")?;
        for plan in &child_plans {
            hooks::run(&plan.pre, &opts.dest, &plan.answers, &eval).with_context(|| {
                format!("a pre-render hook of include `{}` failed", plan.include)
            })?;
        }
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

    // Pin the new template state, incl. re-pinned include instances.
    let secret_specs = collect_secret_specs(&template, &state, &new_answers);
    let instance_states: Vec<crate::state::InstanceState> = new_parts
        .iter()
        .map(|p| crate::state::InstanceState {
            include: p.instance.include.clone(),
            key: p.instance.key.clone(),
            mount: p.instance.mount.to_string(),
            base: p.patches.iter().map(|patch| patch.id).collect(),
            answers: State::plain_answers(&p.instance.answers),
            secrets: crate::new::secret_specs(&p.template.manifest.questions, &p.instance.answers),
        })
        .collect();
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
    .with_instances(instance_states)
    .save(&opts.dest)?;

    let skipped_child_posts: usize = child_plans.iter().map(|p| p.post.len()).sum();
    if !opts.skip_tasks && report.conflicts.is_empty() {
        // Child post-hooks first (inside their mounts), then the parent's.
        for plan in &child_plans {
            let cwd = opts.dest.join(&plan.mount);
            hooks::run(&plan.post, &cwd, &plan.answers, &eval)?;
        }
        hooks::run(&post_plan, &opts.dest, &new_answers, &eval)?;
    } else if (!post_plan.is_empty() || skipped_child_posts > 0) && !report.conflicts.is_empty() {
        report.notes.push(format!(
            "skipped {} post-hook(s) because of conflicts; re-run them after resolving",
            post_plan.len() + skipped_child_posts
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

/// Re-resolve one instance's stored child secret references.
fn resolve_instance_secrets(
    child: &Template,
    inst: &crate::state::InstanceState,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let mut resolved = AnswerSet::new();
    for (id, spec_str) in &inst.secrets {
        let spec: weft_core::SecretSpec = spec_str.parse().with_context(|| {
            format!(
                "invalid stored secret reference for `{id}` (instance `{}`)",
                inst.key
            )
        })?;
        let question = child
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
        computed: false,
        section: None,
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
