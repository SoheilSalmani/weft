//! ADR-0002: a template that extends another narrows the questions it
//! inherits with `[refine.<id>]` — fixed and blocked choices, locks, and
//! refined defaults — and scaffolding, describe, and update all honor it.

use std::fs;
use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates")
}

fn skills_dbt() -> PathBuf {
    fixtures().join("skills-dbt")
}

/// Copy skills-base and skills-dbt side by side into `dir` (skills-dbt
/// extends `../skills-base`) and return the copied skills-dbt.
fn copy_templates(dir: &Path) -> PathBuf {
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
    for name in ["skills-base", "skills-dbt"] {
        copy_dir(&fixtures().join(name), &dir.join(name));
    }
    dir.join("skills-dbt")
}

/// Replace `from` with `to` in the template's weft.toml.
fn edit_manifest(template: &Path, from: &str, to: &str) {
    let path = template.join("weft.toml");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{from} not in {}", path.display());
    fs::write(&path, text.replace(from, to)).unwrap();
}

fn scaffold(template: &Path, proj: &Path, answers: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = weft();
    cmd.arg("new")
        .arg(template)
        .arg(proj)
        .arg("--answer")
        .arg("project_name=Demo");
    for a in answers {
        cmd.arg("--answer").arg(a);
    }
    cmd.arg("--skip-tasks").arg("--non-interactive").assert()
}

fn update(proj: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    weft()
        .arg("update")
        .arg(proj)
        .args(args)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
}

fn stderr(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stderr).into_owned()
}

fn state(proj: &Path) -> toml::Value {
    fs::read_to_string(proj.join(".weft/state.toml"))
        .unwrap()
        .parse()
        .unwrap()
}

/// A stored string-array answer, from `[answers]` or `[derived]`.
fn stored_list(s: &toml::Value, table: &str, id: &str) -> Option<Vec<String>> {
    let values = s.get(table)?.get(id)?.as_array()?;
    Some(
        values
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    )
}

/// Which of the stack skills (and the jira skill) the project has.
fn skills(proj: &Path) -> Vec<&'static str> {
    ["dbt", "sql", "airflow", "jira"]
        .into_iter()
        .filter(|s| proj.join(format!(".agents/skills/{s}/SKILL.md")).exists())
        .collect()
}

#[test]
fn defaults_follow_the_refinement() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    scaffold(&skills_dbt(), &proj, &[]).success();

    assert_eq!(skills(&proj), ["dbt", "sql"]);
    let s = state(&proj);
    assert_eq!(
        stored_list(&s, "derived", "stack_skills"),
        Some(vec!["sql".to_owned(), "dbt".to_owned()])
    );
    assert_eq!(s["derived"]["use_jira"].as_bool(), Some(false));
    assert_eq!(s["derived"]["ci"].as_str(), Some("github"));
    assert_eq!(stored_list(&s, "answers", "stack_skills"), None);
}

#[test]
fn an_empty_selection_keeps_the_fixed_choices() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    scaffold(&skills_dbt(), &proj, &["stack_skills="]).success();

    assert_eq!(skills(&proj), ["dbt"]);
    assert_eq!(
        stored_list(&state(&proj), "answers", "stack_skills"),
        Some(vec!["dbt".to_owned()])
    );
}

#[test]
fn a_blocked_choice_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    scaffold(&skills_dbt(), &proj, &["stack_skills=airflow"])
        .failure()
        .stderr(predicates::str::contains(
            "`airflow` is blocked by template `skills-dbt`",
        ));
    assert!(!proj.exists());

    scaffold(&skills_dbt(), &proj, &["ci=gitlab"])
        .failure()
        .stderr(predicates::str::contains(
            "`gitlab` is blocked by template `skills-dbt`",
        ));
    assert!(!proj.exists());
}

#[test]
fn a_lock_refuses_a_different_answer_and_accepts_the_same_one() {
    let dir = tempfile::tempdir().unwrap();
    let proj = dir.path().join("proj");
    scaffold(&skills_dbt(), &proj, &["use_jira=true"])
        .failure()
        .stderr(predicates::str::contains(
            "answer `use_jira` is locked to False by template `skills-dbt`",
        ));
    assert!(!proj.exists());

    scaffold(&skills_dbt(), &proj, &["use_jira=false"]).success();
    assert!(!skills(&proj).contains(&"jira"));
    assert_eq!(state(&proj)["answers"]["use_jira"].as_bool(), Some(false));
}

#[test]
fn describe_shows_the_narrowed_questions() {
    let out = weft()
        .arg("describe")
        .arg(skills_dbt())
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let questions = doc["questions"].as_array().unwrap();
    let by_id = |id: &str| {
        questions
            .iter()
            .find(|q| q["id"] == id)
            .unwrap_or_else(|| panic!("question {id}"))
    };

    let stack = by_id("stack_skills");
    assert_eq!(stack["choices"], serde_json::json!(["dbt", "sql"]));
    assert_eq!(stack["fixed"], serde_json::json!(["dbt"]));
    assert_eq!(stack["blocked"], serde_json::json!(["airflow"]));
    assert_eq!(stack["refined_by"], serde_json::json!(["skills-dbt"]));

    let jira = by_id("use_jira");
    assert_eq!(jira["locked"], true);
    assert_eq!(jira["required"], false);

    assert_eq!(
        by_id("project_name")["description"],
        "Human-facing name; its snake_case slug names the dbt project."
    );
}

#[test]
fn update_follows_a_refined_default() {
    let dir = tempfile::tempdir().unwrap();
    let template = copy_templates(dir.path());
    let proj = dir.path().join("proj");
    scaffold(&template, &proj, &[]).success();
    assert_eq!(skills(&proj), ["dbt", "sql"]);

    edit_manifest(&template, "default = \"['sql']\"", "default = \"[]\"");
    update(&proj, &[]).success();

    assert_eq!(skills(&proj), ["dbt"]);
    assert!(
        !proj.join(".agents/skills/sql").exists(),
        "the update left the emptied skill directory behind"
    );
    assert_eq!(
        stored_list(&state(&proj), "derived", "stack_skills"),
        Some(vec!["dbt".to_owned()])
    );
}

#[test]
fn update_stops_on_a_stored_answer_the_template_now_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let template = copy_templates(dir.path());
    let proj = dir.path().join("proj");
    scaffold(&template, &proj, &["stack_skills=sql"]).success();
    assert_eq!(skills(&proj), ["dbt", "sql"]);

    edit_manifest(
        &template,
        "choices = [\"dbt\", \"sql\"]",
        "choices = [\"dbt\"]",
    );
    let out = update(&proj, &[]).failure();
    let err = stderr(&out);
    assert!(err.contains("no longer allowed"), "{err}");
    assert!(err.contains("--unset stack_skills"), "{err}");
    assert_eq!(skills(&proj), ["dbt", "sql"]);

    update(&proj, &["--unset", "stack_skills"]).success();
    assert_eq!(skills(&proj), ["dbt"]);
    let s = state(&proj);
    assert_eq!(stored_list(&s, "answers", "stack_skills"), None);
    assert_eq!(
        stored_list(&s, "derived", "stack_skills"),
        Some(vec!["dbt".to_owned()])
    );
}

#[test]
fn check_accepts_the_refining_template() {
    weft().arg("check").arg(skills_dbt()).assert().success();
}
