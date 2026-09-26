//! `weft commit`: diff the recording worktree against its pinned base,
//! abstract concrete values back into answer references, and append the
//! result as a new patch.

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use weft_core::{AnswerSet, StarlarkExpr, Value};
use weft_lang::StarlarkEval;

use crate::abstraction::Abstractor;
use crate::interact::Interaction;
use crate::session::{self, Session, SessionMeta};
use crate::start::base_leaves;
use crate::template::Template;
use crate::{diff, fsio, secrets};

pub struct CommitOptions {
    pub template: Utf8PathBuf,
    /// Which session's worktree to commit.
    pub session: String,
    /// Patch name; defaults to `patch-NNN`.
    pub name: Option<String>,
    /// Optional `when` condition for the new patch.
    pub when: Option<String>,
    /// Display title, e.g. "Add Prisma support" (metadata, not hashed).
    pub title: Option<String>,
    /// Human/agent-facing description stored in the patch file (not hashed).
    pub describe: Option<String>,
    pub tags: Vec<String>,
    /// Per-answer abstraction decisions supplied by a non-terminal caller
    /// (agents/UIs). `None` = confirm interactively; undecided candidates
    /// default to accepted, secrets are always abstracted.
    pub decisions: Option<std::collections::BTreeMap<String, bool>>,
    /// Occurrences to keep literal, as `ANSWER@PATH:LINE[:NTH]` (nth
    /// defaults to 1). Applies to added/created content; secrets can never
    /// be kept literal.
    pub keep_literal: Vec<String>,
    /// How the next patch should relate to this one when changes remain after
    /// the commit. `None` prompts interactively and defaults to `Sibling` when
    /// non-interactive.
    pub link: Option<CommitLink>,
    /// Place the patch in the graph explicitly instead of depending on the
    /// base's active leaves. Names must be patches in the session's base.
    pub depends_on: Option<Vec<String>>,
}

/// After a commit that leaves more work, how the next patch relates to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitLink {
    /// Advance the base to include this patch: the next patch builds on it.
    Stack,
    /// Keep the base where it is and peel this patch's content out of the
    /// session: the next patch is an independent sibling that commutes with it.
    Sibling,
}

/// Parse `--keep-literal ANSWER@PATH:LINE[:NTH]` into the excepted set.
pub fn parse_keep_literal(specs: &[String]) -> Result<crate::abstraction::Excepted> {
    let mut excepted = crate::abstraction::Excepted::new();
    for spec in specs {
        let (answer, rest) = spec.split_once('@').with_context(|| {
            format!("--keep-literal expects ANSWER@PATH:LINE[:NTH], got `{spec}`")
        })?;
        let mut parts = rest.rsplitn(3, ':');
        // rsplit so paths containing `:` are still wrong loudly, not silently.
        let first = parts.next().unwrap_or_default();
        let second = parts.next();
        let third = parts.next();
        let (path, line, nth) = match (second, third) {
            (Some(line_s), Some(path)) => (path, line_s, first),
            (Some(path), None) => (path, first, "1"),
            _ => bail!("--keep-literal expects ANSWER@PATH:LINE[:NTH], got `{spec}`"),
        };
        let line: usize = line
            .parse()
            .with_context(|| format!("`{line}` is not a line number in `{spec}`"))?;
        let nth: usize = nth
            .parse()
            .with_context(|| format!("`{nth}` is not an occurrence index in `{spec}`"))?;
        excepted.insert((
            weft_core::AnswerId(answer.to_owned()),
            camino::Utf8PathBuf::from(path),
            line,
            nth,
        ));
    }
    Ok(excepted)
}

