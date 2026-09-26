//! Composition: the workspace fixture includes `hello` at `services/hello`.
//! Covers bind seeding, namespaced answers, mounted rendering, composed
//! state, fleet update, and check on composed templates.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn workspace_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/workspace")
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel))
        .unwrap_or_else(|e| panic!("reading {rel} in scaffold: {e}"))
}

#[test]
fn scaffolds_the_composed_tree() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(workspace_template())
        .arg(&dest_path)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // parent files at the root
    assert!(read(&dest_path, "README.md").contains("# Acme"));
    // child files under the mount; project_name seeded by the bind
    assert!(read(&dest_path, "services/hello/README.md").contains("# Acme Service"));
    // child Starlark default derived from the bound answer
    assert!(read(&dest_path, "services/hello/pyproject.toml").contains("name = \"acme-service\""));
    // hello's use_docker defaults to True inside the instance
    assert!(dest_path.join("services/hello/Dockerfile").exists());

    // state pins the instance: include, key, mount, child base, answers
    let state = read(&dest_path, ".weft/state.toml");
    let parsed: toml::Value = state.parse().unwrap();
    let inst = &parsed["instance"].as_array().unwrap()[0];
    assert_eq!(inst["include"].as_str(), Some("svc"));
    assert_eq!(inst["mount"].as_str(), Some("services/hello"));
    assert_eq!(inst["base"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        inst["answers"]["project_name"].as_str(),
        Some("Acme Service")
    );
}

#[test]
fn namespaced_answers_override_binds() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(workspace_template())
        .arg(&dest_path)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--answer")
        .arg("svc.project_name=Custom Name")
        .arg("--answer")
        .arg("svc.use_docker=false")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    assert!(read(&dest_path, "services/hello/README.md").contains("# Custom Name"));
    assert!(!dest_path.join("services/hello/Dockerfile").exists());
}

#[test]
fn update_propagates_child_template_changes() {
    // Copy the fixtures so we can evolve the child template.
    let tpl_root = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates");
    for name in ["workspace", "hello"] {
        copy_dir(&fixtures.join(name), &tpl_root.path().join(name));
    }
    let parent = tpl_root.path().join("workspace");

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&parent)
        .arg(&dest_path)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // local edit inside the mounted instance must survive the update
    let readme = dest_path.join("services/hello/README.md");
    let mut content = std::fs::read_to_string(&readme).unwrap();
    content.push_str("Local note.\n");
    std::fs::write(&readme, content).unwrap();

    // evolve the CHILD template: a new patch adding a file
    std::fs::write(
        tpl_root.path().join("hello/patches/extra.json"),
        r#"{
          "depends_on": ["base"],
          "ops": [{"op":"create_file","path":"EXTRA.txt","content":["from the update"]}]
        }"#,
    )
    .unwrap();

    weft()
        .arg("update")
        .arg(&dest_path)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // the child's new file landed inside the mount
    assert_eq!(
        read(&dest_path, "services/hello/EXTRA.txt").trim(),
        "from the update"
    );
    // the local edit survived
    assert!(read(&dest_path, "services/hello/README.md").contains("Local note."));
    // idempotent second run
    weft()
        .arg("update")
        .arg(&dest_path)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
}

#[test]
fn nested_includes_render_three_levels() {
    // platform → workspace → hello: grandchild answers derive from binds
    // chained through each level.
    let tpl_root = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates");
    for name in ["workspace", "hello"] {
        copy_dir(&fixtures.join(name), &tpl_root.path().join(name));
    }
    let platform = tpl_root.path().join("platform");
    std::fs::create_dir_all(platform.join("patches")).unwrap();
    std::fs::write(
        platform.join("weft.toml"),
        r#"[template]
name = "platform"
weft-version = "0.1"

[[question]]
id = "platform_name"
kind = "string"

[[include]]
name = "acme"
template = "../workspace"
path = "teams/acme"

[include.bind]
workspace_name = "platform_name + ' Acme'"
"#,
    )
    .unwrap();
    std::fs::write(
        platform.join("patches/base.json"),
        r##"{"ops":[{"op":"create_file","path":"PLATFORM.md","content":[["# ",{"answer":"platform_name"}]]}]}"##,
    )
    .unwrap();

    let dest = tempfile::tempdir().unwrap();
    let out = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&platform)
        .arg(&out)
        .arg("--answer")
        .arg("platform_name=Mega")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // level 0: platform's own file
    assert!(read(&out, "PLATFORM.md").contains("# Mega"));
    // level 1: workspace mounted, bind chained from platform_name
    assert!(read(&out, "teams/acme/README.md").contains("# Mega Acme"));
    // level 2: hello mounted inside workspace, bind chained again
    assert!(read(&out, "teams/acme/services/hello/README.md").contains("# Mega Acme Service"));
    assert!(read(&out, "teams/acme/services/hello/pyproject.toml")
        .contains("name = \"mega-acme-service\""));
}

