//! A recording session: the pinned base state a scratch worktree was
//! materialized from, stored in the template dir between `weft record` and
//! `weft commit`.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerId, AnswerSet, PatchId};

pub const RECORD_DIR: &str = ".weft-record";
pub const SESSION_FILE: &str = "session.toml";
pub const WORKTREE_DIR: &str = "worktree";

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub session: SessionMeta,
    /// Concrete answers the base was rendered with (secrets excluded).
    #[serde(default)]
    pub answers: AnswerSet,
    /// Secret answers as source references, re-resolved at commit time.
    #[serde(default)]
    pub secrets: BTreeMap<AnswerId, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Ids of the patches that form the pinned base state.
    pub base: Vec<PatchId>,
    /// Hash of the rendered base tree.
    pub tree_hash: String,
}

pub fn record_dir(template_root: &Utf8Path) -> Utf8PathBuf {
    template_root.join(RECORD_DIR)
}

pub fn worktree_dir(template_root: &Utf8Path) -> Utf8PathBuf {
    record_dir(template_root).join(WORKTREE_DIR)
}

impl Session {
    pub fn save(&self, template_root: &Utf8Path) -> Result<()> {
        let dir = record_dir(template_root);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(SESSION_FILE);
        std::fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn load(template_root: &Utf8Path) -> Result<Self> {
        let path = record_dir(template_root).join(SESSION_FILE);
        let src = std::fs::read_to_string(&path).with_context(|| {
            format!("no recording session found at `{path}`; run `weft record` first")
        })?;
        toml::from_str(&src).with_context(|| format!("parsing {path}"))
    }

    pub fn exists(template_root: &Utf8Path) -> bool {
        record_dir(template_root).join(SESSION_FILE).exists()
    }

    pub fn discard(template_root: &Utf8Path) -> Result<()> {
        let dir = record_dir(template_root);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {dir}"))?;
        }
        Ok(())
    }
}
