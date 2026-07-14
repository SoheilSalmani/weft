//! `weft commit`: diff the recording worktree against its pinned base,
//! abstract concrete values back into answer references, and append the
//! result as a new patch.

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::{AnswerSet, StarlarkExpr, Value};
use weft_lang::StarlarkEval;

use crate::abstraction::Abstractor;
use crate::interact::Interaction;
use crate::record::base_leaves;
use crate::session::{self, Session};
use crate::template::Template;
use crate::{diff, fsio, secrets};

pub struct CommitOptions {
    pub template: Utf8PathBuf,
    /// Patch name; defaults to `patch-NNN`.
    pub name: Option<String>,
    /// Optional `when` condition for the new patch.
    pub when: Option<String>,
    /// Display title, e.g. "Add Prisma support" (metadata, not hashed).
    pub title: Option<String>,
    /// Human/agent-facing description stored in the patch file (not hashed).
    pub describe: Option<String>,
    pub tags: Vec<String>,
    /// Per-answer abstraction decisions supplied by a non-terminal caller
    /// (agents/UIs). `None` = confirm interactively; undecided candidates
    /// default to accepted, secrets are always abstracted.
    pub decisions: Option<std::collections::BTreeMap<String, bool>>,
}

pub fn run(opts: &CommitOptions, interaction: &mut dyn Interaction) -> Result<()> {
    let template = Template::load(&opts.template)?;
    let sess = Session::load(&opts.template)?;
    let eval = StarlarkEval;

    // The base must still be reconstructible from the template.
    for id in &sess.session.base {
        if !template.id_to_name.contains_key(id) {
            bail!(
                "pinned base patch {} no longer exists in the template; \
                 the template changed since `weft record` — re-record",
                id.short()
            );
        }
    }

    let answers = resolve_session_answers(&template, &sess, interaction)?;
    // Foreach patches never participate in the base (see record.rs) — and
    // the new patch must not depend on one (graph-leaf rule).
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| sess.session.base.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();
    // A foreach session's base includes the mounted sample instance.
    let sample = match &sess.foreach {
        Some(f) => Some(crate::record::sample_part(
            &template,
            f,
            &eval,
            interaction,
        )?),
        None => None,
    };
    let parts: Vec<_> = sample.into_iter().collect();
    let base_tree = crate::compose::render_composed(&base_patches, &answers, &parts, &eval)
        .context("re-rendering base state")?;
    if base_tree.hash() != sess.session.tree_hash {
        bail!(
            "base state hash changed since `weft record` (template or secret \
             values moved underneath the session); re-record"
        );
    }

    let worktree = session::worktree_dir(&opts.template);
    let work_tree = fsio::read_tree(&worktree)?;
    guard_mounts(&template, &sess, &base_tree, &work_tree)?;

    // Foreach patches render with `key` and `instance_<id>` in scope, so the
    // abstractor must see the sample instance's values under those names.
    // `key` is canonical: instance answers that merely echo the key (e.g. a
    // `service_name = "key"` bind) are dropped so abstraction prefers `key`.
    let abstractor = match parts.first() {
        Some(part) => {
            let scope = crate::compose::foreach_scope(&answers, &part.instance);
            let key_value = weft_core::Value::String(part.instance.key.clone());
            let deduped: weft_core::AnswerSet = scope
                .iter()
                .filter(|(id, v)| id.0 == "key" || **v != key_value)
                .map(|(id, v)| (id.clone(), v.clone()))
                .collect();
            Abstractor::from_answers(&deduped)
        }
        None => Abstractor::from_answers(&answers),
    };
    let ops = match &opts.decisions {
        Some(decisions) => {
            let texts = diff::collect_texts(&base_tree, &work_tree);
            let decisions = decisions
                .iter()
                .map(|(k, v)| (weft_core::AnswerId(k.clone()), *v))
                .collect();
            let confirmed = abstractor.confirmed_from_decisions(&texts, &decisions);
            diff::build_ops_with(&base_tree, &work_tree, &abstractor, &confirmed)
        }
        None => diff::build_ops(&base_tree, &work_tree, &abstractor, interaction)?,
    };
    if ops.is_empty() {
        bail!("worktree has no changes against the base state; nothing to commit");
    }

    let name = match &opts.name {
        Some(name) => name.clone(),
        None => format!("patch-{:03}", template.patches.len() + 1),
    };
    let when = opts.when.clone().map(StarlarkExpr);
    // Depend only on the *active* leaves of the base: a patch gated off
    // under the session answers contributed nothing to the recorded state,
    // and depending on it would gate the new patch off with it.
    let active_base = active_ids(&base_patches, &answers, &eval)?;
    let depends_on = base_leaves(&template, &active_base);

    // Validation before writing: the new patch applied to the base must
    // reproduce the worktree byte-for-byte (this also catches ambiguous
    // hunk contexts). Skipped when a `when` gate is false under the
    // session answers.
    let foreach_include = sess.foreach.as_ref().map(|f| f.include.clone());
    let candidate = weft_core::Patch::new_foreach(
        depends_on.iter().map(|n| template.name_to_id[n]).collect(),
        when.clone(),
        foreach_include.clone(),
        ops.clone(),
    );
    let mut with_new = base_patches.clone();
    with_new.push(candidate);
    let replayed = crate::compose::render_composed(&with_new, &answers, &parts, &eval)
        .context("replaying the recorded patch against the base state")?;
    let gate_open = match &when {
        None => true,
        Some(expr) => eval_gate(expr, &answers)?,
    };
    if gate_open && replayed.hash() != work_tree.hash() {
        bail!(
            "replaying the recorded patch does not reproduce the worktree \
             (likely an ambiguous hunk context); refusing to write a broken patch"
        );
    }

    template.write_patch_full(
        &name,
        depends_on,
        when,
        foreach_include,
        ops.clone(),
        weft_core::PatchMeta {
            title: opts.title.clone(),
            description: opts.describe.clone(),
            tags: opts.tags.clone(),
            hooks: vec![],
        },
    )?;
    Session::discard(&opts.template)?;

    eprintln!(
        "committed patch `{name}` with {} op(s) to `{}`",
        ops.len(),
        template.manifest.template.name
    );
    Ok(())
}