pub fn run(
    opts: &CommitOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let template = Template::load_with(&opts.template, resolver)?;
    let SessionTrees {
        sess,
        answers,
        base_patches,
        parts,
        base_tree,
        work_tree: full_work,
        worktree_root,
    } = session_trees(&template, &opts.session, interaction)?;
    let eval = StarlarkEval;
    // Edits under a repeat include's instances are only recordable against
    // the foreach sample; everything else under a mount is legal and becomes
    // a dependency on the child nodes that own the files (below).
    guard_instances(&template, sess.foreach.as_ref(), &base_tree, &full_work)?;
    // Refuse impossible options before writing anything: the sibling
    // transition reverts committed paths in the worktree, which on an adopted
    // project would delete the author's own files.
    if sess.session.adopted && opts.link == Some(CommitLink::Sibling) {
        bail!(
            "`--sibling` would revert the committed files in `{worktree_root}` back to the \
             base state, and that worktree is an adopted project, not a scratch render. \
             Commit it stacked (the default there) instead."
        );
    }

    // The staged tree (`weft add`) is what we commit. An empty index stages
    // nothing, so the staged tree equals the base and we fall back to the whole
    // worktree — the classic one-shot flow. When something is staged, the diff
    // (and everything downstream) sees only the staged subset.
    let staged =
        crate::stage::staged_tree(&opts.template, &opts.session, &template.ignore, &base_tree)?;
    let index_active = staged.hash() != base_tree.hash();
    if index_active && (sess.foreach.is_some() || sess.generator.is_some() || sess.amend.is_some())
    {
        bail!("staging (`weft add`) is not supported for a --foreach, --exec, or amend session");
    }
    let work_tree = if index_active {
        staged
    } else {
        full_work.clone()
    };

    // Foreach patches render with `key` and `instance_<id>` in scope, so the
    // abstractor must see the sample instance's values under those names.
    // `key` is canonical: instance answers that merely echo the key (e.g. a
    // `service_name = "key"` bind) are dropped so abstraction prefers `key`.
    let abstractor = match sess.foreach.as_ref().and_then(|f| sample_of(&parts, f)) {
        Some(part) => {
            let scope = crate::compose::foreach_scope(&answers, &part.instance);
            let key_value = weft_core::Value::String(part.instance.key.clone());
            let deduped: weft_core::AnswerSet = scope
                .iter()
                .filter(|(id, v)| id.0 == "key" || **v != key_value)
                .map(|(id, v)| (id.clone(), v.clone()))
                .collect();
            Abstractor::from_answers(&deduped)
        }
        None => Abstractor::from_answers(&answers),
    };
    // Generator sessions warn when the author edited on top of the command's
    // output — resync re-runs the command only, so those edits won't survive.
    if let Some(pending) = &sess.generator {
        if work_tree.hash() != pending.tree_hash_after_exec {
            eprintln!(
                "warning: the worktree was edited after the generator command ran; \
                 those edits will be lost on `weft patch resync`"
            );
        }
    }
    let keep_literal = parse_keep_literal(&opts.keep_literal)?;
    let ops = match &opts.decisions {
        Some(decisions) => {
            let texts = diff::collect_texts(&base_tree, &work_tree);
            let decisions = decisions
                .iter()
                .map(|(k, v)| (weft_core::AnswerId(k.clone()), *v))
                .collect();
            let confirmed = abstractor.confirmed_from_decisions(&texts, &decisions);
            diff::build_ops_decided(
                &base_tree,
                &work_tree,
                &abstractor,
                &confirmed,
                &keep_literal,
            )
        }
        // Generator sessions always take the scripted path (confirm-all
        // minus --keep-literal): interactive per-occurrence choices can't be
        // replayed by resync, so they are disabled here.
        None if !keep_literal.is_empty() || sess.generator.is_some() => {
            // Scripted per-occurrence decisions: confirm everything (the
            // --yes semantics) minus the kept-literal occurrences.
            let texts = diff::collect_texts(&base_tree, &work_tree);
            let confirmed = abstractor.confirmed_from_decisions(&texts, &Default::default());
            diff::build_ops_decided(
                &base_tree,
                &work_tree,
                &abstractor,
                &confirmed,
                &keep_literal,
            )
        }
        None => diff::build_ops(&base_tree, &work_tree, &abstractor, interaction)?,
    };
    if ops.is_empty() {
        bail!("worktree has no changes against the base state; nothing to commit");
    }

    // An amend session rewrites an existing patch in place rather than
    // appending a new one.
    if let Some(target) = sess.amend.clone() {
        return crate::amend::finish(
            &template,
            &opts.template,
            &sess,
            &opts.session,
            &target,
            &base_patches,
            &parts,
            &answers,
            &work_tree,
            ops,
            interaction,
        );
    }

    let name = match &opts.name {
        Some(name) => name.clone(),
        None => format!("patch-{:03}", template.patches.len() + 1),
    };
    let when = opts.when.clone().map(StarlarkExpr);
    // Depend only on the *active* nodes of the base: a patch gated off under
    // the session answers contributed nothing to the recorded state, and
    // depending on it would gate the new patch off with it. Root-frame
    // leaves by default (or what `--depends-on` names), plus the child nodes
    // owning any file edited under an include's mount.
    let active = crate::compose::active_nodes(&base_patches, &answers, &parts, &eval)?;
    let inferred = include_deps(
        &template,
        &parts,
        sess.foreach.as_ref(),
        &base_tree,
        &work_tree,
        &active,
        &eval,
    )?;
    let mut depends_on = match &opts.depends_on {
        Some(declared) => declared_deps(&template, declared, &active)?,
        None => base_leaves(&template, &active),
    };
    for dep in &inferred {
        if !depends_on.contains(dep) {
            depends_on.push(dep.clone());
        }
    }

    // Validation before writing: the new patch applied to the base must
    // reproduce the worktree byte-for-byte (this also catches ambiguous
    // hunk contexts). Skipped when a `when` gate is false under the
    // session answers.
    let foreach_include = sess.foreach.as_ref().map(|f| f.include.clone());
    let candidate = weft_core::Patch::new_foreach(
        depends_on.iter().map(|n| template.name_to_id[n]).collect(),
        when.clone(),
        foreach_include.clone(),
        ops.clone(),
    );
    let new_id = candidate.id;
    let mut with_new = base_patches.clone();
    with_new.push(candidate.clone());
    let replayed = crate::compose::render_composed(&with_new, &answers, &parts, &eval)
        .context("replaying the recorded patch against the base state")?;

    // Declared or inferred dependencies are a claim of independence from
    // everything else in the base, so prove it: the patch must also apply
    // against its own dependency closure alone. A hunk that only anchors
    // because of a sibling fails here instead of at `weft check` (or in
    // someone's scaffold).
    if opts.depends_on.is_some() || !inferred.is_empty() {
        let closure: BTreeSet<_> = depends_on
            .iter()
            .flat_map(|n| template.ancestor_closure(template.name_to_id[n]))
            .collect();
        let mut alone: Vec<_> = base_patches
            .iter()
            .filter(|p| closure.contains(&p.id))
            .cloned()
            .collect();
        alone.push(candidate);
        let mut alone_parts = parts.clone();
        crate::compose::filter_parts(&mut alone_parts, &closure);
        if let Err(e) = crate::compose::render_composed(&alone, &answers, &alone_parts, &eval) {
            bail!(
                "this patch does not apply with only `{}` in the base: {e}\n\
                 its content anchors on something else in the session's base — \
                 declare that patch too with --depends-on, or drop --depends-on/--after \
                 to depend on the base's leaves",
                depends_on.join(", ")
            );
        }
    }
    if !inferred.is_empty() {
        eprintln!(
            "edits under an include's mount: depending on {}",
            inferred.join(", ")
        );
    }
    let gate_open = match &when {
        None => true,
        Some(expr) => eval_gate(expr, &answers)?,
    };
    // Uncommitted work left in the worktree beyond this patch, and the paths
    // this patch touches.
    let remaining = crate::stage::changed_paths(&work_tree, &full_work);
    let committed = crate::stage::changed_paths(&base_tree, &work_tree);
    if !remaining.is_empty() && !gate_open {
        bail!(
            "this patch is gated off under the session answers, so the session \
             cannot continue on top of it; commit it in its own session, or change \
             the answers with `weft session refresh`"
        );
    }
    if gate_open && replayed.hash() != work_tree.hash() {
        bail!(
            "replaying the recorded patch does not reproduce the worktree \
             (likely an ambiguous hunk context); refusing to write a broken patch"
        );
    }

    // A generator session stores the command + record-time answers (secrets
    // as source refs) so `weft patch resync` can reproduce this patch.
    let generator = sess.generator.as_ref().map(|p| weft_core::Generator {
        command: p.command.clone(),
        answers: sess.answers.clone(),
        secrets: sess.secrets.clone(),
        keep_literal: opts.keep_literal.clone(),
    });
    template.write_patch_full(
        &name,
        depends_on,
        when,
        foreach_include,
        ops.clone(),
        weft_core::PatchMeta {
            title: opts.title.clone(),
            description: opts.describe.clone(),
            tags: opts.tags.clone(),
            hooks: vec![],
            generator,
        },
    )?;
    // The index is now empty against whatever base we end at.
    crate::stage::clear(&opts.template, &opts.session)?;

    let noun = template.manifest.template.name.clone();
    if remaining.is_empty() {
        // The whole worktree is committed: end the session (the classic
        // one-shot flow, and what the existing tests expect).
        sess.end(&opts.template, &opts.session)?;
        eprintln!(
            "committed patch `{name}` with {} op(s) to `{noun}`",
            ops.len()
        );
        return Ok(());
    }

    // Changes remain, so the session stays open. Decide how the next patch
    // relates to this one.
    //
    // An adopted worktree is the author's real project, not a scratch render:
    // peeling the committed content back out would delete the very files they
    // just promoted. Stack is the only safe transition there (checked up
    // front, before anything is written).
    let link = if sess.session.adopted {
        CommitLink::Stack
    } else {
        match opts.link {
            Some(l) => l,
            None => {
                if interaction.confirm(
                    "keep building on this patch? (no = start the next patch as an independent sibling)",
                    false,
                )? {
                    CommitLink::Stack
                } else {
                    CommitLink::Sibling
                }
            }
        }
    };
    match link {
        CommitLink::Stack => {
            // Advance the base to include this patch; the next patch depends on
            // it. The stored tree hash is the staged tree, which is exactly
            // `render(base + patch)`.
            let mut base = sess.session.base.clone();
            base.push(new_id);
            Session {
                session: SessionMeta {
                    base,
                    tree_hash: work_tree.hash(),
                    ..sess.session
                },
                answers: sess.answers.clone(),
                secrets: sess.secrets.clone(),
                instances: sess.instances.clone(),
                foreach: None,
                generator: None,
                amend: None,
            }
            .save(&opts.template, &opts.session)?;
            eprintln!(
                "committed patch `{name}` to `{noun}`, building on it. \
                 {} file(s) still uncommitted",
                remaining.len()
            );
        }
        CommitLink::Sibling => {
            // Keep the base where it is and peel this patch's content out of the
            // worktree, so the next patch is diffed against the same base and
            // commutes with this one.
            session::revert_worktree_paths(&worktree_root, &base_tree, &committed)?;
            eprintln!(
                "committed patch `{name}` to `{noun}` as an independent sibling. \
                 {} file(s) still uncommitted",
                remaining.len()
            );
        }
    }
    Ok(())
}

