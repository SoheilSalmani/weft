//! The agent contract: `weft describe --json`, `--agents-md`,
//! `weft new --answers-json`, and `weft check --json`.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn hello_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

#[test]
fn describe_json_is_a_complete_contract() {
    let out = weft()
        .arg("describe")
        .arg(hello_template())
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();

    assert_eq!(doc["template"]["name"], "hello");
    let questions = doc["questions"].as_array().unwrap();
    let by_id = |id: &str| {
        questions
            .iter()
            .find(|q| q["id"] == id)
            .unwrap_or_else(|| panic!("question {id}"))
    };
    assert_eq!(by_id("project_name")["required"], true);
    assert_eq!(by_id("package_name")["required"], false);
    // evaluated default preview (bool literal)
    assert_eq!(by_id("use_docker")["default_preview"], "True");

    assert!(doc["usage"]["scaffold"]
        .as_str()
        .unwrap()
        .contains("--answer \"project_name="));
    assert!(doc["usage"]["scaffold_json"]
        .as_str()
        .unwrap()
        .contains("--answers-json"));
    let patches = doc["patches"].as_array().unwrap();
    assert_eq!(patches.len(), 2);
}

#[test]
fn agents_md_renders_question_table() {
    weft()
        .arg("describe")
        .arg(hello_template())
        .arg("--agents-md")
        .arg("-")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "| `project_name` | string | **yes** |",
        ))
        .stdout(predicates::str::contains("## Scaffold"))
        .stdout(predicates::str::contains("weft describe --json"));
}

#[test]
fn answers_json_scaffolds() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(&dest_path)
        .arg("--answers-json")
        .arg(r#"{"project_name":"Json App","use_docker":false}"#)
        .arg("--non-interactive")
        .assert()
        .success();
    let readme = std::fs::read_to_string(dest_path.join("README.md")).unwrap();
    assert!(readme.starts_with("# Json App"));
    assert!(!dest_path.join("Dockerfile").exists());
}

#[test]
fn answers_json_rejects_wrong_types_and_unknown_keys() {
    let dest = tempfile::tempdir().unwrap();
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(dest.path().join("a"))
        .arg("--answers-json")
        .arg(r#"{"use_docker":"yes"}"#)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("expects a boolean"));
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(dest.path().join("b"))
        .arg("--answers-json")
        .arg(r#"{"nope":1}"#)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("no question with id `nope`"));
}

#[test]
fn check_json_shape() {
    let out = weft()
        .arg("check")
        .arg(hello_template())
        .arg("--json")
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(doc["ok"], true);
    assert!(doc["issues"].as_array().unwrap().is_empty());
}

#[test]
fn commit_metadata_lands_in_describe() {
    // copy template, record, commit with --describe/--tag, then describe
    let tmp = tempfile::tempdir().unwrap();
    let template = tmp.path().join("tpl");
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_dir(&hello_template(), &template);

    weft()
        .arg("record")
        .arg("--template")
        .arg(&template)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success();
    std::fs::write(template.join(".weft-record/worktree/extra.txt"), "hi\n").unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&template)
        .arg("--name")
        .arg("extra")
        .arg("--describe")
        .arg("adds an extra file for testing")
        .arg("--tag")
        .arg("demo")
        .arg("--yes")
        .assert()
        .success();

    let out = weft()
        .arg("describe")
        .arg(&template)
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let patch = doc["patches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "extra")
        .unwrap();
    assert_eq!(patch["description"], "adds an extra file for testing");
    assert_eq!(patch["tags"][0], "demo");
}
