//! M4 acceptance: record a change, commit it, and `weft new` with different
//! answers produces correctly abstracted output.

use std::fs;
use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn fixture_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

/// Copy the fixture template into a tempdir so recording never mutates the
/// checked-in fixture.
fn copy_template(to: &Path) {
    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_dir(&fixture_template(), to);
}

fn read(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

#[test]
fn record_commit_new_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    copy_template(&template);

    // 1. record against the base rendered with concrete answers
    weft()
        .arg("record")
        .arg("--template")
        .arg(&template)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success()
        .stdout(predicates::str::contains(".weft-record/worktree"));

    let worktree = template.join(".weft-record/worktree");
    assert_eq!(
        read(&worktree, "README.md"),
        "# My Demo\n\nScaffolded by weft.\n\nShips with Docker.\n",
        "worktree starts as exactly the rendered base"
    );

    // 2. author edits: modify a file and create one that mentions answers
    let readme = read(&worktree, "README.md");
    fs::write(
        worktree.join("README.md"),
        format!("{readme}\nMade with love.\n"),
    )
    .unwrap();
    fs::write(
        worktree.join("Makefile"),
        "serve-my-demo:\n\techo My Demo\n",
    )
    .unwrap();

    // 3. commit with value abstraction (non-interactive => accept proposals)
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&template)
        .arg("--name")
        .arg("extras")
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("committed patch `extras`"));

    let patch_json = read(&template, "patches/extras.json");
    assert!(
        !patch_json.contains("My Demo") && !patch_json.contains("my-demo"),
        "concrete values must be abstracted out of the stored patch: {patch_json}"
    );
    assert!(
        !template.join(".weft-record").exists(),
        "session cleaned up after commit"
    );

    // 4. scaffold with *different* answers: the recorded patch must adapt
    let dest = tmp.path().join("out");
    weft()
        .arg("new")
        .arg(&template)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=Other App")
        .arg("--non-interactive")
        .assert()
        .success();

    assert_eq!(
        read(&dest, "README.md"),
        "# Other App\n\nScaffolded by weft.\n\nShips with Docker.\n\nMade with love.\n"
    );
    assert_eq!(
        read(&dest, "Makefile"),
        "serve-other-app:\n\techo Other App\n"
    );
}

#[test]
fn record_refuses_second_session_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    copy_template(&template);

    let record = |extra: &[&str]| {
        let mut cmd = weft();
        cmd.arg("record")
            .arg("--template")
            .arg(&template)
            .arg("--answer")
            .arg("project_name=x");
        for arg in extra {
            cmd.arg(arg);
        }
        cmd.assert()
    };
    record(&[]).success();
    record(&[])
        .failure()
        .stderr(predicates::str::contains("already active"));
    record(&["--force"]).success();
}

#[test]
fn commit_without_changes_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    copy_template(&template);

    weft()
        .arg("record")
        .arg("--template")
        .arg(&template)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&template)
        .arg("--yes")
        .assert()
        .failure()
        .stderr(predicates::str::contains("no changes"));
}
