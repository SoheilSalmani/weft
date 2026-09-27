//! Changing a scaffolded project's answers with `weft update --answer`:
//! values derived from them (defaults, include binds) follow, values you gave
//! stay until `--unset`, local edits merge, conflicts get markers and block
//! the next update until resolved, and uncommitted git changes stop a write.

use std::path::{Path, PathBuf};
use std::process::Command;

use weft_e2e::weft;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates")
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

fn state(dest: &Path) -> toml::Value {
    read(dest, ".weft/state.toml").parse().unwrap()
}

/// Scaffold the hello fixture into `<dir>/proj` (hooks skipped).
fn hello(dir: &Path, answers: &[&str]) -> PathBuf {
    let proj = dir.join("proj");
    let mut cmd = weft();
    cmd.arg("new").arg(fixtures().join("hello")).arg(&proj);
    for a in answers {
        cmd.arg("--answer").arg(a);
    }
    cmd.arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    proj
}

fn update(proj: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    weft()
        .arg("update")
        .arg(proj)
        .args(args)
        .arg("--non-interactive")
        .assert()
}

fn stderr(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stderr).into_owned()
}

#[test]
fn renaming_rederives_what_follows_from_it_and_keeps_local_edits() {
    let dir = tempfile::tempdir().unwrap();
    let proj = hello(dir.path(), &["project_name=Old Name"]);

    // The state separates what you gave from what the template derived.
    let s = state(&proj);
    assert_eq!(s["answers"]["project_name"].as_str(), Some("Old Name"));
    assert_eq!(s["derived"]["package_name"].as_str(), Some("old-name"));
    let listed = weft()
        .arg("answers")
        .arg(&proj)
        .arg("--json")
        .output()
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let origin = |id: &str| {
        listed["answers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .map(|r| r["origin"].as_str().unwrap().to_owned())
    };
    assert_eq!(origin("project_name").as_deref(), Some("given"));
    assert_eq!(origin("package_name").as_deref(), Some("derived"));

    std::fs::write(
        proj.join("README.md"),
        read(&proj, "README.md") + "Local note.\n",
    )
    .unwrap();
    // Hooks run: pyproject.toml changes, so its post-hook re-fires.
    let out = update(&proj, &["--answer", "project_name=New Name"]).success();
    let err = stderr(&out);
    assert!(err.contains("\"Old Name\" → \"New Name\""), "{err}");
    assert!(
        err.contains("\"old-name\" → \"new-name\"  (derived)"),
        "{err}"
    );
    let readme = read(&proj, "README.md");
    assert!(readme.starts_with("# New Name\n"), "{readme}");
    assert!(readme.ends_with("Local note.\n"), "{readme}");
    assert!(read(&proj, "pyproject.toml").contains("name = \"new-name\""));
    assert!(read(&proj, "Dockerfile").contains("COPY . /app/new-name"));
    assert_eq!(read(&proj, ".task-ran").trim(), "synced");
    let s = state(&proj);
    assert_eq!(s["answers"]["project_name"].as_str(), Some("New Name"));
    assert_eq!(s["derived"]["package_name"].as_str(), Some("new-name"));

    update(&proj, &["--skip-tasks"])
        .success()
        .stderr(predicates::str::contains("0 file(s) written"));
}

#[test]
fn a_given_answer_stays_until_unset() {
    let dir = tempfile::tempdir().unwrap();
    let proj = hello(
        dir.path(),
        &["project_name=Old Name", "package_name=custom-pkg"],
    );
    update(
        &proj,
        &["--answer", "project_name=New Name", "--skip-tasks"],
    )
    .success();
    assert!(read(&proj, "pyproject.toml").contains("name = \"custom-pkg\""));

    update(&proj, &["--unset", "package_name", "--skip-tasks"])
        .success()
        .stderr(predicates::str::contains(
            "\"custom-pkg\" → \"new-name\"  (derived)",
        ));
    assert!(read(&proj, "pyproject.toml").contains("name = \"new-name\""));

    // Nothing to fall back to, or contradictory: refused before anything runs.
    update(&proj, &["--unset", "project_name"])
        .failure()
        .stderr(predicates::str::contains("has no default to fall back to"));
    update(
        &proj,
        &["--answer", "use_docker=false", "--unset", "use_docker"],
    )
    .failure()
    .stderr(predicates::str::contains("both set and unset"));
}

#[test]
fn a_project_from_before_provenance_keeps_its_answers_and_says_how_to_release_them() {
    let dir = tempfile::tempdir().unwrap();
    let proj = hello(dir.path(), &["project_name=Old Name"]);
    // Pre-provenance layout: every answer under [answers], no [derived].
    let mut s = state(&proj);
    let derived = s.as_table_mut().unwrap().remove("derived").unwrap();
    for (id, value) in derived.as_table().unwrap() {
        s["answers"]
            .as_table_mut()
            .unwrap()
            .insert(id.clone(), value.clone());
    }
    std::fs::write(proj.join(".weft/state.toml"), toml::to_string(&s).unwrap()).unwrap();

    let out = update(
        &proj,
        &["--answer", "project_name=New Name", "--skip-tasks"],
    )
    .success();
    let err = stderr(&out);
    assert!(err.contains("kept as given:"), "{err}");
    assert!(err.contains("`--unset package_name` to follow it"), "{err}");
    assert!(read(&proj, "pyproject.toml").contains("name = \"old-name\""));

    update(&proj, &["--unset", "package_name", "--skip-tasks"]).success();
    assert!(read(&proj, "pyproject.toml").contains("name = \"new-name\""));
}

#[test]
fn a_conflict_gets_markers_and_blocks_the_next_update_until_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let proj = hello(dir.path(), &["project_name=Old Name"]);
    let readme = read(&proj, "README.md").replace("# Old Name", "# Old Name (fork)");
    std::fs::write(proj.join("README.md"), &readme).unwrap();

    // The dry run shows the conflict as a diff and writes nothing.
    let plan = update(
        &proj,
        &[
            "--answer",
            "project_name=New Name",
            "--dry-run",
            "--diff",
            "--skip-tasks",
        ],
    )
    .success();
    let diff = String::from_utf8_lossy(&plan.get_output().stdout).into_owned();
    assert!(diff.contains("+<<<<<<< local"), "{diff}");
    assert!(diff.contains("+# New Name"), "{diff}");
    assert!(stderr(&plan).contains("dry run: conflict in README.md"));
    assert_eq!(read(&proj, "README.md"), readme);

    update(
        &proj,
        &["--answer", "project_name=New Name", "--skip-tasks"],
    )
    .failure()
    .stderr(predicates::str::contains(
        "conflicts in 1 file(s): README.md",
    ));
    let marked = read(&proj, "README.md");
    assert!(
        marked.contains("<<<<<<< local\n# Old Name (fork)\n=======\n# New Name\n>>>>>>> template")
    );

    update(&proj, &["--skip-tasks"])
        .failure()
        .stderr(predicates::str::contains(
            "conflict markers that are still there: README.md",
        ));
    std::fs::write(
        proj.join("README.md"),
        "# New Name (fork)\n\nScaffolded by weft.\n\nShips with Docker.\n",
    )
    .unwrap();
    update(&proj, &["--skip-tasks"])
        .success()
        .stderr(predicates::str::contains("0 file(s) written"));
}

