//! `weft new`: scaffold a template into a destination.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use weft_core::{AnswerKind, AnswerSet, Question};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::state::{InstanceState, State, StoredSource};
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
    /// What `.weft/state.toml` records as the template source (a resolved
    /// `hub:` or git ref). `None` = the local template path.
    pub stored: Option<StoredSource>,
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

/// Scaffold the template into `opts.dest`. Returns the root answers a person
/// typed at a prompt (see [`answers::Gathered::entered`]).
pub fn run(
    opts: &NewOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let template = Template::load_with(&opts.template, resolver)?;
    let eval = StarlarkEval;

    let layered = answers::layered_with_json_full(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        opts.answers_json.as_deref(),
    )?;
    // Split flat answers into parent + per-include child sets, resolve the
    // parent, then each include instance (binds → provided → child gather).
    let (parent_provided, child_provided) = compose::split_provided(&template, &layered.answers)?;
    let answers::Gathered {
        answers: resolved,
        entered,
    } = answers::gather_reviewed(
        &template,
        &parent_provided,
        &layered.constraints,
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

    // Plan the hooks of every active node, in composed order. Pre-hooks
    // (guards) run before writing anything — a failure aborts with nothing
    // but an empty dest dir. They run at the dest root (mounts don't exist
    // yet); post-hooks run inside their frame's mount.
    let plan = hooks::plan(&template, &resolved, &parts, &eval)?;

    fsio::ensure_empty_dest(&opts.dest)?;
    if !opts.skip_tasks {
        hooks::run_planned(&plan.pre, &opts.dest, &eval)
            .context("a pre-render hook failed; no files were written")?;
    }
    fsio::write_tree(&opts.dest, &tree)?;

    // Pin state: parent answers/base plus one entry per include instance.
    // Answers are stored with provenance: what was supplied (or prompted)
    // is given; defaults, computed values, and binds are derived and follow
    // the template on the next update.
    let parent_secrets = secret_specs(&template.manifest.questions, &resolved);
    let mut supplied = parent_provided;
    supplied.overlay(&entered);
    let root_answers = answers::provenance(
        &template.manifest.questions,
        &supplied,
        &Default::default(),
        &resolved,
        &eval,
    );
    let instance_states: Vec<InstanceState> = parts
        .iter()
        .map(|p| {
            let slot = (p.instance.include.clone(), p.instance.key.clone());
            let supplied = child_provided.get(&slot).cloned().unwrap_or_default();
            let (given, derived) = answers::provenance(
                &p.template.manifest.questions,
                &supplied,
                &compose::bind_ids(&template, &p.instance.include),
                &p.instance.answers,
                &eval,
            );
            InstanceState {
                include: p.instance.include.clone(),
                key: p.instance.key.clone(),
                mount: p.instance.mount.to_string(),
                base: p.patches.iter().map(|patch| patch.id).collect(),
                answers: given,
                derived,
                secrets: secret_specs(&p.template.manifest.questions, &p.instance.answers),
            }
        })
        .collect();
    let source = opts
        .stored
        .clone()
        .unwrap_or_else(|| StoredSource::path(&opts.template));
    let mut state = State::new(
        source,
        template.patches.iter().map(|p| p.id).collect(),
        tree.hash(),
        root_answers,
        &parent_secrets,
    )
    .with_instances(instance_states);
    state.save(&opts.dest)?;
    // The self-contained base: store the patch bodies this tree was rendered
    // from — root frame and every instance — so `weft update` can
    // reconstruct the merge base even after a template rewrites patch ids
    // (amend/squash/resync).
    crate::state::BaseSnapshot::from_render(&template.patches, &parts).save(&opts.dest)?;

    if !opts.skip_tasks {
        // A post-hook that fails leaves itself and the rest pending in the
        // state, for the next `weft update` to run.
        let post: Vec<&hooks::PlannedHook> = plan.post.iter().collect();
        crate::update::run_post_hooks(&post, &opts.dest, &mut state, &eval)?;
    }

    eprintln!(
        "scaffolded `{}` into {} ({} files)",
        template.manifest.template.name,
        opts.dest,
        tree.len()
    );
    Ok(entered)
}
