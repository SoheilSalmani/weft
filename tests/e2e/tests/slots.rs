//! Slots: a named place in a file that independent patches add lines to.
//! The owner declares it (here `.mcp.json`, left out while nothing fills
//! it); every other patch contributes with `fill_slot`, and the slot renders
//! the contributions sorted by key, so the order the patches apply in never
//! matters.

use std::fs;
use std::path::{Path, PathBuf};

use weft_e2e::weft;

const MANIFEST: &str = r#"[template]
name = "slots"
weft-version = "0.1"

[[question]]
id = "use_linear"
kind = "bool"
default = "True"

[[question]]
id = "use_lightdash"
kind = "bool"
default = "False"

[[question]]
id = "lightdash_url"
kind = "string"
default = "'https://app.lightdash.cloud'"
"#;

const MCP: &str = r#"{
  "ops": [{"op": "create_file", "path": ".mcp.json", "omit_when_empty": ["servers"],
    "content": ["{", "  \"mcpServers\": {", {"slot": "servers", "separator": ","}, "  }", "}"]}]
}"#;

fn filler(key: &str, when: &str, line: &str) -> String {
    format!(
        r#"{{"depends_on": ["mcp"], "when": "{when}", "ops": [{{"op": "fill_slot",
            "path": ".mcp.json", "slot": "servers", "key": "{key}", "lines": [{line}]}}]}}"#
    )
}

fn linear() -> String {
    filler(
        "linear",
        "use_linear",
        r#""    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }""#,
    )
}

fn lightdash() -> String {
    filler(
        "lightdash",
        "use_lightdash",
        r#"["    \"lightdash\": { \"url\": \"", {"answer": "lightdash_url"}, "/api/v1/mcp\" }"]"#,
    )
}

fn template(root: &Path, patches: &[(&str, &str)]) -> PathBuf {
    let tpl = root.join("slots");
    fs::create_dir_all(tpl.join("patches")).unwrap();
    fs::write(tpl.join("weft.toml"), MANIFEST).unwrap();
    fs::write(tpl.join("patches/mcp.json"), MCP).unwrap();
    for (name, body) in patches {
        fs::write(tpl.join("patches").join(format!("{name}.json")), body).unwrap();
    }
    tpl
}

fn scaffold(tpl: &Path, dest: &Path, answers: &[&str]) {
    let mut cmd = weft();
    cmd.arg("new").arg(tpl).arg(dest).arg("--non-interactive");
    for answer in answers {
        cmd.args(["--answer", answer]);
    }
    cmd.assert().success();
}

#[test]
fn sibling_fills_render_in_key_order_and_check_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(
        tmp.path(),
        &[("linear", &linear()), ("lightdash", &lightdash())],
    );
    weft()
        .arg("check")
        .arg(&tpl)
        .args(["--answer", "use_lightdash=true"])
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "1 more only fill the same slots, which commutes by construction",
        ));

    let out = tmp.path().join("both");
    scaffold(
        &tpl,
        &out,
        &[
            "use_lightdash=true",
            "lightdash_url=https://eu1.lightdash.cloud",
        ],
    );
    assert_eq!(
        fs::read_to_string(out.join(".mcp.json")).unwrap(),
        "{\n  \"mcpServers\": {\n    \
         \"lightdash\": { \"url\": \"https://eu1.lightdash.cloud/api/v1/mcp\" },\n    \
         \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }\n  }\n}\n"
    );
}

#[test]
fn an_empty_slot_with_omit_when_empty_leaves_the_file_out() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    let none = tmp.path().join("none");
    scaffold(&tpl, &none, &["use_linear=false"]);
    assert!(!none.join(".mcp.json").exists());
    let some = tmp.path().join("some");
    scaffold(&tpl, &some, &[]);
    assert_eq!(
        fs::read_to_string(some.join(".mcp.json")).unwrap(),
        "{\n  \"mcpServers\": {\n    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }\n  }\n}\n"
    );
}

