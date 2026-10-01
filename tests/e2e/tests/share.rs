//! Sharing a file: independent patches that each create `.mcp.json` become
//! a patch that owns the lines they share and fillers of its slot, so they
//! can all be on at once. Each patch alone still renders what it did.

use std::fs;
use std::path::{Path, PathBuf};

use weft_e2e::weft;

const MANIFEST: &str = r#"[template]
name = "shared"
weft-version = "0.1"

[[question]]
id = "use_linear"
kind = "bool"
default = "False"

[[question]]
id = "use_jira"
kind = "bool"
default = "False"
"#;

const BASE: &str =
    r##"{"ops": [{"op": "create_file", "path": "README.md", "content": ["# Shop"]}]}"##;

/// `.mcp.json` with one server written over several lines, as a gated root.
fn creator(when: &str, key: &str, url: &str) -> String {
    serde_json::json!({
        "when": when,
        "ops": [{"op": "create_file", "path": ".mcp.json", "content": [
            "{",
            "  \"mcpServers\": {",
            format!("    \"{key}\": {{"),
            "      \"type\": \"http\",",
            format!("      \"url\": \"{url}\""),
            "    }",
            "  }",
            "}"
        ]}]
    })
    .to_string()
}

fn linear() -> String {
    creator("use_linear", "linear", "https://mcp.linear.app/mcp")
}

fn jira() -> String {
    creator("use_jira", "atlassian", "https://mcp.atlassian.com/v1/sse")
}

fn template(root: &Path, patches: &[(&str, &str)]) -> PathBuf {
    let tpl = root.join("shared");
    fs::create_dir_all(tpl.join("patches")).unwrap();
    fs::write(tpl.join("weft.toml"), MANIFEST).unwrap();
    fs::write(tpl.join("patches/base.json"), BASE).unwrap();
    for (name, body) in patches {
        fs::write(tpl.join("patches").join(format!("{name}.json")), body).unwrap();
    }
    tpl
}

/// The project's `.mcp.json` under these answers, or `None` without one.
fn rendered(tpl: &Path, answers: &[&str]) -> Option<String> {
    let dest = tempfile::tempdir().unwrap();
    let dest = dest.path().join("app");
    let mut cmd = weft();
    cmd.arg("new")
        .arg(tpl)
        .arg(&dest)
        .args(["--non-interactive", "--skip-tasks"]);
    for answer in answers {
        cmd.args(["--answer", answer]);
    }
    cmd.assert().success();
    fs::read_to_string(dest.join(".mcp.json")).ok()
}

fn patch(tpl: &Path, name: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(tpl.join(format!("patches/{name}.json"))).unwrap())
        .unwrap()
}

fn share(tpl: &Path) -> assert_cmd::Command {
    let mut cmd = weft();
    cmd.args(["share", ".mcp.json", "--name", "mcp"])
        .arg("--template")
        .arg(tpl);
    cmd
}

const BOTH: &str = "{\n  \"mcpServers\": {\n    \"atlassian\": {\n      \"type\": \"http\",\n      \
                    \"url\": \"https://mcp.atlassian.com/v1/sse\"\n    },\n    \"linear\": {\n      \
                    \"type\": \"http\",\n      \"url\": \"https://mcp.linear.app/mcp\"\n    }\n  }\n}\n";

#[test]
fn sharing_keeps_each_patch_alone_and_renders_them_together() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear()), ("jira", &jira())]);
    let alone: Vec<_> = [&["use_linear=true"][..], &["use_jira=true"], &[]]
        .iter()
        .map(|answers| rendered(&tpl, answers))
        .collect();

    share(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "created patch `mcp`: creates .mcp.json, left out while no patch adds to it",
        ))
        .stderr(predicates::str::contains(
            "rewrote patch `linear`: adds its lines to .mcp.json, depends on `mcp`",
        ));

    let after: Vec<_> = [&["use_linear=true"][..], &["use_jira=true"], &[]]
        .iter()
        .map(|answers| rendered(&tpl, answers))
        .collect();
    assert_eq!(after, alone, "each patch alone renders what it did");
    assert_eq!(
        rendered(&tpl, &["use_linear=true", "use_jira=true"]).as_deref(),
        Some(BOTH)
    );
    assert_eq!(
        patch(&tpl, "mcp")["ops"][0]["content"][2],
        serde_json::json!({"slot": "entries", "separator": ","})
    );
    assert_eq!(patch(&tpl, "linear")["ops"][0]["op"], "fill_slot");
    assert_eq!(
        patch(&tpl, "jira")["depends_on"],
        serde_json::json!(["mcp"])
    );
    weft()
        .arg("check")
        .arg(&tpl)
        .args(["--answer", "use_linear=true", "--answer", "use_jira=true"])
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "1 more only fill the same slots, which commutes by construction",
        ));
}

#[test]
fn a_dry_run_prints_the_combined_file_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear()), ("jira", &jira())]);
    share(&tpl)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(format!(".mcp.json, with every patch on:\n{BOTH}"));
    assert!(!tpl.join("patches/mcp.json").exists());
    assert_eq!(
        fs::read_to_string(tpl.join("patches/linear.json")).unwrap(),
        linear()
    );
}

