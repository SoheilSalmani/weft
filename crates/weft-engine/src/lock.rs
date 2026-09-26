//! `weft.lock` — exact resolved versions for a composed template's remote
//! includes. Cargo-shaped: `weft.toml` declares *requirements* (a semver
//! range for `hub:`, a tracked rev for git), the lock pins *versions* +
//! checksums (hub) or *commits* (git) so `Template::load` of a composed
//! template is reproducible across machines. Committed and published with
//! the template.
//!
//! The lock is flat and transitive: a remote child's own remote includes
//! contribute entries too, keyed by ref. Entries are kept in sorted order
//! so the file is stable under version control.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};

pub const LOCK_FILE: &str = "weft.lock";

/// One resolved include. Hub entries carry `version` + `sha256`; git
/// entries carry `commit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locked {
    /// The `hub:owner/name` ref, or the git `repo[//subdir]` ref.
    pub r#ref: String,
    /// The requirement it was resolved from: a semver range (`^1.0`) for
    /// hub, the tracked rev (`v1.2.0`, `main`, or `HEAD` for the default
    /// branch) for git.
    pub req: String,
    /// Hub: the exact version chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Hub: sha256 of the child's published tarball.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Git: the commit the rev resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

impl Locked {
    pub fn hub(r#ref: String, req: String, version: String, sha256: String) -> Self {
        Locked {
            r#ref,
            req,
            version: Some(version),
            sha256: Some(sha256),
            commit: None,
        }
    }

    pub fn git(r#ref: String, req: String, commit: String) -> Self {
        Locked {
            r#ref,
            req,
            version: None,
            sha256: None,
            commit: Some(commit),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default, rename = "include")]
    pub includes: Vec<Locked>,
}

#[derive(Serialize, Deserialize)]
struct LockFile {
    #[serde(default, rename = "include")]
    include: Vec<Locked>,
}

impl Lock {
    /// Read `weft.lock` from a template root; empty when absent.
    pub fn load(root: &Utf8Path) -> Result<Self> {
        let path = root.join(LOCK_FILE);
        let Ok(src) = std::fs::read_to_string(&path) else {
            return Ok(Self::default());
        };
        let file: LockFile = toml::from_str(&src).with_context(|| format!("parsing {path}"))?;
        Ok(Self {
            includes: file.include,
        })
    }

    /// Write `weft.lock` (sorted, deduped). Removes the file when empty.
    pub fn save(&self, root: &Utf8Path) -> Result<()> {
        let path = root.join(LOCK_FILE);
        if self.includes.is_empty() {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        let mut includes = self.includes.clone();
        includes.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
        includes.dedup_by(|a, b| a.r#ref == b.r#ref);
        let file = LockFile { include: includes };
        let toml = toml::to_string_pretty(&file).context("serializing lock")?;
        let header = "# weft.lock — resolved include versions. Do not edit by hand.\n";
        std::fs::write(&path, format!("{header}{toml}"))
            .with_context(|| format!("writing {path}"))?;
        Ok(())
    }

    pub fn entry(&self, r#ref: &str) -> Option<&Locked> {
        self.includes.iter().find(|e| e.r#ref == r#ref)
    }

    /// Insert or replace the entry for a ref; returns true if it changed.
    pub fn upsert(&mut self, locked: Locked) -> bool {
        match self.includes.iter_mut().find(|e| e.r#ref == locked.r#ref) {
            Some(existing) => {
                if *existing == locked {
                    false
                } else {
                    *existing = locked;
                    true
                }
            }
            None => {
                self.includes.push(locked);
                true
            }
        }
    }

    /// The ref → locked map, for validation/lookups.
    pub fn by_ref(&self) -> BTreeMap<&str, &Locked> {
        self.includes
            .iter()
            .map(|e| (e.r#ref.as_str(), e))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_sorted_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        let mut lock = Lock::default();
        assert!(lock.upsert(Locked::hub(
            "hub:acme/zeta".into(),
            "^1".into(),
            "1.2.0".into(),
            "aa".into()
        )));
        assert!(lock.upsert(Locked::hub(
            "hub:acme/alpha".into(),
            "^0.1".into(),
            "0.1.5".into(),
            "bb".into()
        )));
        assert!(lock.upsert(Locked::git(
            "gh:acme/tpls//base".into(),
            "v1".into(),
            "c0ffee".into()
        )));
        lock.save(root).unwrap();

        let reloaded = Lock::load(root).unwrap();
        // sorted by ref
        assert_eq!(reloaded.includes[0].r#ref, "gh:acme/tpls//base");
        assert_eq!(reloaded.includes[1].r#ref, "hub:acme/alpha");
        assert_eq!(reloaded.includes[2].r#ref, "hub:acme/zeta");
        assert_eq!(
            reloaded.entry("hub:acme/zeta").unwrap().version.as_deref(),
            Some("1.2.0")
        );
        let git = reloaded.entry("gh:acme/tpls//base").unwrap();
        assert_eq!(git.commit.as_deref(), Some("c0ffee"));
        assert_eq!(git.version, None);

        // upsert with the same value reports no change
        let mut again = reloaded.clone();
        assert!(!again.upsert(Locked::hub(
            "hub:acme/zeta".into(),
            "^1".into(),
            "1.2.0".into(),
            "aa".into()
        )));
        // a new version reports a change
        assert!(again.upsert(Locked::hub(
            "hub:acme/zeta".into(),
            "^1".into(),
            "1.3.0".into(),
            "cc".into()
        )));
    }

    #[test]
    fn absent_lock_loads_empty_and_empty_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(dir.path()).unwrap();
        assert!(Lock::load(root).unwrap().includes.is_empty());
        // saving empty leaves no file
        Lock::default().save(root).unwrap();
        assert!(!root.join(LOCK_FILE).exists());
    }
}
