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
