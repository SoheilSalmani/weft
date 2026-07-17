//! Generator patches: `weft record --exec` turns a command's output into a
//! patch (command stored as metadata), and `weft patch resync` re-runs the
//! stored commands and rewrites the ops from the fresh outputs.

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

/// Copy the hello fixture and record one generated patch whose command
/// `cat`s `src` (a file outside the template the test can mutate).
fn template_with_generated(src: &Path, out_file: &str, name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    record_generated(&tpl, src, out_file, name);
    (dir, tpl)
}

fn record_generated(tpl: &Path, src: &Path, out_file: &str, name: &str) {
    weft()
        .arg("record")
        .arg("--template")
        .arg(tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg(format!("cat '{}' > {out_file}", src.display()))
        .assert()
        .success();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(tpl)
        .arg("--name")
        .arg(name)
        .arg("--yes")
        .assert()
        .success();
}

#[test]
fn record_exec_commits_generator_patch() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .arg("record")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg("printf 'component for My Demo\\n' > component.tsx")
        .assert()
        .success()
        .stderr(predicates::str::contains("generator"));
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("committed patch `gen`"));

    let patch = read(&tpl, "patches/gen.json");
    assert!(patch.contains("\"generator\""), "patch: {patch}");
    assert!(
        patch.contains("component for My Demo"),
        "command stored verbatim: {patch}"
    );
    // The concrete value is abstracted out of the recorded content.
    assert!(
        patch.contains("\"answer\": \"project_name\""),
        "patch: {patch}"
    );
    assert!(
        patch.contains("\"project_name\": \"My Demo\""),
        "record-time answers stored for resync: {patch}"
    );
    assert!(!tpl.join(".weft-record").exists(), "session discarded");

    // The generated patch scaffolds like any other.
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=Other App")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        read(&dest_path, "component.tsx"),
        "component for Other App\n"
    );
}

#[test]
fn record_exec_failure_leaves_no_session() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .arg("record")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .arg("false")
        .assert()
        .failure();
    assert!(!tpl.join(".weft-record").exists());
}

#[test]
fn resync_noop_when_output_unchanged() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "generated for My Demo v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    let before = read(&tpl, "patches/gen.json");
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));
    assert_eq!(read(&tpl, "patches/gen.json"), before);
}

#[test]
fn resync_rewrites_on_output_change() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "generated for My Demo v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    std::fs::write(&src, "regenerated for My Demo v2\nnew extra line\n").unwrap();
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("rewritten"));

    // The rewritten ops abstract the answer and carry the new content.
    let patch = read(&tpl, "patches/gen.json");
    assert!(patch.contains("regenerated for "), "patch: {patch}");
    assert!(patch.contains("new extra line"), "patch: {patch}");

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=Other App")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        read(&dest_path, "gen.txt"),
        "regenerated for Other App v2\nnew extra line\n"
    );

    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
}

#[test]
fn resync_dry_run_writes_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    std::fs::write(&src, "v2\n").unwrap();
    let before = read(&tpl, "patches/gen.json");
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .arg("--dry-run")
        .assert()
        .success()
        .stderr(predicates::str::contains("would rewrite"));
    assert_eq!(read(&tpl, "patches/gen.json"), before);
}

#[test]
fn resync_refuses_active_session() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    weft()
        .arg("record")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    weft()
        .arg("patch")
        .arg("resync")
        .arg("--all")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("recording session is active"));
}

#[test]
fn resync_requires_names_or_all() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .arg("patch")
        .arg("resync")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("--all"));
    weft()
        .arg("patch")
        .arg("resync")
        .arg("docker")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("no generator command"));
}

#[test]
fn resync_reports_broken_dependent() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "anchor line v1\nbody\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    // Hand-record a patch whose hunk anchors on the generated content.
    weft()
        .arg("record")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success();
    let worktree = tpl.join(".weft-record/worktree");
    std::fs::write(
        worktree.join("gen.txt"),
        "anchor line v1\nbody\nmanual addition\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("manual")
        .arg("--yes")
        .assert()
        .success();

    // Upstream changes shape: the anchor vanishes.
    std::fs::write(&src, "totally different output\n").unwrap();
    let manual_before = read(&tpl, "patches/manual.json");
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("rewritten"))
        .stderr(predicates::str::contains("no longer renders"))
        .stderr(predicates::str::contains("re-record"));
    // The generated patch was rewritten; the broken dependent was not touched.
    assert!(read(&tpl, "patches/gen.json").contains("totally different"));
    assert_eq!(read(&tpl, "patches/manual.json"), manual_before);
}

#[test]
fn resync_all_follows_the_dependency_chain() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "base v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "g1.txt", "g1");
    // g2's command consumes g1's output from the base, so g2 depends on g1
    // and must be resynced after it.
    weft()
        .arg("record")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg("cat g1.txt > g2.txt")
        .assert()
        .success();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("g2")
        .arg("--yes")
        .assert()
        .success();

    std::fs::write(&src, "base v2\n").unwrap();
    weft()
        .arg("patch")
        .arg("resync")
        .arg("--all")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("g1: rewritten"))
        .stderr(predicates::str::contains("g2: rewritten"));

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
    assert_eq!(read(&dest_path, "g1.txt"), "base v2\n");
    assert_eq!(read(&dest_path, "g2.txt"), "base v2\n");
}
