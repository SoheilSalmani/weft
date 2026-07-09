//! M5 acceptance: update applies template evolution over local edits via
//! 3-way merge; tasks re-fire only when their declared inputs changed;
//! update is idempotent.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn weft() -> Command {
    Command::cargo_bin("weft").expect("weft binary built by workspace")
}

fn fixture_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

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

fn scaffold(template: &Path, dest: &Path) {
    weft()
        .arg("new")
        .arg(template)
        .arg(dest)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--non-interactive")
        .assert()
        .success();
}

/// Evolve the template: record + commit a patch built from `edit`.
fn evolve(template: &Path, name: &str, edit: impl FnOnce(&Path)) {
    weft()
        .arg("record")
        .arg("--template")
        .arg(template)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success();
    edit(&template.join(".weft-record/worktree"));
    weft()
        .arg("commit")
        .arg("--template")
        .arg(template)
        .arg("--name")
        .arg(name)
        .arg("--yes")
        .assert()
        .success();
}

#[test]
fn update_merges_template_changes_over_local_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    let dest = tmp.path().join("project");
    copy_template(&template);
    scaffold(&template, &dest);
    fs::remove_file(dest.join(".task-ran")).unwrap();

    // Local edit at the bottom of README
    let readme = read(&dest, "README.md");
    fs::write(dest.join("README.md"), format!("{readme}\nLocal notes.\n")).unwrap();

    // Template evolves: new top line in README + a new file (does NOT touch
    // pyproject.toml, so the glob task must not re-fire)
    evolve(&template, "banner", |worktree| {
        let readme = fs::read_to_string(worktree.join("README.md")).unwrap();
        fs::write(worktree.join("README.md"), format!("> banner\n{readme}")).unwrap();
        fs::write(worktree.join("CONTRIBUTING.md"), "Be kind.\n").unwrap();
    });

    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();

    let merged = read(&dest, "README.md");
    assert!(
        merged.starts_with("> banner\n"),
        "template change applied: {merged}"
    );
    assert!(
        merged.ends_with("Local notes.\n"),
        "local edit kept: {merged}"
    );
    assert_eq!(read(&dest, "CONTRIBUTING.md"), "Be kind.\n");
    assert!(
        !dest.join(".task-ran").exists(),
        "task with glob:pyproject.toml input must not fire when that file didn't change"
    );

    // Idempotence: a second update is a no-op
    let before = read(&dest, "README.md");
    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success()
        .stderr(predicates::str::contains("0 file(s) written"));
    assert_eq!(read(&dest, "README.md"), before);
}

#[test]
fn update_refires_task_when_glob_input_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    let dest = tmp.path().join("project");
    copy_template(&template);
    scaffold(&template, &dest);
    fs::remove_file(dest.join(".task-ran")).unwrap();

    evolve(&template, "deps", |worktree| {
        let py = fs::read_to_string(worktree.join("pyproject.toml")).unwrap();
        fs::write(
            worktree.join("pyproject.toml"),
            format!("{py}dependencies = []\n"),
        )
        .unwrap();
    });

    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();

    assert!(read(&dest, "pyproject.toml").contains("dependencies = []"));
    assert_eq!(
        read(&dest, ".task-ran").trim(),
        "synced",
        "glob:pyproject.toml task re-fires when that file changed"
    );
}

#[test]
fn update_conflict_gets_markers_and_nonzero_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    let dest = tmp.path().join("project");
    copy_template(&template);
    scaffold(&template, &dest);

    // User and template both rewrite the same line differently.
    let readme = read(&dest, "README.md");
    fs::write(
        dest.join("README.md"),
        readme.replace("Scaffolded by weft.", "Scaffolded by ME."),
    )
    .unwrap();
    evolve(&template, "rebrand", |worktree| {
        let readme = fs::read_to_string(worktree.join("README.md")).unwrap();
        fs::write(
            worktree.join("README.md"),
            readme.replace("Scaffolded by weft.", "Scaffolded by TEMPLATE."),
        )
        .unwrap();
    });

    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("conflict"));

    let merged = read(&dest, "README.md");
    assert!(merged.contains("<<<<<<< local"), "{merged}");
    assert!(merged.contains("Scaffolded by ME."));
    assert!(merged.contains("Scaffolded by TEMPLATE."));
    assert!(merged.contains(">>>>>>> template"));
}

#[test]
fn update_dry_run_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("template");
    let dest = tmp.path().join("project");
    copy_template(&template);
    scaffold(&template, &dest);

    evolve(&template, "extra", |worktree| {
        fs::write(worktree.join("EXTRA.md"), "extra\n").unwrap();
    });

    weft()
        .arg("update")
        .arg(&dest)
        .arg("--dry-run")
        .arg("--non-interactive")
        .assert()
        .success()
        .stderr(predicates::str::contains("dry run: update EXTRA.md"));
    assert!(!dest.join("EXTRA.md").exists());

    // and the real run applies it
    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(read(&dest, "EXTRA.md"), "extra\n");
}