#[test]
fn turning_a_gate_off_deletes_untouched_files_and_keeps_edited_ones() {
    let dir = tempfile::tempdir().unwrap();
    let clean = hello(dir.path(), &["project_name=Demo"]);
    update(&clean, &["--answer", "use_docker=false", "--skip-tasks"])
        .success()
        .stderr(predicates::str::contains("deleted Dockerfile"));
    assert!(!clean.join("Dockerfile").exists());
    assert!(!read(&clean, "README.md").contains("Ships with Docker."));

    let edited_dir = tempfile::tempdir().unwrap();
    let edited = hello(edited_dir.path(), &["project_name=Demo"]);
    std::fs::write(edited.join("Dockerfile"), "FROM custom\n").unwrap();
    update(&edited, &["--answer", "use_docker=false", "--skip-tasks"])
        .success()
        .stderr(predicates::str::contains("Dockerfile: kept your version"));
    assert_eq!(read(&edited, "Dockerfile"), "FROM custom\n");
}

#[test]
fn include_answers_and_binds_follow_the_parent() {
    let dir = tempfile::tempdir().unwrap();
    for t in ["base", "nextjs-app", "monorepo"] {
        copy_dir(&fixtures().join(t), &dir.path().join(t));
    }
    let proj = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(dir.path().join("monorepo"))
        .arg(&proj)
        .args([
            "--answer",
            "workspace=acme",
            "--skip-tasks",
            "--non-interactive",
        ])
        .assert()
        .success();

    // A bound child answer is derived from the parent's: it follows.
    update(&proj, &["--answer", "workspace=beta", "--skip-tasks"])
        .success()
        .stderr(predicates::str::contains(
            "web.app_name  \"acme-web\" → \"beta-web\"  (bind)",
        ));
    assert!(read(&proj, "apps/web/package.json").contains("\"name\": \"beta-web\""));
    assert!(read(&proj, "apps/web/next.config.ts").contains("// workspace: beta"));

    // An include answer, namespaced: the child patch goes, and so does the
    // parent's glue patch that depends on it.
    update(
        &proj,
        &["--answer", "web.use_tailwind=false", "--skip-tasks"],
    )
    .success();
    assert!(!proj.join("apps/web/tailwind.config.ts").exists());
    assert_eq!(
        read(&proj, "apps/web/next.config.ts"),
        "const config = {\n  reactStrictMode: true,\n};\nexport default config;\n"
    );
}

