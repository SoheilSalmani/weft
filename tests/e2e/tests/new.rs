//! M3 acceptance: `weft new` scaffolds the fixture template non-interactively.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn weft() -> Command {
    Command::cargo_bin("weft").expect("weft binary built by workspace")
}

fn hello_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel))
        .unwrap_or_else(|e| panic!("reading {rel} in scaffold: {e}"))
}

#[test]
fn scaffolds_with_answers_and_defaults() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--non-interactive")
        .assert()
        .success();

    assert_eq!(
        read(&dest_path, "README.md"),
        "# My Demo\n\nScaffolded by weft.\n\nShips with Docker.\n"
    );
    // package_name derived via Starlark default
    assert!(read(&dest_path, "pyproject.toml").contains("name = \"my-demo\""));
    // use_docker defaults to True
    assert!(read(&dest_path, "Dockerfile").contains("COPY . /app/my-demo"));
    // task fired on initial scaffold
    assert_eq!(read(&dest_path, ".task-ran").trim(), "synced");

    let state = read(&dest_path, ".weft/state.toml");
    let parsed: toml::Value = state.parse().unwrap();
    assert_eq!(
        parsed["answers"]["project_name"].as_str(),
        Some("My Demo"),
        "state records concrete answers: {state}"
    );
    assert_eq!(
        parsed["state"]["base"].as_array().map(Vec::len),
        Some(2),
        "both patches pinned: {state}"
    );
}

#[test]
fn preset_layers_under_cli_answers() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(&dest_path)
        .arg("--preset")
        .arg("no-docker")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();

    assert!(
        !dest_path.join("Dockerfile").exists(),
        "preset disables docker"
    );
    assert!(dest_path.join("README.md").exists());
    assert!(
        !read(&dest_path, "README.md").contains("Docker"),
        "docker hunk must not apply"
    );
}

#[test]
fn missing_answer_fails_non_interactively_with_hint() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(&dest_path)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("project_name"));
    assert!(!dest_path.join("README.md").exists());
}

#[test]
fn refuses_non_empty_destination() {
    let dest = tempfile::tempdir().unwrap();
    std::fs::write(dest.path().join("existing.txt"), "keep me").unwrap();
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(dest.path())
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("not empty"));
    assert_eq!(read(dest.path(), "existing.txt"), "keep me");
}

#[test]
fn presets_list_and_show() {
    weft()
        .arg("presets")
        .arg("list")
        .arg(hello_template())
        .assert()
        .success()
        .stdout(predicates::str::contains("no-docker"));
    weft()
        .arg("presets")
        .arg("show")
        .arg("no-docker")
        .arg(hello_template())
        .assert()
        .success()
        .stdout(predicates::str::contains("use_docker = false"));
}

#[test]
fn check_validates_fixture_template() {
    weft()
        .arg("check")
        .arg(hello_template())
        .assert()
        .success()
        .stdout(predicates::str::contains("2 patch(es)"));
}
