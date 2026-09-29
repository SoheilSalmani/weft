//! `weft session new`: materialize a pinned base state into a worktree the
//! author edits with real tools. Their dirty working directory never leaks
//! in — the worktree starts as exactly `render(base, answers)`.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::{AnswerKind, PatchId};
use weft_lang::StarlarkEval;

use crate::discover::WorktreeLink;
use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::template::Template;
use crate::{answers, fsio};

pub struct StartOptions {
    pub template: Utf8PathBuf,
    /// The session's name; also its directory under `.weft-sessions/`.
    pub name: String,
    /// Where to put the worktree. `None` = the default location inside the
    /// session directory.
    pub path: Option<Utf8PathBuf>,
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
    /// Discard an existing session of the same name instead of erroring.
    pub force: bool,
}

pub fn run(
    opts: &StartOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<Utf8PathBuf> {
    let template = Template::load_with(&opts.template, resolver)?;
    let name = &opts.name;
    if !session::valid_name(name) {
        bail!("invalid session name `{name}` (letters, digits, `-`, `_`, `.`)");
    }
    if Session::exists(&opts.template, name) {
        if opts.force {
            let existing = Session::load(&opts.template, name)?;
            existing.end(&opts.template, name)?;
        } else {
            bail!(
                "session `{name}` already exists in `{}`; \
                 pick another name, or `weft session end {name}` to finish it",
                opts.template
            );
        }
    }

    let eval = StarlarkEval;
    let layered = answers::layered_with_json_full(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        opts.answers_json.as_deref(),
    )?;
    // Namespaced answers seed the single includes mounted into the base;
    // repeat instances are project-time and only enter a session as the one
    // `--foreach` sample.
    let (parent_provided, child_provided) =
        crate::compose::split_provided(&template, &layered.answers)?;
    let mut include_provided: BTreeMap<String, weft_core::AnswerSet> = BTreeMap::new();
    for ((include, key), set) in child_provided {
        if template.include(&include).is_some_and(|i| i.decl.repeat) {
            bail!(
                "answers for instance `{key}` of repeat include `{include}` cannot seed a \
                 session; mount one sample with `--foreach {include}={key}` instead"
            );
        }
        include_provided.insert(include, set);
    }
    let resolved = answers::gather_reviewed(
        &template,
        &parent_provided,
        &layered.constraints,
        &eval,
        interaction,
    )?
    .answers;

    // The base is a set of composed-graph nodes: root patches plus the single
    // includes' patches (their files are what a parent hunk anchors on).
    // Foreach patches are excluded — nothing may depend on them, and a foreach
    // session's worktree must not contain other foreach patches' output.
    let pinned = pin_base(&template, &opts.base)?;
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id))
        .cloned()
        .collect();
    let mut parts = crate::compose::single_parts(
        &template,
        &resolved,
        &include_provided,
        &BTreeMap::new(),
        &eval,
        interaction,
    )?;
    crate::compose::filter_parts(&mut parts, &pinned);
    let instances: Vec<session::SessionInstance> = parts
        .iter()
        .map(|p| session::SessionInstance {
            include: p.instance.include.clone(),
            answers: strip_secrets(&p.instance.answers),
            secrets: crate::new::secret_specs(&p.template.manifest.questions, &p.instance.answers),
        })
        .collect();

    // A `--foreach include=key` session mounts one *sample* instance of the
    // include into the base, so the author edits parent files against a
    // concrete example (e.g. adds `use ./services/payments` to go.work).
    // Commit later abstracts the sample key/answers into `key` /
    // `instance_<id>` references.
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
    if let Some(f) = &foreach {
        parts.push(sample_part(&template, f, &eval, interaction)?);
    }
    let tree = crate::compose::render_composed(&base_patches, &resolved, &parts, &eval)
        .context("rendering base state")?;

    let worktree = match &opts.path {
        Some(path) => {
            let path = crate::discover::absolute(path);
            if path.is_dir()
                && path
                    .read_dir_utf8()
                    .map(|mut d| d.next().is_some())
                    .unwrap_or(false)
            {
                bail!(
                    "`{path}` is not empty; pick an empty or absent directory, \
                     or link the existing one with `weft session adopt`"
                );
            }
            path
        }
        None => session::default_worktree_dir(&opts.template, name),
    };
    std::fs::create_dir_all(&worktree)?;
    fsio::write_tree(&worktree, &tree)?;
    // The back-pointer that lets weft be run from inside the worktree.
    WorktreeLink {
        template: crate::discover::absolute(&opts.template),
        session: name.clone(),
    }
    .save(&worktree)?;

    // `--exec`: the command's output *is* the patch content. Run it now so
    // the author can inspect (`weft diff`) before committing; the command is
    // held in the session and stored on the patch at commit.
    let generator = match &opts.exec {
        Some(cmd) => {
            if foreach.is_some() {
                bail!("--exec cannot be combined with --foreach (unsupported for resync)");
            }
            // `${…}` interpolates when it names a declared answer or a
            // Starlark expression over them; anything else stays literal
            // for the shell. Echo what was recognized so typos are visible.
            let command = crate::hooks::parse_command(
                cmd,
                &template.manifest.questions,
                &weft_core::AnswerSet::new(),
                &eval,
            );
            if !command.is_literal() {
                eprintln!("generator interpolates: {}", command.source());
            }
            if let Err(e) =
                crate::hooks::run_command(&command, "generator", &worktree, &resolved, &eval)
            {
                // Nothing committed yet — don't leave a broken session behind.
                let _ = std::fs::remove_dir_all(&worktree);
                let _ = Session::discard(&opts.template, name);
                return Err(e.context("running the generator command"));
            }
            // Same `.weftignore` filtering as commit will apply, so the
            // post-exec hash and the commit-time work tree agree.
            let keep: BTreeSet<_> = tree.paths().cloned().collect();
            let after = fsio::read_tree_ignoring(&worktree, &template.ignore, &keep)?;
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
            base: pinned.into_iter().collect(),
            tree_hash: tree.hash(),
            // Only record a path when it isn't the default location, so the
            // session file stays portable with the template.
            worktree: opts.path.is_some().then(|| worktree.clone()),
            scope: Vec::new(),
            adopted: false,
        },
        answers: strip_secrets(&resolved),
        secrets: secret_specs,
        instances,
        foreach,
        generator,
        amend: None,
    };
    sess.save(&opts.template, name)?;

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
            repeat: inc.decl.repeat,
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

