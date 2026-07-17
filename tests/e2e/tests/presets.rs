//! Presets v2 acceptance: locks (skip + conflict errors), multichoice
//! constraints ((user ∪ fixed) − blocked), and `presets save`/`rm`.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn hello_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
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

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

/// A preset entry locks its question: an explicit `--answer` for it errors.
#[test]
fn answer_over_preset_lock_errors() {
    let dest = tempfile::tempdir().unwrap();
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(dest.path().join("out"))
        .arg("--preset")
        .arg("no-docker")
        .arg("--answer")
        .arg("use_docker=true")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("locked"));
}

/// Re-answering a lock with the identical value is allowed.
#[test]
fn identical_answer_over_lock_is_fine() {
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(hello_template())
        .arg(&dest_path)
        .arg("--preset")
        .arg("no-docker")
        .arg("--answer")
        .arg("use_docker=false")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(!dest_path.join("Dockerfile").exists());
}

/// A template copy with a multichoice question and a constraint preset.
fn constrained_template() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    let mut manifest = read(&tpl, "weft.toml");
    manifest.push_str(
        "\n[[question]]\nid = \"features\"\nkind = \"multichoice\"\n\
         choices = [\"lint\", \"ci\", \"docs\", \"experimental\"]\ndefault = \"[]\"\n\
         \n[[preset]]\nname = \"corp\"\nfile = \"presets/corp.toml\"\n",
    );
    std::fs::write(tpl.join("weft.toml"), manifest).unwrap();
    std::fs::write(
        tpl.join("presets/corp.toml"),
        "[features]\nfixed = [\"lint\"]\nblocked = [\"experimental\"]\n",
    )
    .unwrap();
    (dir, tpl)
}

/// Final multichoice value = (user selection ∪ fixed) − blocked.
#[test]
fn constrained_multichoice_merges_fixed() {
    let (_guard, tpl) = constrained_template();
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--preset")
        .arg("corp")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--answer")
        .arg("features=docs")
        .arg("--non-interactive")
        .assert()
        .success();
    let state = read(&dest_path, ".weft/state.toml");
    let parsed: toml::Value = state.parse().unwrap();
    let features: Vec<&str> = parsed["answers"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(features, vec!["docs", "lint"], "state: {state}");
}

/// Picking a blocked choice errors.
#[test]
fn blocked_choice_errors() {
    let (_guard, tpl) = constrained_template();
    let dest = tempfile::tempdir().unwrap();
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(dest.path().join("out"))
        .arg("--preset")
        .arg("corp")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--answer")
        .arg("features=experimental")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("blocked"));
}

/// `presets save` (scripted) → usable preset; `presets rm` removes it.
#[test]
fn preset_save_and_rm_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);

    weft()
        .arg("presets")
        .arg("save")
        .arg("acme")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=Acme")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(tpl.join("presets/acme.toml").exists());
    assert!(read(&tpl, "weft.toml").contains("name = \"acme\""));

    // The saved preset locks project_name, so `new` needs no answers at all.
    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--preset")
        .arg("acme")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(read(&dest_path, "README.md").contains("# Acme"));

    weft()
        .arg("presets")
        .arg("show")
        .arg("acme")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("locked"));

    weft()
        .arg("presets")
        .arg("rm")
        .arg("acme")
        .arg(&tpl)
        .assert()
        .success();
    assert!(!tpl.join("presets/acme.toml").exists());
    assert!(!read(&tpl, "weft.toml").contains("acme"));
}

/// `presets save --fix/--block` authors a multichoice constraint.
#[test]
fn preset_save_constraint_flags() {
    let (_guard, tpl) = constrained_template();
    weft()
        .arg("presets")
        .arg("save")
        .arg("pinned")
        .arg(&tpl)
        .arg("--fix")
        .arg("features=ci")
        .arg("--block")
        .arg("features=experimental")
        .arg("--non-interactive")
        .assert()
        .success();
    let file = read(&tpl, "presets/pinned.toml");
    assert!(file.contains("[features]"), "file: {file}");
    assert!(file.contains("fixed = [\"ci\"]"), "file: {file}");
    assert!(
        file.contains("blocked = [\"experimental\"]"),
        "file: {file}"
    );

    weft()
        .arg("presets")
        .arg("show")
        .arg("pinned")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("fixed: ci"));
}

/// Saving a preset that answers an unknown question fails validation.
#[test]
fn preset_save_rejects_unknown_question() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .arg("presets")
        .arg("save")
        .arg("bad")
        .arg(&tpl)
        .arg("--fix")
        .arg("nope=x")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown question"));
    assert!(!tpl.join("presets/bad.toml").exists());
}