#[test]
fn a_duplicate_key_fails_check_and_names_both_patches() {
    let tmp = tempfile::tempdir().unwrap();
    let twin = filler("linear", "use_linear", r#""    \"twin\": {}""#);
    let tpl = template(tmp.path(), &[("linear", &linear()), ("twin", &twin)]);
    weft()
        .arg("check")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "patches `linear` and `twin` both fill slot `servers` of `.mcp.json` under key \
             `linear`",
        ))
        .stderr(predicates::str::contains("check failed with 1 issue(s)"));
}

#[test]
fn a_duplicate_key_is_reported_once_whichever_patch_applies_first() {
    let tmp = tempfile::tempdir().unwrap();
    // `twin` sorts before `linear-sse` by id but after it by name, so the
    // full render and the pair meet the clash in opposite orders.
    let twin = filler("linear", "use_linear", r#""    \"twin\": {}""#);
    let sse = filler(
        "linear",
        "use_linear",
        r#""    \"linear\": { \"url\": \"https://mcp.linear.app/sse\" }""#,
    );
    let tpl = template(tmp.path(), &[("twin", &twin), ("linear-sse", &sse)]);
    weft()
        .arg("check")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "patches `linear-sse` and `twin` both fill slot `servers` of `.mcp.json`",
        ))
        .stderr(predicates::str::contains("check failed with 1 issue(s)"));
}

#[test]
fn check_names_both_patches_when_an_omitted_file_was_edited() {
    let tmp = tempfile::tempdir().unwrap();
    let header = r#"{"depends_on": ["mcp"], "ops": [{"op": "modify_file", "path": ".mcp.json",
        "hunks": [{"context_before": ["{"], "added": ["  \"$schema\": \"x\","]}]}]}"#;
    let tpl = template(tmp.path(), &[("linear", &linear()), ("header", header)]);
    weft()
        .arg("check")
        .arg(&tpl)
        .args(["--answer", "use_linear=false"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "patch `header` does not apply under these answers: it changes `.mcp.json`, which \
             patch `mcp` leaves out while its slots are empty",
        ));
}

