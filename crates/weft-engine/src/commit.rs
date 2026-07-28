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
    guard_mounts(&template, sess.foreach.as_ref(), &base_tree, &full_work)?;
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
    let abstractor = match parts.first() {
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
            &answers,
            &work_tree,
            ops,
        );
    }

    let name = match &opts.name {
        Some(name) => name.clone(),
        None => format!("patch-{:03}", template.patches.len() + 1),
    };
    let when = opts.when.clone().map(StarlarkExpr);
    // Depend only on the *active* leaves of the base: a patch gated off
    // under the session answers contributed nothing to the recorded state,
    // and depending on it would gate the new patch off with it.
    let active_base = active_ids(&base_patches, &answers, &eval)?;
    let depends_on = match &opts.depends_on {
        Some(declared) => declared_deps(&template, declared, &active_base)?,
        None => base_leaves(&template, &active_base),
    };

    // Validation before writing: the new patch applied to the base must
    // reproduce the worktree byte-for-byte (this also catches ambiguous
    // hunk contexts). Skipped when a `when` gate is false under the
    // session answers.
    //
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

    // Declared dependencies are a claim of independence from everything else
    // in the base, so prove it: the patch must also apply against its own
    // dependency closure alone. A hunk that only anchors because of a sibling
    // fails here instead of at `weft check` (or in someone's scaffold).
    if let Some(declared) = &opts.depends_on {
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
        if let Err(e) = crate::compose::render_composed(&alone, &answers, &parts, &eval) {
            bail!(
                "this patch does not apply with only `{}` in the base: {e}\n\
                 its content anchors on something else in the session's base — \
                 declare that patch too, or drop --depends-on/--after to depend on \
                 the base's leaves",
                declared.join(", ")
            );
        }
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

/// The subset of ordered patches that actually apply under `answers`
/// (gates evaluated, skips propagated to dependents) — mirrors render.
fn active_ids(
    patches: &[weft_core::Patch],
    answers: &AnswerSet,
    eval: &dyn weft_core::render::ExprEval,
) -> Result<Vec<weft_core::PatchId>> {
    let mut skipped = std::collections::BTreeSet::new();
    let mut active = Vec::new();
    for patch in patches {
        if patch.depends_on.iter().any(|d| skipped.contains(d)) {
            skipped.insert(patch.id);
            continue;
        }
        let open = match &patch.when {
            None => true,
            Some(expr) => eval.eval_bool(expr, answers)?,
        };
        if open {
            active.push(patch.id);
        } else {
            skipped.insert(patch.id);
        }
    }
    Ok(active)
}

/// Validate `--depends-on` / `--after` names: each must be a patch in the
/// session's *active* base, since the recorded content was diffed against
/// exactly that state.
fn declared_deps(
    template: &Template,
    declared: &[String],
    active_base: &[weft_core::PatchId],
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
/// the (foreach-filtered) pinned base, the composed base tree, and the
/// current worktree. Shared by `commit` and `weft diff`.
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

    // The base must still be reconstructible from the template.
    for id in &sess.session.base {
        if !template.id_to_name.contains_key(id) {
            bail!(
                "pinned base patch {} no longer exists in the template; \
                 the template changed since the session started — start a new one",
                id.short()
            );
        }
    }

    let answers = resolve_session_answers(template, &sess, interaction)?;
    // Foreach patches never participate in the base (see record.rs) — and
    // the new patch must not depend on one (graph-leaf rule).
    let base_patches: Vec<_> = template
        .patches
        .iter()
        .filter(|p| sess.session.base.contains(&p.id) && p.foreach.is_none())
        .cloned()
        .collect();
    // A foreach session's base includes the mounted sample instance.
    let sample = match &sess.foreach {
        Some(f) => Some(crate::start::sample_part(template, f, &eval, interaction)?),
        None => None,
    };
    let parts: Vec<_> = sample.into_iter().collect();
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

/// Recording captures *this* template's patches only. Reject edits that
/// land under an include's mount: those files belong to the child template
/// (for foreach sessions, the sample instance's mount; otherwise every
/// include's static mount prefix — the declared path up to `{key}`).
/// Shared with `weft patch resync` (which never has a foreach session).
pub(crate) fn guard_mounts(
    template: &Template,
    foreach: Option<&crate::session::ForeachSession>,
    base_tree: &weft_core::Tree,
    work_tree: &weft_core::Tree,
) -> Result<()> {
    if template.includes.is_empty() {
        return Ok(());
    }
    let mut prefixes: Vec<(String, camino::Utf8PathBuf)> = Vec::new();
    match foreach {
        Some(f) => {
            let inc = template.include(&f.include).context("include vanished")?;
            prefixes.push((
                f.include.clone(),
                crate::compose::mount_path(&inc.decl.path, &f.key)?,
            ));
        }
        None => {
            for inc in &template.includes {
                let static_prefix = match inc.decl.path.split_once("{key}") {
                    Some((head, _)) => head.trim_end_matches('/').to_owned(),
                    None => inc.decl.path.clone(),
                };
                if !static_prefix.is_empty() {
                    prefixes.push((inc.decl.name.clone(), static_prefix.into()));
                }
            }
        }
    }
    let changed = |path: &camino::Utf8PathBuf| -> bool {
        match (base_tree.get(path), work_tree.get(path)) {
            (Some(b), Some(w)) => b.content != w.content || b.mode != w.mode,
            (None, None) => false,
            _ => true,
        }
    };
    let mut paths: std::collections::BTreeSet<&camino::Utf8PathBuf> = Default::default();
    paths.extend(base_tree.paths());
    paths.extend(work_tree.paths());
    for path in paths {
        if !changed(path) {
            continue;
        }
        for (include, prefix) in &prefixes {
            if path.starts_with(prefix) {
                bail!(
                    "`{path}` is inside include `{include}`'s mount ({prefix}/…); \
                     files there belong to the child template — record against it directly"
                );
            }
        }
    }
    Ok(())
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
