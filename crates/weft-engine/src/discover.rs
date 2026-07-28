//! Finding the template (and session) you are standing in, so weft can be run
//! from inside a worktree the way git is run from inside a checkout.
//!
//! A worktree carries a back-pointer at `<worktree>/.weft/worktree.toml`
//! naming its template and session — the same trick as git's `.git` *file* in
//! a linked worktree. `.weft/` is skipped unconditionally by the tree walker
//! ([`crate::weftignore`]), so the pointer can never leak into a patch, and it
//! sits beside the `state.toml` / `base.json` a scaffolded project already has.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::state::STATE_DIR;
use crate::template::MANIFEST_FILE;

pub const LINK_FILE: &str = "worktree.toml";

/// `<worktree>/.weft/worktree.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeLink {
    /// Absolute path to the template this worktree records against.
    pub template: Utf8PathBuf,
    /// The session's name within that template.
    pub session: String,
}

pub fn link_path(worktree: &Utf8Path) -> Utf8PathBuf {
    worktree.join(STATE_DIR).join(LINK_FILE)
}

impl WorktreeLink {
    pub fn save(&self, worktree: &Utf8Path) -> Result<()> {
        let path = link_path(worktree);
        std::fs::create_dir_all(path.parent().expect("link path has a parent"))?;
        std::fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn load(worktree: &Utf8Path) -> Result<Self> {
        let path = link_path(worktree);
        let src = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
        toml::from_str(&src).with_context(|| format!("parsing {path}"))
    }

    /// Remove the pointer (unlinking a worktree without deleting it).
    pub fn remove(worktree: &Utf8Path) -> Result<()> {
        let path = link_path(worktree);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {path}"))?;
        }
        Ok(())
    }
}

/// Where a command was run from.
#[derive(Debug, Clone)]
pub struct Location {
    pub template_root: Utf8PathBuf,
    /// The session, when the command was run inside one of its worktrees.
    pub session: Option<String>,
    /// That worktree's root.
    pub worktree_root: Option<Utf8PathBuf>,
    /// Where the command was run, relative to `worktree_root` (empty at the
    /// root). Path arguments are resolved against this, like git.
    pub prefix: Utf8PathBuf,
}

/// Walk up from `cwd` looking for a worktree pointer or a template manifest.
///
/// Both markers are checked **at each level, pointer first**, so the innermost
/// one wins: a worktree whose rendered content includes a `weft.toml` at its
/// root is still recognized as a worktree, while a genuinely nested template
/// inside a worktree still resolves to itself.
pub fn locate(cwd: &Utf8Path) -> Option<Location> {
    for dir in cwd.ancestors() {
        if link_path(dir).is_file() {
            let link = WorktreeLink::load(dir).ok()?;
            return Some(Location {
                template_root: link.template,
                session: Some(link.session),
                worktree_root: Some(dir.to_owned()),
                prefix: relative(cwd, dir),
            });
        }
        if dir.join(MANIFEST_FILE).is_file() {
            return Some(Location {
                template_root: dir.to_owned(),
                session: None,
                worktree_root: None,
                prefix: Utf8PathBuf::new(),
            });
        }
    }
    None
}

/// `path` relative to `base` (both absolute, `base` an ancestor of `path`).
fn relative(path: &Utf8Path, base: &Utf8Path) -> Utf8PathBuf {
    path.strip_prefix(base)
        .map(|p| p.to_owned())
        .unwrap_or_default()
}

/// The process's working directory as UTF-8.
pub fn cwd() -> Result<Utf8PathBuf> {
    let dir = std::env::current_dir().context("reading the current directory")?;
    Utf8PathBuf::from_path_buf(dir)
        .map_err(|p| anyhow::anyhow!("current directory `{}` is not valid UTF-8", p.display()))
}

/// Absolute, symlink-resolved form — the shape stored in links and session
/// files, so two spellings of the same directory compare equal.
pub fn absolute(path: &Utf8Path) -> Utf8PathBuf {
    path.canonicalize_utf8().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_owned()
        } else {
            cwd()
                .map(|c| c.join(path))
                .unwrap_or_else(|_| path.to_owned())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Utf8Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn finds_the_template_from_a_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        touch(&root.join(MANIFEST_FILE));
        let deep = root.join("patches/nested");
        std::fs::create_dir_all(&deep).unwrap();

        let loc = locate(&deep).expect("located");
        assert_eq!(loc.template_root, root);
        assert!(loc.session.is_none());
    }

    #[test]
    fn a_worktree_pointer_wins_over_a_rendered_manifest_beside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let wt = root.join("wt");
        // The worktree renders a template, so it has its own weft.toml.
        touch(&wt.join(MANIFEST_FILE));
        WorktreeLink {
            template: "/tpl".into(),
            session: "docker".into(),
        }
        .save(&wt)
        .unwrap();

        let loc = locate(&wt).expect("located");
        assert_eq!(loc.template_root, Utf8PathBuf::from("/tpl"));
        assert_eq!(loc.session.as_deref(), Some("docker"));
        assert_eq!(loc.worktree_root.as_deref(), Some(wt.as_path()));
        assert_eq!(loc.prefix, Utf8PathBuf::new());
    }

    #[test]
    fn a_template_nested_inside_a_worktree_still_wins_at_its_own_level() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let wt = root.join("wt");
        WorktreeLink {
            template: "/tpl".into(),
            session: "s".into(),
        }
        .save(&wt)
        .unwrap();
        let inner = wt.join("vendor/tpl");
        touch(&inner.join(MANIFEST_FILE));

        let loc = locate(&inner).expect("located");
        assert_eq!(loc.template_root, inner);
        assert!(loc.session.is_none());
    }

    #[test]
    fn prefix_records_where_in_the_worktree_you_are() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let wt = root.join("wt");
        WorktreeLink {
            template: "/tpl".into(),
            session: "s".into(),
        }
        .save(&wt)
        .unwrap();
        let sub = wt.join("src/app");
        std::fs::create_dir_all(&sub).unwrap();

        let loc = locate(&sub).expect("located");
        assert_eq!(loc.prefix, Utf8PathBuf::from("src/app"));
    }

    #[test]
    fn nothing_above_a_bare_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        assert!(locate(root).is_none());
    }
}