#[test]
fn the_first_fill_of_an_omitted_file_records_as_a_fill() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    // With Linear off the base leaves `.mcp.json` out: the author writes the
    // whole file, owner's lines and their own.
    weft()
        .args(["session", "new", "lightdash"])
        .args([
            "--answer",
            "use_linear=false",
            "--answer",
            "use_lightdash=true",
        ])
        .args(["--answer", "lightdash_url=https://demo.lightdash.cloud"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    let worktree = tpl.join(".weft-sessions/lightdash/worktree");
    assert!(!worktree.join(".mcp.json").exists());
    let written = format!("{{\n  \"mcpServers\": {{\n{LIGHTDASH_LINE}\n  }}\n}}\n");
    fs::write(worktree.join(".mcp.json"), &written).unwrap();
    weft()
        .args(["diff", "--session", "lightdash"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "note: line 3 fill slot `servers` under key `lightdash`",
        ));
    weft()
        .args(["commit", "--session", "lightdash", "--name", "lightdash"])
        .args(["--when", "use_lightdash", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    let patch: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tpl.join("patches/lightdash.json")).unwrap())
            .unwrap();
    assert_eq!(patch["ops"].as_array().unwrap().len(), 1, "{patch}");
    assert_eq!(patch["ops"][0]["op"], "fill_slot", "{patch}");
    let both = tmp.path().join("both");
    scaffold(
        &tpl,
        &both,
        &[
            "use_lightdash=true",
            "lightdash_url=https://demo.lightdash.cloud",
        ],
    );
    assert_eq!(
        fs::read_to_string(both.join(".mcp.json")).unwrap(),
        format!("{{\n  \"mcpServers\": {{\n{LIGHTDASH_LINE},\n{LINEAR_LINE}\n  }}\n}}\n")
    );
}

#[test]
fn amending_the_only_fill_of_an_omitted_file_keeps_it_a_fill() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(
        tmp.path(),
        &[("linear", &linear()), ("lightdash", &lightdash())],
    );
    // The amend's base is `mcp` alone, which leaves `.mcp.json` out.
    weft()
        .args(["patch", "amend", "lightdash", "--non-interactive"])
        .args(["--answer", "use_lightdash=true"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    let file = tpl.join(".weft-sessions/lightdash/worktree/.mcp.json");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("/api/v1/mcp", "/api/v2/mcp")).unwrap();
    weft()
        .args(["commit", "--session", "lightdash", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/lightdash.json")).unwrap();
    let patch: serde_json::Value = serde_json::from_str(&patch).unwrap();
    assert_eq!(
        patch["ops"],
        serde_json::json!([{
            "op": "fill_slot",
            "path": ".mcp.json",
            "slot": "servers",
            "key": "lightdash",
            "lines": [["    \"lightdash\": { \"url\": \"", {"answer": "lightdash_url"}, "/api/v2/mcp\" }"]]
        }])
    );
}

/// A session on `mcp` + `linear` with Lightdash on, named after the patch
/// it will record.
fn lightdash_session(tpl: &Path) -> PathBuf {
    weft()
        .args(["session", "new", "lightdash"])
        .args(["--answer", "use_lightdash=true"])
        .args(["--answer", "lightdash_url=https://demo.lightdash.cloud"])
        .arg("--template")
        .arg(tpl)
        .assert()
        .success();
    tpl.join(".weft-sessions/lightdash/worktree")
}

const LINEAR_LINE: &str = "    \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }";
const LIGHTDASH_LINE: &str =
    "    \"lightdash\": { \"url\": \"https://demo.lightdash.cloud/api/v1/mcp\" }";

#[test]
fn commit_records_lines_added_inside_a_slot_as_a_fill() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    let worktree = lightdash_session(&tpl);
    let edited = format!("{{\n  \"mcpServers\": {{\n{LIGHTDASH_LINE},\n{LINEAR_LINE}\n  }}\n}}\n");
    fs::write(worktree.join(".mcp.json"), &edited).unwrap();
    weft()
        .args(["diff", "--session", "lightdash"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "note: line 3 fill slot `servers` under key `lightdash`",
        ));
    // Only `mcp` is a dependency: the fill does not anchor on `linear`'s
    // line, so it applies on `mcp` alone (commit proves that).
    weft()
        .args(["commit", "--session", "lightdash", "--name", "lightdash"])
        .args(["--when", "use_lightdash", "--depends-on", "mcp", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();

    let patch: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tpl.join("patches/lightdash.json")).unwrap())
            .unwrap();
    assert_eq!(
        patch["ops"],
        serde_json::json!([{
            "op": "fill_slot",
            "path": ".mcp.json",
            "slot": "servers",
            "key": "lightdash",
            "lines": [["    \"lightdash\": { \"url\": \"", {"answer": "lightdash_url"}, "/api/v1/mcp\" }"]]
        }]),
        "one fill, no hunk: {patch}"
    );
    weft()
        .arg("check")
        .arg(&tpl)
        .args(["--answer", "use_lightdash=true"])
        .assert()
        .success();
    // Replaying it reproduces the worktree, and without `linear` the fill is
    // the last contribution, so it ends without the separator.
    let same = tmp.path().join("same");
    scaffold(
        &tpl,
        &same,
        &[
            "use_lightdash=true",
            "lightdash_url=https://demo.lightdash.cloud",
        ],
    );
    assert_eq!(fs::read_to_string(same.join(".mcp.json")).unwrap(), edited);
    let alone = tmp.path().join("alone");
    scaffold(
        &tpl,
        &alone,
        &[
            "use_linear=false",
            "use_lightdash=true",
            "lightdash_url=https://demo.lightdash.cloud",
        ],
    );
    assert_eq!(
        fs::read_to_string(alone.join(".mcp.json")).unwrap(),
        format!("{{\n  \"mcpServers\": {{\n{LIGHTDASH_LINE}\n  }}\n}}\n")
    );
}

#[test]
fn commit_refuses_a_contribution_out_of_key_order() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    let worktree = lightdash_session(&tpl);
    fs::write(
        worktree.join(".mcp.json"),
        format!("{{\n  \"mcpServers\": {{\n{LINEAR_LINE},\n{LIGHTDASH_LINE}\n  }}\n}}\n"),
    )
    .unwrap();
    weft()
        .args(["commit", "--session", "lightdash", "--name", "lightdash"])
        .args(["--when", "use_lightdash", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "`.mcp.json`: with this patch's lines under key `lightdash`, slot `servers` renders as:",
        ))
        .stderr(predicates::str::contains(
            "contributions are ordered by key, and every one but the last ends with `,`",
        ));
    assert!(!tpl.join("patches/lightdash.json").exists());
}

/// `weft patch amend mcp`, with the worktree's `.mcp.json`.
fn amend_owner(tpl: &Path, answers: &[&str]) -> PathBuf {
    let mut cmd = weft();
    cmd.args(["patch", "amend", "mcp", "--non-interactive"]);
    for answer in answers {
        cmd.args(["--answer", answer]);
    }
    cmd.arg("--template").arg(tpl).assert().success();
    tpl.join(".weft-sessions/mcp/worktree/.mcp.json")
}

fn commit_amend(tpl: &Path) -> assert_cmd::assert::Assert {
    weft()
        .args(["commit", "--session", "mcp", "--yes"])
        .arg("--template")
        .arg(tpl)
        .assert()
}

#[test]
fn amending_an_owner_edits_its_lines_around_the_contributions() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(
        tmp.path(),
        &[("linear", &linear()), ("lightdash", &lightdash())],
    );
    let file = amend_owner(&tpl, &[]);
    // Every filler's lines are there, whatever their gates say.
    let shown = fs::read_to_string(&file).unwrap();
    assert_eq!(
        shown,
        "{\n  \"mcpServers\": {\n    \
         \"lightdash\": { \"url\": \"https://app.lightdash.cloud/api/v1/mcp\" },\n    \
         \"linear\": { \"url\": \"https://mcp.linear.app/mcp\" }\n  }\n}\n"
    );
    fs::write(&file, shown.replace("\"mcpServers\"", "\"servers\"")).unwrap();
    commit_amend(&tpl).success();
    let owner: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tpl.join("patches/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        owner["ops"],
        serde_json::json!([{
            "op": "create_file",
            "path": ".mcp.json",
            "omit_when_empty": ["servers"],
            "content": ["{", "  \"servers\": {", {"slot": "servers", "separator": ","}, "  }", "}"],
            "mode": 420
        }])
    );
    assert_eq!(
        fs::read_to_string(tpl.join("patches/linear.json")).unwrap(),
        linear(),
        "the fillers are left alone"
    );
}

