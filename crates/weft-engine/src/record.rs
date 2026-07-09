//! `weft record`: materialize a pinned base state into a scratch worktree the
//! author edits with real tools. Their dirty working directory never leaks
//! in — the worktree starts as exactly `render(base, answers)`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::{AnswerKind, PatchId};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::template::Template;
use crate::{answers, fsio};

pub struct RecordOptions {
    pub template: Utf8PathBuf,
    /// `latest` (all patches) or a patch name (that patch plus its ancestors).
    pub base: String,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    /// Discard an existing session instead of erroring.
    pub force: bool,
}

pub fn run(opts: &RecordOptions, interaction: &mut dyn Interaction) -> Result<Utf8PathBuf> {
    let template = Template::load(&opts.template)?;
    if Session::exists(&opts.template) {
        if opts.force {
            Session::discard(&opts.template)?;
        } else {
            bail!(
                "a recording session is already active in `{}`; \
                 run `weft commit` to finish it or `weft record --force` to discard it",
                opts.template
            );
        }
    }

    let eval = StarlarkEval;
    let provided = answers::layered_answers(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
    )?;
    let resolved = answers::gather(
        &template,
        &provided,
        &weft_core::AnswerSet::new(),
        &eval,
        interaction,
    )?;

    let pinned = pin_base(&template, &opts.base)?;
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id))
        .cloned()
        .collect();
    let tree = weft_core::render::render(&base_patches, &resolved, &eval)
        .context("rendering base state")?;

    let worktree = session::worktree_dir(&opts.template);
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &tree)?;

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
            tree_hash: tree.hash(),
        },
        answers: strip_secrets(&resolved),
        secrets: secret_specs,
    };
    sess.save(&opts.template)?;

    Ok(worktree)
}

fn strip_secrets(answers: &weft_core::AnswerSet) -> weft_core::AnswerSet {
    answers
        .iter()
        .filter(|(_, v)| !matches!(v, weft_core::Value::Secret(_)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Resolve a `--base` ref to a set of patch ids.
fn pin_base(template: &Template, base: &str) -> Result<BTreeSet<PatchId>> {
    if base == "latest" {
        return Ok(template.patches.iter().map(|p| p.id).collect());
    }
    let root = *template.name_to_id.get(base).with_context(|| {
        format!(
            "unknown base ref `{base}` (expected `latest` or a patch name: {})",
            template
                .name_to_id
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    // Ancestor closure of the named patch.
    let by_id: BTreeMap<PatchId, &weft_core::Patch> =
        template.patches.iter().map(|p| (p.id, p)).collect();
    let mut pinned = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if pinned.insert(id) {
            stack.extend(&by_id[&id].depends_on);
        }
    }
    Ok(pinned)
}

/// Leaves of the pinned base: patches no *other pinned* patch depends on.
/// These become the recorded patch's dependencies.
pub fn base_leaves(template: &Template, pinned: &[PatchId]) -> Vec<String> {
    let pinned_set: BTreeSet<_> = pinned.iter().copied().collect();
    let mut depended_upon: BTreeSet<PatchId> = BTreeSet::new();
    for p in &template.patches {
        if pinned_set.contains(&p.id) {
            depended_upon.extend(p.depends_on.iter().copied());
        }
    }
    pinned
        .iter()
        .filter(|id| !depended_upon.contains(id))
        .filter_map(|id| template.id_to_name.get(id).cloned())
        .collect()
}

/// Convenience for CLI: print where the worktree is.
pub fn announce(worktree: &Utf8Path) {
    println!("{worktree}");
    eprintln!(
        "recording session started; edit files in the worktree above, then run `weft commit`"
    );
}
