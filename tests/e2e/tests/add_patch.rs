//! `weft add -p`: staging a recording session hunk by hunk. Piped stdin feeds
//! the picker one key per line (a terminal reads single keypresses instead),
//! so the whole flow is drivable end to end.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use weft_e2e::weft;

/// A file with two edits far enough apart to be separate hunks.
const BEFORE: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
const AFTER: &str = "1\nTWO\n3\n4\n5\n6\n7\n8\nNINE\n10\n";

/// Init a template, commit `f.txt` as a patch, then open a fresh session on
/// top of it — the picker needs a file that already exists in the base.
fn session_with_base_file(dir: &Path) -> (PathBuf, PathBuf) {
    let tpl = dir.join("t");
    weft()
        .arg("init")
        .arg(&tpl)
        .args(["--name", "t"])
        .assert()
        .success();
    weft()
        .args(["session", "new", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success();

    let wt = tpl.join(".weft-sessions/main/worktree");
    fs::write(wt.join("f.txt"), BEFORE).unwrap();
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "base", "--yes"])
        .assert()
        .success();
    // The session ended with that commit; the next one starts from `latest`,
    // so `f.txt` is part of the base.
    weft()
        .args(["session", "new", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success();
    (tpl, wt)
}

#[test]
fn keys_stage_one_hunk_and_leave_the_other_unstaged() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, wt) = session_with_base_file(tmp.path());
    fs::write(wt.join("f.txt"), AFTER).unwrap();

    // Take the first hunk, skip the second.
    weft()
        .args(["add", "-p", "--session", "main", "--template"])
        .arg(&tpl)
        .write_stdin("y\nn\n")
        .assert()
        .success()
        .stdout(contains("@@"))
        .stdout(contains("+ TWO"))
        .stdout(contains("+ NINE"))
        .stderr(contains("staged 1 path(s)"));

    // The stage holds only the first edit…
    weft()
        .args(["diff", "--staged", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains("TWO"))
        .stdout(contains("NINE").not());

    // …and the second is still sitting unstaged in the worktree.
    weft()
        .args(["status", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains("unstaged:"))
        .stdout(contains("f.txt"));

    // Committing writes a patch with just the staged hunk.
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "first", "--stack", "--yes"])
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/first.json")).unwrap();
    assert!(patch.contains("TWO"), "{patch}");
    assert!(!patch.contains("NINE"), "{patch}");

    // The leftover edit survives into the next commit.
    weft()
        .args(["diff", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains("NINE"));
}

#[test]
fn a_stages_every_remaining_hunk_in_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, wt) = session_with_base_file(tmp.path());
    fs::write(wt.join("f.txt"), AFTER).unwrap();

    weft()
        .args(["add", "-p", "--session", "main", "--template"])
        .arg(&tpl)
        .write_stdin("a\n")
        .assert()
        .success()
        .stderr(contains("staged 1 path(s)"));

    weft()
        .args(["status", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains("unstaged:").not());
}

#[test]
fn s_splits_a_merged_hunk_into_separately_stageable_pieces() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, wt) = session_with_base_file(tmp.path());
    // Two edits two lines apart share one hunk until it is split.
    fs::write(wt.join("f.txt"), "1\nTWO\n3\nFOUR\n5\n6\n7\n8\n9\n10\n").unwrap();

    weft()
        .args(["add", "-p", "--session", "main", "--template"])
        .arg(&tpl)
        .write_stdin("s\ny\nn\n")
        .assert()
        .success()
        .stdout(contains("split into 2 hunks"));

    weft()
        .args(["diff", "--staged", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains("TWO"))
        .stdout(contains("FOUR").not());
}

#[test]
fn declining_everything_stages_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, wt) = session_with_base_file(tmp.path());
    fs::write(wt.join("f.txt"), AFTER).unwrap();

    weft()
        .args(["add", "-p", "--session", "main", "--template"])
        .arg(&tpl)
        .write_stdin("n\nn\n")
        .assert()
        .success()
        .stderr(contains("staged 0 path(s)"));

    weft()
        .args(["diff", "--staged", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stderr(contains("nothing staged"));
}

#[test]
fn nothing_to_stage_is_reported_not_prompted() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, _wt) = session_with_base_file(tmp.path());
    weft()
        .args(["add", "-p", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stderr(contains("nothing to stage"));
}
