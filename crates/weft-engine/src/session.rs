//! A recording session: the pinned base state a worktree was materialized
//! from, plus that worktree's own staging index. Sessions are named and a
//! template can hold several at once, each with its own directory — the
//! `git worktree` model.
//!
//! Everything about a session except the worktree itself lives under
//! `<template>/.weft-sessions/<name>/`. The worktree defaults to
//! `<template>/.weft-sessions/<name>/worktree/` but may live anywhere; when it
//! does, `SessionMeta::worktree` records where, and the worktree carries a
//! back-pointer (see [`crate::discover`]) so weft can be run from inside it.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerId, AnswerSet, PatchId};

pub const SESSIONS_DIR: &str = ".weft-sessions";
/// The pre-multi-session layout (`.weft-record/{session.toml,worktree,stage}`),
/// migrated to `.weft-sessions/default/` on first touch.
pub const LEGACY_RECORD_DIR: &str = ".weft-record";
pub const SESSION_FILE: &str = "session.toml";
pub const WORKTREE_DIR: &str = "worktree";
/// The session a command acts on when it was not given a name. A migrated
/// legacy session — the single unnamed session of old — takes it too.
pub const DEFAULT_SESSION_NAME: &str = "default";

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub session: SessionMeta,
    /// Concrete answers the base was rendered with (secrets excluded).
    #[serde(default)]
    pub answers: AnswerSet,
    /// Secret answers as source references, re-resolved at commit time.
    #[serde(default)]
    pub secrets: BTreeMap<AnswerId, String>,
    /// The single (non-repeat) includes mounted into the base, with the
    /// child answers they were rendered with. Their pinned nodes are part of
    /// `session.base` (keyed ids); nested includes re-derive from binds.
    #[serde(default, rename = "instance", skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<SessionInstance>,
    /// Present when recording a `foreach` integration patch: the sample
    /// instance mounted into the base so the author edits against a
    /// concrete example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreach: Option<ForeachSession>,
    /// Present when this session was started with `weft session new --exec`:
    /// commit attaches the command as the new patch's generator metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<PendingGenerator>,
    /// Present when started with `weft patch amend <name>`: the base is the
    /// patch's ancestors, the worktree is seeded with the patch applied, and
    /// `weft commit` re-derives that patch's ops in place (keeping its name,
    /// deps, gate, and metadata) instead of writing a new patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amend: Option<String>,
}

/// The generator command a `weft session new --exec` session ran, held until
/// commit turns it into `weft_core::Generator` metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingGenerator {
    pub command: weft_core::Command,
    /// Worktree hash right after the command ran — commit warns when the
    /// author edited on top (those edits would be lost on resync).
    pub tree_hash_after_exec: String,
}

/// The sample instance of a `weft session new --foreach include=key` session.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ForeachSession {
    pub include: String,
    pub key: String,
    /// The sample instance's resolved child answers.
    pub answers: AnswerSet,
}

/// One single include mounted into a session's base (its instance key is
/// the include name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInstance {
    pub include: String,
    /// Child answers (secrets excluded — see `secrets`).
    #[serde(default)]
    pub answers: AnswerSet,
    /// Child secret references (answer id → source spec string).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<AnswerId, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Ids of the patches that form the pinned base state.
    pub base: Vec<PatchId>,
    /// Hash of the rendered base tree.
    pub tree_hash: String,
    /// Where the worktree lives, when not in the default location. Absolute,
    /// so the session survives the template itself being moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Utf8PathBuf>,
    /// Path globs limiting what weft looks at. Empty = the whole tree. Set by
    /// `weft session adopt --scope` so linking a real project does not make
    /// its entire contents read as new files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    /// True when the worktree was an existing directory weft linked rather
    /// than created — `weft session end` unlinks it instead of deleting it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adopted: bool,
}

pub fn sessions_dir(template_root: &Utf8Path) -> Utf8PathBuf {
    template_root.join(SESSIONS_DIR)
}

/// Where a session's metadata and index live.
pub fn session_dir(template_root: &Utf8Path, name: &str) -> Utf8PathBuf {
    sessions_dir(template_root).join(name)
}

/// The worktree location for a session that did not ask for its own path.
pub fn default_worktree_dir(template_root: &Utf8Path, name: &str) -> Utf8PathBuf {
    session_dir(template_root, name).join(WORKTREE_DIR)
}

