//! `weft instance`: manage repeatable-include instances of a scaffolded
//! project. Instances are **project-side data** pinned in
//! `.weft/state.toml`; the template only declares the repeatable slot.
//!
//! `add` and `remove` are implemented over the update path: `add` pins a new
//! instance with an empty base (so the old side renders nothing and every
//! child file lands as an addition), `remove` drops the instance from the
//! new side (template-deleted semantics: untouched files are deleted,
//! user-modified ones are kept and reported).

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;

use crate::interact::Interaction;
use crate::state::{InstanceState, State};
use crate::template::Template;
use crate::update::{self, UpdateOptions, UpdateReport};
use crate::{answers, compose};

pub struct InstanceAddOptions {
    pub dest: Utf8PathBuf,
    pub include: String,
    pub key: String,
    /// Child answers (`id=value`, child-scoped — no namespace needed).
    pub answers: Vec<String>,
    pub skip_tasks: bool,
}

pub fn add(
    opts: &InstanceAddOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<UpdateReport> {
    let mut state = State::load(&opts.dest)?;
    let template = Template::load_with(&Utf8PathBuf::from(&state.state.template), resolver)?;
    let inc = template.include(&opts.include).with_context(|| {
        format!(
            "template `{}` has no include named `{}`",
            template.manifest.template.name, opts.include
        )
    })?;
    if !inc.decl.repeat {
        bail!(
            "include `{}` is not repeatable; it always has exactly one instance",
            opts.include
        );
    }
    if !compose::valid_key(&opts.key) {
        bail!(
            "invalid instance key {:?} (lowercase alphanumerics, `-`, `_`)",
            opts.key
        );
    }
    if state
        .instances
        .iter()
        .any(|i| i.include == opts.include && i.key == opts.key)
    {
        bail!(
            "instance `{}` of include `{}` already exists",
            opts.key,
            opts.include
        );
    }

    // Parse explicit child answers against the child's questions.
    let mut provided = weft_core::AnswerSet::new();
    for arg in &opts.answers {
        let (id, value) = answers::parse_answer_arg(&inc.template.manifest.questions, arg)?;
        provided
            .insert_strict(id, value)
            .map_err(|e| anyhow::anyhow!("--answer {arg}: {e}"))?;
    }

    // Pin the instance with an empty base and only the explicit answers; the
    // update pass resolves the rest (binds, defaults, secrets, prompts) and
    // re-pins the full picture.
    state.instances.push(InstanceState {
        include: opts.include.clone(),
        key: opts.key.clone(),
        mount: compose::mount_path(&inc.decl.path, &opts.key)?.to_string(),
        base: vec![],
        answers: provided,
        secrets: Default::default(),
    });
    state
        .instances
        .sort_by(|a, b| (&a.include, &a.key).cmp(&(&b.include, &b.key)));
    state.save(&opts.dest)?;

    let report = update::run(
        &UpdateOptions {
            dest: opts.dest.clone(),
            dry_run: false,
            template_override: None,
            skip_tasks: opts.skip_tasks,
            drop_instances: vec![],
        },
        resolver,
        interaction,
    )?;
    eprintln!(
        "added instance `{}` of include `{}`",
        opts.key, opts.include
    );
    Ok(report)
}

pub fn remove(
    dest: &Utf8PathBuf,
    include: &str,
    key: &str,
    skip_tasks: bool,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<UpdateReport> {
    let state = State::load(dest)?;
    if !state
        .instances
        .iter()
        .any(|i| i.include == include && i.key == key)
    {
        bail!("no instance `{key}` of include `{include}` in this project");
    }
    let report = update::run(
        &UpdateOptions {
            dest: dest.clone(),
            dry_run: false,
            template_override: None,
            skip_tasks,
            drop_instances: vec![(include.to_owned(), key.to_owned())],
        },
        resolver,
        interaction,
    )?;
    eprintln!("removed instance `{key}` of include `{include}`");
    Ok(report)
}

/// Print the project's instances (include, key, mount).
pub fn list(dest: &Utf8PathBuf) -> Result<()> {
    let state = State::load(dest)?;
    if state.instances.is_empty() {
        eprintln!("no include instances");
        return Ok(());
    }
    for inst in &state.instances {
        println!(
            "{}={} @ {} ({} patch(es) pinned)",
            inst.include,
            inst.key,
            inst.mount,
            inst.base.len()
        );
    }
    Ok(())
}
