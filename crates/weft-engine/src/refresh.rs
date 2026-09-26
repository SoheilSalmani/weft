//! `weft session refresh`: re-read the manifest, re-resolve answers, re-render
//! the pinned base, and 3-way merge the new base render onto the worktree so an
//! author can edit `weft.toml` or change answers mid-session without losing
//! their worktree edits or stranding the session on a stale base tree hash.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use weft_core::merge::merge3;
use weft_core::{AnswerKind, AnswerSet, FileEntry};
use weft_lang::StarlarkEval;

use crate::interact::Interaction;
use crate::session::{Session, SessionMeta};
use crate::start::strip_secrets;
use crate::template::Template;
use crate::{answers, fsio};

pub struct RefreshOptions {
    pub template: Utf8PathBuf,
    /// Which session to refresh.
    pub session: String,
    pub presets: Vec<String>,
    pub answers: Vec<String>,
    pub answers_file: Option<Utf8PathBuf>,
    pub answers_json: Option<String>,
}

#[derive(Debug, Default)]
pub struct RefreshReport {
    /// Files rewritten in the worktree (adopted from the new base render).
    pub updated: usize,
    /// Files where both the new render and your edits changed the same lines.
    pub conflicts: Vec<Utf8PathBuf>,
    /// Human notes (a file you deleted the template now modifies, and so on).
    pub notes: Vec<String>,
}

