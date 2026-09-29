//! The staging index of one session: a git-style stage that lives in
//! `.weft-sessions/<name>/stage/` as a snapshot tree. It is absent until you
//! `weft add` something (absent means "nothing staged", i.e. the staged tree
//! equals the base), and it is removed again after each commit.
//!
//! Every session has its own index, so staging in one worktree never touches
//! another. The staged tree is what `weft commit` turns into a patch; when
//! nothing is staged, commit falls back to the whole worktree (the classic
//! one-shot flow).

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::GlobSet;
use weft_core::Tree;

use crate::fsio;
use crate::session::session_dir;
use crate::weftignore::IgnoreRules;

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

/// Indices of the patterns in `globs` that match no path in any of `trees`.
/// `weft add` and `weft reset` refuse those, as git refuses a pathspec that
/// matches nothing: staging nothing and succeeding would let the next
/// `weft commit` take the whole worktree.
pub fn unmatched(globs: &GlobSet, trees: &[&Tree]) -> Vec<usize> {
    let mut hit = vec![false; globs.len()];
    let mut left = globs.len();
    let mut found = Vec::new();
    for path in trees.iter().flat_map(|tree| tree.paths()) {
        if left == 0 {
            break;
        }
        globs.matches_into(path.as_str(), &mut found);
        for &i in &found {
            if !std::mem::replace(&mut hit[i], true) {
                left -= 1;
            }
        }
    }
    (0..hit.len()).filter(|&i| !hit[i]).collect()
}

pub fn stage_dir(root: &Utf8Path, session: &str) -> Utf8PathBuf {
    session_dir(root, session).join(STAGE_DIR)
}

pub fn exists(root: &Utf8Path, session: &str) -> bool {
    stage_dir(root, session).exists()
}

/// The staged tree: the on-disk stage if present, otherwise `base` (an empty
/// index stages nothing, so the staged tree is exactly the base).
pub fn staged_tree(
    root: &Utf8Path,
    session: &str,
    rules: &IgnoreRules,
    base: &Tree,
) -> Result<Tree> {
    let dir = stage_dir(root, session);
    if !dir.exists() {
        return Ok(base.clone());
    }
    let keep: BTreeSet<_> = base.paths().cloned().collect();
    fsio::read_tree_ignoring(&dir, rules, &keep)
}

/// Replace the stage with `tree`, or remove it entirely when `tree` carries no
/// changes against the base (keeps "absent == nothing staged" true).
pub fn write(root: &Utf8Path, session: &str, tree: &Tree, base: &Tree) -> Result<()> {
    let dir = stage_dir(root, session);
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
pub fn clear(root: &Utf8Path, session: &str) -> Result<()> {
    let dir = stage_dir(root, session);
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

/// Restrict a tree to the session's scope globs (empty = no restriction).
/// Applied to the base tree as well as the worktree, so files an adopted
/// project never opted into do not read as deletions.
pub fn in_scope(tree: &Tree, scope: &GlobSet, unrestricted: bool) -> Tree {
    if unrestricted {
        return tree.clone();
    }
    tree.iter()
        .filter(|(path, _)| scope.is_match(path.as_str()))
        .map(|(path, entry)| (path.clone(), entry.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use weft_core::FileEntry;

    use super::*;

    fn tree(paths: &[&str]) -> Tree {
        paths
            .iter()
            .map(|p| (Utf8PathBuf::from(*p), FileEntry::text("x\n")))
            .collect()
    }

    #[test]
    fn a_deleted_file_is_matched_but_a_typo_is_not() {
        // `gone.txt` exists only in the base: adding it stages a deletion.
        let base = tree(&["gone.txt"]);
        let work = tree(&["src/app.rs"]);
        let globs = globset(&["gone.txt".into(), "gnoe.txt".into(), "src/**".into()]).unwrap();
        assert_eq!(unmatched(&globs, &[&base, &work]), vec![1]);
    }
}