/// The subset of ordered patches that actually apply under `answers`
/// (gates evaluated, skips propagated to dependents) — mirrors render.
fn active_ids(
    patches: &[weft_core::Patch],
    answers: &AnswerSet,
    eval: &dyn weft_core::render::ExprEval,
) -> Result<Vec<weft_core::PatchId>> {
    let mut skipped = std::collections::BTreeSet::new();
    let mut active = Vec::new();
    for patch in patches {
        if patch.depends_on.iter().any(|d| skipped.contains(d)) {
            skipped.insert(patch.id);
            continue;
        }
        let open = match &patch.when {
            None => true,
            Some(expr) => eval.eval_bool(expr, answers)?,
        };
        if open {
            active.push(patch.id);
        } else {
            skipped.insert(patch.id);
        }
    }
    Ok(active)
}

fn eval_gate(expr: &StarlarkExpr, answers: &AnswerSet) -> Result<bool> {
    use weft_core::render::ExprEval;
    Ok(StarlarkEval.eval_bool(expr, answers)?)
}

/// Recording captures *this* template's patches only. Reject edits that
/// land under an include's mount: those files belong to the child template
/// (for foreach sessions, the sample instance's mount; otherwise every
/// include's static mount prefix — the declared path up to `{key}`).
fn guard_mounts(
    template: &Template,
    sess: &Session,
    base_tree: &weft_core::Tree,
    work_tree: &weft_core::Tree,
) -> Result<()> {
    if template.includes.is_empty() {
        return Ok(());
    }
    let mut prefixes: Vec<(String, camino::Utf8PathBuf)> = Vec::new();
    match &sess.foreach {
        Some(f) => {
            let inc = template.include(&f.include).context("include vanished")?;
            prefixes.push((
                f.include.clone(),
                crate::compose::mount_path(&inc.decl.path, &f.key)?,
            ));
        }
        None => {
            for inc in &template.includes {
                let static_prefix = match inc.decl.path.split_once("{key}") {
                    Some((head, _)) => head.trim_end_matches('/').to_owned(),
                    None => inc.decl.path.clone(),
                };
                if !static_prefix.is_empty() {
                    prefixes.push((inc.decl.name.clone(), static_prefix.into()));
                }
            }
        }
    }
    let changed = |path: &camino::Utf8PathBuf| -> bool {
        match (base_tree.get(path), work_tree.get(path)) {
            (Some(b), Some(w)) => b.content != w.content || b.mode != w.mode,
            (None, None) => false,
            _ => true,
        }
    };
    let mut paths: std::collections::BTreeSet<&camino::Utf8PathBuf> = Default::default();
    paths.extend(base_tree.paths());
    paths.extend(work_tree.paths());
    for path in paths {
        if !changed(path) {
            continue;
        }
        for (include, prefix) in &prefixes {
            if path.starts_with(prefix) {
                bail!(
                    "`{path}` is inside include `{include}`'s mount ({prefix}/…); \
                     files there belong to the child template — record against it directly"
                );
            }
        }
    }
    Ok(())
}

/// Rebuild the full answer set for the session: stored plain answers plus
/// secrets re-resolved from their stored references.
fn resolve_session_answers(
    template: &Template,
    sess: &Session,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let mut answers = sess.answers.clone();
    for (id, spec_str) in &sess.secrets {
        let question = template
            .manifest
            .questions
            .iter()
            .find(|q| q.id == *id)
            .with_context(|| format!("secret question `{id}` no longer exists in the manifest"))?;
        let spec: weft_core::SecretSpec = spec_str
            .parse()
            .with_context(|| format!("invalid secret reference for `{id}`"))?;
        let value = secrets::resolve(question, &spec, interaction)?;
        answers.insert(id.clone(), Value::Secret(value));
    }
    Ok(answers)
}
