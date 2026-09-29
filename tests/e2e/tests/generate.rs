//! Generator patches: `weft record --exec` turns a command's output into a
//! patch (command stored as metadata), and `weft patch resync` re-runs the
//! stored commands and rewrites the ops from the fresh outputs.

use std::path::{Path, PathBuf};

use predicates::prelude::PredicateBooleanExt;
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
        .args(["session", "new", "main"])
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
        .args(["session", "new", "main"])
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
    assert!(!tpl.join(".weft-sessions").exists(), "session discarded");

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
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .arg("false")
        .assert()
        .failure();
    assert!(!tpl.join(".weft-sessions").exists());
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
fn resync_skips_a_generated_patch_that_declares_a_slot() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "static v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");
    // A slot added by hand to the generated file, then an upstream change.
    let patch = read(&tpl, "patches/gen.json").replacen(
        "\"static v1\"",
        "\"static v1\", {\"slot\": \"notes\"}",
        1,
    );
    std::fs::write(tpl.join("patches/gen.json"), &patch).unwrap();
    std::fs::write(&src, "static v2\n").unwrap();
    weft()
        .args(["patch", "resync", "gen"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "declares slot(s) `notes` in `gen.txt`, which regenerating from the command would \
             drop",
        ));
    assert_eq!(read(&tpl, "patches/gen.json"), patch, "left as it was");
}

#[test]
fn resync_answer_override_persists_when_up_to_date() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "static v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");

    let before = read(&tpl, "patches/gen.json");
    weft()
        .args(["patch", "resync", "gen", "--answer", "project_name=Other"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));
    let after = read(&tpl, "patches/gen.json");
    assert!(
        after.contains("\"project_name\": \"Other\""),
        "patch: {after}"
    );
    let ops = |s: &str| s[s.find("\"ops\"").unwrap()..s.find("\"generator\"").unwrap()].to_owned();
    assert_eq!(ops(&after), ops(&before), "ops unchanged");
}

/// Add a defaulted question after recording and reference it from `base`.
fn add_site_url_question(tpl: &Path) {
    let toml = read(tpl, "weft.toml");
    std::fs::write(
        tpl.join("weft.toml"),
        format!(
            "{toml}\n[[question]]\nid = \"site_url\"\nkind = \"string\"\n\
             default = \"'https://site.example'\"\n"
        ),
    )
    .unwrap();
    let base = read(tpl, "patches/base.json");
    let base = base.replacen(
        "\"ops\": [",
        "\"ops\": [{\"op\": \"create_file\", \"path\": \"site.txt\", \
         \"content\": [[{\"answer\": \"site_url\"}]]},",
        1,
    );
    std::fs::write(tpl.join("patches/base.json"), base).unwrap();
}

#[test]
fn resync_fills_default_of_question_added_after_recording() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "static v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");
    add_site_url_question(&tpl);

    weft()
        .args(["patch", "resync", "gen"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));
}

#[test]
fn resync_new_answer_reference_needs_consent() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "static v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");
    add_site_url_question(&tpl);

    // The regenerated output now contains the new question's default.
    std::fs::write(&src, "static v2 at https://site.example\n").unwrap();
    let before = read(&tpl, "patches/gen.json");
    weft()
        .args(["patch", "resync", "gen"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("site_url@gen.txt:1:1"))
        .stderr(predicates::str::contains("--yes"));
    assert_eq!(read(&tpl, "patches/gen.json"), before, "skipped, untouched");

    // Keeping the occurrence literal means no new reference.
    weft()
        .args(["patch", "resync", "gen", "--dry-run"])
        .args(["--keep-literal", "site_url@gen.txt:1"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();

    weft()
        .args(["patch", "resync", "gen", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("rewritten"));
    let after = read(&tpl, "patches/gen.json");
    assert!(after.contains("\"answer\": \"site_url\""), "patch: {after}");
    assert!(
        after.contains("\"site_url\": \"https://site.example\""),
        "default recorded in generator answers: {after}"
    );
}

#[test]
fn resync_keep_literal_repairs_a_patch_whose_output_is_unchanged() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("upstream.txt");
    std::fs::write(&src, "static v1\n").unwrap();
    let (_guard, tpl) = template_with_generated(&src, "gen.txt", "gen");
    add_site_url_question(&tpl);
    std::fs::write(&src, "static v2 at https://site.example\n").unwrap();
    let resync = |extra: &[&str]| {
        let mut cmd = weft();
        cmd.args(["patch", "resync", "gen"])
            .args(extra)
            .arg("--template")
            .arg(&tpl);
        cmd.assert().success()
    };
    resync(&["--yes"]);
    assert!(read(&tpl, "patches/gen.json").contains("\"answer\": \"site_url\""));

    // Same output, but the repair must reach the ops.
    resync(&["--keep-literal", "site_url@gen.txt:1"])
        .stderr(predicates::str::contains("rewritten"));
    let repaired = read(&tpl, "patches/gen.json");
    assert!(
        !repaired.contains("\"answer\": \"site_url\""),
        "patch: {repaired}"
    );
    assert!(
        repaired.contains("\"site_url@gen.txt:1\""),
        "spec stored: {repaired}"
    );

    // Repeating it re-derives the same ops: nothing to rewrite.
    resync(&["--keep-literal", "site_url@gen.txt:1"])
        .stderr(predicates::str::contains("up to date"));
    assert_eq!(read(&tpl, "patches/gen.json"), repaired);
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
        .args(["session", "new", "main"])
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
        .stderr(predicates::str::contains("are open in"));
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
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/main/worktree");
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
        .args(["session", "new", "main"])
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

// ---- ${…} interpolation --------------------------------------------------

#[test]
fn exec_interpolates_declared_answers() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg("printf 'hi ${project_name}\\n' > f.txt")
        .assert()
        .success()
        .stderr(predicates::str::contains("generator interpolates:"));
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success();

    // The stored command is a segment array with the answer reference.
    let patch = read(&tpl, "patches/gen.json");
    assert!(patch.contains("\"command\": ["), "patch: {patch}");
    assert!(
        patch.contains("\"answer\": \"project_name\""),
        "patch: {patch}"
    );

    // The recorded output abstracts the answer like any patch.
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
    assert_eq!(read(&dest_path, "f.txt"), "hi Other App\n");

    // The patch is answer-parametric, so re-running the command with a
    // different answer reproduces the same (abstracted) patch: up to date.
    weft()
        .arg("patch")
        .arg("resync")
        .arg("gen")
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Resynced")
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));
}

