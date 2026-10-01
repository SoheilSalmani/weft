//! `weft patch resync`: re-run the stored generator command of patches
//! recorded with `weft record --exec` and rewrite their ops from the fresh
//! output — how a template tracks an upstream generator (e.g. a shadcn
//! component) that changed shape.
//!
//! Patches are processed in dependency order, reloading the template after
//! every rewrite: dependencies are stored by *name* on disk and content ids
//! are recomputed on load, so rewriting an ancestor requires no edits to
//! its dependents' files — the next iteration simply sees the fresh DAG.

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use serde::Serialize;
use weft_core::AnswerSet;
use weft_lang::StarlarkEval;

use crate::abstraction::Abstractor;
use crate::interact::Interaction;
use crate::session::Session;
use crate::template::Template;
use crate::{commit, diff, fsio};

pub struct ResyncOptions {
    pub template: Utf8PathBuf,
    /// Patch names to resync. Empty requires `all`.
    pub names: Vec<String>,
    /// Resync every generated patch.
    pub all: bool,
    /// `KEY=VALUE` overrides over the stored record-time answers
    /// (single-patch runs only; persisted back into the metadata).
    pub answers: Vec<String>,
    /// Replacement `ANSWER@PATH:LINE[:NTH]` keep-literal specs
    /// (single-patch runs only; persisted back into the metadata).
    pub keep_literal: Vec<String>,
    /// Report what would change without writing anything.
    pub dry_run: bool,
    /// Accept answer references the previous version of the patch did not
    /// have (otherwise such patches are skipped with an issue).
    pub accept_new_refs: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// The command's output still replays to the current patch — untouched.
    UpToDate,
    /// Ops rewritten from the fresh output.
    Rewritten { ops: usize },
    /// Dry run: ops would be rewritten.
    WouldRewrite { ops: usize },
    /// Not resynced; the reason is also listed under `issues`.
    Skipped { reason: String },
}

#[derive(Debug, Serialize)]
pub struct ResyncEntry {
    pub name: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Serialize)]
pub struct ResyncReport {
    pub entries: Vec<ResyncEntry>,
    /// Problems that need the author: broken dependents, empty output, …
    pub issues: Vec<String>,
    /// Non-fatal observations (e.g. a keep-literal spec matching nothing).
    pub notes: Vec<String>,
}