/// Validate `--depends-on` / `--after` names: each must be a node in the
/// session's *active* base (a root patch or `<include>/<patch>`), since the
/// recorded content was diffed against exactly that state.
fn declared_deps(
    template: &Template,
    declared: &[String],
    active_base: &BTreeSet<weft_core::PatchId>,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for name in declared {
        let id = template.name_to_id.get(name).with_context(|| {
            format!("--depends-on `{name}`: no patch by that name in this template")
        })?;
        if !active_base.contains(id) {
            bail!(
                "--depends-on `{name}`: that patch is not in this session's base, so the \
                 recorded content was never diffed against it. Base a new session on it \
                 (`weft session new N --base {name}`) to build on top."
            );
        }
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    if out.is_empty() {
        bail!("--depends-on needs at least one patch name");
    }
    Ok(out)
}

fn eval_gate(expr: &StarlarkExpr, answers: &AnswerSet) -> Result<bool> {
    use weft_core::render::ExprEval;
    Ok(StarlarkEval.eval_bool(expr, answers)?)
}

/// The reconstructed state of the active record session: resolved answers,
/// the pinned base (root patches + mounted single includes, plus the foreach
/// sample), the composed base tree, and the current worktree. Shared by
/// `commit` and `weft diff`.
pub struct SessionTrees<'t> {
    pub sess: Session,
    pub answers: weft_core::AnswerSet,
    pub base_patches: Vec<weft_core::Patch>,
    pub parts: Vec<crate::compose::ComposedPart<'t>>,
    pub base_tree: weft_core::Tree,
    pub work_tree: weft_core::Tree,
    /// Where this session's worktree lives (it may be anywhere on disk).
    pub worktree_root: Utf8PathBuf,
}

