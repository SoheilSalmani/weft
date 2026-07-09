//! M6 acceptance: `weft check` catches structural problems and proves (or
//! disproves) commutation of independent patches.

use std::fs;
use std::path::Path;

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