#[test]
fn amending_an_owner_refuses_a_change_to_a_contribution() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[("linear", &linear())]);
    let file = amend_owner(&tpl, &[]);
    let shown = fs::read_to_string(&file).unwrap();
    fs::write(&file, shown.replace("mcp.linear.app", "linear.example")).unwrap();
    commit_amend(&tpl)
        .failure()
        .stderr(predicates::str::contains(
            "slot `servers` of `.mcp.json` held the lines `linear` add, and they are no longer \
             in the file as they were",
        ));
    assert_eq!(
        fs::read_to_string(tpl.join("patches/mcp.json")).unwrap(),
        MCP
    );
}

#[test]
fn amending_an_owner_keeps_the_place_of_a_slot_nobody_fills() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = template(tmp.path(), &[]);
    let file = amend_owner(&tpl, &[]);
    let shown = fs::read_to_string(&file).unwrap();
    assert!(
        shown.contains("⟪slot servers: other patches add their lines here⟫"),
        "{shown}"
    );
    fs::write(&file, format!("// MCP servers\n{shown}")).unwrap();
    commit_amend(&tpl).success();
    let owner: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tpl.join("patches/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        owner["ops"][0]["content"],
        serde_json::json!(["// MCP servers", "{", "  \"mcpServers\": {",
            {"slot": "servers", "separator": ","}, "  }", "}"])
    );
}

// ---- separators that take lines of their own ------------------------------