pub fn session_trees<'t>(
    template: &'t Template,
    name: &str,
    interaction: &mut dyn Interaction,
) -> Result<SessionTrees<'t>> {
    let sess = Session::load(&template.root, name)?;
    let eval = StarlarkEval;
    let answers = resolve_session_answers(template, &sess, interaction)?;
    let (base_patches, mut parts) = session_base(template, &sess, &answers, interaction)?;
    // A foreach session's base includes the mounted sample instance.
    if let Some(f) = &sess.foreach {
        parts.push(crate::start::sample_part(template, f, &eval, interaction)?);
    }
    let full_base = crate::compose::render_composed(&base_patches, &answers, &parts, &eval)
        .context("re-rendering base state")?;
    if full_base.hash() != sess.session.tree_hash {
        bail!(
            "base state hash changed since the session started (template or secret \
             values moved underneath it); start a new session"
        );
    }

    // An adopted worktree may opt only part of itself in. The scope is applied
    // to the base as well, or base files the project never adopted would read
    // as deletions.
    let unrestricted = sess.session.scope.is_empty();
    let scope = crate::stage::globset(&sess.session.scope)?;
    let base_tree = crate::stage::in_scope(&full_base, &scope, unrestricted);

    let worktree_root = sess.worktree(&template.root, name);
    // `.weftignore` filters junk (generator side-products, OS files) out of
    // the read-back — except paths the base rendered, so deletions of
    // rendered files are still recorded.
    let keep: std::collections::BTreeSet<_> = base_tree.paths().cloned().collect();
    let full_work = fsio::read_tree_ignoring(&worktree_root, &template.ignore, &keep)?;
    let work_tree = crate::stage::in_scope(&full_work, &scope, unrestricted);
    Ok(SessionTrees {
        sess,
        answers,
        base_patches,
        parts,
        base_tree,
        work_tree,
        worktree_root,
    })
}

