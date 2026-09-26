//! `weft session adopt`: link a directory you already have to a template, so
//! code you wrote in a real project can be promoted back into patches. The
//! inverse of `weft update`, which pushes template → project.
//!
//! A **scaffolded** project needs almost nothing: `weft new` already recorded
//! the template ref, the pinned base and the answers in `.weft/state.toml`
//! (see [`crate::state`]), so the diff is exactly your edits. Any other
//! directory must say which template, which base, and with what answers — and
//! usually wants a `--scope`, or the whole project reads as new files.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::AnswerKind;
use weft_lang::StarlarkEval;

use crate::discover::{self, WorktreeLink};
use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::state::State;
use crate::template::Template;
use crate::{answers, start};

pub struct AdoptOptions {
    /// The directory to adopt.
    pub path: Utf8PathBuf,
    /// The session's name.
    pub name: String,
    /// Template to record against. Required unless the directory was
    /// scaffolded by weft (then it comes from `.weft/state.toml`).
    pub template: Option<Utf8PathBuf>,
    /// Base state: `latest` or a patch name. Defaults to the project's own
    /// pinned base when scaffolded, `latest` otherwise.
    pub base: Option<String>,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    pub answers_json: Option<String>,
    /// Path globs weft may look at. Empty = the whole directory.
    pub scope: Vec<String>,
    /// Replace an existing session of this name instead of failing.
    pub force: bool,
}

/// Link `path` as a session's worktree. Returns (template root, worktree).
pub fn run(
    opts: &AdoptOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<(Utf8PathBuf, Utf8PathBuf)> {
    let name = &opts.name;
    if !session::valid_name(name) {
        bail!("invalid session name `{name}` (letters, digits, `-`, `_`, `.`)");
    }
    let worktree = discover::absolute(&opts.path);
    if !worktree.is_dir() {
        bail!("`{worktree}` is not a directory");
    }
    if discover::link_path(&worktree).is_file() && !opts.force {
        let link = WorktreeLink::load(&worktree)?;
        bail!(
            "`{worktree}` is already the worktree of session `{}` in `{}`; \
             end that session first, or pass --force",
            link.session,
            link.template
        );
    }

    // A weft-scaffolded project already recorded everything we need. A
    // remote source (hub/git) is not a directory to record into, though:
    // recording wants the author's own clone.
    let state = State::load(&worktree).ok();
    let template_path = match (&opts.template, &state) {
        (Some(path), _) => path.clone(),
        (None, Some(s)) => {
            if crate::source::kind(&s.state.template) != crate::source::Kind::Path {
                bail!(
                    "`{worktree}` was scaffolded from `{}`, a remote template; recording \
                     needs a writable checkout of it — pass --template DIR pointing at \
                     your clone",
                    s.state.template
                );
            }
            Utf8PathBuf::from(&s.state.template)
        }
        (None, None) => bail!(
            "`{worktree}` was not scaffolded by weft (no `.weft/state.toml`), \
             so it cannot tell which template it belongs to — pass --template DIR"
        ),
    };
    let template = Template::load_with(&template_path, resolver)?;
    let template_root = discover::absolute(&template.root);

    if Session::exists(&template_root, name) {
        if !opts.force {
            bail!(
                "session `{name}` already exists in `{template_root}`; \
                 pick another name, or pass --force"
            );
        }
        Session::load(&template_root, name)?.end(&template_root, name)?;
    }

    let eval = StarlarkEval;
    // Stored answers are the starting point; anything passed now overrides.
    let provided = {
        let mut merged = state
            .as_ref()
            .map(|s| s.answers.clone())
            .unwrap_or_default();
        let layered = answers::layered_with_json(
            &template,
            &opts.presets,
            opts.answers_file.as_deref(),
            &opts.answers,
            opts.answers_json.as_deref(),
        )?;
        for (k, v) in layered.iter() {
            merged.insert(k.clone(), v.clone());
        }
        merged
    };
    let resolved = answers::gather(
        &template,
        &provided,
        &weft_core::AnswerSet::new(),
        &eval,
        interaction,
    )?;

    // The base a scaffolded project should be diffed against is the one it was
    // rendered from — not `latest`, or every patch added since would read as a
    // deletion the adopter never made.
    let pinned: BTreeSet<_> = match (&opts.base, &state) {
        (Some(base), _) => start::pin_base(&template, base)?,
        (None, Some(s)) => {
            for id in &s.state.base {
                if !template.id_to_name.contains_key(id) {
                    bail!(
                        "the project pins patch {} which no longer exists in \
                         `{template_path}` (its history was rewritten); pass --base to \
                         choose a base explicitly",
                        id.short()
                    );
                }
            }
            s.state.base.iter().copied().collect()
        }
        (None, None) => start::pin_base(&template, "latest")?,
    };
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();
    let base_tree = weft_core::render::render(&base_patches, &resolved, &eval)
        .context("rendering the base state to diff against")?;

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
    Session {
        session: SessionMeta {
            base: base_patches.iter().map(|p| p.id).collect(),
            tree_hash: base_tree.hash(),
            worktree: Some(worktree.clone()),
            scope: opts.scope.clone(),
            adopted: true,
        },
        answers: start::strip_secrets(&resolved),
        secrets: secret_specs,
        foreach: None,
        generator: None,
        amend: None,
    }
    .save(&template_root, name)?;

    WorktreeLink {
        template: template_root.clone(),
        session: name.clone(),
    }
    .save(&worktree)?;

    Ok((template_root, worktree))
}
