//! Generator metadata: the shell command a patch's content was produced by
//! (`weft record --exec`), plus everything needed to re-run it
//! deterministically later (`weft patch resync`).
//!
//! Lives in `PatchMeta`, so it is **never part of the content hash** —
//! attaching or editing a generator can't change patch ids.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hook::Command;
use crate::{AnswerId, AnswerSet};

/// How a generated patch reproduces its content: render the patch's base
/// with `answers`, run `command` in the worktree, and diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// The command run in the recording worktree (same interpolatable form
    /// as hook actions: a bare string, or segments with answer/expr refs).
    pub command: Command,
    /// The record-time answers the base was rendered with (secrets
    /// excluded) — the reproducibility anchor for resync.
    #[serde(default, skip_serializing_if = "AnswerSet::is_empty")]
    pub answers: AnswerSet,
    /// Secret answers as source references (`env:…`, `cmd:…`, `prompt`),
    /// re-resolved at resync time. Values never land here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<AnswerId, String>,
    /// Occurrences kept literal at commit (`ANSWER@PATH:LINE[:NTH]`),
    /// replayed on resync so abstraction decisions stay stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep_literal: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    #[test]
    fn serde_round_trip_bare_command() {
        let generator = Generator {
            command: Command::literal("npx shadcn@latest add button"),
            answers: [(AnswerId::from("project_name"), Value::String("x".into()))]
                .into_iter()
                .collect(),
            secrets: BTreeMap::new(),
            keep_literal: vec!["project_name@README.md:1".into()],
        };
        let json = serde_json::to_string(&generator).unwrap();
        // A fully-literal command serializes as a bare string.
        assert!(json.contains("\"command\":\"npx shadcn@latest add button\""));
        assert!(!json.contains("secrets"), "empty maps are omitted: {json}");
        let back: Generator = serde_json::from_str(&json).unwrap();
        assert_eq!(back, generator);
    }
}