/// A session's pinned base as (root patches, single-include parts): every
/// pinned node must still exist in the template; the mounted includes are
/// rebuilt from the stored child answers (secrets re-resolved from their
/// references) and cut down to the pinned nodes. Shared by commit, refresh,
/// and amend.
pub(crate) fn session_base<'t>(
    template: &'t Template,
    sess: &Session,
    answers: &AnswerSet,
    interaction: &mut dyn Interaction,
) -> Result<(Vec<weft_core::Patch>, Vec<crate::compose::ComposedPart<'t>>)> {
    for id in &sess.session.base {
        if template.node(*id).is_none() {
            bail!(
                "pinned base patch {} no longer exists in the template; \
                 the template changed since the session started — start a new one",
                id.short()
            );
        }
    }
    let pinned: BTreeSet<_> = sess.session.base.iter().copied().collect();
    // Foreach patches never participate in a base — the new patch must not
    // depend on one (graph-leaf rule).
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| pinned.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();
    for inc in template.includes.iter().filter(|i| !i.decl.repeat) {
        if !sess.instances.iter().any(|i| i.include == inc.decl.name) {
            bail!(
                "include `{}` was added to the template after this session started; \
                 start a new one",
                inc.decl.name
            );
        }
    }
    let mut provided = std::collections::BTreeMap::new();
    let mut presolved = std::collections::BTreeMap::new();
    for inst in &sess.instances {
        let inc = template.include(&inst.include).with_context(|| {
            format!(
                "include `{}` no longer exists in the template; start a new session",
                inst.include
            )
        })?;
        let mut secrets = AnswerSet::new();
        resolve_secret_refs(&inc.template, &inst.secrets, &mut secrets, interaction)?;
        provided.insert(inst.include.clone(), inst.answers.clone());
        presolved.insert(inst.include.clone(), secrets);
    }
    let mut parts = crate::compose::single_parts(
        template,
        answers,
        &provided,
        &presolved,
        &StarlarkEval,
        interaction,
    )?;
    crate::compose::filter_parts(&mut parts, &pinned);
    Ok((base_patches, parts))
}