/// Session names are directory names and appear in paths, so keep them tame.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl Session {
    /// Where this session's worktree is, given the template it belongs to.
    pub fn worktree(&self, template_root: &Utf8Path, name: &str) -> Utf8PathBuf {
        self.session
            .worktree
            .clone()
            .unwrap_or_else(|| default_worktree_dir(template_root, name))
    }

    pub fn save(&self, template_root: &Utf8Path, name: &str) -> Result<()> {
        let dir = session_dir(template_root, name);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(SESSION_FILE);
        std::fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn load(template_root: &Utf8Path, name: &str) -> Result<Self> {
        migrate_legacy(template_root)?;
        let path = session_dir(template_root, name).join(SESSION_FILE);
        let src = std::fs::read_to_string(&path).with_context(|| {
            format!("no session `{name}` in `{template_root}`; run `weft session new {name}` first")
        })?;
        toml::from_str(&src).with_context(|| format!("parsing {path}"))
    }

    pub fn exists(template_root: &Utf8Path, name: &str) -> bool {
        let _ = migrate_legacy(template_root);
        session_dir(template_root, name).join(SESSION_FILE).exists()
    }

    /// Forget a session: remove its metadata and index. The worktree is the
    /// caller's business (an adopted one is never deleted).
    pub fn discard(template_root: &Utf8Path, name: &str) -> Result<()> {
        let dir = session_dir(template_root, name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {dir}"))?;
        }
        // Leave no empty `.weft-sessions/` behind once the last one goes.
        let _ = std::fs::remove_dir(sessions_dir(template_root));
        Ok(())
    }

    /// Finish a session: forget its metadata and index, and clean up the
    /// worktree. A worktree weft created is removed; an **adopted** one is
    /// only unlinked — it is the author's own directory.
    pub fn end(&self, template_root: &Utf8Path, name: &str) -> Result<()> {
        let worktree = self.worktree(template_root, name);
        if self.session.adopted {
            crate::discover::WorktreeLink::remove(&worktree)?;
        } else if worktree.exists() {
            std::fs::remove_dir_all(&worktree)
                .with_context(|| format!("removing worktree {worktree}"))?;
        }
        Self::discard(template_root, name)
    }

    /// Every session in a template, by name (sorted).
    pub fn list(template_root: &Utf8Path) -> Result<Vec<(String, Session)>> {
        migrate_legacy(template_root)?;
        let dir = sessions_dir(template_root);
        let Ok(entries) = dir.read_dir_utf8() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_owned();
            if !entry.path().join(SESSION_FILE).is_file() {
                continue;
            }
            match Self::load(template_root, &name) {
                Ok(sess) => out.push((name, sess)),
                // A half-written session shouldn't break `weft session list`.
                Err(e) => eprintln!("warning: skipping session `{name}`: {e:#}"),
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// The one session to act on when the caller named none: the session the
    /// command was run inside, else the only one that exists.
    pub fn only(template_root: &Utf8Path) -> Result<String> {
        let sessions = Self::list(template_root)?;
        match sessions.len() {
            0 => bail!("no session in `{template_root}`; run `weft session new` first"),
            1 => Ok(sessions.into_iter().next().unwrap().0),
            _ => bail!(
                "several sessions in `{template_root}` ({}); \
                 pass --session NAME or run this from inside a worktree",
                sessions
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// Move a pre-multi-session `.weft-record/` into the default session. Runs
/// at most once per template — after the move the legacy dir is gone.
pub fn migrate_legacy(template_root: &Utf8Path) -> Result<()> {
    let legacy = template_root.join(LEGACY_RECORD_DIR);
    if !legacy.join(SESSION_FILE).exists() {
        return Ok(());
    }
    let dest = session_dir(template_root, DEFAULT_SESSION_NAME);
    if dest.exists() {
        bail!(
            "both `{legacy}` (the old single-session layout) and `{dest}` exist; \
             remove one by hand"
        );
    }
    std::fs::create_dir_all(sessions_dir(template_root))?;
    std::fs::rename(&legacy, &dest).with_context(|| format!("migrating {legacy} to {dest}"))?;
    eprintln!("migrated the recording session in `{legacy}` to `{dest}`");
    Ok(())
}

/// Revert `paths` in `worktree` back to their base version (used by the
/// "sibling" transition to peel a committed patch's content out of the
/// session).
pub fn revert_worktree_paths(
    worktree: &Utf8Path,
    base: &weft_core::Tree,
    paths: &[Utf8PathBuf],
) -> Result<()> {
    for path in paths {
        match base.get(path) {
            Some(entry) => crate::fsio::write_file(worktree, path, entry)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_directory_safe() {
        assert!(valid_name("docker"));
        assert!(valid_name("feature-1_x"));
        assert!(!valid_name(""));
        assert!(!valid_name(".."));
        assert!(!valid_name("a/b"));
    }

    #[test]
    fn worktree_defaults_inside_the_session_dir_and_honours_an_explicit_path() {
        let mut sess = Session {
            session: SessionMeta {
                base: vec![],
                tree_hash: "h".into(),
                worktree: None,
                scope: vec![],
                adopted: false,
            },
            answers: Default::default(),
            secrets: Default::default(),
            instances: Vec::new(),
            foreach: None,
            generator: None,
            amend: None,
        };
        let root = Utf8Path::new("/t");
        assert_eq!(
            sess.worktree(root, "docker"),
            Utf8PathBuf::from("/t/.weft-sessions/docker/worktree")
        );
        sess.session.worktree = Some("/elsewhere/api".into());
        assert_eq!(
            sess.worktree(root, "docker"),
            Utf8PathBuf::from("/elsewhere/api")
        );
    }

    #[test]
    fn legacy_layout_migrates_to_the_default_session() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();
        let legacy = root.join(LEGACY_RECORD_DIR);
        std::fs::create_dir_all(legacy.join(WORKTREE_DIR)).unwrap();
        std::fs::write(
            legacy.join(SESSION_FILE),
            "[session]\nbase = []\ntree_hash = \"h\"\n",
        )
        .unwrap();

        migrate_legacy(root).unwrap();
        assert!(!legacy.exists());
        let moved = session_dir(root, DEFAULT_SESSION_NAME);
        assert!(moved.join(SESSION_FILE).is_file());
        assert!(moved.join(WORKTREE_DIR).is_dir());
        // Idempotent: a second run is a no-op.
        migrate_legacy(root).unwrap();
        assert!(Session::exists(root, DEFAULT_SESSION_NAME));
    }
}
