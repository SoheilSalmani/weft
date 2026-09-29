//! Recording-session lifecycle: the staging index (`weft add`/`reset`/`status`),
//! multiple patches per session with the sibling-vs-stack choice, `session
//! refresh` (edit the manifest or change answers mid-session), `session end`,
//! and back-compat of the classic one-shot `record → commit`.

use std::fs;
use std::path::{Path, PathBuf};

use weft_e2e::weft;

/// Init a blank template in a tempdir and declare a `project_name` question.
fn init_template(dir: &Path) -> PathBuf {
    let tpl = dir.join("t");
    weft()
        .arg("init")
        .arg(&tpl)
        .args(["--name", "t"])
        .assert()
        .success();
    append(
        &tpl.join("weft.toml"),
        "\n[[question]]\nid = \"project_name\"\nkind = \"string\"\nprompt = \"Project name\"\n",
    );
    tpl
}

fn append(path: &Path, text: &str) {
    let mut src = fs::read_to_string(path).unwrap();
    src.push_str(text);
    fs::write(path, src).unwrap();
}

fn record(tpl: &Path, name_value: &str) {
    weft()
        .args(["session", "new", "main", "--template"])
        .arg(tpl)
        .args(["--answer", name_value])
        .assert()
        .success();
}

fn deps(tpl: &Path, patch: &str) -> String {
    fs::read_to_string(tpl.join("patches").join(format!("{patch}.json"))).unwrap()
}

#[test]
fn staged_commits_default_to_independent_siblings() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("A.txt"), "a\n").unwrap();
    fs::write(wt.join("B.txt"), "b\n").unwrap();

    // Stage only A. Status shows A staged and B unstaged.
    weft()
        .args(["add", "A.txt", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success();
    weft()
        .args(["status", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "staged (the next commit takes these):",
        ))
        .stdout(predicates::str::contains("A.txt"))
        .stdout(predicates::str::contains("unstaged:"));

    // Commit the staged subset. Non-interactive defaults to sibling: A is
    // peeled out and B remains.
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "a", "--yes"])
        .assert()
        .success()
        .stderr(predicates::str::contains("independent sibling"));

    // Commit the rest (whole worktree, since nothing is staged now).
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "b", "--yes"])
        .assert()
        .success();

    // Both patches are roots (independent siblings), and weft check proves they
    // commute.
    assert!(!deps(&tpl, "a").contains("depends_on"));
    assert!(!deps(&tpl, "b").contains("depends_on"));
    weft()
        .args([
            "check",
            &tpl.to_string_lossy(),
            "--answer",
            "project_name=x",
        ])
        .assert()
        .success();

    // The session ended once everything was committed.
    weft()
        .args(["session", "list", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn stack_makes_the_next_patch_depend_on_the_committed_one() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("A.txt"), "a\n").unwrap();
    fs::write(wt.join("B.txt"), "b\n").unwrap();
    weft()
        .args(["add", "A.txt", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success();
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "a", "--stack", "--yes"])
        .assert()
        .success()
        .stderr(predicates::str::contains("building on it"));
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "b", "--yes"])
        .assert()
        .success();

    assert!(
        deps(&tpl, "b").contains("\"a\""),
        "the stacked patch should depend on the one below it: {}",
        deps(&tpl, "b")
    );
}

#[test]
fn plain_commit_still_commits_everything_and_ends_the_session() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("README.md"), "# Demo\n").unwrap();
    // No staging: the classic one-shot flow commits the whole worktree.
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "base", "--yes"])
        .assert()
        .success();
    assert!(tpl.join("patches/base.json").exists());
    weft()
        .args(["session", "list", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::is_empty());
}

#[test]
fn session_end_guards_dirty_worktree_and_discards() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=A");
    fs::write(wt.join("X.txt"), "x\n").unwrap();
    // Refuses to end with uncommitted changes.
    weft()
        .args(["session", "end", "--template"])
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("uncommitted change"));
    // --discard ends it anyway.
    weft()
        .args(["session", "end", "--template"])
        .arg(&tpl)
        .arg("--discard")
        .assert()
        .success()
        .stderr(predicates::str::contains("ended"));
    // A fresh session starts with no --force needed.
    record(&tpl, "project_name=B");
    weft()
        .args(["status", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("session `main` on"));
}

#[test]
fn refresh_picks_up_a_new_question_for_abstraction() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    append(
        &tpl.join("weft.toml"),
        "\n[[question]]\nid = \"license\"\nkind = \"string\"\ndefault = \"'MIT'\"\n",
    );
    weft()
        .args(["session", "refresh", "--template"])
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .success();

    fs::write(wt.join("README.md"), "# Demo\nLicense: MIT\n").unwrap();
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "doc", "--yes"])
        .assert()
        .success();

    let doc = fs::read_to_string(tpl.join("patches/doc.json")).unwrap();
    assert!(
        doc.contains("\"license\""),
        "the newly added answer should be abstracted after refresh: {doc}"
    );
    assert!(doc.contains("\"project_name\""));
}

#[test]
fn refresh_merges_an_answer_change_onto_worktree_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("README.md"), "# Demo\n").unwrap();
    fs::write(wt.join("NOTES.md"), "notes\n").unwrap();
    weft()
        .args(["commit", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--name", "base", "--yes"])
        .assert()
        .success();

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("NOTES.md"), "notes\n\nLocal note.\n").unwrap();

    weft()
        .args(["session", "refresh", "--template"])
        .arg(&tpl)
        .args(["--answer", "project_name=Renamed", "--non-interactive"])
        .assert()
        .success();

    let readme = fs::read_to_string(wt.join("README.md")).unwrap();
    let notes = fs::read_to_string(wt.join("NOTES.md")).unwrap();
    assert!(readme.contains("# Renamed"), "README re-rendered: {readme}");
    assert!(
        notes.contains("Local note."),
        "NOTES edit preserved: {notes}"
    );
    assert!(
        !readme.contains("<<<<<<<") && !notes.contains("<<<<<<<"),
        "clean merge, no conflict"
    );
}

#[test]
fn diff_staged_shows_only_the_staged_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = tpl.join(".weft-sessions/main/worktree");

    record(&tpl, "project_name=Demo");
    fs::write(wt.join("A.txt"), "a\n").unwrap();
    fs::write(wt.join("B.txt"), "b\n").unwrap();
    weft()
        .args(["add", "A.txt", "--session", "main", "--template"])
        .arg(&tpl)
        .assert()
        .success();

    // The whole-worktree diff lists both files.
    let whole = weft()
        .args(["diff", "--session", "main", "--template"])
        .arg(&tpl)
        .arg("--json")
        .output()
        .unwrap();
    let whole = String::from_utf8_lossy(&whole.stdout);
    assert!(
        whole.contains("A.txt") && whole.contains("B.txt"),
        "{whole}"
    );

    // The staged diff lists only the staged file.
    let staged = weft()
        .args(["diff", "--session", "main", "--template"])
        .arg(&tpl)
        .args(["--staged", "--json"])
        .output()
        .unwrap();
    let staged = String::from_utf8_lossy(&staged.stdout);
    assert!(
        staged.contains("A.txt") && !staged.contains("B.txt"),
        "staged diff should list only the staged file: {staged}"
    );
}
