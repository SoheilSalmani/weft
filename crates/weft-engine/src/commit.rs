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
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| sess.session.base.contains(&p.id))
        .cloned()
        .collect();
    let base_tree = weft_core::render::render(&base_patches, &answers, &eval)
        .context("re-rendering base state")?;
    if base_tree.hash() != sess.session.tree_hash {
        bail!(
            "base state hash changed since `weft record` (template or secret \
             values moved underneath the session); re-record"
        );
    }

    let worktree = session::worktree_dir(&opts.template);
    let work_tree = fsio::read_tree(&worktree)?;

    let abstractor = Abstractor::from_answers(&answers);
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
    let depends_on = base_leaves(&template, &sess.session.base);

    // Validation before writing: the new patch applied to the base must
    // reproduce the worktree byte-for-byte (this also catches ambiguous
    // hunk contexts). Skipped when a `when` gate is false under the
    // session answers.
    let candidate = weft_core::Patch::new(
        depends_on.iter().map(|n| template.name_to_id[n]).collect(),
        when.clone(),
        ops.clone(),
    );
    let mut with_new = base_patches.clone();
    with_new.push(candidate);
    let replayed = weft_core::render::render(&with_new, &answers, &eval)
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

    template.write_patch(
        &name,
        depends_on,
        when,
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

fn eval_gate(expr: &StarlarkExpr, answers: &AnswerSet) -> Result<bool> {
    use weft_core::render::ExprEval;
    Ok(StarlarkEval.eval_bool(expr, answers)?)
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