pub fn run(
    opts: &RefreshOptions,
    resolver: &mut dyn crate::template::IncludeResolver,
    interaction: &mut dyn Interaction,
) -> Result<RefreshReport> {
    let template = Template::load_with(&opts.template, resolver)?;
    let name = &opts.session;
    if !Session::exists(&opts.template, name) {
        bail!("no session `{name}` to refresh; run `weft session new {name}` first");
    }
    let sess = Session::load(&opts.template, name)?;
    if sess.foreach.is_some() || sess.generator.is_some() || sess.amend.is_some() {
        bail!(
            "refresh is only supported for a plain session \
             (not --foreach, --exec, or `weft patch amend`)"
        );
    }
    let eval = StarlarkEval;

    // Old base: what the worktree was rendered from.
    let mut old_answers = sess.answers.clone();
    crate::commit::resolve_secret_refs(&template, &sess.secrets, &mut old_answers, interaction)?;
    let (base_patches, old_parts) =
        crate::commit::session_base(&template, &sess, &old_answers, interaction)?;
    let old_base = crate::compose::render_composed(&base_patches, &old_answers, &old_parts, &eval)
        .context("re-rendering old base")?;
    if old_base.hash() != sess.session.tree_hash {
        bail!(
            "base state hash changed since the session started (a base patch or \
             secret moved underneath it); start a new session"
        );
    }

    // New answers: the session's answers, overlaid with anything passed now,
    // then re-gathered so new questions resolve and computed values recompute.
    // Namespaced answers do the same for the mounted includes.
    let new_layered = answers::layered_with_json(
        &template,
        &opts.presets,
        opts.answers_file.as_deref(),
        &opts.answers,
        opts.answers_json.as_deref(),
    )?;
    let (parent_layered, child_layered) = crate::compose::split_provided(&template, &new_layered)?;
    let mut provided = sess.answers.clone();
    for (k, v) in parent_layered.iter() {
        provided.insert(k.clone(), v.clone());
    }
    let mut presolved = AnswerSet::new();
    crate::commit::resolve_secret_refs(&template, &sess.secrets, &mut presolved, interaction)?;
    let new_answers = answers::gather(&template, &provided, &presolved, &eval, interaction)?;
    let mut include_provided: BTreeMap<String, AnswerSet> = BTreeMap::new();
    let mut include_presolved: BTreeMap<String, AnswerSet> = BTreeMap::new();
    for part in &old_parts {
        let name = part.instance.include.clone();
        let mut stored = AnswerSet::new();
        let mut secrets = AnswerSet::new();
        for (id, value) in part.instance.answers.iter() {
            if matches!(value, weft_core::Value::Secret(_)) {
                secrets.insert(id.clone(), value.clone());
            } else {
                stored.insert(id.clone(), value.clone());
            }
        }
        if let Some(overlay) = child_layered.get(&(name.clone(), name.clone())) {
            stored.overlay(overlay);
        }
        include_provided.insert(name.clone(), stored);
        include_presolved.insert(name, secrets);
    }
    let pinned: BTreeSet<_> = sess.session.base.iter().copied().collect();
    let mut new_parts = crate::compose::single_parts(
        &template,
        &new_answers,
        &include_provided,
        &include_presolved,
        &eval,
        interaction,
    )?;
    crate::compose::filter_parts(&mut new_parts, &pinned);
    let new_base = crate::compose::render_composed(&base_patches, &new_answers, &new_parts, &eval)
        .context("re-rendering new base")?;

    // Current worktree (your edits on top of the old base).
    let worktree = sess.worktree(&template.root, name);
    let keep: BTreeSet<_> = old_base.paths().cloned().collect();
    let ours = fsio::read_tree_ignoring(&worktree, &template.ignore, &keep)?;

    // 3-way merge, mirroring `weft update`: adopt template-only changes, keep
    // your untouched files, merge overlaps, and mark real conflicts.
    let mut report = RefreshReport::default();
    let all: BTreeSet<Utf8PathBuf> = old_base
        .paths()
        .chain(ours.paths())
        .chain(new_base.paths())
        .cloned()
        .collect();
    for path in all {
        let base = old_base.get(&path);
        let our = ours.get(&path);
        let their = new_base.get(&path);
        if base == their || our == their {
            continue; // base unchanged, or worktree already matches the new base
        }
        if our == base {
            match their {
                Some(entry) => {
                    fsio::write_file(&worktree, &path, entry)?;
                    report.updated += 1;
                }
                None => {
                    remove_worktree_file(&worktree, &path)?;
                    report.updated += 1;
                }
            }
            continue;
        }
        match (our, their) {
            (None, Some(_)) => report.notes.push(format!(
                "kept `{path}` deleted (the template now creates it)"
            )),
            (Some(_), None) => report.notes.push(format!(
                "kept your `{path}` (the template no longer creates it)"
            )),
            (Some(o), Some(t)) => {
                let (Some(ours_text), Some(theirs_text)) = (o.content.text(), t.content.text())
                else {
                    report
                        .notes
                        .push(format!("kept your binary `{path}` (both sides changed it)"));
                    continue;
                };
                let base_text = base.and_then(|e| e.content.text()).unwrap_or("");
                let outcome = merge3(base_text, ours_text, theirs_text);
                let mode = if base.map(|e| e.mode) == Some(o.mode) {
                    t.mode
                } else {
                    o.mode
                };
                let entry = FileEntry {
                    content: outcome.text.into(),
                    mode,
                };
                fsio::write_file(&worktree, &path, &entry)?;
                if outcome.conflicts == 0 {
                    report.updated += 1;
                } else {
                    report.conflicts.push(path);
                }
            }
            (None, None) => unreachable!("path came from some tree"),
        }
    }

    // Re-pin the session to the new base render and answers.
    let secrets: BTreeMap<_, _> = template
        .manifest
        .questions
        .iter()
        .filter_map(|q| match &q.kind {
            AnswerKind::Secret { source } if new_answers.contains(&q.id) => {
                Some((q.id.clone(), source.to_string()))
            }
            _ => None,
        })
        .collect();
    let instances = new_parts
        .iter()
        .map(|p| crate::session::SessionInstance {
            include: p.instance.include.clone(),
            answers: strip_secrets(&p.instance.answers),
            secrets: crate::new::secret_specs(&p.template.manifest.questions, &p.instance.answers),
        })
        .collect();
    let refreshed = Session {
        session: SessionMeta {
            base: sess.session.base.clone(),
            tree_hash: new_base.hash(),
            worktree: sess.session.worktree.clone(),
            scope: sess.session.scope.clone(),
            adopted: sess.session.adopted,
        },
        answers: strip_secrets(&new_answers),
        secrets,
        instances,
        foreach: None,
        generator: None,
        amend: None,
    };
    refreshed.save(&template.root, name)?;
    Ok(report)
}

fn remove_worktree_file(worktree: &Utf8Path, rel: &Utf8Path) -> Result<()> {
    let path = worktree.join(rel);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("removing {path}"))?;
    }
    Ok(())
}
