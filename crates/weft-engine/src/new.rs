//! `weft new`: scaffold a template into a destination.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use weft_core::{AnswerKind, AnswerSet, Question};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::state::{InstanceState, State};
use crate::template::Template;
use crate::{answers, compose, fsio, hooks};

pub struct NewOptions {
    pub template: Utf8PathBuf,
    pub dest: Utf8PathBuf,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    /// Answers as a JSON object (inline, `@file`, or `-` for stdin).
    pub answers_json: Option<String>,
    /// Repeatable-include instance declarations (`include=key`). An instance
    /// is also implicitly declared by any `include.key.answer=…` answer.
    pub instances: Vec<String>,
    pub skip_tasks: bool,
    /// What `.weft/state.toml` records as the template (e.g. a resolved
    /// `hub:owner/name@version` ref). Defaults to the template path.
    pub stored_ref: Option<String>,
}

/// Parse `include=key` instance declarations.
pub fn parse_instance_args(
    args: &[String],
) -> Result<std::collections::BTreeSet<(String, String)>> {
    args.iter()
        .map(|arg| {
            arg.split_once('=')
                .map(|(i, k)| (i.to_owned(), k.to_owned()))
                .with_context(|| format!("--instance {arg:?} is not INCLUDE=KEY"))
        })
        .collect()
}

/// Secret specs (`answer id → source string`) for the answered secret
/// questions of a template.
pub(crate) fn secret_specs(
    questions: &[Question],
    resolved: &AnswerSet,
) -> BTreeMap<weft_core::AnswerId, String> {
    questions
        .iter()
        .filter_map(|q| match &q.kind {
            AnswerKind::Secret { source } if resolved.contains(&q.id) => {
                Some((q.id.clone(), source.to_string()))
            }
            _ => None,
        })
        .collect()
}

pub fn run(
    opts: &NewOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let template = Template::load_with(&opts.template, resolver)?;
    let eval = StarlarkEval;

    let provided = answers::layered_with_json(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        opts.answers_json.as_deref(),
    )?;
    // Split flat answers into parent + per-include child sets, resolve the
    // parent, then each include instance (binds → provided → child gather).
    let (parent_provided, child_provided) = compose::split_provided(&template, &provided)?;
    let resolved = answers::gather(
        &template,
        &parent_provided,
        &weft_core::AnswerSet::new(),
        &eval,
        interaction,
    )?;
    let declared = parse_instance_args(&opts.instances)?;
    let instances = compose::resolve_instances(
        &template,
        &resolved,
        &child_provided,
        &declared,
        &eval,
        interaction,
    )?;
    let parts = compose::full_parts(&template, instances, &eval, interaction)?;

    let tree = compose::render_composed(&template.patches, &resolved, &parts, &eval)
        .context("rendering template")?;

    // Collect hooks: the parent's, plus each instance's child hooks. Pre-hooks
    // (guards) run before writing anything — a failure aborts with nothing
    // but an empty dest dir. Child pre-hooks run at the dest root (their
    // mount doesn't exist yet); child post-hooks run inside their mount.
    let collected = hooks::collect(&template, &resolved, &eval)?;
    let child_collected: Vec<_> = parts
        .iter()
        .map(|p| hooks::collect(p.template, &p.instance.answers, &eval))
        .collect::<Result<_>>()?;

    fsio::ensure_empty_dest(&opts.dest)?;
    if !opts.skip_tasks {
        hooks::run(&collected.pre, &opts.dest, &resolved, &eval)
            .context("a pre-render hook failed; no files were written")?;
        for (part, child) in parts.iter().zip(&child_collected) {
            hooks::run(&child.pre, &opts.dest, &part.instance.answers, &eval).with_context(
                || {
                    format!(
                        "a pre-render hook of include `{}` failed; no files were written",
                        part.instance.include
                    )
                },
            )?;
        }
    }
    fsio::write_tree(&opts.dest, &tree)?;

    // Pin state: parent answers/base plus one entry per include instance.
    let parent_secrets = secret_specs(&template.manifest.questions, &resolved);
    let instance_states: Vec<InstanceState> = parts
        .iter()
        .map(|p| InstanceState {
            include: p.instance.include.clone(),
            key: p.instance.key.clone(),
            mount: p.instance.mount.to_string(),
            base: p.patches.iter().map(|patch| patch.id).collect(),
            answers: State::plain_answers(&p.instance.answers),
            secrets: secret_specs(&p.template.manifest.questions, &p.instance.answers),
        })
        .collect();
    // Absolutize so `weft update` works from any cwd later.
    let stored_ref = opts.stored_ref.clone().unwrap_or_else(|| {
        opts.template
            .canonicalize_utf8()
            .unwrap_or_else(|_| opts.template.clone())
            .to_string()
    });
    let state = State::new(
        stored_ref,
        template.patches.iter().map(|p| p.id).collect(),
        tree.hash(),
        &resolved,
        &parent_secrets,
    )
    .with_instances(instance_states);
    state.save(&opts.dest)?;
    // The self-contained base: store the patch bodies this tree was rendered
    // from, so `weft update` can reconstruct the merge base even after the
    // template rewrites patch ids (amend/squash/resync).
    crate::state::BaseSnapshot::from_patches(&template.patches).save(&opts.dest)?;

    if !opts.skip_tasks {
        // Child post-hooks first (inside their mounts), then the parent's —
        // so parent finalize hooks (format, git commit) run last.
        for (part, child) in parts.iter().zip(&child_collected) {
            let cwd = opts.dest.join(&part.instance.mount);
            hooks::run(&child.post, &cwd, &part.instance.answers, &eval)?;
        }
        hooks::run(&collected.post, &opts.dest, &resolved, &eval)?;
    }

    eprintln!(
        "scaffolded `{}` into {} ({} files)",
        template.manifest.template.name,
        opts.dest,
        tree.len()
    );
    Ok(())
}
