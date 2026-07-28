//! Authoring without hand-editing patch JSON: `weft init`, `weft hook
//! add/rm/ls`, `weft patch set/ls` — and the invariant that metadata edits
//! never change patch ids.

use std::path::Path;

use weft_e2e::weft;

/// init → question → record/commit → hook add → patch set, all via the CLI.
fn scaffold_template(dir: &Path) {
    weft()
        .arg("init")
        .arg(dir)
        .arg("--name")
        .arg("demo")
        .assert()
        .success();
    // Declare a question (the manifest is the one hand-edited file).
    let manifest = std::fs::read_to_string(dir.join("weft.toml")).unwrap();
    std::fs::write(
        dir.join("weft.toml"),
        format!("{manifest}\n[[question]]\nid = \"project_name\"\nkind = \"string\"\n"),
    )
    .unwrap();
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(dir)
        .arg("--answer")
        .arg("project_name=Demo Project")
        .assert()
        .success();
    std::fs::write(
        dir.join(".weft-sessions/main/worktree/README.md"),
        "# Demo Project\n",
    )
    .unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(dir)
        .arg("--name")
        .arg("base")
        .arg("--title")
        .arg("Initialize project")
        .arg("--yes")
        .assert()
        .success();
}

fn node_id(dir: &Path) -> String {
    let out = weft().arg("graph").arg(dir).arg("--json").output().unwrap();
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    doc["nodes"][0]["id"].as_str().unwrap().to_owned()
}

#[test]
fn init_refuses_existing_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    weft().arg("init").arg(&tpl).assert().success();
    weft()
        .arg("init")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[test]
fn full_cli_authoring_loop_keeps_ids_stable() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    scaffold_template(&tpl);
    let id_before = node_id(&tpl);

    // hooks + metadata, all metadata-only
    weft()
        .args(["hook", "add", "base", "--id", "verify-git"])
        .args(["--phase", "pre", "--effect", "check"])
        .args(["--label", "Verify git", "--action", "command -v git"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    weft()
        .args(["hook", "add", "base", "--id", "install"])
        .args(["--phase", "post", "--effect", "setup"])
        .args(["--label", "Install", "--action", "true"])
        .args(["--after", "verify-git", "--input", "glob:README.md"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    weft()
        .args([
            "patch",
            "set",
            "base",
            "--describe",
            "The base.",
            "--tag",
            "core",
        ])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();

    assert_eq!(node_id(&tpl), id_before, "metadata edits must not move ids");

    // listed in execution order (pre before post)
    weft()
        .args(["hook", "ls"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("verify-git"))
        .stdout(predicates::str::contains("install"));
    weft()
        .args(["patch", "ls"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("2 hook(s)"));

    // scaffolding actually runs the pre-check (git exists on CI machines)
    let out = dir.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&out)
        .arg("--answer")
        .arg("project_name=Other Name")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(std::fs::read_to_string(out.join("README.md"))
        .unwrap()
        .contains("# Other Name"));

    weft()
        .args(["hook", "rm", "base", "install"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    assert_eq!(node_id(&tpl), id_before);
}

#[test]
fn invalid_hook_edits_roll_back() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    scaffold_template(&tpl);
    let original = std::fs::read_to_string(tpl.join("patches/base.json")).unwrap();

    // unknown `after` reference → rejected and file restored
    weft()
        .args(["hook", "add", "base", "--id", "bad"])
        .args(["--phase", "post", "--effect", "setup"])
        .args(["--label", "x", "--action", "true", "--after", "nope"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown `after` hook"));
    assert_eq!(
        std::fs::read_to_string(tpl.join("patches/base.json")).unwrap(),
        original
    );

    // inputs on a pre hook → rejected
    weft()
        .args(["hook", "add", "base", "--id", "bad2"])
        .args(["--phase", "pre", "--effect", "check"])
        .args(["--label", "x", "--action", "true", "--input", "glob:*"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("pre-hook"));
}

// ---- non-TTY behavior of the interactive forms ------------------------
// e2e runs pipe stdio, so the TUI must never open; missing options fail
// with a message pointing at the interactive form.

#[test]
fn missing_hook_flags_error_without_a_tty() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    scaffold_template(&tpl);
    weft()
        .args(["hook", "add", "base", "--id", "x"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("run in a terminal"));
}

#[test]
fn missing_new_template_errors_without_a_tty() {
    weft()
        .arg("new")
        .assert()
        .failure()
        .stderr(predicates::str::contains("missing TEMPLATE"));
}

#[test]
fn patch_set_without_changes_errors_without_a_tty() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    scaffold_template(&tpl);
    weft()
        .args(["patch", "set", "base"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("nothing to change"));
}

#[test]
fn commit_without_name_still_defaults_without_a_tty() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    scaffold_template(&tpl);
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Demo Project")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/EXTRA.md"), "x\n").unwrap();
    // No --name, no TTY: the historical auto-name (patch-NNN) still applies.
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("patch-002"));
}

// ---- weft diff + per-occurrence abstraction control --------------------

#[test]
fn diff_previews_candidates_and_keep_literal_survives_renames() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    weft()
        .arg("init")
        .arg(&tpl)
        .arg("--name")
        .arg("demo")
        .assert()
        .success();
    let manifest = std::fs::read_to_string(tpl.join("weft.toml")).unwrap();
    std::fs::write(
        tpl.join("weft.toml"),
        format!("{manifest}\n[[question]]\nid = \"project_name\"\nkind = \"string\"\n"),
    )
    .unwrap();
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=my_project")
        .assert()
        .success();
    std::fs::write(
        tpl.join(".weft-sessions/main/worktree/README.md"),
        "# my_project\n\nRun my_project now.\nThe word \"my_project\" is prose here.\n",
    )
    .unwrap();

    // The piped diff shows ⟨…⟩ spans and the legend.
    weft()
        .arg("diff")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("⟨my_project⟩"))
        .stdout(predicates::str::contains("3 occurrence(s)"));
    // Abstracted view shows the stored form.
    weft()
        .args(["diff", "--abstracted"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("⟨{project_name}⟩"));

    // Keep the prose occurrence (line 4) literal; commit the rest.
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&tpl)
        .args([
            "--name",
            "base",
            "--keep-literal",
            "project_name@README.md:4",
        ])
        .assert()
        .success();

    // Scaffolding under a different name renames 2 spots, keeps the prose.
    let out = dir.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&out)
        .arg("--answer")
        .arg("project_name=acme")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(readme.contains("# acme"), "{readme}");
    assert!(readme.contains("Run acme now."), "{readme}");
    assert!(readme.contains("\"my_project\" is prose"), "{readme}");
}

#[test]
fn diff_without_a_session_errors_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("tpl");
    weft().arg("init").arg(&tpl).assert().success();
    weft()
        .arg("diff")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains("weft session new"));
}
