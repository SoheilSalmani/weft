//! `weft update`: re-render with the pinned inputs, re-render with the new
//! template state, and 3-way merge the difference onto the user's tree.
//! Conflicts get markers plus a summary; nothing is silently clobbered.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::merge::merge3;
use weft_core::{AnswerId, AnswerSet, FileEntry, Question, Value};
use weft_lang::StarlarkEval;

use crate::hooks::{self, ChangeSet};
use crate::interact::Interaction;
use crate::state::{State, StoredSource};
use crate::template::Template;
use crate::{answers, compose, fsio, secrets};

pub struct UpdateOptions {
    pub dest: Utf8PathBuf,
    /// Print the plan without touching anything.
    pub dry_run: bool,
    /// Use this template path instead of the one recorded in state.
    pub template_override: Option<Utf8PathBuf>,
    /// What to record as the template source afterwards (a remote ref the
    /// CLI resolved `template_override` from). `None` = the template path.
    pub stored: Option<StoredSource>,
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

pub fn run(
    opts: &UpdateOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<UpdateReport> {
    let state = State::load(&opts.dest)?;
    let template_path = opts
        .template_override
        .clone()
        .unwrap_or_else(|| Utf8PathBuf::from(&state.state.template));
    let template = Template::load_with(&template_path, resolver)?;
    let eval = StarlarkEval;

    // The self-contained base: if the project stored its base patch bodies,
    // reconstruct the merge base from those (robust to template history
    // rewrites) — the root frame and every instance the snapshot holds.
    // Older projects have no snapshot and fall back to matching pinned ids
    // against the current template (which bails on a rewrite); so does an
    // instance the snapshot has no entry for (added since it was written).
    let snapshot = crate::state::BaseSnapshot::load(&opts.dest)?.filter(|s| !s.parent.is_empty());
    if snapshot.is_none() {
        for id in &state.state.base {
            if !template.id_to_name.contains_key(id) {
                bail!(
                    "pinned patch {} no longer exists in `{template_path}`; the template \
                     history was rewritten and this project has no base snapshot to update \
                     from (re-scaffold, or update once against the un-rewritten template to \
                     record one)",
                    id.short()
                );
            }
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
        if snapshot
            .as_ref()
            .is_some_and(|s| s.instance(&inst.include, &inst.key).is_some())
        {
            continue;
        }
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
    let base_patches: Vec<_> = match &snapshot {
        // Self-contained: the base is exactly what the project stored,
        // independent of the template's current patch ids.
        Some(snap) => snap.parent_patches(),
        None => template
            .patches
            .iter()
            .filter(|p| state.state.base.contains(&p.id))
            .cloned()
            .collect(),
    };

    let mut old_parts: Vec<compose::ComposedPart> = Vec::new();
    let mut new_parts: Vec<compose::ComposedPart> = Vec::new();
    for inst in &state.instances {
        let inc = template.include(&inst.include).expect("checked above");
        // Old side: stored answers + re-resolved stored child secrets.
        // Nested (grandchild) instances re-derive their answers from
        // binds/defaults on both sides (nested answers are not pinned yet);
        // their patches come from the snapshot where it has them.
        let child_secrets = resolve_instance_secrets(&inc.template, inst, interaction)?;
        let mut old_child_answers = inst.answers.clone();
        old_child_answers.overlay(&child_secrets);
        let mut old_children = compose::resolve_child_parts(
            &inc.template,
            &old_child_answers,
            compose::SecretMode::Resolve,
            &eval,
            interaction,
        )?;
        let old_patches = match snapshot
            .as_ref()
            .and_then(|s| s.instance(&inst.include, &inst.key))
        {
            Some(stored) => {
                pin_stored_children(&mut old_children, stored);
                stored.patches()
            }
            None => inc
                .template
                .patches
                .iter()
                .filter(|p| inst.base.contains(&p.id))
                .cloned()
                .collect(),
        };
        old_parts.push(compose::ComposedPart {
            template: &inc.template,
            patches: old_patches,
            instance: compose::ResolvedInstance {
                include: inst.include.clone(),
                key: inst.key.clone(),
                mount: Utf8PathBuf::from(&inst.mount),
                answers: old_child_answers,
                repeat: inc.decl.repeat,
            },
            children: old_children,
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
            &weft_core::AnswerSet::new(),
            &eval,
            interaction,
        )?;
        let new_children = compose::resolve_child_parts(
            &inc.template,
            &new_child_answers,
            compose::SecretMode::Resolve,
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
                repeat: inc.decl.repeat,
            },
            children: new_children,
        });
    }

    let old_render = compose::render_composed(&base_patches, &old_answers, &old_parts, &eval)
        .context("re-rendering pinned inputs")?;
    if snapshot.is_some() && old_render.hash() != state.state.tree_hash {
        eprintln!(
            "warning: `.weft/base.json` no longer matches the recorded tree hash; it may \
             have been edited"
        );
    }
    let new_render = compose::render_composed(&template.patches, &new_answers, &new_parts, &eval)
        .context("rendering new template state")?;

    // Read only the paths the template ever touched (old or new render):
    // everything else in the project — user files, `node_modules/`,
    // virtualenvs — is irrelevant to the merge and may be binary.
    let mut user_tree = weft_core::Tree::new();
    let mut tracked: BTreeSet<&Utf8PathBuf> = BTreeSet::new();
    tracked.extend(old_render.paths());
    tracked.extend(new_render.paths());
    for path in tracked {
        if let Some(entry) = fsio::read_file(&opts.dest, path)? {
            user_tree.insert(path.clone(), entry);
        }
    }

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

    // Task planning happens against what changed between the two renders:
    // root-relative paths, plus the changed answers of every frame (the
    // root's, and each instance's child answers; nested frames re-derive
    // from binds on both sides and report no answer changes).
    let changed_paths: BTreeSet<Utf8PathBuf> = old_render
        .paths()
        .chain(new_render.paths())
        .filter(|p| old_render.get(p) != new_render.get(p))
        .cloned()
        .collect();
    let changes = ChangeSet {
        paths: changed_paths,
        answers: changed_answers(&old_answers, &new_answers),
    };
    let mut changed_by_frame: BTreeMap<Vec<(String, String)>, BTreeSet<AnswerId>> = BTreeMap::new();
    changed_by_frame.insert(Vec::new(), changes.answers.clone());
    for new in &new_parts {
        // Dropped instances make the two sides diverge, so match by identity.
        let old = old_parts
            .iter()
            .find(|o| {
                o.instance.include == new.instance.include && o.instance.key == new.instance.key
            })
            .expect("every new part has an old counterpart");
        changed_by_frame.insert(
            vec![(new.instance.include.clone(), new.instance.key.clone())],
            changed_answers(&old.instance.answers, &new.instance.answers),
        );
    }
    // Pre-hooks always run on update (they're guards); post-hooks re-fire only
    // when one of their inputs changed.
    let plan = hooks::plan(&template, &new_answers, &new_parts, &eval)?;
    let post_plan = hooks::fire_on_update_planned(&plan.post, &changes, &changed_by_frame, &eval)?;

    if opts.dry_run {
        if actions.is_empty() {
            eprintln!("dry run: tree already up to date");
        }
        for (path, action) in &actions {
            eprintln!("dry run: {} {path}", action.verb());
        }
        for hook in &plan.pre {
            eprintln!(
                "dry run: would run pre-hook `{}` ({}){}",
                hook.hook.id,
                hook.hook.label,
                frame_suffix(hook)
            );
        }
        for hook in &post_plan {
            eprintln!(
                "dry run: would run post-hook `{}` ({}){}",
                hook.hook.id,
                hook.hook.label,
                frame_suffix(hook)
            );
        }
        return Ok(report);
    }

    // Pre-hooks run before touching files.
    if !opts.skip_tasks {
        hooks::run_planned(&plan.pre, &opts.dest, &eval).context("a pre-render hook failed")?;
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
        opts.stored
            .clone()
            .unwrap_or_else(|| StoredSource::path(&template_path)),
        template.patches.iter().map(|p| p.id).collect(),
        new_render.hash(),
        &new_answers,
        &secret_specs,
    )
    .with_instances(instance_states)
    .save(&opts.dest)?;
    // Re-pin the self-contained base (root frame and every instance) to the
    // new template state.
    crate::state::BaseSnapshot::from_render(&template.patches, &new_parts).save(&opts.dest)?;

    if !opts.skip_tasks && report.conflicts.is_empty() {
        hooks::run_planned(post_plan.iter().copied(), &opts.dest, &eval)?;
    } else if !post_plan.is_empty() && !report.conflicts.is_empty() {
        report.notes.push(format!(
            "skipped {} post-hook(s) because of conflicts; re-run them after resolving",
            post_plan.len()
        ));
    }

    Ok(report)
}

/// Replace nested parts' patches with the bodies the snapshot stored for
/// them, recursively. A nested instance the snapshot doesn't know keeps the
/// child template's current patches (pre-feature projects).
fn pin_stored_children(
    parts: &mut [compose::ComposedPart<'_>],
    stored: &crate::state::InstanceSnapshot,
) {
    for part in parts {
        if let Some(child) = stored.child(&part.instance.include, &part.instance.key) {
            part.patches = child.patches();
            pin_stored_children(&mut part.children, child);
        }
    }
}

/// The answer ids whose value differs between two answer sets. Secret
/// values are opaque; only presence changes count.
fn changed_answers(old: &AnswerSet, new: &AnswerSet) -> BTreeSet<AnswerId> {
    old.iter()
        .map(|(id, _)| id)
        .chain(new.iter().map(|(id, _)| id))
        .filter(|id| match (old.get(id), new.get(id)) {
            (Some(Value::Secret(_)), Some(Value::Secret(_))) => false,
            (a, b) => a != b,
        })
        .cloned()
        .collect()
}

/// ` [include web]` for a hook of an include's frame; nothing at the root.
fn frame_suffix(hook: &hooks::PlannedHook<'_>) -> String {
    if hook.frame.is_empty() {
        String::new()
    } else {
        format!(" [include {}]", hook.frame_prefix())
    }
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
            // A binary side can't be line-merged (and conflict markers can't
            // be written into bytes): keep the user's version, note it.
            let (Some(ours_text), Some(theirs_text)) = (o.content.text(), t.content.text()) else {
                return Some(Action::Note(
                    "template changed a binary file you also modified; kept your version"
                        .to_owned(),
                ));
            };
            let base_text = base.and_then(|e| e.content.text()).unwrap_or("");
            let outcome = merge3(base_text, ours_text, theirs_text);
            let mode = if base.map(|e| e.mode) == Some(o.mode) {
                t.mode
            } else {
                o.mode
            };
            let entry = FileEntry {
                content: outcome.text.into(),
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
