//! M6 acceptance: `weft check` catches structural problems and proves (or
//! disproves) commutation of independent patches.

use std::fs;
use std::path::Path;

use predicates::prelude::PredicateBooleanExt;
use weft_e2e::weft;

fn write_template(root: &Path, patches: &[(&str, &str)]) {
    fs::create_dir_all(root.join("patches")).unwrap();
    fs::write(
        root.join("weft.toml"),
        r#"
[template]
name = "checkme"
weft-version = "0.1"

[[question]]
id = "name"
kind = "string"
default = "'demo'"
"#,
    )
    .unwrap();
    for (name, body) in patches {
        fs::write(root.join("patches").join(format!("{name}.json")), body).unwrap();
    }
}

#[test]
fn detects_non_commuting_independent_patches() {
    let tmp = tempfile::tempdir().unwrap();
    // Two independent patches create the same file: order matters, so one
    // application order fails => they do not commute.
    write_template(
        tmp.path(),
        &[
            (
                "a",
                r#"{"ops":[{"op":"create_file","path":"same.txt","content":["from a"]}]}"#,
            ),
            (
                "b",
                r#"{"ops":[{"op":"create_file","path":"same.txt","content":["from b"]}]}"#,
            ),
        ],
    );
    weft()
        .arg("check")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("do not commute"));
}

#[test]
fn passes_commuting_patches_and_reports_pairs() {
    let tmp = tempfile::tempdir().unwrap();
    write_template(
        tmp.path(),
        &[
            (
                "a",
                r#"{"ops":[{"op":"create_file","path":"a.txt","content":["a"]}]}"#,
            ),
            (
                "b",
                r#"{"ops":[{"op":"create_file","path":"b.txt","content":["b"]}]}"#,
            ),
        ],
    );
    weft()
        .arg("check")
        .arg(tmp.path())
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "commutation ok for 1 independent pair(s)",
        ));
}

#[test]
fn detects_unknown_answer_reference() {
    let tmp = tempfile::tempdir().unwrap();
    write_template(
        tmp.path(),
        &[(
            "a",
            r#"{"ops":[{"op":"create_file","path":"a.txt","content":[[{"answer":"nope"}]]}]}"#,
        )],
    );
    weft()
        .arg("check")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown answer `nope`"));
}

#[test]
fn detects_patch_dependency_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    write_template(
        tmp.path(),
        &[
            ("a", r#"{"depends_on":["b"],"ops":[]}"#),
            ("b", r#"{"depends_on":["a"],"ops":[]}"#),
        ],
    );
    weft()
        .arg("check")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("cycle"));
}

#[test]
fn detects_bad_starlark_expression() {
    let tmp = tempfile::tempdir().unwrap();
    write_template(
        tmp.path(),
        &[(
            "a",
            r#"{"when":"not not(","ops":[{"op":"create_file","path":"a.txt","content":[]}]}"#,
        )],
    );
    weft()
        .arg("check")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("does not parse"));
}

/// A template whose `base` renders `config.toml` with a line from an
/// expression gated on `extra` right after the `[servers]` anchor.
fn expr_template(root: &Path, patches: &[(&str, &str)]) {
    fs::create_dir_all(root.join("patches")).unwrap();
    fs::write(
        root.join("weft.toml"),
        "[template]\nname = \"exprs\"\nweft-version = \"0.1\"\n\n\
         [[question]]\nid = \"extra\"\nkind = \"bool\"\ndefault = \"False\"\n",
    )
    .unwrap();
    fs::write(
        root.join("patches/base.json"),
        r##"{"ops":[{"op":"create_file","path":"config.toml","content":[
            "# config", "", "[servers]",
            [{"expr": "'extra = 1' if extra else ''"}],
            [{"expr": "'more = 1' if extra else ''"}],
            "tail"]}]}"##,
    )
    .unwrap();
    for (name, body) in patches {
        fs::write(root.join("patches").join(format!("{name}.json")), body).unwrap();
    }
}

fn session_with_extra(root: &Path) -> std::path::PathBuf {
    weft()
        .args(["session", "new", "--answer", "extra=true"])
        .arg("--template")
        .arg(root)
        .assert()
        .success();
    root.join(".weft-sessions/default/worktree")
}

#[test]
fn commit_keeps_expression_output_out_of_hunk_context() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    expr_template(root, &[]);
    let worktree = session_with_extra(root);
    let config = fs::read_to_string(worktree.join("config.toml")).unwrap();
    assert_eq!(config, "# config\n\n[servers]\nextra = 1\nmore = 1\ntail\n");
    fs::write(
        worktree.join("config.toml"),
        config.replace("[servers]\n", "[servers]\nmine = 2\n"),
    )
    .unwrap();
    weft()
        .args(["commit", "--session", "default", "--name", "mine", "--yes"])
        .arg("--template")
        .arg(root)
        .assert()
        .success();

    let patch: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("patches/mine.json")).unwrap()).unwrap();
    let hunk = &patch["ops"][0]["hunks"][0];
    assert_eq!(hunk["context_before"], serde_json::json!(["", "[servers]"]));
    assert_eq!(hunk["context_after"], serde_json::json!([]), "{hunk}");
    // With the expression lines empty, the patch still applies.
    weft()
        .arg("check")
        .arg(root)
        .args(["--answer", "extra=false"])
        .assert()
        .success();
}

#[test]
fn commit_refuses_a_change_with_only_expression_lines_around_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    expr_template(root, &[]);
    let worktree = session_with_extra(root);
    let config = fs::read_to_string(worktree.join("config.toml")).unwrap();
    fs::write(
        worktree.join("config.toml"),
        config.replace("extra = 1\n", "extra = 1\nmine = 2\n"),
    )
    .unwrap();
    weft()
        .args(["commit", "--session", "default", "--name", "mine", "--yes"])
        .arg("--template")
        .arg(root)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "`config.toml`: the change at line 5 sits between lines that read differently \
             under other answers or with other patches active (`expr` segment \
             `'extra = 1' if extra else ''`, `expr` segment `'more = 1' if extra else ''`)",
        ));
    assert!(!root.join("patches/mine.json").exists());
}

#[test]
fn check_names_the_expression_a_failing_hunk_anchored_on() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // `mine` as an older weft recorded it with `extra` on: its context is
    // the expression's output.
    expr_template(
        root,
        &[
            (
                "mine",
                r#"{"depends_on":["base"],"ops":[{"op":"modify_file","path":"config.toml",
                    "hunks":[{"context_before":["", "[servers]"],"added":["mine = 2"],
                    "context_after":["extra = 1", "more = 1"]}]}]}"#,
            ),
            (
                "other",
                r#"{"ops":[{"op":"create_file","path":"other.txt","content":["o"]}]}"#,
            ),
        ],
    );
    weft()
        .arg("check")
        .arg(root)
        .args(["--answer", "extra=true"])
        .assert()
        .success();
    weft()
        .arg("check")
        .arg(root)
        .args(["--answer", "extra=false"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "patch `mine` does not apply under these answers: hunk 0 does not match \
             `config.toml`: it expects `extra = 1` after the change, where line 4 is empty, \
             rendered from an `expr` segment of patch `base`",
        ))
        .stderr(predicates::str::contains("do not commute").not())
        .stderr(predicates::str::contains("full render failed").not())
        .stderr(predicates::str::contains("check failed with 1 issue(s)"));
}