#[test]
fn answers_for_a_repeat_instance_need_the_instance() {
    let dir = tempfile::tempdir().unwrap();
    for t in ["workspace", "hello"] {
        copy_dir(&fixtures().join(t), &dir.path().join(t));
    }
    let proj = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(dir.path().join("workspace"))
        .arg(&proj)
        .args([
            "--answer",
            "workspace_name=Acme",
            "--instance",
            "connector=github",
            "--skip-tasks",
            "--non-interactive",
        ])
        .assert()
        .success();
    update(&proj, &["--answer", "connector.jira.project_name=Jira"])
        .failure()
        .stderr(predicates::str::contains(
            "add it with `weft instance add connector jira`",
        ));
    update(
        &proj,
        &[
            "--answer",
            "connector.github.project_name=GitHub",
            "--skip-tasks",
        ],
    )
    .success();
    assert!(read(&proj, "connectors/github/README.md").starts_with("# GitHub\n"));
}

#[test]
fn uncommitted_changes_stop_the_update_unless_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let proj = hello(dir.path(), &["project_name=Old Name"]);
    git(&proj, &["init", "-q"]);
    git(&proj, &["add", "-A"]);
    git(&proj, &["commit", "-qm", "scaffold"]);
    std::fs::write(proj.join("README.md"), read(&proj, "README.md") + "wip\n").unwrap();

    update(
        &proj,
        &["--answer", "project_name=New Name", "--skip-tasks"],
    )
    .failure()
    .stderr(predicates::str::contains(
        "uncommitted changes in file(s) this update would write: README.md",
    ));
    assert!(
        read(&proj, "pyproject.toml").contains("old-name"),
        "nothing written"
    );

    update(
        &proj,
        &[
            "--answer",
            "project_name=New Name",
            "--allow-dirty",
            "--skip-tasks",
        ],
    )
    .success();
    assert!(read(&proj, "README.md").ends_with("wip\n"));
    assert!(read(&proj, "pyproject.toml").contains("new-name"));
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "weft")
        .env("GIT_AUTHOR_EMAIL", "weft@example.com")
        .env("GIT_COMMITTER_NAME", "weft")
        .env("GIT_COMMITTER_EMAIL", "weft@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .expect("git on PATH");
    assert!(status.success(), "git {args:?}");
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