#[test]
fn record_foreach_authors_an_integration_patch() {
    // The workspace fixture already has a hand-written foreach patch
    // (`registry`); this authors an equivalent one with `record --foreach`
    // and proves it renders per instance.
    let tpl_root = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates");
    for name in ["workspace", "hello"] {
        copy_dir(&fixtures.join(name), &tpl_root.path().join(name));
    }
    let parent = tpl_root.path().join("workspace");

    // Mount one sample `connector` instance into the recording base.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&parent)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--foreach")
        .arg("connector=stripe")
        .assert()
        .success();
    let worktree = parent.join(".weft-sessions/main/worktree");
    assert!(
        worktree.join("connectors/stripe/README.md").is_file(),
        "sample instance must be mounted in the worktree"
    );

    // A foreach patch may reach inside the instance: the sample mount
    // abstracts to a `key` path segment, and a parent file references the
    // sample key too.
    std::fs::write(
        worktree.join("connectors/stripe/CONNECTOR.md"),
        "Registered in the workspace.\n",
    )
    .unwrap();
    let readme = worktree.join("README.md");
    let mut content = std::fs::read_to_string(&readme).unwrap();
    content.push_str("Connector directory: connectors/stripe\n");
    std::fs::write(&readme, content).unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&parent)
        .arg("--name")
        .arg("directory")
        .arg("--title")
        .arg("List connector directories")
        .arg("--yes")
        .assert()
        .success();

    // The sample key was abstracted to `key` — in content and in the path
    // under the mount…
    let patch = std::fs::read_to_string(parent.join("patches/directory.json")).unwrap();
    assert!(patch.contains("\"foreach\": \"connector\""), "{patch}");
    assert!(patch.contains("\"answer\": \"key\""), "{patch}");
    assert!(
        patch.contains("\"connectors/\""),
        "path must be split around the key: {patch}"
    );
    assert!(
        !patch.contains("stripe"),
        "sample key must not leak: {patch}"
    );

    // …so it renders once per real instance.
    let dest = tempfile::tempdir().unwrap();
    let out = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&parent)
        .arg(&out)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--instance")
        .arg("connector=github")
        .arg("--instance")
        .arg("connector=jira")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    let rendered = read(&out, "README.md");
    assert!(rendered.contains("Connector directory: connectors/github"));
    assert!(rendered.contains("Connector directory: connectors/jira"));
    assert!(out.join("connectors/github/CONNECTOR.md").is_file());
    assert!(out.join("connectors/jira/CONNECTOR.md").is_file());

    // Outside a foreach session, an edit under a repeat include's instances
    // has nothing to abstract the key out of — refused with directions.
    weft()
        .args(["session", "new", "plain"])
        .arg("--template")
        .arg(&parent)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .assert()
        .success();
    let plain = parent.join(".weft-sessions/plain/worktree");
    std::fs::create_dir_all(plain.join("connectors/stripe")).unwrap();
    std::fs::write(plain.join("connectors/stripe/HACK.md"), "nope\n").unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&parent)
        .arg("--session")
        .arg("plain")
        .arg("--name")
        .arg("bad")
        .arg("--yes")
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "inside the instances of repeat include `connector`",
        ));
}

#[test]
fn check_validates_composed_templates() {
    weft()
        .arg("check")
        .arg(workspace_template())
        .arg("--answer")
        .arg("workspace_name=x")
        .assert()
        .success()
        .stdout(predicates::str::contains("passed all checks"));
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
