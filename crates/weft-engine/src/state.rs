use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use weft_core::{AnswerId, AnswerSet, PatchId, Value};

pub const STATE_DIR: &str = ".weft";
pub const STATE_FILE: &str = "state.toml";

/// `.weft/state.toml` in a scaffolded destination. Answers are stored as
/// concrete values *except* secrets, which are stored as references only
/// (`[secrets]` maps answer id to its source spec string).
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub state: StateMeta,
    #[serde(default)]
    pub answers: AnswerSet,
    #[serde(default)]
    pub secrets: BTreeMap<AnswerId, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateMeta {
    /// Template ref as given on the command line (path for MVP).
    pub template: String,
    /// Pinned base: ids of the patches this tree was rendered from.
    pub base: Vec<PatchId>,
    /// Hash of the rendered tree (pre user edits).
    pub tree_hash: String,
}

impl State {
    /// Split `answers` into persistable values and secret refs.
    pub fn new(
        template: String,
        base: Vec<PatchId>,
        tree_hash: String,
        answers: &AnswerSet,
        secret_specs: &BTreeMap<AnswerId, String>,
    ) -> Self {
        let mut plain = AnswerSet::new();
        for (id, value) in answers.iter() {
            match value {
                Value::Secret(_) => {}
                other => {
                    plain.insert(id.clone(), other.clone());
                }
            }
        }
        State {
            state: StateMeta {
                template,
                base,
                tree_hash,
            },
            answers: plain,
            secrets: secret_specs.clone(),
        }
    }

    pub fn save(&self, dest: &Utf8Path) -> Result<()> {
        let dir = dest.join(STATE_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(STATE_FILE);
        let toml = toml::to_string_pretty(self).context("serializing state")?;
        std::fs::write(&path, toml).with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn load(dest: &Utf8Path) -> Result<Self> {
        let path = state_path(dest);
        let src = std::fs::read_to_string(&path).with_context(|| {
            format!("`{path}` not found: is `{dest}` a weft-scaffolded project?")
        })?;
        toml::from_str(&src).with_context(|| format!("parsing {path}"))
    }
}

pub fn state_path(dest: &Utf8Path) -> Utf8PathBuf {
    dest.join(STATE_DIR).join(STATE_FILE)
}
