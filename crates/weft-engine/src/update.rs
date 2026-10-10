//! `weft update`: re-render with the pinned inputs, re-render with the new
//! template state and answers, and 3-way merge the difference onto the
//! user's tree. Conflicts get markers plus a summary; nothing is silently
//! clobbered.
//!
//! Answers carry provenance ([`answers::provenance`]): the old render uses
//! exactly what was rendered before (given and derived), the new render
//! starts from the *given* answers plus this run's changes, so defaults,
//! computed values, and include binds re-derive.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use weft_core::merge::merge3;
use weft_core::render::{narrow, ExprEval, RenderError, ValueOrigin};
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
    /// With `dry_run`: also print the unified diff of every planned write.
    pub diff: bool,
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
    /// Answer changes to apply with this update (`--answer`, `--unset`, …).
    pub answers: AnswerChanges,
    /// Write even when files this update touches have uncommitted git
    /// changes (otherwise refused, so git can review and undo the merge).
    pub allow_dirty: bool,
}

/// Answer changes requested with an update: new values layered exactly like
/// `weft new`'s inputs (answers file → `--answer` → JSON, with the selected
/// presets' locks enforced over them), plus answer ids handed back to their
/// defaults. Ids are flat: `id`, `<include>.<id>`, or `<include>.<key>.<id>`
/// for a repeat instance.
#[derive(Debug, Clone, Default)]
pub struct AnswerChanges {
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    pub answers_json: Option<String>,
    pub unset: Vec<String>,
}

impl AnswerChanges {
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
            && self.answers.is_empty()
            && self.answers_file.is_none()
            && self.answers_json.is_none()
            && self.unset.is_empty()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateReport {
    pub conflicts: Vec<Utf8PathBuf>,
    pub notes: Vec<String>,
    pub written: usize,
    /// Answers whose value changes with this update (root and instances).
    pub answer_changes: Vec<AnswerChange>,
    /// Given answers that stayed put while the default (or bind) they once
    /// matched moved on — candidates for `--unset`.
    pub kept: Vec<KeptAnswer>,
}

/// One answer whose value changes between the old and the new render.
#[derive(Debug, Clone, Serialize)]
pub struct AnswerChange {
    /// Flat id: `id`, `<include>.<id>`, or `<include>.<key>.<id>`.
    pub id: String,
    pub old: Option<Value>,
    pub new: Option<Value>,
    pub origin: Origin,
}

/// Where an answer's new value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// An input: set on this or an earlier run, or prompted for.
    Given,
    /// A default or computed value, re-derived from the other answers.
    Derived,
    /// An include instance answer seeded by the parent's `bind`.
    Bind,
    /// The question no longer exists (or no longer resolves).
    Removed,
}

/// A given answer that no longer matches its default (or bind).
#[derive(Debug, Clone, Serialize)]
pub struct KeptAnswer {
    pub id: String,
    pub value: Value,
    /// What the default (or bind) evaluates to under the new answers.
    pub would_be: Value,
}