#[test]
fn an_example_decides_the_separator() {
    let tmp = tempfile::tempdir().unwrap();
    let table = |when: &str, key: &str, url: &str| {
        serde_json::json!({"when": when, "ops": [{"op": "create_file", "path": "config.toml",
            "content": [format!("[mcp_servers.{key}]"), format!("url = \"{url}\"")]}]})
        .to_string()
    };
    let tpl = template(
        tmp.path(),
        &[
            (
                "linear",
                &table("use_linear", "linear", "https://mcp.linear.app/mcp"),
            ),
            (
                "jira",
                &table("use_jira", "atlassian", "https://mcp.atlassian.com/v1/sse"),
            ),
        ],
    );
    let both = "[mcp_servers.atlassian]\nurl = \"https://mcp.atlassian.com/v1/sse\"\n\n\
                [mcp_servers.linear]\nurl = \"https://mcp.linear.app/mcp\"\n";
    let example = tmp.path().join("config.toml");
    fs::write(&example, both).unwrap();
    weft()
        .args(["share", "config.toml", "--name", "codex"])
        .arg(format!("--example=config.toml={}", example.display()))
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    let dest = tmp.path().join("app");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .args(["--non-interactive", "--skip-tasks"])
        .args(["--answer", "use_linear=true", "--answer", "use_jira=true"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(dest.join("config.toml")).unwrap(), both);
}

#[test]
fn an_example_out_of_name_order_is_refused_and_nothing_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear()), ("jira", &jira())]);
    let reversed = "{\n  \"mcpServers\": {\n    \"linear\": {\n      \"type\": \"http\",\n      \
                    \"url\": \"https://mcp.linear.app/mcp\"\n    },\n    \"atlassian\": {\n      \
                    \"type\": \"http\",\n      \"url\": \"https://mcp.atlassian.com/v1/sse\"\n    \
                    }\n  }\n}\n";
    let example = tmp.path().join("mcp.json");
    fs::write(&example, reversed).unwrap();
    share(&tpl)
        .arg(format!("--example=.mcp.json={}", example.display()))
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the patches' lines go in the order of the patches' names (`jira`, `linear`)",
        ));
    assert!(!tpl.join("patches/mcp.json").exists());
    assert_eq!(
        fs::read_to_string(tpl.join("patches/jira.json")).unwrap(),
        jira()
    );
}

#[test]
fn a_file_another_patch_changes_is_not_shared() {
    let tmp = tempfile::tempdir().unwrap();
    let tweak = r#"{"depends_on": ["linear"], "ops": [{"op": "modify_file", "path": ".mcp.json",
        "hunks": [{"context_before": ["{"], "added": ["  \"$schema\": \"x\","]}]}]}"#;
    let tpl = template(
        tmp.path(),
        &[("linear", &linear()), ("jira", &jira()), ("tweak", tweak)],
    );
    share(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "patch `tweak` changes `.mcp.json`; weft shares a file that patches only create",
        ));
}

/// A session recording Jira's server with Linear off: its `.mcp.json`
/// clashes with `linear`'s.
fn jira_session(tpl: &Path) -> PathBuf {
    weft()
        .args(["session", "new", "jira"])
        .args(["--answer", "use_jira=true"])
        .arg("--template")
        .arg(tpl)
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/jira/worktree");
    let text: serde_json::Value = serde_json::from_str(&jira()).unwrap();
    let lines: Vec<String> = text["ops"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| format!("{}\n", l.as_str().unwrap()))
        .collect();
    fs::write(worktree.join(".mcp.json"), lines.concat()).unwrap();
    worktree
}

fn commit_jira(tpl: &Path) -> assert_cmd::Command {
    let mut cmd = weft();
    cmd.args([
        "commit",
        "--session",
        "jira",
        "--name",
        "jira",
        "--when",
        "use_jira",
    ])
    .arg("--yes")
    .arg("--template")
    .arg(tpl);
    cmd
}

#[test]
fn commit_share_records_the_patch_as_a_fill() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    jira_session(&tpl);
    commit_jira(&tpl)
        .args(["--share", "mcp"])
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "committed patch `jira`: adds its lines to .mcp.json, depends on `mcp`",
        ));
    assert!(!tpl.join(".weft-sessions").exists(), "the session ended");
    assert_eq!(patch(&tpl, "jira")["ops"][0]["op"], "fill_slot");
    assert_eq!(
        patch(&tpl, "jira")["depends_on"],
        serde_json::json!(["base", "mcp"])
    );
    assert_eq!(
        rendered(&tpl, &["use_linear=true", "use_jira=true"]).as_deref(),
        Some(BOTH)
    );
}

#[test]
fn commit_notes_the_clash_and_check_says_how_to_share() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    jira_session(&tpl);
    commit_jira(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "note: patch `linear` (when use_linear) also creates .mcp.json. The two clash in a \
             project that has both on; if they belong together, run `weft share .mcp.json \
             --name NAME`",
        ));
    assert_eq!(patch(&tpl, "jira")["ops"][0]["op"], "create_file");
    weft()
        .arg("check")
        .arg(&tpl)
        .args(["--answer", "use_linear=true", "--answer", "use_jira=true"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "if both belong in one project, run `weft share .mcp.json --name NAME`",
        ));
}
