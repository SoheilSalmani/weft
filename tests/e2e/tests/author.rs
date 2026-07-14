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
        .arg("record")
        .arg("--template")
        .arg(dir)
        .arg("--answer")
        .arg("project_name=Demo Project")
        .assert()
        .success();
    std::fs::write(
        dir.join(".weft-record/worktree/README.md"),
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