pub fn run(
    opts: &UpdateOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<UpdateReport> {
    let state = State::load(&opts.dest)?;
    // Markers the last update left would be merged as if they were content.
    guard_conflicts(&opts.dest, &state)?;
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

    // This run's answer changes, validated and routed to their frames.
    let requested = Requested::route(&template, &state, &opts.answers, &opts.drop_instances)?;

    // Secrets resolve through the *stored* references — never re-prompted
    // while the reference still resolves.
    let presolved_secrets = resolve_stored_secrets(&template, &state, interaction)?;

    // Old side: exactly the answers the last render used.
    let mut old_answers = state.rendered_answers();
    old_answers.overlay(&presolved_secrets);

    // New side: the given answers plus this run's changes are the inputs;
    // derived values re-derive, new questions get defaults or prompts.
    let given = requested.root.apply(&state.answers);
    let new_answers = answers::gather(&template, &given, &presolved_secrets, &eval, interaction)
        .map_err(|e| stored_answer_hint(e, &requested.root.set, |id| id.to_string()))?;
    let root_answers = answers::provenance(
        &template.manifest.questions,
        &given,
        &BTreeSet::new(),
        &new_answers,
        &eval,
    );
    let root_id = |id: &AnswerId| id.to_string();
    let mut report = UpdateReport {
        answer_changes: frame_changes(
            &template.manifest.questions,
            &old_answers,
            &new_answers,
            &root_answers,
            &BTreeSet::new(),
            root_id,
        ),
        kept: kept_given(
            &template.manifest.questions,
            &old_answers,
            &new_answers,
            &root_answers,
            &requested.root.set,
            |q, new_side| {
                let scope = if new_side { &new_answers } else { &old_answers };
                let default = q.default.as_ref().and_then(|d| eval.eval(d, scope).ok())?;
                narrow(q, default, ValueOrigin::Default).ok()
            },
            root_id,
        ),
        ..UpdateReport::default()
    };
    note_gated_off(
        &mut report,
        &requested.root.set,
        &root_answers,
        &new_answers,
        root_id,
    );

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

    let no_changes = FrameChanges::default();
    let mut old_parts: Vec<compose::ComposedPart> = Vec::new();
    let mut new_parts: Vec<compose::ComposedPart> = Vec::new();
    let mut instance_answers: BTreeMap<(String, String), (AnswerSet, AnswerSet)> = BTreeMap::new();
    for inst in &state.instances {
        let inc = template.include(&inst.include).expect("checked above");
        // Old side: the instance's rendered answers + re-resolved stored
        // child secrets. Nested (grandchild) instances re-derive their
        // answers from binds/defaults on both sides (nested answers are not
        // pinned yet); their patches come from the snapshot where it has them.
        let child_secrets = resolve_instance_secrets(&inc.template, inst, interaction)?;
        let mut old_child_answers = inst.rendered_answers();
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
                answers: old_child_answers.clone(),
                repeat: inc.decl.repeat,
            },
            children: old_children,
        });
        // New side: the instance's given answers plus this run's changes;
        // binds re-seed and defaults re-derive (same policy as the root).
        // Dropped instances (`weft instance remove`) have no new side.
        if opts
            .drop_instances
            .iter()
            .any(|(i, k)| *i == inst.include && *k == inst.key)
        {
            continue;
        }
        let slot = (inst.include.clone(), inst.key.clone());
        let changes = requested.instances.get(&slot).unwrap_or(&no_changes);
        let provided = changes.apply(&inst.answers);
        let new_child_answers = compose::resolve_instance_answers(
            inc,
            &inst.key,
            &new_answers,
            &provided,
            &child_secrets,
            &eval,
            interaction,
        )
        .map_err(|e| stored_answer_hint(e, &changes.set, |id| inst.flat_id(inc.decl.repeat, id)))?;
        let binds = compose::bind_ids(&template, &inst.include);
        let child_answers = answers::provenance(
            &inc.template.manifest.questions,
            &provided,
            &binds,
            &new_child_answers,
            &eval,
        );
        // Only an instance the old side rendered has answers to compare
        // (`weft instance add` pins an empty base first).
        if !inst.base.is_empty() {
            let flat = |id: &AnswerId| inst.flat_id(inc.decl.repeat, id);
            let bind_scope = |parent: &AnswerSet| {
                let mut scope = parent.clone();
                scope.insert(AnswerId::from("key"), Value::String(inst.key.clone()));
                scope
            };
            let (old_scope, new_scope) = (bind_scope(&old_answers), bind_scope(&new_answers));
            report.answer_changes.extend(frame_changes(
                &inc.template.manifest.questions,
                &old_child_answers,
                &new_child_answers,
                &child_answers,
                &binds,
                flat,
            ));
            report.kept.extend(kept_given(
                &inc.template.manifest.questions,
                &old_child_answers,
                &new_child_answers,
                &child_answers,
                &changes.set,
                |q, new_side| match inc.decl.bind.get(&q.id.0) {
                    Some(bind) => eval
                        .eval(bind, if new_side { &new_scope } else { &old_scope })
                        .ok()
                        .and_then(|v| narrow(q, v, ValueOrigin::Input).ok()),
                    None => q.default.as_ref().and_then(|d| {
                        let scope = if new_side {
                            &new_child_answers
                        } else {
                            &old_child_answers
                        };
                        let default = eval.eval(d, scope).ok()?;
                        narrow(q, default, ValueOrigin::Default).ok()
                    }),
                },
                flat,
            ));
            note_gated_off(
                &mut report,
                &changes.set,
                &child_answers,
                &new_child_answers,
                flat,
            );
        }
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
        instance_answers.insert(slot, child_answers);
    }
    print_answer_report(&report);

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
    // Pre-hooks always run on update (they're guards). Post-hooks re-fire
    // when one of their inputs changed, when an earlier update owes them, or,
    // all of them, in an instance this update creates: `weft instance add`
    // pins it with an empty base, and its hooks run as they would have on
    // `weft new`.
    let created: BTreeSet<Vec<(String, String)>> = state
        .instances
        .iter()
        .filter(|inst| inst.base.is_empty())
        .map(|inst| vec![(inst.include.clone(), inst.key.clone())])
        .collect();
    let pending: BTreeSet<String> = state.state.pending_hooks.iter().cloned().collect();
    let plan = hooks::plan(&template, &new_answers, &new_parts, &eval)?;
    let post_plan = hooks::fire_on_update_planned(
        &plan.post,
        &changes,
        &changed_by_frame,
        &created,
        &pending,
        &eval,
    )?;
    if !opts.skip_tasks {
        for id in &pending {
            if !post_plan.iter().any(|hook| hook.id == *id) {
                report.notes.push(format!(
                    "dropped pending post-hook `{id}`: the template no longer runs it here"
                ));
            }
        }
    }

    // Files with uncommitted git changes that this update would write: the
    // merge could be neither reviewed nor undone with git.
    let dirty = if opts.allow_dirty {
        Vec::new()
    } else {
        let touched: Vec<&Utf8PathBuf> = actions
            .iter()
            .filter(|(_, a)| a.writes())
            .map(|(p, _)| p)
            .collect();
        crate::vcs::dirty_paths(&opts.dest, &touched)
    };

    if opts.dry_run {
        if actions.is_empty() {
            eprintln!("dry run: tree already up to date");
        }
        for (path, action) in &actions {
            match action {
                Action::Note(msg) => eprintln!("dry run: note: {path}: {msg}"),
                _ => eprintln!("dry run: {} {path}", action.verb()),
            }
        }
        if !dirty.is_empty() {
            eprintln!("dry run: the update would stop — {}", dirty_message(&dirty));
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
        if opts.diff {
            for (path, action) in &actions {
                print_diff(path, user_tree.get(path), action);
            }
        }
        return Ok(report);
    }
    if !dirty.is_empty() {
        bail!("{}", dirty_message(&dirty));
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
                fsio::remove_file(&opts.dest, &path)?;
                report.notes.push(format!("deleted {path}"));
            }
            Action::Conflict(entry, n) => {
                fsio::write_file(&opts.dest, &path, &entry)?;
                report
                    .notes
                    .push(format!("{n} conflict(s) in {path} — resolve the markers"));
                report.conflicts.push(path);
            }
            Action::Note(msg) => report.notes.push(format!("{path}: {msg}")),
        }
    }

    // Pin the new state: source, base, answers with provenance, re-pinned
    // include instances, the files left with conflict markers, and the
    // post-hooks held back until they are gone.
    let secret_specs = collect_secret_specs(&template, &state, &new_answers);
    let instance_states: Vec<crate::state::InstanceState> = new_parts
        .iter()
        .map(|p| {
            let slot = (p.instance.include.clone(), p.instance.key.clone());
            let (given, derived) = instance_answers.remove(&slot).unwrap_or_default();
            crate::state::InstanceState {
                include: slot.0,
                key: slot.1,
                mount: p.instance.mount.to_string(),
                base: p.patches.iter().map(|patch| patch.id).collect(),
                answers: given,
                derived,
                secrets: crate::new::secret_specs(
                    &p.template.manifest.questions,
                    &p.instance.answers,
                ),
            }
        })
        .collect();
    let mut next = State::new(
        opts.stored
            .clone()
            .unwrap_or_else(|| StoredSource::path(&template_path)),
        template.patches.iter().map(|p| p.id).collect(),
        new_render.hash(),
        root_answers,
        &secret_specs,
    )
    .with_instances(instance_states);
    next.state.conflicts = report.conflicts.clone();
    let held_back = !opts.skip_tasks && !report.conflicts.is_empty();
    next.state.pending_hooks = if opts.skip_tasks {
        // Nothing ran: what an earlier update held back still waits.
        state.state.pending_hooks.clone()
    } else if held_back {
        post_plan.iter().map(|hook| hook.id.clone()).collect()
    } else {
        Vec::new()
    };
    next.save(&opts.dest)?;
    // Re-pin the self-contained base (root frame and every instance) to the
    // new template state.
    crate::state::BaseSnapshot::from_render(&template.patches, &new_parts).save(&opts.dest)?;

    if held_back {
        if !post_plan.is_empty() {
            let ids: Vec<String> = post_plan
                .iter()
                .map(|hook| format!("`{}`", hook.id))
                .collect();
            report.notes.push(format!(
                "held back post-hook(s) {} because of conflicts; the next `weft update` runs \
                 them once the markers are gone",
                ids.join(", ")
            ));
        }
    } else if !opts.skip_tasks {
        run_post_hooks(&post_plan, &opts.dest, &mut next, &eval)?;
    }

    Ok(report)
}