/// The foreach sample part of a session, if mounted.
fn sample_of<'p, 't>(
    parts: &'p [crate::compose::ComposedPart<'t>],
    f: &crate::session::ForeachSession,
) -> Option<&'p crate::compose::ComposedPart<'t>> {
    parts
        .iter()
        .find(|p| p.instance.repeat && p.instance.include == f.include && p.instance.key == f.key)
}

/// One changed file in a session preview.
#[derive(Debug, serde::Serialize)]
pub struct PreviewFile {
    pub path: camino::Utf8PathBuf,
    /// `created` | `modified` | `deleted`.
    pub change: &'static str,
    pub before: String,
    pub after: String,
    /// 1-based lines of `after` a diff marks as added (occurrence targets).
    pub added_lines: std::collections::BTreeSet<usize>,
    /// Either side is binary: before/after are empty, content is opaque.
    #[serde(default)]
    pub binary: bool,
}

/// What `weft diff` (and UIs) show before a commit.
#[derive(Debug, serde::Serialize)]
pub struct Preview {
    pub files: Vec<PreviewFile>,
    pub candidates: Vec<crate::abstraction::Candidate>,
    pub occurrences: Vec<crate::abstraction::Occurrence>,
}

/// Compute the pre-commit preview for a session (the whole worktree against
/// the base).
pub fn preview(
    template: &Template,
    session: &str,
    interaction: &mut dyn Interaction,
) -> Result<Preview> {
    preview_target(template, session, false, interaction)
}

/// The pre-commit preview, diffing the base against either the whole worktree
/// (`staged = false`) or the staged tree (`staged = true`, i.e. exactly what
/// the next `weft commit` will write). An empty index makes the two identical.
pub fn preview_target(
    template: &Template,
    session: &str,
    staged: bool,
    interaction: &mut dyn Interaction,
) -> Result<Preview> {
    let trees = session_trees(template, session, interaction)?;
    let base_tree = &trees.base_tree;
    let after = if staged {
        crate::stage::staged_tree(&template.root, session, &template.ignore, base_tree)?
    } else {
        trees.work_tree.clone()
    };
    let abstractor = Abstractor::from_answers(&trees.answers);
    let texts = diff::collect_texts(base_tree, &after);
    let candidates = abstractor.candidates(&texts);
    let occurrences = diff::added_occurrences(base_tree, &after, &abstractor);

    // Binary sides show as opaque (empty text, `binary: true`) — previews
    // and diffs never try to line-render bytes.
    let text_of = |e: &weft_core::FileEntry| e.content.text().unwrap_or_default().to_owned();
    let mut files = Vec::new();
    for (path, entry) in after.iter() {
        match base_tree.get(path) {
            None => files.push(PreviewFile {
                path: path.clone(),
                change: "created",
                before: String::new(),
                added_lines: match entry.content.text() {
                    Some(text) => (1..=text.lines().count()).collect(),
                    None => Default::default(),
                },
                after: text_of(entry),
                binary: entry.content.is_binary(),
            }),
            Some(base_entry) if base_entry.content != entry.content => {
                let binary = base_entry.content.is_binary() || entry.content.is_binary();
                files.push(PreviewFile {
                    path: path.clone(),
                    change: "modified",
                    added_lines: match (base_entry.content.text(), entry.content.text()) {
                        (Some(old), Some(new)) => diff::added_line_numbers(old, new),
                        _ => Default::default(),
                    },
                    before: text_of(base_entry),
                    after: text_of(entry),
                    binary,
                });
            }
            Some(_) => {}
        }
    }
    for path in base_tree.paths().filter(|p| after.get(p).is_none()) {
        let base_entry = base_tree.get(path);
        files.push(PreviewFile {
            path: path.clone(),
            change: "deleted",
            before: base_entry.map(text_of).unwrap_or_default(),
            after: String::new(),
            added_lines: Default::default(),
            binary: base_entry.is_some_and(|e| e.content.is_binary()),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Preview {
        files,
        candidates,
        occurrences,
    })
}

/// The paths that differ between two trees (content or mode).
fn changed_between<'a>(
    base_tree: &'a weft_core::Tree,
    work_tree: &'a weft_core::Tree,
) -> Vec<&'a camino::Utf8PathBuf> {
    let mut paths: std::collections::BTreeSet<&camino::Utf8PathBuf> = Default::default();
    paths.extend(base_tree.paths());
    paths.extend(work_tree.paths());
    paths
        .into_iter()
        .filter(|path| match (base_tree.get(path), work_tree.get(path)) {
            (Some(b), Some(w)) => b.content != w.content || b.mode != w.mode,
            (None, None) => false,
            _ => true,
        })
        .collect()
}