#[test]
fn exec_shell_vars_stay_literal() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .arg("echo \"${SOME_VAR:-fallback}\" > v.txt")
        .assert()
        .success()
        .stderr(predicates::str::contains("generator interpolates").not());
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success();

    // Nothing matched → the command stays a bare string with the ${…} intact.
    let patch = read(&tpl, "patches/gen.json");
    assert!(
        patch.contains("\"command\": \"echo"),
        "bare-string command: {patch}"
    );
    assert!(patch.contains("${SOME_VAR:-fallback}"), "patch: {patch}");
}

#[test]
fn hook_add_action_interpolates_and_check_validates() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .arg("hook")
        .arg("add")
        .arg("base")
        .arg("--template")
        .arg(&tpl)
        .arg("--id")
        .arg("greet")
        .arg("--phase")
        .arg("post")
        .arg("--effect")
        .arg("setup")
        .arg("--label")
        .arg("Greet")
        .arg("--action")
        .arg("test -n \"${project_name}\"")
        .assert()
        .success()
        .stderr(predicates::str::contains("action interpolates:"));
    let patch = read(&tpl, "patches/base.json");
    assert!(
        patch.contains("\"answer\": \"project_name\""),
        "patch: {patch}"
    );
    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();

    // A command referencing an undeclared answer is a check issue.
    let mut json: serde_json::Value = serde_json::from_str(&patch).unwrap();
    json["hooks"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "bad",
            "phase": "post",
            "effect": "setup",
            "label": "Bad",
            "action": [{"answer": "nope"}],
        }));
    std::fs::write(
        tpl.join("patches/base.json"),
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();
    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown answer `nope`"));
}

// ---- $EDITOR fallback ----------------------------------------------------

#[test]
fn exec_editor_fallback_records_command() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    let script = dir.path().join("fake-editor.sh");
    std::fs::write(
        &script,
        "printf '%s\\n' 'printf \"from-editor\\n\" > e.txt' > \"$1\"\n",
    )
    .unwrap();

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .env("VISUAL", format!("sh {}", script.display()))
        .env("EDITOR", format!("sh {}", script.display()))
        .assert()
        .success();
    assert_eq!(
        read(&tpl, ".weft-sessions/main/worktree/e.txt"),
        "from-editor\n",
        "the editor-provided command ran in the worktree"
    );
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("gen")
        .arg("--yes")
        .assert()
        .success();
    assert!(read(&tpl, "patches/gen.json").contains("from-editor"));
}

#[test]
fn exec_editor_empty_aborts() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    let script = dir.path().join("fake-editor.sh");
    std::fs::write(&script, ": > \"$1\"\n").unwrap();

    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .env("VISUAL", format!("sh {}", script.display()))
        .env("EDITOR", format!("sh {}", script.display()))
        .assert()
        .failure()
        .stderr(predicates::str::contains("aborted"));
    assert!(!tpl.join(".weft-sessions").exists());
}