/// Run post-hooks in order. When one fails, it and the hooks after it go
/// into the project's state as pending, so the next `weft update` runs them
/// once the cause is fixed, whether or not their inputs change again; the
/// failure is returned, naming them. Hooks before it ran and stay done.
pub(crate) fn run_post_hooks(
    post: &[&hooks::PlannedHook<'_>],
    dest: &Utf8Path,
    state: &mut State,
    eval: &dyn ExprEval,
) -> Result<()> {
    for (i, hook) in post.iter().enumerate() {
        if let Err(err) = hooks::run_planned(std::iter::once(*hook), dest, eval) {
            state.state.pending_hooks = post[i..].iter().map(|h| h.id.clone()).collect();
            state.save(dest)?;
            let ids: Vec<String> = state
                .state
                .pending_hooks
                .iter()
                .map(|id| format!("`{id}`"))
                .collect();
            return Err(err.context(format!(
                "post-hook `{}` failed; the next `weft update` runs {}",
                hook.id,
                ids.join(", ")
            )));
        }
    }
    Ok(())
}

/// This run's answer changes for one frame (the root or one instance).
#[derive(Default)]
struct FrameChanges {
    set: AnswerSet,
    unset: BTreeSet<AnswerId>,
}

impl FrameChanges {
    /// The frame's inputs for the new render: its stored given answers, this
    /// run's values over them, minus the ids handed back to their defaults.
    fn apply(&self, given: &AnswerSet) -> AnswerSet {
        let mut inputs = given.clone();
        inputs.overlay(&self.set);
        for id in &self.unset {
            inputs.remove(id);
        }
        inputs
    }
}

/// This run's answer changes, routed to their frames.
#[derive(Default)]
struct Requested {
    root: FrameChanges,
    instances: BTreeMap<(String, String), FrameChanges>,
}

impl Requested {
    fn route(
        template: &Template,
        state: &State,
        changes: &AnswerChanges,
        dropped: &[(String, String)],
    ) -> Result<Self> {
        let mut requested = Requested::default();
        if changes.is_empty() {
            return Ok(requested);
        }
        let flat = answers::layered_with_json(
            template,
            &changes.presets,
            changes.answers_file.as_deref(),
            &changes.answers,
            changes.answers_json.as_deref(),
        )?;
        let (root, children) = compose::split_provided(template, &flat)?;
        requested.root.set = root;
        for ((include, key), set) in children {
            existing_instance(template, state, &include, &key, dropped)?;
            requested.instances.entry((include, key)).or_default().set = set;
        }
        for raw in &changes.unset {
            if flat.contains(&AnswerId(raw.clone())) {
                bail!("`{raw}` is both set and unset in this update; pick one");
            }
            let route = compose::route_answer(template, raw).with_context(|| {
                format!(
                    "--unset {raw}: no question `{raw}` in template `{}` or its includes",
                    template.manifest.template.name
                )
            })?;
            if matches!(route.question().kind, weft_core::AnswerKind::Secret { .. }) {
                bail!("--unset {raw}: secrets resolve through their declared source");
            }
            match route {
                compose::AnswerRoute::Root(q) => {
                    if q.default.is_none() {
                        bail!(
                            "--unset {raw}: `{raw}` has no default to fall back to; set it \
                             with --answer {raw}=…"
                        );
                    }
                    requested.root.unset.insert(q.id.clone());
                }
                compose::AnswerRoute::Instance {
                    include,
                    key,
                    question,
                } => {
                    existing_instance(template, state, &include.decl.name, &key, dropped)?;
                    if question.default.is_none() && !include.decl.bind.contains_key(&question.id.0)
                    {
                        bail!(
                            "--unset {raw}: `{raw}` has neither a default nor a bind to fall \
                             back to; set it with --answer {raw}=…"
                        );
                    }
                    requested
                        .instances
                        .entry((include.decl.name.clone(), key))
                        .or_default()
                        .unset
                        .insert(question.id.clone());
                }
            }
        }
        Ok(requested)
    }
}

/// Answers can only change for an instance the project has (and keeps).
fn existing_instance(
    template: &Template,
    state: &State,
    include: &str,
    key: &str,
    dropped: &[(String, String)],
) -> Result<()> {
    let exists = state
        .instances
        .iter()
        .any(|i| i.include == include && i.key == key)
        && !dropped.iter().any(|(i, k)| i == include && k == key);
    if exists {
        return Ok(());
    }
    if template.include(include).is_some_and(|i| i.decl.repeat) {
        bail!(
            "this project has no instance `{key}` of include `{include}`; add it with \
             `weft instance add {include} {key}`"
        );
    }
    bail!("include `{include}` is not instantiated in this project")
}

/// The answers of one frame whose rendered value changes, with where each
/// new value comes from. Secret values are opaque and never listed.
fn frame_changes(
    questions: &[Question],
    old: &AnswerSet,
    new: &AnswerSet,
    (given, derived): &(AnswerSet, AnswerSet),
    binds: &BTreeSet<AnswerId>,
    flat_id: impl Fn(&AnswerId) -> String,
) -> Vec<AnswerChange> {
    // In declaration order, so a changed answer comes before what derives
    // from it; ids no question declares any more go last.
    let position = |id: &AnswerId| {
        questions
            .iter()
            .position(|q| q.id == *id)
            .unwrap_or(usize::MAX)
    };
    let mut ids: Vec<&AnswerId> = old
        .iter()
        .chain(new.iter())
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ids.sort_by_key(|id| position(id));
    let mut out = Vec::new();
    for id in ids {
        let (o, n) = (old.get(id), new.get(id));
        if o == n || matches!(o, Some(Value::Secret(_))) || matches!(n, Some(Value::Secret(_))) {
            continue;
        }
        let origin = match n {
            None => Origin::Removed,
            Some(_) if given.contains(id) && !derived.contains(id) => Origin::Given,
            Some(_) if binds.contains(id) => Origin::Bind,
            Some(_) => Origin::Derived,
        };
        out.push(AnswerChange {
            id: flat_id(id),
            old: o.cloned(),
            new: n.cloned(),
            origin,
        });
    }
    out
}

/// A stored answer the template no longer allows (a choice it now blocks,
/// or a value other than a lock it added) stops the update; say how to
/// change it. An answer set on this run explains itself.
fn stored_answer_hint(
    err: anyhow::Error,
    set_now: &AnswerSet,
    flat_id: impl Fn(&AnswerId) -> String,
) -> anyhow::Error {
    let hint = match err.downcast_ref::<RenderError>() {
        Some(RenderError::Blocked { id, .. }) if !set_now.contains(id) => {
            let flat = flat_id(id);
            format!(
                "the answer this project stored for `{flat}` is no longer allowed; set \
                 another with `weft update --answer {flat}=…`, or hand it back to the \
                 template with `weft update --unset {flat}`"
            )
        }
        Some(RenderError::Locked { id, .. }) if !set_now.contains(id) => {
            let flat = flat_id(id);
            format!(
                "the template now locks `{flat}`; drop the answer this project stored \
                 with `weft update --unset {flat}`"
            )
        }
        _ => return err,
    };
    err.context(hint)
}

/// Given answers (not set on this run) that matched what their default —
/// or bind — produced before, and no longer match it: after a rename they
/// are the values that look stale. `expected(question, new_side)` evaluates
/// the default/bind over the old or the new answers.
fn kept_given(
    questions: &[Question],
    old: &AnswerSet,
    new: &AnswerSet,
    (given, derived): &(AnswerSet, AnswerSet),
    set_now: &AnswerSet,
    expected: impl Fn(&Question, bool) -> Option<Value>,
    flat_id: impl Fn(&AnswerId) -> String,
) -> Vec<KeptAnswer> {
    let mut out = Vec::new();
    for q in questions {
        if q.computed
            || matches!(q.kind, weft_core::AnswerKind::Secret { .. })
            || !given.contains(&q.id)
            || derived.contains(&q.id)
            || set_now.contains(&q.id)
        {
            continue;
        }
        let (Some(before), Some(now)) = (old.get(&q.id), new.get(&q.id)) else {
            continue;
        };
        if before != now {
            continue;
        }
        let (Some(was), Some(would_be)) = (expected(q, false), expected(q, true)) else {
            continue;
        };
        if was == *before && would_be != *now {
            out.push(KeptAnswer {
                id: flat_id(&q.id),
                value: now.clone(),
                would_be,
            });
        }
    }
    out
}

/// Note answers set on this run whose question is off under the new
/// answers: the value is stored, but nothing renders it yet.
fn note_gated_off(
    report: &mut UpdateReport,
    set_now: &AnswerSet,
    (given, _): &(AnswerSet, AnswerSet),
    rendered: &AnswerSet,
    flat_id: impl Fn(&AnswerId) -> String,
) {
    for (id, _) in set_now.iter() {
        if given.get(id) != rendered.get(id) {
            report.notes.push(format!(
                "`{}` is set, but its question is off under the new answers (its `when` is \
                 false); the value is kept for when it applies",
                flat_id(id)
            ));
        }
    }
}

/// Print the answer part of an update before any file is touched.
fn print_answer_report(report: &UpdateReport) {
    let width = report
        .answer_changes
        .iter()
        .map(|c| c.id.len())
        .chain(report.kept.iter().map(|k| k.id.len()))
        .max()
        .unwrap_or(0);
    if !report.answer_changes.is_empty() {
        eprintln!("answers:");
        for c in &report.answer_changes {
            let tag = match c.origin {
                Origin::Given => "",
                Origin::Derived => "  (derived)",
                Origin::Bind => "  (bind)",
                Origin::Removed => "  (no longer asked)",
            };
            eprintln!(
                "  {:width$}  {} → {}{tag}",
                c.id,
                show_opt(c.old.as_ref()),
                show_opt(c.new.as_ref())
            );
        }
    }
    if !report.kept.is_empty() {
        eprintln!("kept as given:");
        for k in &report.kept {
            eprintln!(
                "  {:width$}  {} — its default is now {} (`--unset {}` to follow it)",
                k.id,
                show_value(&k.value),
                show_value(&k.would_be),
                k.id
            );
        }
    }
}

/// An answer value as the user would type it: strings quoted.
pub fn show_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::List(items) => format!(
            "[{}]",
            items.iter().map(show_value).collect::<Vec<_>>().join(", ")
        ),
        Value::Secret(_) => "<secret>".to_owned(),
    }
}

