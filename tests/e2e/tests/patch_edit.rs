//! Editing recorded patches: `weft patch set-command` / `detach` (generator
//! metadata) and `weft patch amend` (re-derive a leaf patch's content).

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

/// Copy the fixture and record a generator patch whose command cats `src`.
fn with_generator(src: &Path) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--exec")
        .arg(format!("cat '{}' > gen.txt", src.display()))
        .assert()
        .success();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success();
    (dir, tpl)
}

fn gen_command(tpl: &Path) -> String {
    let d: serde_json::Value = serde_json::from_str(&read(tpl, "patches/gen.json")).unwrap();
    serde_json::to_string(&d["generator"]["command"]).unwrap()
}

#[test]
fn set_command_updates_and_optionally_resyncs() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("v1.txt");
    std::fs::write(&src, "one\n").unwrap();
    let (_g, tpl) = with_generator(&src);

    // Change the command to emit different content, and resync in one step.
    weft()
        .arg("patch")
        .arg("set-command")
        .arg("gen")
        .arg("printf 'two\\n' > gen.txt")
        .arg("--template")
        .arg(&tpl)
        .arg("--resync")
        .assert()
        .success();

    assert!(gen_command(&tpl).contains("two"), "command updated");
    // The resync regenerated the ops from the new command.
    assert!(
        read(&tpl, "patches/gen.json").contains("two"),
        "ops regenerated"
    );

    // set-command refuses a non-generator patch.
    weft()
        .arg("patch")
        .arg("set-command")
        .arg("base")
        .arg("echo hi")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("not a generator patch"));
}

#[test]
fn detach_makes_a_generator_patch_plain() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("v.txt");
    std::fs::write(&src, "x\n").unwrap();
    let (_g, tpl) = with_generator(&src);

    weft()
        .arg("patch")
        .arg("detach")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    assert!(
        !read(&tpl, "patches/gen.json").contains("generator"),
        "generator metadata dropped"
    );
    // resync now refuses it (no generator).
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("no generator command"));
}

#[test]
fn amend_refuses_generator_patches() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("v.txt");
    std::fs::write(&src, "x\n").unwrap();
    let (_g, tpl) = with_generator(&src);

    weft()
        .arg("patch")
        .arg("amend")
        .arg("gen")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("generator patch"))
        .stderr(predicates::str::contains("resync"));
    assert!(
        !tpl.join(".weft-sessions").exists(),
        "no session left behind"
    );
}

#[test]
fn amend_non_leaf_warns_about_dependents() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    // `base` has dependents (docker, deploy): amend is allowed, with a note.
    weft()
        .arg("patch")
        .arg("amend")
        .arg("base")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--answer")
        .arg("use_docker=true")
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .success()
        .stderr(predicates::str::contains("has dependents"));
    assert!(tpl.join(".weft-sessions/base/worktree").exists());
}

/// Open an amend session on `base` and add a file to its worktree.
fn amend_base_with_edit(tpl: &Path) {
    weft()
        .args(["patch", "amend", "base"])
        .args([
            "--answer",
            "project_name=Demo",
            "--answer",
            "use_docker=true",
        ])
        .arg("--template")
        .arg(tpl)
        .arg("--non-interactive")
        .assert()
        .success();
    std::fs::write(
        tpl.join(".weft-sessions/base/worktree/extra.txt"),
        "extra\n",
    )
    .unwrap();
}

#[test]
fn amend_commit_applies_title_and_description() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    amend_base_with_edit(&tpl);
    weft()
        .args(["commit", "--yes", "--no-tui"])
        .args(["--describe", "New words.", "--title", "New title"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("amended patch `base`"));
    let after = read(&tpl, "patches/base.json");
    assert!(after.contains("\"description\": \"New words.\""), "{after}");
    assert!(after.contains("\"title\": \"New title\""), "{after}");
    assert!(
        after.contains("extra.txt"),
        "ops rewritten in the same save: {after}"
    );
}

#[test]
fn amend_commit_refuses_when() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    amend_base_with_edit(&tpl);
    let before = read(&tpl, "patches/base.json");
    weft()
        .args(["commit", "--yes", "--no-tui", "--when", "x"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("amend keeps the patch's name"));
    assert_eq!(read(&tpl, "patches/base.json"), before);
    assert!(tpl.join(".weft-sessions/base").exists(), "session kept");
}

