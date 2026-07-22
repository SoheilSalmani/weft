//! The staging index for a recording session: a git-style stage that lives in
//! `.weft-record/stage/` as a snapshot tree. It is absent until you `weft add`
//! something (absent means "nothing staged", i.e. the staged tree equals the
//! base), and it is removed again after each commit.
//!
//! The staged tree is what `weft commit` turns into a patch. When nothing is
//! staged, commit falls back to the whole worktree (the classic one-shot flow).

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::GlobSet;
use weft_core::Tree;

use crate::session::record_dir;
use crate::weftignore::IgnoreRules;
use crate::{fsio, session};

pub const STAGE_DIR: &str = "stage";

/// Build a glob set from `weft add`/`reset` patterns (matched against
/// tree-relative paths).
pub fn globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        builder.add(globset::Glob::new(p).with_context(|| format!("invalid path pattern `{p}`"))?);
    }
    builder.build().context("building path glob set")
}

pub fn stage_dir(root: &Utf8Path) -> Utf8PathBuf {
    record_dir(root).join(STAGE_DIR)
}

pub fn exists(root: &Utf8Path) -> bool {
    stage_dir(root).exists()
}

/// The staged tree: the on-disk stage if present, otherwise `base` (an empty
/// index stages nothing, so the staged tree is exactly the base).
pub fn staged_tree(root: &Utf8Path, rules: &IgnoreRules, base: &Tree) -> Result<Tree> {
    let dir = stage_dir(root);
    if !dir.exists() {
        return Ok(base.clone());
    }
    let keep: BTreeSet<_> = base.paths().cloned().collect();
    fsio::read_tree_ignoring(&dir, rules, &keep)
}

/// Replace the stage with `tree`, or remove it entirely when `tree` carries no
/// changes against the base (keeps "absent == nothing staged" true).
pub fn write(root: &Utf8Path, tree: &Tree, base: &Tree) -> Result<()> {
    let dir = stage_dir(root);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {dir}"))?;
    }
    if tree.hash() != base.hash() {
        fsio::write_tree(&dir, tree)?;
    }
    Ok(())
}

/// Remove the stage (used after a commit: the index is empty against the new
/// base).
pub fn clear(root: &Utf8Path) -> Result<()> {
    let dir = stage_dir(root);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {dir}"))?;
    }
    Ok(())
}

/// `weft add`: overlay the worktree version of matched paths onto the staged
/// tree. A path present in `base` but absent in the worktree stages a deletion.
/// Returns the number of paths whose staged state changed.
pub fn add(staged: &mut Tree, worktree: &Tree, base: &Tree, globs: &GlobSet) -> usize {
    let mut changed = 0;
    let all: BTreeSet<Utf8PathBuf> = base.paths().chain(worktree.paths()).cloned().collect();
    for path in all {
        if !globs.is_match(path.as_str()) {
            continue;
        }
        let before = staged.get(&path).cloned();
        match worktree.get(&path) {
            Some(entry) => {
                staged.insert(path.clone(), entry.clone());
            }
            None => {
                staged.remove(&path);
            }
        }
        if staged.get(&path).cloned() != before {
            changed += 1;
        }
    }
    changed
}

/// `weft reset`: restore matched paths to their base state (unstage). `None`
/// resets everything.
pub fn reset(staged: &mut Tree, base: &Tree, globs: Option<&GlobSet>) {
    let all: BTreeSet<Utf8PathBuf> = staged.paths().chain(base.paths()).cloned().collect();
    for path in all {
        if let Some(g) = globs {
            if !g.is_match(path.as_str()) {
                continue;
            }
        }
        match base.get(&path) {
            Some(entry) => {
                staged.insert(path.clone(), entry.clone());
            }
            None => {
                staged.remove(&path);
            }
        }
    }
}

/// Paths where two trees differ (created, modified, or deleted).
pub fn changed_paths(from: &Tree, to: &Tree) -> Vec<Utf8PathBuf> {
    let all: BTreeSet<Utf8PathBuf> = from.paths().chain(to.paths()).cloned().collect();
    all.into_iter()
        .filter(|p| from.get(p) != to.get(p))
        .collect()
}

/// Revert `paths` in the worktree back to their base version (used by the
/// "sibling" transition to peel a committed patch's content out of the
/// session).
pub fn revert_worktree_paths(root: &Utf8Path, base: &Tree, paths: &[Utf8PathBuf]) -> Result<()> {
    let worktree = session::worktree_dir(root);
    for path in paths {
        match base.get(path) {
            Some(entry) => fsio::write_file(&worktree, path, entry)?,
            None => {
                let full = worktree.join(path);
                if full.exists() {
                    std::fs::remove_file(&full).with_context(|| format!("removing {full}"))?;
                }
            }
        }
    }
    Ok(())
}