/// A template whose patch `owner` creates `path` as `content`, around slot
/// `entries`, with `patches` filling it.
fn owned(
    root: &Path,
    path: &str,
    content: serde_json::Value,
    patches: &[(&str, serde_json::Value)],
) -> PathBuf {
    let tpl = root.join("owned");
    fs::create_dir_all(tpl.join("patches")).unwrap();
    fs::write(
        tpl.join("weft.toml"),
        "[template]\nname = \"owned\"\nweft-version = \"0.1\"\n",
    )
    .unwrap();
    let owner = serde_json::json!({"ops": [{"op": "create_file", "path": path,
        "omit_when_empty": ["entries"], "content": content}]});
    fs::write(tpl.join("patches/owner.json"), owner.to_string()).unwrap();
    for (name, body) in patches {
        fs::write(
            tpl.join("patches").join(format!("{name}.json")),
            body.to_string(),
        )
        .unwrap();
    }
    tpl
}

/// The patch filling slot `entries` of `path` with `lines` under `key`.
fn fill(path: &str, key: &str, depends_on: &[&str], lines: &[&str]) -> serde_json::Value {
    serde_json::json!({"depends_on": depends_on, "ops": [{"op": "fill_slot", "path": path,
        "slot": "entries", "key": key, "lines": lines}]})
}

fn ops(tpl: &Path, name: &str) -> serde_json::Value {
    let patch = fs::read_to_string(tpl.join(format!("patches/{name}.json"))).unwrap();
    serde_json::from_str::<serde_json::Value>(&patch).unwrap()["ops"].clone()
}

/// Record patch `name` filling the slot of `path`, written as `text` in a
/// session on every patch so far.
fn record_fill(tpl: &Path, name: &str, path: &str, text: &str) -> assert_cmd::assert::Assert {
    weft()
        .args(["session", "new", name])
        .arg("--template")
        .arg(tpl)
        .assert()
        .success();
    fs::write(
        tpl.join(".weft-sessions")
            .join(name)
            .join("worktree")
            .join(path),
        text,
    )
    .unwrap();
    weft()
        .args(["commit", "--session", name, "--name", name])
        .args(["--depends-on", "owner", "--yes"])
        .arg("--template")
        .arg(tpl)
        .assert()
}

fn rendered(tpl: &Path, dest: &Path, path: &str) -> String {
    scaffold(tpl, dest, &[]);
    fs::read_to_string(dest.join(path)).unwrap()
}

#[test]
fn amending_a_filler_next_to_another_records_only_its_own_lines() {
    let tmp = tempfile::tempdir().unwrap();
    // `gamma` builds on `alpha`, so its amend shows `alpha`'s lines too.
    let alpha = fill(".env", "alpha", &["owner"], &["# Alpha", "ALPHA=1"]);
    let gamma = fill(".env", "gamma", &["alpha"], &["# Gamma", "GAMMA=1"]);
    let tpl = owned(
        tmp.path(),
        ".env",
        serde_json::json!([{"slot": "entries", "separator": "\n"}]),
        &[("alpha", alpha), ("gamma", gamma.clone())],
    );
    let amend = || {
        weft()
            .args(["patch", "amend", "gamma", "--non-interactive"])
            .arg("--template")
            .arg(&tpl)
            .assert()
            .success();
        tpl.join(".weft-sessions/gamma/worktree/.env")
    };
    let commit = || {
        weft()
            .args(["commit", "--session", "gamma", "--yes"])
            .arg("--template")
            .arg(&tpl)
            .assert()
            .success();
    };
    let file = amend();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# Alpha\nALPHA=1\n\n# Gamma\nGAMMA=1\n"
    );
    commit();
    assert_eq!(ops(&tpl, "gamma"), gamma["ops"], "nothing changed");

    let file = amend();
    fs::write(&file, "# Alpha\nALPHA=1\n\n# Gamma\nGAMMA=2\n").unwrap();
    commit();
    assert_eq!(
        ops(&tpl, "gamma"),
        serde_json::json!([{"op": "fill_slot", "path": ".env", "slot": "entries", "key": "gamma",
            "lines": ["# Gamma", "GAMMA=2"]}])
    );
    assert_eq!(
        rendered(&tpl, &tmp.path().join("app"), ".env"),
        "# Alpha\nALPHA=1\n\n# Gamma\nGAMMA=2\n"
    );
}