fn show_opt(value: Option<&Value>) -> String {
    value.map(show_value).unwrap_or_else(|| "(none)".to_owned())
}

/// Refuse to run while files the last update left conflict markers in
/// still have them: the next merge would treat the markers as content.
fn guard_conflicts(dest: &Utf8Path, state: &State) -> Result<()> {
    let unresolved: Vec<&str> = state
        .state
        .conflicts
        .iter()
        .filter(|path| has_markers(&dest.join(path)))
        .map(|path| path.as_str())
        .collect();
    if !unresolved.is_empty() {
        bail!(
            "the last update left conflict markers that are still there: {}\n\
             resolve them first (keep the lines you want, delete the `{}` / `{}` / `{}` \
             lines), then run the update again",
            unresolved.join(", "),
            weft_core::merge::MARKER_OURS,
            weft_core::merge::MARKER_SEP,
            weft_core::merge::MARKER_THEIRS
        );
    }
    Ok(())
}

/// Does this file still contain a conflict block weft wrote?
fn has_markers(path: &Utf8Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let mut ours = false;
    for line in text.lines() {
        if line == weft_core::merge::MARKER_OURS {
            ours = true;
        } else if ours && line == weft_core::merge::MARKER_THEIRS {
            return true;
        }
    }
    false
}

fn dirty_message(dirty: &[Utf8PathBuf]) -> String {
    format!(
        "uncommitted changes in file(s) this update would write: {}\n\
         commit or stash them first, so git can show and undo exactly what the merge does — \
         or pass --allow-dirty",
        dirty
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// The unified diff of one planned action against the file as it is now
/// (stdout, so it pipes into a pager or `git apply --check`).
fn print_diff(path: &Utf8Path, ours: Option<&FileEntry>, action: &Action) {
    let theirs = match action {
        Action::Write(entry) | Action::Conflict(entry, _) => Some(entry),
        Action::Delete => None,
        Action::Note(_) => return,
    };
    let before = ours.map(|e| e.content.text());
    let after = theirs.map(|e| e.content.text());
    if matches!(before, Some(None)) || matches!(after, Some(None)) {
        println!("Binary file {path} changes");
        return;
    }
    let a = match ours {
        Some(_) => format!("a/{path}"),
        None => "/dev/null".to_owned(),
    };
    let b = match theirs {
        Some(_) => format!("b/{path}"),
        None => "/dev/null".to_owned(),
    };
    let diff = similar::TextDiff::from_lines(
        before.flatten().unwrap_or(""),
        after.flatten().unwrap_or(""),
    );
    print!("{}", diff.unified_diff().header(&a, &b));
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

    /// Does carrying out this action change the file on disk?
    fn writes(&self) -> bool {
        !matches!(self, Action::Note(_))
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
            "kept it deleted: you deleted it, and the new render changes it".to_owned(),
        )),
        (Some(_), None) => Some(Action::Note(
            "kept your version: the new render no longer creates it, and you modified it"
                .to_owned(),
        )),
        (Some(o), Some(t)) => {
            // A binary side can't be line-merged (and conflict markers can't
            // be written into bytes): keep the user's version, note it.
            let (Some(ours_text), Some(theirs_text)) = (o.content.text(), t.content.text()) else {
                return Some(Action::Note(
                    "kept your version: the new render changes this binary file, and you \
                     modified it too"
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
        narrowing: Default::default(),
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
