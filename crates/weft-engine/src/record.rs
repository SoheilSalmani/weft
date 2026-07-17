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
    /// Answers as a JSON object (agents).
    pub answers_json: Option<String>,
    /// Record a foreach integration patch: `include=key` mounts one sample
    /// instance of the include into the base; commit abstracts the sample
    /// key back out and writes a `foreach` patch.
    pub foreach: Option<String>,
    /// Run this command in the freshly rendered worktree; its output becomes
    /// the patch, and commit stores it as generator metadata so `weft patch
    /// resync` can re-run it later.
    pub exec: Option<String>,
    /// Discard an existing session instead of erroring.
    pub force: bool,
}

pub fn run(
    opts: &RecordOptions,
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
                 run `weft commit` to finish it or `weft record --force` to discard it",
                opts.template
            );
        }
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

    let pinned = pin_base(&template, &opts.base)?;
    // Foreach patches are excluded from the base: nothing may depend on
    // them (graph-leaf rule), and a foreach session's worktree must not
    // contain other foreach patches' output — recorded hunk contexts would
    // otherwise anchor on lines that only exist for the sample instance.
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();

    // A `--foreach include=key` session mounts one *sample* instance of the
    // include into the base, so the author edits parent files against a
    // concrete example (e.g. adds `use ./services/payments` to go.work).
    // Commit later abstracts the sample key/answers into `key` /
    // `instance_<id>` references. Otherwise, a template with includes
    // renders its own patches only (children belong to their template).
    let foreach = match &opts.foreach {
        Some(spec) => {
            let (include, key) = spec
                .split_once('=')
                .with_context(|| format!("--foreach expects `include=key`, got {spec:?}"))?;
            let inc = template.include(include).with_context(|| {
                format!("--foreach {include}={key}: unknown include `{include}`")
            })?;
            if !crate::compose::valid_key(key) {
                bail!("invalid sample key {key:?} (lowercase alphanumerics, `-`, `_`)");
            }
            let instance_answers = crate::compose::resolve_instance_answers(
                inc,
                key,
                &resolved,
                &weft_core::AnswerSet::new(),
                &weft_core::AnswerSet::new(),
                &eval,
                interaction,
            )?;
            Some(crate::session::ForeachSession {
                include: include.to_owned(),
                key: key.to_owned(),
                answers: instance_answers,
            })
        }
        None => None,
    };
    let tree = match &foreach {
        Some(f) => {
            let parts = vec![sample_part(&template, f, &eval, interaction)?];
            crate::compose::render_composed(&base_patches, &resolved, &parts, &eval)
                .context("rendering base state with the sample instance")?
        }
        None => weft_core::render::render(&base_patches, &resolved, &eval)
            .context("rendering base state")?,
    };

    let worktree = session::worktree_dir(&opts.template);
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &tree)?;

    // `--exec`: the command's output *is* the patch content. Run it now so
    // the author can inspect (`weft diff`) before committing; the command is
    // held in the session and stored on the patch at commit.
    let generator = match &opts.exec {
        Some(cmd) => {
            if foreach.is_some() {
                bail!("--exec cannot be combined with --foreach (unsupported for resync)");
            }
            let command = weft_core::Command::literal(cmd);
            if let Err(e) =
                crate::hooks::run_command(&command, "generator", &worktree, &resolved, &eval)
            {
                // Nothing committed yet — don't leave a broken session behind.
                let _ = std::fs::remove_dir_all(session::record_dir(&opts.template));
                return Err(e.context("running the generator command"));
            }
            let after = fsio::read_tree(&worktree)?;
            Some(session::PendingGenerator {
                command,
                tree_hash_after_exec: after.hash(),
            })
        }
        None => None,
    };

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
        foreach,
        generator,
    };
    sess.save(&opts.template)?;

    Ok(worktree)
}

/// Reconstruct the sample instance's composed part from a foreach session.
pub fn sample_part<'t>(
    template: &'t Template,
    foreach: &crate::session::ForeachSession,
    eval: &dyn weft_core::render::ExprEval,
    interaction: &mut dyn Interaction,
) -> Result<crate::compose::ComposedPart<'t>> {
    let inc = template.include(&foreach.include).with_context(|| {
        format!(
            "foreach session references include `{}` which no longer exists",
            foreach.include
        )
    })?;
    let children = crate::compose::resolve_child_parts(
        &inc.template,
        &foreach.answers,
        crate::compose::SecretMode::Resolve,
        eval,
        interaction,
    )?;
    Ok(crate::compose::ComposedPart {
        instance: crate::compose::ResolvedInstance {
            include: foreach.include.clone(),
            key: foreach.key.clone(),
            mount: crate::compose::mount_path(&inc.decl.path, &foreach.key)?,
            answers: foreach.answers.clone(),
        },
        template: &inc.template,
        patches: inc.template.patches.clone(),
        children,
    })
}

/// Drop secret values, keeping only plain answers (shared with resync's
/// metadata persistence).
pub(crate) fn strip_secrets(answers: &weft_core::AnswerSet) -> weft_core::AnswerSet {
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
    Ok(template.ancestor_closure(root))
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