#[test]
fn amend_rewrites_a_leaf_patch_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    // Record a fresh leaf patch: adds note.txt referencing project_name.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(
        tpl.join(".weft-sessions/main/worktree/note.txt"),
        "welcome to Demo\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("note")
        .arg("--yes")
        .assert()
        .success();
    let before_id = {
        // content id isn't stored; capture the ops to compare change
        read(&tpl, "patches/note.json")
    };
    assert!(before_id.contains("welcome to"));

    // Amend it: the worktree is seeded with note applied; edit the content.
    weft()
        .arg("patch")
        .arg("amend")
        .arg("note")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/note/worktree");
    assert_eq!(
        read(&worktree, "note.txt"),
        "welcome to Demo\n",
        "worktree seeded with the patch applied"
    );
    std::fs::write(worktree.join("note.txt"), "hello from Demo — updated\n").unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("amended patch `note`"));

    // The ops were re-derived in place; the answer is still abstracted.
    let after = read(&tpl, "patches/note.json");
    assert!(after.contains("hello from"), "content updated: {after}");
    assert!(!after.contains("welcome to"), "old content gone");
    assert!(
        after.contains("\"answer\": \"project_name\""),
        "still abstracted"
    );

    // Scaffolding reflects the amended content under a different answer.
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=Acme")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(read(&dest, "note.txt"), "hello from Acme — updated\n");

    // check stays green.
    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
}

// ---- amend with descendants (rebase) -------------------------------------

/// Record base p1 (creates config.txt) and p2 (inserts a line, anchored on
/// p1's content). Returns the template dir.
fn stacked_template() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    // p1: create a 10-line config.txt
    let base = "a1\na2\na3\na4\na5\na6\na7\na8\na9\na10\n";
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/config.txt"), base).unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("p1")
        .arg("--yes")
        .assert()
        .success();
    // p2: insert near the bottom (hunk anchors on a7..a10, far from the top)
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--base")
        .arg("p1")
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(
        tpl.join(".weft-sessions/main/worktree/config.txt"),
        "a1\na2\na3\na4\na5\na6\na7\na8\nINSERTED\na9\na10\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("p2")
        .arg("--yes")
        .assert()
        .success();
    (dir, tpl)
}

#[test]
fn amend_descendant_still_applies() {
    let (_g, tpl) = stacked_template();
    // Amend p1, editing a line p2 does NOT anchor on (line1).
    weft()
        .arg("patch")
        .arg("amend")
        .arg("p1")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .success();
    let w = tpl.join(".weft-sessions/p1/worktree");
    // Edit a1 — 7 lines from p2's anchor, outside its context radius.
    std::fs::write(
        w.join("config.txt"),
        "A1\na2\na3\na4\na5\na6\na7\na8\na9\na10\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("amended patch `p1`"))
        .stderr(predicates::str::contains("dependents still apply"));
    // Full render composes: p2's insert still lands on the amended p1.
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        read(&dest, "config.txt"),
        "A1\na2\na3\na4\na5\na6\na7\na8\nINSERTED\na9\na10\n"
    );
}

#[test]
fn amend_reports_broken_descendant() {
    let (_g, tpl) = stacked_template();
    // Amend p1, removing the line p2 anchors on (line2).
    weft()
        .arg("patch")
        .arg("amend")
        .arg("p1")
        .arg("--answer")
        .arg("project_name=Demo")
        .arg("--template")
        .arg(&tpl)
        .arg("--non-interactive")
        .assert()
        .success();
    let w = tpl.join(".weft-sessions/p1/worktree");
    // Remove a8 — inside p2's context; p2's hunk will no longer anchor.
    std::fs::write(
        w.join("config.txt"),
        "a1\na2\na3\na4\na5\na6\na7\na9\na10\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--yes")
        .assert()
        .failure()
        .stderr(predicates::str::contains("amended patch `p1`"))
        .stderr(predicates::str::contains(
            "dependent patch no longer applies",
        ))
        .stderr(predicates::str::contains("re-record"));
}

// ---- squash --------------------------------------------------------------

#[test]
fn squash_combines_a_chain() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    // s1 creates x.txt; s2 (on s1) creates y.txt.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/x.txt"), "ex\n").unwrap();
    weft()
        .arg("commit")
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
    std::fs::write(tpl.join(".weft-sessions/main/worktree/y.txt"), "why\n").unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("s2")
        .arg("--yes")
        .assert()
        .success();

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
        .success()
        .stderr(predicates::str::contains("squashed"));
    assert!(!tpl.join("patches/s1.json").exists());
    assert!(!tpl.join("patches/s2.json").exists());
    assert!(tpl.join("patches/combined.json").exists());

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(read(&dest, "x.txt"), "ex\n");
    assert_eq!(read(&dest, "y.txt"), "why\n");
    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
}

#[test]
fn squash_refuses_generator_and_mixed_gates() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("v.txt");
    std::fs::write(&src, "x\n").unwrap();
    let (_g, tpl) = with_generator(&src);
    // squash a generator with base → refused.
    weft()
        .arg("patch")
        .arg("squash")
        .arg("base")
        .arg("gen")
        .arg("--into")
        .arg("c")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("generator patch"));
    // base (no gate) + docker (when use_docker) → different gates refused.
    weft()
        .arg("patch")
        .arg("squash")
        .arg("base")
        .arg("docker")
        .arg("--into")
        .arg("c")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("different `when` gates"));
}
