//! `weft new`: scaffold a template into a destination.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use weft_core::AnswerKind;
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::state::State;
use crate::template::Template;
use crate::{answers, fsio, tasks};

pub struct NewOptions {
    pub template: Utf8PathBuf,
    pub dest: Utf8PathBuf,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    /// Answers as a JSON object (inline, `@file`, or `-` for stdin).
    pub answers_json: Option<String>,
    pub skip_tasks: bool,
}

pub fn run(opts: &NewOptions, interaction: &mut dyn Interaction) -> Result<()> {
    let template = Template::load(&opts.template)?;
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

    let tree = weft_core::render::render(&template.patches, &resolved, &eval)
        .context("rendering template")?;

    fsio::ensure_empty_dest(&opts.dest)?;
    fsio::write_tree(&opts.dest, &tree)?;

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
    // Absolutize so `weft update` works from any cwd later.
    let template_abs = opts
        .template
        .canonicalize_utf8()
        .unwrap_or_else(|_| opts.template.clone());
    let state = State::new(
        template_abs.to_string(),
        template.patches.iter().map(|p| p.id).collect(),
        tree.hash(),
        &resolved,
        &secret_specs,
    );
    state.save(&opts.dest)?;

    if !opts.skip_tasks {
        let plan = tasks::plan(&template.manifest.tasks, &resolved, &eval, None)?;
        tasks::run(&plan, &opts.dest)?;
    }

    eprintln!(
        "scaffolded `{}` into {} ({} files)",
        template.manifest.template.name,
        opts.dest,
        tree.len()
    );
    Ok(())
}