pub fn resync(
    opts: &ResyncOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<ResyncReport> {
    if opts.names.is_empty() && !opts.all {
        bail!("name at least one patch, or pass --all to resync every generated patch");
    }
    if opts.names.len() != 1 && (!opts.answers.is_empty() || !opts.keep_literal.is_empty()) {
        bail!("--answer/--keep-literal apply to a single named patch, not --all");
    }
    // Resync rewrites patch bodies underneath any open session's pinned base,
    // so it refuses while one exists.
    let open = Session::list(&opts.template)?;
    if !open.is_empty() {
        bail!(
            "session(s) {} are open in `{}`; commit or end them before resyncing",
            open.iter()
                .map(|(n, _)| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", "),
            opts.template
        );
    }

    // Targets in dependency order (template.patches is already topological).
    let template = Template::load_with(&opts.template, resolver)?;
    for name in &opts.names {
        let id = *template
            .name_to_id
            .get(name)
            .with_context(|| format!("unknown patch `{name}`"))?;
        let patch = template
            .patches
            .iter()
            .find(|p| p.id == id)
            .filter(|_| !template.is_inherited(name))
            .with_context(|| format!("patch `{name}` is not one of this template's own patches"))?;
        if patch.meta.generator.is_none() {
            bail!("patch `{name}` has no generator command (it was not recorded with --exec)");
        }
    }
    let targets: Vec<String> = template
        .patches
        .iter()
        .filter(|p| p.meta.generator.is_some())
        .filter_map(|p| template.id_to_name.get(&p.id).cloned())
        .filter(|n| !template.is_inherited(n))
        .filter(|n| opts.all || opts.names.contains(n))
        .collect();

    let eval = StarlarkEval;
    let mut report = ResyncReport {
        entries: Vec::new(),
        issues: Vec::new(),
        notes: Vec::new(),
    };
    // (name, resolved answers) of every rewritten patch, for the post-pass.
    let mut rewritten: Vec<(String, AnswerSet)> = Vec::new();

    for name in &targets {
        // Fresh load: earlier rewrites changed ancestor ids under us.
        let template = Template::load_with(&opts.template, resolver)?;
        let id = template.name_to_id[name];
        let patch = template
            .patches
            .iter()
            .find(|p| p.id == id)
            .expect("target exists")
            .clone();
        let generator = patch.meta.generator.clone().expect("target is generated");
        let skip = |report: &mut ResyncReport, reason: String| {
            report.issues.push(format!("`{name}`: {reason}"));
            report.entries.push(ResyncEntry {
                name: name.clone(),
                outcome: Outcome::Skipped { reason },
            });
        };
        if patch.foreach.is_some() {
            skip(
                &mut report,
                "foreach patches are not supported by resync — re-record it".into(),
            );
            continue;
        }
        // Regenerated ops come from the command's text, which cannot carry a
        // slot: rewriting would drop the patch's slots.
        let slots = crate::amend::declared_slots(&patch);
        if !slots.is_empty() {
            skip(
                &mut report,
                format!(
                    "it declares slot(s) {}, which regenerating from the command would drop; \
                     to keep them, `weft patch detach {name}` and change it with `weft patch \
                     amend {name}`",
                    slots.join(", ")
                ),
            );
            continue;
        }

        // Answers like `weft update`: the stored record-time set plus CLI
        // overrides as the provided layer, secrets re-resolved from their
        // refs, then gathered so questions added since recording take their
        // defaults (stored values win).
        let mut provided = generator.answers.clone();
        for arg in &opts.answers {
            let (id, value) = crate::answers::parse_answer_arg(&template.manifest.questions, arg)?;
            provided.insert(id, value);
        }
        let mut presolved = AnswerSet::new();
        commit::resolve_secret_refs(&template, &generator.secrets, &mut presolved, interaction)?;
        let mut answers = crate::answers::gather(
            &template,
            &provided,
            &presolved,
            &eval,
            &mut crate::interact::NonInteractive,
        )
        .with_context(|| {
            format!(
                "resolving the resync answers of `{name}`; pass a missing answer with \
                 `weft patch resync {name} --answer ID=VALUE`"
            )
        })?;
        // Keep stored values that are not root questions (e.g. namespaced
        // include answers), as resync always replayed them.
        for (k, v) in provided.iter() {
            if !answers.contains(k) {
                answers.insert(k.clone(), v.clone());
            }
        }
        let overrides = !opts.answers.is_empty() || !opts.keep_literal.is_empty();

        // The patch's gate must be open under these answers, or the replay
        // comparison below is meaningless.
        if let Some(when) = &patch.when {
            use weft_core::render::ExprEval;
            if !eval.eval_bool(when, &answers)? {
                skip(
                    &mut report,
                    format!(
                        "its `when` gate ({}) is closed under the resync answers",
                        when.0
                    ),
                );
                continue;
            }
        }

        // Base = the ancestor closure of the patch's recorded dependencies,
        // across frames (a dependency on `web/x` mounts that include).
        let base_ids: std::collections::BTreeSet<_> = patch
            .depends_on
            .iter()
            .flat_map(|dep| template.ancestor_closure(*dep))
            .filter(|id| {
                template
                    .node(*id)
                    .and_then(|n| template.node_patch(n))
                    .is_some_and(|p| p.foreach.is_none())
            })
            .collect();
        let base_patches: Vec<_> = template
            .patches
            .iter()
            .filter(|p| base_ids.contains(&p.id))
            .cloned()
            .collect();
        let mut parts = crate::compose::single_parts(
            &template,
            &answers,
            &Default::default(),
            &Default::default(),
            &eval,
            interaction,
        )?;
        crate::compose::filter_parts(&mut parts, &base_ids);
        let (base_tree, base_trace) =
            crate::compose::render_composed_traced(&base_patches, &answers, &parts, &eval)
                .with_context(|| format!("rendering the base of `{name}`"))?;

        // Re-run the generator in a scratch worktree.
        let scratch = tempfile::tempdir().context("creating a scratch worktree")?;
        let worktree = Utf8PathBuf::from_path_buf(scratch.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("non-UTF-8 temp dir {}", p.display()))?;
        fsio::write_tree(&worktree, &base_tree)?;
        crate::hooks::run_command(
            &generator.command,
            &format!("resync `{name}`"),
            &worktree,
            &answers,
            &eval,
        )?;
        let keep: std::collections::BTreeSet<_> = base_tree.paths().cloned().collect();
        let work_tree = fsio::read_tree_ignoring(&worktree, &template.ignore, &keep)?;
        commit::guard_instances(&template, None, &base_tree, &work_tree)?;
        // The patch's dependencies are fixed: output that now lands under an
        // include's mount the patch doesn't depend on needs re-recording.
        let active = crate::compose::active_nodes(&base_patches, &answers, &parts, &eval)?;
        let needed = commit::include_deps(
            &template, &parts, None, &base_tree, &work_tree, &active, &eval,
        )?;
        let missing: Vec<_> = needed
            .into_iter()
            .filter(|n| !base_ids.contains(&template.name_to_id[n]))
            .collect();
        if !missing.is_empty() {
            skip(
                &mut report,
                format!(
                    "the command now writes under an include's mount without depending on \
                     {}; re-record it in a session so the dependency is inferred",
                    missing.join(", ")
                ),
            );
            continue;
        }

        // Up to date? The existing patch replayed over the base must equal
        // the fresh output byte-for-byte. A --keep-literal repair changes how
        // the output is recorded, not what it renders, so it goes on to
        // re-derive the ops.
        let mut with_existing = base_patches.clone();
        with_existing.push(patch.clone());
        let existing = crate::compose::render_composed(&with_existing, &answers, &parts, &eval)
            .with_context(|| format!("replaying the current `{name}`"))?;
        if existing.hash() == work_tree.hash() && opts.keep_literal.is_empty() {
            if overrides && !opts.dry_run {
                store_inputs(&template, name, &answers, &[], None)?;
                report.notes.push(stored_note(name, opts));
            }
            report.entries.push(ResyncEntry {
                name: name.clone(),
                outcome: Outcome::UpToDate,
            });
            continue;
        }

        // Rebuild ops the way a scripted commit would: confirm-all minus the
        // keep-literal specs (CLI specs replace the stored ones — repairs).
        let specs = if opts.keep_literal.is_empty() {
            generator.keep_literal.clone()
        } else {
            opts.keep_literal.clone()
        };
        let keep = commit::parse_keep_literal(&specs)?;
        let abstractor = Abstractor::from_answers(&answers);
        let occurrences = diff::added_occurrences(&base_tree, &work_tree, &abstractor);
        for (answer, path, line, nth) in &keep {
            let hit = occurrences
                .iter()
                .any(|o| o.id == *answer && o.path == *path && o.line == *line && o.nth == *nth);
            if !hit {
                report.notes.push(format!(
                    "`{name}`: keep-literal `{answer}@{path}:{line}:{nth}` matched no \
                     occurrence in the new output (the upstream layout moved)"
                ));
            }
        }
        let texts = diff::collect_texts(&base_tree, &work_tree);
        let confirmed = abstractor.confirmed_from_decisions(&texts, &Default::default());
        let keys = diff::SlotKeys::named(name).keeping(&patch, &answers);
        let names = |id: weft_core::PatchId| {
            template
                .id_to_name
                .get(&id)
                .cloned()
                .unwrap_or_else(|| id.short())
        };
        let rec = diff::Recording {
            base: &base_tree,
            trace: &base_trace,
            keys: &keys,
            names: &names,
        };
        let ops = match diff::build_ops_decided(&rec, &work_tree, &abstractor, &confirmed, &keep) {
            Ok(ops) => ops,
            Err(e) => {
                skip(&mut report, format!("{e:#}"));
                continue;
            }
        };
        if ops.is_empty() {
            skip(
                &mut report,
                "the command's output now matches the base — the patch would be empty".into(),
            );
            continue;
        }

        // Replay guard, as at commit: the rewritten patch applied to the base
        // must reproduce the fresh output (catches ambiguous hunk contexts).
        let candidate =
            weft_core::Patch::new(patch.depends_on.clone(), patch.when.clone(), ops.clone());
        let mut with_new = base_patches.clone();
        with_new.push(candidate);
        let replayed = crate::compose::render_composed(&with_new, &answers, &parts, &eval)
            .with_context(|| format!("replaying the resynced `{name}`"))?;
        if replayed.hash() != work_tree.hash() {
            skip(
                &mut report,
                "replaying the resynced ops does not reproduce the command output \
                 (likely an ambiguous hunk context); left unchanged"
                    .into(),
            );
            continue;
        }
        // A repair that re-derives the ops the patch already has leaves the
        // patch alone; only its inputs are stored.
        if with_new[with_new.len() - 1].id == patch.id {
            if !opts.dry_run {
                store_inputs(&template, name, &answers, &opts.keep_literal, None)?;
                report.notes.push(stored_note(name, opts));
            }
            report.entries.push(ResyncEntry {
                name: name.clone(),
                outcome: Outcome::UpToDate,
            });
            continue;
        }

        // New-reference guard: answers the previous version did not
        // reference (e.g. a question added since recording whose default
        // appears in the output) need the author's consent.
        let old_refs = crate::check::answer_refs(&patch);
        let new_refs: Vec<_> = crate::check::answer_refs(&with_new[with_new.len() - 1])
            .into_iter()
            .filter(|id| !old_refs.contains(id))
            .collect();
        if !new_refs.is_empty() {
            let listed: Vec<String> = new_refs
                .iter()
                .map(|id| {
                    let hits: Vec<_> = occurrences
                        .iter()
                        .filter(|o| o.id == *id)
                        .filter(|o| {
                            !keep.iter().any(|(a, p, l, n)| {
                                a == &o.id && p == &o.path && *l == o.line && *n == o.nth
                            })
                        })
                        .collect();
                    // Never print a secret: its occurrences are its value.
                    let value = match answers.get(id) {
                        Some(weft_core::Value::Secret(_)) => "(secret)".to_owned(),
                        Some(v) => format!("{:?}", v.render_text()),
                        None => "(unset)".to_owned(),
                    };
                    let keys: Vec<_> = hits
                        .iter()
                        .map(|o| format!("{}@{}:{}:{}", o.id, o.path, o.line, o.nth))
                        .collect();
                    if keys.is_empty() {
                        format!("`{id}` = {value}")
                    } else {
                        format!("`{id}` = {value} at {}", keys.join(", "))
                    }
                })
                .collect();
            if !opts.accept_new_refs {
                skip(
                    &mut report,
                    format!(
                        "the resynced ops reference answer(s) the previous version did not: \
                         {}; pass --yes to accept them, or --keep-literal \
                         ID@PATH:LINE[:NTH] to keep those occurrences literal",
                        listed.join("; ")
                    ),
                );
                continue;
            }
            report.notes.push(format!(
                "`{name}`: accepted new answer reference(s) {}",
                listed.join("; ")
            ));
        }

        if opts.dry_run {
            report.entries.push(ResyncEntry {
                name: name.clone(),
                outcome: Outcome::WouldRewrite { ops: ops.len() },
            });
            continue;
        }

        // Rewrite ops in place; deps/when/meta stay. Persist any repaired
        // metadata so the next resync replays the same inputs.
        let count = ops.len();
        store_inputs(&template, name, &answers, &opts.keep_literal, Some(ops))?;
        report.entries.push(ResyncEntry {
            name: name.clone(),
            outcome: Outcome::Rewritten { ops: count },
        });
        rewritten.push((name.clone(), answers));
    }

    // Post-pass: dependents of a rewritten patch may anchor on content that
    // changed shape. A full composed render surfaces exactly which patch
    // broke.
    if !opts.dry_run {
        let template = Template::load_with(&opts.template, resolver)?;
        let pinned = crate::start::pin_base(&template, "latest")?;
        let root: Vec<_> = template
            .patches
            .iter()
            .filter(|p| pinned.contains(&p.id))
            .cloned()
            .collect();
        for (name, answers) in &rewritten {
            let parts = crate::compose::single_parts(
                &template,
                answers,
                &Default::default(),
                &Default::default(),
                &eval,
                interaction,
            )?;
            if let Err(e) = crate::compose::render_composed(&root, answers, &parts, &eval) {
                report.issues.push(format!(
                    "after resyncing `{name}`, the template no longer renders: {e} — a \
                     dependent patch's context anchors moved with the regenerated \
                     content; re-record that patch (`weft session new --base <its name>`)"
                ));
            }
        }
    }
    Ok(report)
}

/// Stores the answers a resync ran with, and the CLI keep-literal specs when
/// given, in the patch's generator metadata so the next resync replays the
/// same inputs. `ops`, when given, replaces the patch's ops.
fn store_inputs(
    template: &Template,
    name: &str,
    answers: &AnswerSet,
    keep_literal: &[String],
    ops: Option<Vec<weft_core::Op>>,
) -> Result<()> {
    let mut file = template.patch_file(name)?;
    if let Some(ops) = ops {
        file.ops = ops;
    }
    if let Some(stored) = &mut file.generator {
        stored.answers = crate::start::strip_secrets(answers);
        if !keep_literal.is_empty() {
            stored.keep_literal = keep_literal.to_vec();
        }
    }
    template.save_patch_file(name, &file)
}

/// The note for a resync that stored its inputs and left the ops alone.
fn stored_note(name: &str, opts: &ResyncOptions) -> String {
    let what = match (opts.answers.is_empty(), opts.keep_literal.is_empty()) {
        (false, false) => "the --answer override(s) and --keep-literal spec(s)",
        (false, true) => "the --answer override(s)",
        _ => "the --keep-literal spec(s)",
    };
    format!("`{name}`: stored {what} in its generator metadata (ops unchanged)")
}
