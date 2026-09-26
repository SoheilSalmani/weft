//! Git-sourced templates. Cargo's layout: one bare mirror per repository
//! under `~/.weft/git/db/<slug>-<hash>/`, and one immutable exported tree per
//! commit under `~/.weft/git/checkouts/<slug>-<hash>/<sha>/`. The engine reads
//! exports; a pinned commit that is already exported means no network at
//! all — the same guarantee the hub cache gives.
//!
//! Everything goes through the `git` binary, so the user's SSH keys and
//! credential helpers apply unchanged. Exports are whole trees, not just the
//! `//subdir`, so a template inside a repository of templates can still
//! `[[include]]` its siblings by relative path.

use std::io::IsTerminal;
use std::process::Command;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use weft_engine::source::GitRef;

pub fn cache_root() -> Result<Utf8PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(Utf8PathBuf::from(home).join(".weft").join("git"))
}

/// `<slug>-<hash>`: readable, and distinct per canonical URL.
fn key(r: &GitRef) -> String {
    let canonical = r.canonical_url();
    let slug: String = canonical
        .rsplit('/')
        .next()
        .unwrap_or("repo")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let slug = if slug.is_empty() {
        "repo".to_owned()
    } else {
        slug
    };
    let hash: String = Sha256::digest(canonical.as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{slug}-{hash}")
}

pub fn db_dir(r: &GitRef) -> Result<Utf8PathBuf> {
    Ok(cache_root()?.join("db").join(key(r)))
}

pub fn checkout_dir(r: &GitRef, sha: &str) -> Result<Utf8PathBuf> {
    Ok(cache_root()?.join("checkouts").join(key(r)).join(sha))
}

/// The first 12 characters, for messages.
pub fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

fn git(args: &[&str], cwd: Option<&Utf8Path>) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    // Without a terminal a credential prompt could only hang.
    if !std::io::stdin().is_terminal() {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
    }
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("`git` is required for git-sourced templates but was not found on PATH")
        } else {
            anyhow::anyhow!("running git: {e}")
        }
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git {}: {}", args.join(" "), stderr.trim());
    }
    Ok(out.stdout)
}

