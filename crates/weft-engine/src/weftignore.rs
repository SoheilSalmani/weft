//! `.weftignore`: gitignore-syntax patterns at the template root that keep
//! junk out of recordings — generator side-products (`node_modules/` from a
//! `--exec 'pnpm dlx shadcn init'`), OS droppings, build output.
//!
//! The rules apply when weft reads a worktree **back** (`weft commit`,
//! `weft diff`, `weft patch resync`) — never to rendered output, and never
//! to a path the base render produced (so deleting a rendered file is still
//! recorded even if a pattern matches it). `.git/` and `.weft/` are always
//! skipped by the tree walker; `.DS_Store` is a built-in default here.

use anyhow::{Context, Result};
use camino::Utf8Path;

pub const IGNORE_FILE: &str = ".weftignore";

/// Ignored even without a `.weftignore`.
const DEFAULTS: &[&str] = &[".DS_Store"];

pub struct IgnoreRules {
    matcher: ignore::gitignore::Gitignore,
}

impl std::fmt::Debug for IgnoreRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IgnoreRules({} pattern(s))", self.matcher.num_ignores())
    }
}

impl IgnoreRules {
    /// Load `<root>/.weftignore` (if present) plus the built-in defaults.
    pub fn load(root: &Utf8Path) -> Result<Self> {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        for line in DEFAULTS {
            builder
                .add_line(None, line)
                .expect("built-in ignore lines are valid");
        }
        let file = root.join(IGNORE_FILE);
        if file.is_file() {
            if let Some(err) = builder.add(&file) {
                return Err(err).with_context(|| format!("parsing {file}"));
            }
        }
        Ok(Self {
            matcher: builder
                .build()
                .with_context(|| format!("building ignore rules for {root}"))?,
        })
    }

    /// Whether a root-relative path is ignored. Callers prune ignored
    /// directories whole, matching git: a negation cannot re-include a file
    /// below an excluded directory.
    pub fn ignores(&self, rel: &Utf8Path, is_dir: bool) -> bool {
        self.matcher.matched(rel.as_std_path(), is_dir).is_ignore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(body: &str) -> (tempfile::TempDir, IgnoreRules) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::write(root.join(IGNORE_FILE), body).unwrap();
        let rules = IgnoreRules::load(root).unwrap();
        (dir, rules)
    }

    #[test]
    fn gitignore_semantics() {
        let (_guard, r) = rules("node_modules/\n*.log\n!keep.log\n");
        assert!(r.ignores("node_modules".into(), true));
        assert!(r.ignores("debug.log".into(), false));
        assert!(!r.ignores("keep.log".into(), false), "negation wins");
        assert!(!r.ignores("src/main.rs".into(), false));
    }

    #[test]
    fn ds_store_is_a_default() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        let r = IgnoreRules::load(root).unwrap();
        assert!(r.ignores(".DS_Store".into(), false));
        assert!(r.ignores("sub/.DS_Store".into(), false));
    }
}
