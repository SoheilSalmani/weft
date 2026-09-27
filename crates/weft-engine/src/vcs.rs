//! Just enough git for `weft update`'s dirty guard: which of the files an
//! update would write have uncommitted changes. Shells out to `git` (no
//! library), and degrades to "nothing is dirty" wherever git can't answer —
//! the guard exists to keep a merge reviewable and undoable with git, so
//! outside a git work tree there is nothing for it to protect.

use std::collections::BTreeSet;
use std::process::{Command, Output};

use camino::{Utf8Path, Utf8PathBuf};

/// The subset of `paths` (relative to `dest`) that git reports as changed
/// since the last commit: modified, staged, deleted, or untracked. Empty
/// when `dest` is not inside a git work tree or git is unavailable.
pub fn dirty_paths(dest: &Utf8Path, paths: &[&Utf8PathBuf]) -> Vec<Utf8PathBuf> {
    if paths.is_empty() {
        return Vec::new();
    }
    // `status --porcelain` prints paths relative to the repository root;
    // `--show-prefix` is where `dest` sits in it (empty at the root).
    let Some(prefix) = git(dest, &["rev-parse", "--show-prefix"]) else {
        return Vec::new();
    };
    let prefix = String::from_utf8_lossy(&prefix.stdout)
        .trim_end()
        .to_owned();
    let Some(status) = git(
        dest,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    ) else {
        return Vec::new();
    };
    let changed = changed_entries(&status.stdout, &prefix);
    paths
        .iter()
        .filter(|p| changed.contains(p.as_str()))
        .map(|p| (*p).clone())
        .collect()
}

/// Run `git -C dest …`; `None` when git is missing or the command fails
/// (e.g. `dest` is not in a work tree).
fn git(dest: &Utf8Path, args: &[&str]) -> Option<Output> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dest)
        .arg("--literal-pathspecs")
        .args(args)
        // `status` may refresh the index; never take its lock for a read.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    out.status.success().then_some(out)
}

/// Parse `status --porcelain=v1 -z` output into dest-relative paths.
/// Entries are `XY path\0`; a rename or copy carries its source as an extra
/// `\0`-terminated field, and both sides count as changed.
fn changed_entries(stdout: &[u8], prefix: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut fields = stdout.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let (xy, path) = (&entry[..2], &entry[3..]);
        let mut paths = vec![path];
        if xy.contains(&b'R') || xy.contains(&b'C') {
            if let Some(source) = fields.next() {
                paths.push(source);
            }
        }
        for path in paths {
            let path = String::from_utf8_lossy(path);
            if let Some(rel) = path.strip_prefix(prefix) {
                out.insert(rel.to_owned());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_entries_relative_to_the_project() {
        let raw = b" M proj/README.md\0?? proj/new.txt\0R  proj/b.txt\0proj/a.txt\0 M other/x\0";
        let changed = changed_entries(raw, "proj/");
        let expected: BTreeSet<String> = ["README.md", "new.txt", "b.txt", "a.txt"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(changed, expected);
    }
}