/// Is `path` at or under `mount`? (An empty mount is the root.)
fn under_mount(path: &camino::Utf8Path, mount: &camino::Utf8Path) -> bool {
    mount.as_str().is_empty() || path.starts_with(mount)
}

/// A repeat include's instances are project-time: no template patch can
/// name one, so edits under their static prefix (the declared path up to
/// `{key}`) are only recordable in a `--foreach` session for that include —
/// against the sample mount, where the key abstracts back out. Shared with
/// `weft patch resync` (which never has a foreach session).
pub(crate) fn guard_instances(
    template: &Template,
    foreach: Option<&crate::session::ForeachSession>,
    base_tree: &weft_core::Tree,
    work_tree: &weft_core::Tree,
) -> Result<()> {
    let prefixes: Vec<(&str, camino::Utf8PathBuf)> = template
        .includes
        .iter()
        .filter(|inc| inc.decl.repeat && foreach.is_none_or(|f| f.include != inc.decl.name))
        .filter_map(|inc| {
            let head = inc.decl.path.split_once("{key}")?.0.trim_end_matches('/');
            (!head.is_empty()).then(|| (inc.decl.name.as_str(), head.into()))
        })
        .collect();
    for path in changed_between(base_tree, work_tree) {
        for (include, prefix) in &prefixes {
            if path.starts_with(prefix) {
                bail!(
                    "`{path}` is inside the instances of repeat include `{include}` \
                     ({prefix}/…); record it in a `weft session new --foreach {include}=<key>` \
                     session, where the sample key abstracts to `{{key}}`"
                );
            }
        }
    }
    Ok(())
}

