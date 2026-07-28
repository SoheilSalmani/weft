//! `weft patch amend <name>`: edit a recorded patch's *content* through the
//! record loop instead of hand-editing its JSON. The base is the patch's
//! ancestors; the worktree is seeded with the patch applied, so you edit its
//! result directly. `weft commit` re-derives the patch's ops in place —
//! keeping its name, dependencies, gate, and metadata — which changes its
//! content id (as any content edit does).
//!
//! Amending a patch that others depend on is a rebase: their
//! context-anchored hunks replay over the new content. If the edit doesn't
//! move what a dependent anchors on, it just works; if it does, `weft
//! commit` reports which dependent broke so you can re-record it. Generator
//! patches are refused — their content is owned by their command (`weft
//! patch resync` / `set-command`, or `detach` to take ownership first).

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
    /// The session to open for the edit; defaults to the patch's name.
    pub session: String,
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
    let session_name = &opts.session;
    if Session::exists(&opts.template, session_name) {
        if opts.force {
            Session::load(&opts.template, session_name)?.end(&opts.template, session_name)?;
        } else {
            bail!(
                "session `{session_name}` already exists in `{}`; \
                 finish it with `weft commit`, or pass `--force` to discard it",
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
             re-record it with `weft session new --foreach …`",
            opts.name
        );
    }
    // Amending a patch with dependents is a rebase: the dependents' hunks
    // replay over the new content and may conflict. That's handled at commit
    // (a full render surfaces any that break); warn up front.
    let dependents = direct_dependents(&template, id);
    if !dependents.is_empty() {
        eprintln!(
            "note: `{}` has dependents ({}); if your edit moves the content they anchor \
             on, `weft commit` will report which one to re-record.",
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

    let worktree = session::default_worktree_dir(&opts.template, session_name);
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &seed)?;
    crate::discover::WorktreeLink {
        template: crate::discover::absolute(&opts.template),
        session: session_name.clone(),
    }
    .save(&worktree)?;

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
            worktree: None,
            scope: Vec::new(),
            adopted: false,
        },
        answers: crate::start::strip_secrets(&resolved),
        secrets: secret_specs,
        foreach: None,
        generator: None,
        amend: Some(opts.name.clone()),
    };
    sess.save(&opts.template, session_name)?;
    Ok(worktree)
}

/// Finish an amend session (called by `weft commit`): re-derive the target's
/// ops from the worktree and write them back into its file in place. Keeps
/// the target's name, dependencies, gate, foreach, and metadata.
#[allow(clippy::too_many_arguments)] // one call site; all of it is the amend's state
pub(crate) fn finish(
    template: &Template,
    template_root: &Utf8Path,
    session: &crate::session::Session,
    session_name: &str,
    target: &str,
    base_patches: &[weft_core::Patch],
    answers: &weft_core::AnswerSet,
    work_tree: &weft_core::Tree,
    ops: Vec<weft_core::Op>,
) -> Result<()> {
    let eval = StarlarkEval;
    let has_dependents = template
        .name_to_id
        .get(target)
        .map(|id| !direct_dependents(template, *id).is_empty())
        .unwrap_or(false);

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
    session.end(template_root, session_name)?;
    eprintln!("amended patch `{target}` ({op_count} op(s)); its content id changed");

    // Rebase check: if anything depends on the amended patch, its
    // context-anchored hunks may have moved with the new content. A full
    // render under the amend answers surfaces a dependent that broke.
    if has_dependents {
        let reloaded = Template::load(template_root)?;
        let non_foreach: Vec<_> = reloaded
            .patches
            .iter()
            .filter(|p| p.foreach.is_none())
            .cloned()
            .collect();
        if let Err(e) = weft_core::render::render(&non_foreach, answers, &eval) {
            bail!(
                "a dependent patch no longer applies after the amend: {e}\n\
                 its context anchored on content that moved — re-record it \
                 (`weft patch amend <name>` or `weft session new N --base <name>`), then \
                 `weft check`. The amend to `{target}` is written; fix the dependent to \
                 finish the rebase."
            );
        }
        eprintln!("dependents still apply — no rebase conflicts");
    }
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