/// Resolve a `--base` ref to a set of composed-graph node ids: `latest` is
/// every node, a name is that node plus its ancestors. Foreach patches and
/// repeat includes' opaque nodes are never part of a base.
pub fn pin_base(template: &Template, base: &str) -> Result<BTreeSet<PatchId>> {
    let pinnable = |id: &PatchId| {
        template
            .node(*id)
            .and_then(|n| template.node_patch(n))
            .is_some_and(|p| p.foreach.is_none())
    };
    if base == "latest" {
        return Ok(template
            .nodes
            .iter()
            .map(|n| n.id)
            .filter(pinnable)
            .collect());
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
    if !pinnable(&root) {
        bail!("`{base}` cannot be a base: foreach patches and repeat includes are not pinnable");
    }
    Ok(template
        .ancestor_closure(root)
        .into_iter()
        .filter(pinnable)
        .collect())
}

/// Root-frame leaves of an active node set: root patches no *other active
/// root* patch depends on. These become a recorded patch's default
/// dependencies; edits under an include's mount add that include's nodes
/// (see `commit::include_deps`).
pub fn base_leaves(template: &Template, active: &BTreeSet<PatchId>) -> Vec<String> {
    let mut depended_upon: BTreeSet<PatchId> = BTreeSet::new();
    for p in &template.patches {
        if active.contains(&p.id) {
            depended_upon.extend(p.depends_on.iter().copied());
        }
    }
    template
        .patches
        .iter()
        .filter(|p| active.contains(&p.id) && !depended_upon.contains(&p.id))
        .filter_map(|p| template.id_to_name.get(&p.id).cloned())
        .collect()
}