/// Build into a per-process staging directory beside `target`.
fn staging_for(target: &Utf8Path) -> Result<Utf8PathBuf> {
    let staging = Utf8PathBuf::from(format!("{target}.staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(target.parent().expect("cache path has a parent"))?;
    Ok(staging)
}

/// Move a finished staging directory into place. Concurrent weft processes
/// each build their own staging copy; whichever finishes first wins and the
/// others adopt its result (same URL or same commit ⇒ same content), so a
/// directory another process may already be reading is never removed.
fn install(
    staging: &Utf8Path,
    target: &Utf8Path,
    complete: impl Fn(&Utf8Path) -> bool,
) -> Result<bool> {
    if complete(target) {
        let _ = std::fs::remove_dir_all(staging);
        return Ok(false);
    }
    match std::fs::rename(staging, target) {
        Ok(()) => Ok(true),
        Err(_) if complete(target) => {
            let _ = std::fs::remove_dir_all(staging);
            Ok(false)
        }
        Err(e) => Err(e).with_context(|| format!("moving {staging} into place at {target}")),
    }
}

fn is_db(dir: &Utf8Path) -> bool {
    dir.join("HEAD").is_file()
}

/// The mirror for `r`, cloning it on first use. Returns `(db, fresh)`;
/// a fresh clone is already current, so callers skip the fetch.
fn ensure_db(r: &GitRef) -> Result<(Utf8PathBuf, bool)> {
    let db = db_dir(r)?;
    if is_db(&db) {
        return Ok((db, false));
    }
    // A directory without HEAD is a leftover nobody can be using.
    let _ = std::fs::remove_dir_all(&db);
    let staging = staging_for(&db)?;
    git(
        &["clone", "--mirror", "--quiet", &r.url, staging.as_str()],
        None,
    )
    .with_context(|| format!("cloning {}", r.repo))?;
    if install(&staging, &db, is_db)? {
        eprintln!("cloned {}", r.repo);
    }
    Ok((db, true))
}

/// Bring the mirror up to date with the remote (clone when absent).
pub fn fetch(r: &GitRef) -> Result<Utf8PathBuf> {
    let (db, fresh) = ensure_db(r)?;
    if !fresh {
        git(&["fetch", "--quiet", "--prune", "origin"], Some(&db))
            .with_context(|| format!("fetching {}", r.repo))?;
    }
    Ok(db)
}

/// Resolve a tag, branch, or commit (`None` = the default branch) against
/// the local mirror. Tags win over branches of the same name, like git.
pub fn resolve_rev(r: &GitRef, rev: Option<&str>) -> Result<String> {
    let db = db_dir(r)?;
    if !db.join("HEAD").is_file() {
        bail!(
            "`{}` is not in the local cache; drop --offline to fetch it",
            r.repo
        );
    }
    let candidates = match rev {
        None => vec!["HEAD".to_owned()],
        Some(rev) => vec![
            format!("refs/tags/{rev}"),
            format!("refs/heads/{rev}"),
            rev.to_owned(),
        ],
    };
    for candidate in &candidates {
        let spec = format!("{candidate}^{{commit}}");
        if let Ok(out) = git(&["rev-parse", "--verify", "--quiet", &spec], Some(&db)) {
            let sha = String::from_utf8_lossy(&out).trim().to_owned();
            if !sha.is_empty() {
                return Ok(sha);
            }
        }
    }
    match rev {
        Some(rev) => bail!("`{rev}` is not a tag, branch, or commit in {}", r.repo),
        None => bail!("{} has no default branch (empty repository?)", r.repo),
    }
}

/// The exported tree for `sha`, creating it from the mirror on first use.
/// `offline` refuses to clone a missing mirror.
pub fn export(r: &GitRef, sha: &str, offline: bool) -> Result<Utf8PathBuf> {
    let dir = checkout_dir(r, sha)?;
    if dir.is_dir() {
        return Ok(dir);
    }
    let db = if offline {
        let db = db_dir(r)?;
        if !db.join("HEAD").is_file() {
            bail!(
                "commit {} of {} is not in the local cache; drop --offline to fetch it",
                short(sha),
                r.repo
            );
        }
        db
    } else {
        ensure_db(r)?.0
    };
    let bytes = match git(&["archive", "--format=tar", sha], Some(&db)) {
        Ok(bytes) => bytes,
        Err(_) if !offline => {
            // The mirror may predate the commit (a lock pinned upstream).
            git(&["fetch", "--quiet", "--prune", "origin"], Some(&db))
                .with_context(|| format!("fetching {}", r.repo))?;
            git(&["archive", "--format=tar", sha], Some(&db))
                .with_context(|| format!("commit {} not found in {}", short(sha), r.repo))?
        }
        Err(e) => return Err(e.context(format!("commit {} not found in {}", short(sha), r.repo))),
    };

    let staging = staging_for(&dir)?;
    std::fs::create_dir_all(&staging)?;
    let mut archive = tar::Archive::new(&bytes[..]);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type() == tar::EntryType::XGlobalHeader {
            continue;
        }
        let rel = entry.path()?.to_string_lossy().to_string();
        if rel.starts_with('/') || rel.split('/').any(|seg| seg == "..") {
            bail!("archive entry escapes the export root: {rel:?}");
        }
        entry.unpack_in(staging.as_std_path())?;
    }
    install(&staging, &dir, Utf8Path::is_dir)?;
    Ok(dir)
}

/// The template directory inside an export: the `//subdir` (or the root),
/// which must hold a `weft.toml`. On a miss, list the templates the
/// repository does contain so the right `//path` is one copy-paste away.
pub fn template_dir(r: &GitRef, export: &Utf8Path) -> Result<Utf8PathBuf> {
    let dir = match &r.subdir {
        Some(sub) => export.join(sub),
        None => export.to_owned(),
    };
    if dir.join("weft.toml").is_file() {
        return Ok(dir);
    }
    let mut found = Vec::new();
    find_manifests(export, export, 0, &mut found);
    let at = match &r.subdir {
        Some(sub) => format!("`{sub}`"),
        None => "the repository root".to_owned(),
    };
    if found.is_empty() {
        bail!(
            "{}: no weft.toml at {at}, and no weft template anywhere in the repository",
            r.repo
        );
    }
    bail!(
        "{}: no weft.toml at {at}; templates in this repository: {} (use `{}//<path>`)",
        r.repo,
        found.join(", "),
        r.repo
    );
}

fn find_manifests(root: &Utf8Path, dir: &Utf8Path, depth: usize, found: &mut Vec<String>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = dir.read_dir_utf8() else {
        return;
    };
    let mut dirs: Vec<Utf8PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            dirs.push(path.to_owned());
        }
    }
    dirs.sort();
    for sub in dirs {
        if sub.join("weft.toml").is_file() {
            if let Ok(rel) = sub.strip_prefix(root) {
                found.push(rel.to_string());
            }
        }
        find_manifests(root, &sub, depth + 1, found);
    }
}
