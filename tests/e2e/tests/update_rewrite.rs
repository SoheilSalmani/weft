//! `weft update` survives template history rewrites. The project stores its
//! base patch bodies (`.weft/base.json`), so the 3-way-merge base is the
//! content it was scaffolded from — independent of the template's current
//! patch ids. amend/resync propagate; squash is a no-op; the merge keeps
//! user edits.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn hello_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

/// A template with a leaf `note` patch creating a multi-line note.txt, plus a
/// scaffolded project. Returns (guard, template, project).
fn scaffolded(note_body: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/note.txt"), note_body).unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("note")
        .arg("--yes")
        .assert()
        .success();
    let proj = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&proj)
        .arg("--answer")
        .arg("project_name=Acme")
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    assert!(
        proj.join(".weft/base.json").exists(),
        "project stores a base snapshot"
    );
    (dir, tpl, proj)
}

/// Amend the template's `note` patch to `new_body`.
fn amend_note(tpl: &Path, new_body: &str) {
    weft()
        .arg("patch")
        .arg("amend")
        .arg("note")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--template")
        .arg(tpl)
        .arg("--non-interactive")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/note/worktree/note.txt"), new_body).unwrap();
    weft()
        .arg("commit")
        .args(["--session", "note"])
        .arg("--template")
        .arg(tpl)
        .arg("--yes")
        .assert()
        .success();
}

#[test]
fn amend_propagates_to_project_across_id_rewrite() {
    let (_g, tpl, proj) = scaffolded("original\nkeep\n");
    amend_note(&tpl, "amended\nkeep\n");
    // Update must ADOPT the amend, not bail on a missing pinned id.
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    assert_eq!(read(&proj, "note.txt"), "amended\nkeep\n");
    // A second update is a clean no-op (base re-pinned).
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    assert_eq!(read(&proj, "note.txt"), "amended\nkeep\n");
}

#[test]
fn update_merges_amend_around_a_user_edit() {
    // 6 lines so the user edit (top) and the template amend (bottom) are far
    // apart — a clean 3-way merge, only possible if the base is the OLD
    // content.
    let (_g, tpl, proj) = scaffolded("l1\nl2\nl3\nl4\nl5\nl6\n");
    // User edits line 1 locally.
    std::fs::write(proj.join("note.txt"), "USER1\nl2\nl3\nl4\nl5\nl6\n").unwrap();
    // Template amends line 6.
    amend_note(&tpl, "l1\nl2\nl3\nl4\nl5\nTPL6\n");
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    // Both survive: user's line-1 edit kept, template's line-6 change adopted.
    assert_eq!(read(&proj, "note.txt"), "USER1\nl2\nl3\nl4\nl5\nTPL6\n");
}

#[test]
fn squash_update_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    // s1 creates a.txt; s2 (on s1) creates b.txt.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/a.txt"), "aaa\n").unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("s1")
        .arg("--yes")
        .assert()
        .success();
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--base")
        .arg("s1")
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/b.txt"), "bbb\n").unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("s2")
        .arg("--yes")
        .assert()
        .success();

    let proj = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&proj)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();

    // Squash (rendered output unchanged) then update → no file changes.
    weft()
        .arg("patch")
        .arg("squash")
        .arg("s1")
        .arg("s2")
        .arg("--into")
        .arg("combined")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    // Squash leaves the rendered output identical → the project is unchanged.
    assert_eq!(read(&proj, "a.txt"), "aaa\n");
    assert_eq!(read(&proj, "b.txt"), "bbb\n");
}

#[test]
fn back_compat_without_snapshot() {
    let (_g, tpl, proj) = scaffolded("v1\n");
    // Simulate a pre-feature project: drop the base snapshot.
    std::fs::remove_file(proj.join(".weft/base.json")).unwrap();
    // Non-rewritten template: update still works (id-match fallback).
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    // …and the update re-records a snapshot, healing the project.
    assert!(
        proj.join(".weft/base.json").exists(),
        "update heals the snapshot"
    );

    // A fresh old-format project against a REWRITTEN template still bails.
    let (_g2, tpl2, proj2) = scaffolded("v1\n");
    std::fs::remove_file(proj2.join(".weft/base.json")).unwrap();
    amend_note(&tpl2, "v2\n");
    weft()
        .arg("update")
        .arg(&proj2)
        .arg("--template")
        .arg(&tpl2)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .failure()
        .stderr(predicates::str::contains("no longer exists"));
}