#[test]
fn a_separator_ending_a_line_and_adding_one_records_wherever_the_key_sorts() {
    let tmp = tempfile::tempdir().unwrap();
    // `,\n` ends every entry but the last with a comma and an empty line.
    let tpl = owned(
        tmp.path(),
        "settings.json",
        serde_json::json!(["{", {"slot": "entries", "separator": ",\n"}, "}"]),
        &[
            (
                "alpha",
                fill("settings.json", "alpha", &["owner"], &["  \"alpha\": 1"]),
            ),
            (
                "gamma",
                fill("settings.json", "gamma", &["owner"], &["  \"gamma\": 3"]),
            ),
        ],
    );
    // Between two entries, then after the last, which gains the separator.
    let between = "{\n  \"alpha\": 1,\n\n  \"beta\": 2,\n\n  \"gamma\": 3\n}\n";
    record_fill(&tpl, "beta", "settings.json", between).success();
    let after = "{\n  \"alpha\": 1,\n\n  \"beta\": 2,\n\n  \"gamma\": 3,\n\n  \"zeta\": 4\n}\n";
    record_fill(&tpl, "zeta", "settings.json", after).success();
    for (name, line) in [("beta", "  \"beta\": 2"), ("zeta", "  \"zeta\": 4")] {
        assert_eq!(
            ops(&tpl, name),
            serde_json::json!([{"op": "fill_slot", "path": "settings.json", "slot": "entries",
                "key": name, "lines": [line]}])
        );
    }
    weft().arg("check").arg(&tpl).assert().success();
    assert_eq!(
        rendered(&tpl, &tmp.path().join("app"), "settings.json"),
        after
    );
}

#[test]
fn lines_added_after_the_last_contribution_stay_in_the_slot_before_an_empty_line() {
    let tmp = tempfile::tempdir().unwrap();
    // The owner's own section follows the slot after an empty line, the
    // line the separator adds too.
    let tpl = owned(
        tmp.path(),
        ".env",
        serde_json::json!([{"slot": "entries", "separator": "\n"}, "", "# Common", "COMMON=1"]),
        &[(
            "gamma",
            fill(".env", "gamma", &["owner"], &["# Gamma", "GAMMA=1"]),
        )],
    );
    let text = "# Gamma\nGAMMA=1\n\n# Zeta\nZETA=1\n\n# Common\nCOMMON=1\n";
    record_fill(&tpl, "zeta", ".env", text).success();
    assert_eq!(
        ops(&tpl, "zeta"),
        serde_json::json!([{"op": "fill_slot", "path": ".env", "slot": "entries", "key": "zeta",
            "lines": ["# Zeta", "ZETA=1"]}])
    );
    weft().arg("check").arg(&tpl).assert().success();
    assert_eq!(rendered(&tpl, &tmp.path().join("app"), ".env"), text);
}

#[test]
fn a_contribution_that_sorts_last_keeps_the_empty_line_it_ends_with() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = owned(
        tmp.path(),
        ".env",
        serde_json::json!([{"slot": "entries", "separator": "\n"}]),
        &[(
            "gamma",
            fill(".env", "gamma", &["owner"], &["# Gamma", "GAMMA=1"]),
        )],
    );
    // `zeta` sorts after `gamma`, so no separator follows it: the empty line
    // it ends with is its own.
    let text = "# Gamma\nGAMMA=1\n\n# Zeta\nZETA=1\n\n";
    record_fill(&tpl, "zeta", ".env", text).success();
    let recorded = ops(&tpl, "zeta");
    assert_eq!(
        recorded,
        serde_json::json!([{"op": "fill_slot", "path": ".env", "slot": "entries", "key": "zeta",
            "lines": ["# Zeta", "ZETA=1", ""]}])
    );
    assert_eq!(rendered(&tpl, &tmp.path().join("app"), ".env"), text);

    // Its amend holds it alone in the slot, and records it unchanged.
    weft()
        .args(["patch", "amend", "zeta", "--non-interactive"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(tpl.join(".weft-sessions/zeta/worktree/.env")).unwrap(),
        "# Zeta\nZETA=1\n\n"
    );
    weft()
        .args(["commit", "--session", "zeta", "--yes"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success();
    assert_eq!(ops(&tpl, "zeta"), recorded, "nothing changed");
}
