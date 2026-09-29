//! `.weftignore`: gitignore-style patterns at the template root keep junk
//! (generator side-products, OS files) out of recordings, and `weft update`
//! only reads template-tracked paths from a project.

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

/// A generator that drops a node_modules full of binaries must not break
/// or pollute the commit when `.weftignore` covers it.
#[test]
fn generator_junk_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    std::fs::write(tpl.join(".weftignore"), "node_modules/\n").unwrap();

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg(
            "mkdir -p node_modules/pkg && printf '\\377\\376\\000' > node_modules/pkg/native.bin \
             && echo 'generated' > app.txt",
        )
        .assert()
        .success();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success();

    // The ops carry only the real output — the junk never entered the diff
    // (the command string itself legitimately mentions node_modules).
    let patch = read(&tpl, "patches/gen.json");
    let json: serde_json::Value = serde_json::from_str(&patch).unwrap();
    let paths: Vec<&str> = json["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|op| op["path"].as_str())
        .collect();
    assert_eq!(paths, vec!["app.txt"], "patch: {patch}");

    // Resync re-runs the command (junk regenerated in the scratch worktree)
    // and still ignores it.
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));

    // Scaffolds contain only the real output.
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(dest_path.join("app.txt").exists());
    assert!(!dest_path.join("node_modules").exists());
}

/// `.DS_Store` is a built-in default — no `.weftignore` needed.
#[test]
fn ds_store_ignored_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/main/worktree");
    std::fs::write(worktree.join(".DS_Store"), "finder junk\n").unwrap();
    std::fs::write(worktree.join("real.txt"), "real change\n").unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("edit")
        .arg("--yes")
        .assert()
        .success();
    let patch = read(&tpl, "patches/edit.json");
    assert!(patch.contains("real.txt"), "patch: {patch}");
    assert!(!patch.contains("DS_Store"), "patch: {patch}");
}

/// Deleting a base-rendered file is still recorded even when a pattern
/// matches it — base paths are exempt from ignoring.
#[test]
fn base_rendered_paths_are_exempt() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    // README.md is rendered by the base patch; ignore it anyway.
    std::fs::write(tpl.join(".weftignore"), "README.md\n").unwrap();

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/main/worktree");
    std::fs::remove_file(worktree.join("README.md")).unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("drop-readme")
        .arg("--yes")
        .assert()
        .success();
    assert!(
        read(&tpl, "patches/drop-readme.json").contains("delete_file"),
        "the deletion is recorded despite the ignore pattern"
    );
}

/// `weft update` reads only template-tracked paths from the project —
/// user junk (binary node_modules) neither breaks nor enters the merge.
#[test]
fn update_skips_untracked_project_files() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    let dest = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();

    // A real project accumulates binary junk the template never tracked.
    std::fs::create_dir_all(dest.join("node_modules/pkg")).unwrap();
    std::fs::write(
        dest.join("node_modules/pkg/native.bin"),
        [0xffu8, 0xfe, 0x00],
    )
    .unwrap();

    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(
        dest.join("node_modules/pkg/native.bin").exists(),
        "junk untouched"
    );
}
