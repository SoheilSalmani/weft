//! `weft patch amend <name>`: edit a recorded patch's *content* through the
//! record loop instead of hand-editing its JSON. The base is the patch's
//! ancestors; the worktree is seeded with the patch applied, so you edit its
//! result directly. `weft commit` re-derives the patch's ops in place —
//! keeping its name, dependencies, gate, and metadata — which changes its
//! content id (as any content edit does).
//!
//! **Leaf patches only for now.** Amending a patch that others depend on is
//! a subgraph rebase (descendants must replay over the new content), which
//! is a separate feature; amend refuses it with a clear message. Generator
//! patches are also refused — their content is owned by their command
//! (`weft patch resync` / `set-command`, or `detach` to take ownership).

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::render::ExprEval;
use weft_core::{AnswerKind, PatchId};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::template::Template;
use crate::{answers, fsio};

pub struct AmendOptions {
    pub template: Utf8PathBuf,
    /// The patch to amend (file stem).
    pub name: String,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    pub answers_json: Option<String>,
    /// Discard an existing session instead of erroring.
    pub force: bool,
}

/// Patches that directly depend on `id` (by name).
pub(crate) fn direct_dependents(template: &Template, id: PatchId) -> Vec<String> {
    template
        .patches
        .iter()
        .filter(|p| p.depends_on.contains(&id))
        .filter_map(|p| template.id_to_name.get(&p.id).cloned())
        .collect()
}

/// Start an amend session: render the target's ancestors, seed the worktree
/// with the target applied on top, and print the worktree path.
pub fn start(
    opts: &AmendOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<Utf8PathBuf> {
    let template = Template::load_with(&opts.template, resolver)?;
    if Session::exists(&opts.template) {
        if opts.force {
            Session::discard(&opts.template)?;
        } else {
            bail!(
                "a recording session is already active in `{}`; \
                 run `weft commit` to finish it or pass `--force` to discard it",
                opts.template
            );
        }
    }

    let id = *template
        .name_to_id
        .get(&opts.name)
        .with_context(|| format!("no patch `{}` in this template", opts.name))?;
    let target = template
        .patches
        .iter()
        .find(|p| p.id == id)
        .expect("name points at a loaded patch");
    if target.meta.generator.is_some() {
        bail!(
            "patch `{}` is a generator patch; its content is owned by its command — \
             use `weft patch resync {0}` to regenerate, `weft patch set-command {0} …` to \
             change the command, or `weft patch detach {0}` to take ownership first",
            opts.name
        );
    }
    if target.foreach.is_some() {
        bail!(
            "patch `{}` is a foreach (integration) patch; amend does not support these yet — \
             re-record it with `weft record --foreach …`",
            opts.name
        );
    }
    let dependents = direct_dependents(&template, id);
    if !dependents.is_empty() {
        bail!(
            "patch `{}` has dependents ({}); amending it would rebase them over the new \
             content, which is not supported yet. Layer a new patch on top \
             (`weft record --base {0}`) or edit `patches/{0}.json` by hand.",
            opts.name,
            dependents.join(", ")
        );
    }

    let eval = StarlarkEval;
    let provided = answers::layered_with_json(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        opts.answers_json.as_deref(),
    )?;
    let resolved = answers::gather(
        &template,
        &provided,
        &weft_core::AnswerSet::new(),
        &eval,
        interaction,
    )?;

    // Base = the target's strict ancestors (its dependency closure minus
    // itself). The worktree is that base with the target applied on top, so
    // the author edits the target's own contribution.
    let ancestors = template.ancestor_closure(id);
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| p.id != id && ancestors.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();
    let base_tree = weft_core::render::render(&base_patches, &resolved, &eval)
        .context("rendering the patch's ancestors")?;
    let mut seed = base_tree.clone();
    // Force the target on regardless of its gate (we're editing it).
    weft_core::render::apply_ops(&mut seed, target, &resolved, &eval)
        .with_context(|| format!("applying patch `{}` to seed the worktree", opts.name))?;

    let worktree = session::worktree_dir(&opts.template);
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &seed)?;

    let secret_specs: BTreeMap<_, _> = template
        .manifest
        .questions
        .iter()
        .filter_map(|q| match &q.kind {
            AnswerKind::Secret { source } if resolved.contains(&q.id) => {
                Some((q.id.clone(), source.to_string()))
            }
            _ => None,
        })
        .collect();
    let sess = Session {
        session: SessionMeta {
            base: base_patches.iter().map(|p| p.id).collect(),
            // The diff target is the base *without* the patch, so commit
            // re-derives the patch's full contribution from the worktree.
            tree_hash: base_tree.hash(),
        },
        answers: crate::record::strip_secrets(&resolved),
        secrets: secret_specs,
        foreach: None,
        generator: None,
        amend: Some(opts.name.clone()),
    };
    sess.save(&opts.template)?;
    Ok(worktree)
}

/// Finish an amend session (called by `weft commit`): re-derive the target's
/// ops from the worktree and write them back into its file in place. Keeps
/// the target's name, dependencies, gate, foreach, and metadata.
pub(crate) fn finish(
    template: &Template,
    template_root: &Utf8Path,
    target: &str,
    base_patches: &[weft_core::Patch],
    answers: &weft_core::AnswerSet,
    work_tree: &weft_core::Tree,
    ops: Vec<weft_core::Op>,
) -> Result<()> {
    let eval = StarlarkEval;
    let id = *template
        .name_to_id
        .get(target)
        .with_context(|| format!("patch `{target}` vanished mid-amend"))?;
    let dependents = direct_dependents(template, id);
    if !dependents.is_empty() {
        bail!(
            "patch `{target}` gained dependents ({}) since the amend started; \
             refusing to rewrite it",
            dependents.join(", ")
        );
    }

    // Preserve everything but the ops.
    let mut file = template.patch_file(target)?;
    let dep_ids: Vec<PatchId> = file
        .depends_on
        .iter()
        .map(|n| {
            template
                .name_to_id
                .get(n)
                .copied()
                .with_context(|| format!("unknown dependency `{n}`"))
        })
        .collect::<Result<_>>()?;

    // Replay guard: the amended patch on its base must reproduce the
    // worktree (skipped when its gate is closed under the answers).
    let candidate = weft_core::Patch::new_foreach(
        dep_ids,
        file.when.clone(),
        file.foreach.clone(),
        ops.clone(),
    );
    let mut with_new = base_patches.to_vec();
    with_new.push(candidate);
    let replayed = weft_core::render::render(&with_new, answers, &eval)
        .context("replaying the amended patch against its base")?;
    let gate_open = match &file.when {
        None => true,
        Some(expr) => eval.eval_bool(expr, answers).unwrap_or(true),
    };
    if gate_open && replayed.hash() != work_tree.hash() {
        bail!(
            "replaying the amended patch does not reproduce the worktree \
             (likely an ambiguous hunk context); refusing to write a broken patch"
        );
    }

    let op_count = ops.len();
    file.ops = ops;
    template.save_patch_file(target, &file)?;
    Session::discard(template_root)?;
    eprintln!(
        "amended patch `{target}` ({op_count} op(s)); its content id changed \
         (nothing depends on it, so no rebase was needed)"
    );
    Ok(())
}

/// Print where the amend worktree is.
pub fn announce(worktree: &Utf8Path, name: &str) {
    println!("{worktree}");
    eprintln!(
        "amending `{name}`: the worktree above is the patch applied on its base — \
         edit it, then run `weft commit`"
    );
}