/// Edits under a single include's mount are legal iff the patch depends on
/// the child nodes that own those files — infer them. For each changed path
/// under a mounted part: the *owners* are the part's active nodes whose ops
/// touch that path; the leaves among them (owners no other owner depends
/// on) become dependencies. A path no node owns (a new file under the
/// mount) depends on the part's root nodes, so it inherits the include's
/// existence and nothing narrower. Edits under the foreach sample mount
/// abstract through `key` and need no dependency.
pub(crate) fn include_deps(
    template: &Template,
    parts: &[crate::compose::ComposedPart<'_>],
    foreach: Option<&crate::session::ForeachSession>,
    base_tree: &weft_core::Tree,
    work_tree: &weft_core::Tree,
    active: &BTreeSet<weft_core::PatchId>,
    eval: &dyn weft_core::render::ExprEval,
) -> Result<Vec<String>> {
    let empty = AnswerSet::new();
    let nodes = crate::compose::graph_nodes(&[], &empty, parts);
    // Rendered path → owning (active, single-include) node ids.
    let mut owners: std::collections::BTreeMap<Utf8PathBuf, Vec<weft_core::PatchId>> =
        Default::default();
    for node in nodes.iter().filter(|n| {
        active.contains(&n.id) && template.id_to_name.contains_key(&n.id) && !n.frame.is_empty()
    }) {
        for op in &node.patch.ops {
            let paths = match op {
                weft_core::Op::CreateFile { path, .. }
                | weft_core::Op::CreateBinaryFile { path, .. }
                | weft_core::Op::ModifyFile { path, .. }
                | weft_core::Op::SetMode { path, .. } => vec![path],
                weft_core::Op::DeleteFile { .. } => vec![],
                weft_core::Op::RenamePath { to, .. } => vec![to],
            };
            for path in paths {
                let Ok(rendered) = weft_core::render::render_path(path, node.answers, eval) else {
                    continue;
                };
                owners
                    .entry(node.mount.join(rendered))
                    .or_default()
                    .push(node.id);
            }
        }
    }
    let sample_mount = foreach
        .and_then(|f| sample_of(parts, f))
        .map(|p| p.instance.mount.clone());
    // Single-include frames that rendered something: (frame, mount, root
    // nodes), deepest first so a nested include claims its files before the
    // enclosing one.
    type Frame = (Vec<(String, String)>, Utf8PathBuf, Vec<weft_core::PatchId>);
    let mut frames: Vec<Frame> = Vec::new();
    for node in nodes
        .iter()
        .filter(|n| template.id_to_name.contains_key(&n.id))
    {
        let entry = match frames.iter_mut().find(|(f, _, _)| *f == node.frame) {
            Some(entry) => entry,
            None => {
                frames.push((node.frame.clone(), node.mount.clone(), Vec::new()));
                frames.last_mut().expect("just pushed")
            }
        };
        if active.contains(&node.id) && node.depends_on.is_empty() {
            entry.2.push(node.id);
        }
    }
    frames.sort_by_key(|(f, _, _)| std::cmp::Reverse(f.len()));

    let mut deps: BTreeSet<String> = BTreeSet::new();
    for path in changed_between(base_tree, work_tree) {
        if sample_mount
            .as_deref()
            .is_some_and(|m| under_mount(path, m))
        {
            continue;
        }
        let chosen: Vec<weft_core::PatchId> = match owners.get(path) {
            Some(ids) => {
                // Owned: the deepest owning frame, then the leaves among its
                // owners.
                let frame = frames
                    .iter()
                    .find(|(f, _, _)| nodes.iter().any(|n| ids.contains(&n.id) && n.frame == *f))
                    .map(|(f, _, _)| f.clone())
                    .expect("owners have a frame");
                let in_frame: Vec<_> = ids
                    .iter()
                    .copied()
                    .filter(|id| nodes.iter().any(|n| n.id == *id && n.frame == frame))
                    .collect();
                in_frame
                    .iter()
                    .copied()
                    .filter(|id| {
                        !in_frame
                            .iter()
                            .any(|o| o != id && template.ancestor_closure(*o).contains(id))
                    })
                    .collect()
            }
            // Unowned: a new file under a (non-root) mount depends on that
            // frame's roots; a root-mounted include claims nothing it did
            // not create.
            None => frames
                .iter()
                .find(|(_, mount, _)| !mount.as_str().is_empty() && path.starts_with(mount))
                .map(|(_, _, roots)| roots.clone())
                .unwrap_or_default(),
        };
        for id in chosen {
            if let Some(name) = template.id_to_name.get(&id) {
                deps.insert(name.clone());
            }
        }
    }
    Ok(deps.into_iter().collect())
}

/// Rebuild the full answer set for the session: stored plain answers plus
/// secrets re-resolved from their stored references.
fn resolve_session_answers(
    template: &Template,
    sess: &Session,
    interaction: &mut dyn Interaction,
) -> Result<AnswerSet> {
    let mut answers = sess.answers.clone();
    resolve_secret_refs(template, &sess.secrets, &mut answers, interaction)?;
    Ok(answers)
}

/// Re-resolve stored secret *references* (`env:…`, `cmd:…`, `prompt`) into
/// concrete values. Shared by commit (session secrets) and `weft patch
/// resync` (generator secrets).
pub(crate) fn resolve_secret_refs(
    template: &Template,
    refs: &std::collections::BTreeMap<weft_core::AnswerId, String>,
    answers: &mut AnswerSet,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    for (id, spec_str) in refs {
        let question = template
            .manifest
            .questions
            .iter()
            .find(|q| q.id == *id)
            .with_context(|| format!("secret question `{id}` no longer exists in the manifest"))?;
        let spec: weft_core::SecretSpec = spec_str
            .parse()
            .with_context(|| format!("invalid secret reference for `{id}`"))?;
        let value = secrets::resolve(question, &spec, interaction)?;
        answers.insert(id.clone(), Value::Secret(value));
    }
    Ok(())
}
