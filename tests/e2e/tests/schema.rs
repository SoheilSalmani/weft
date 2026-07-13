//! `weft schema`: the generated JSON Schemas must accept every fixture
//! template file and reject malformed ones.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates")
}

fn generate_schemas() -> (serde_json::Value, serde_json::Value) {
    let out = tempfile::tempdir().unwrap();
    weft()
        .arg("schema")
        .arg("--out")
        .arg(out.path())
        .assert()
        .success();
    let load = |name: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(out.path().join(name)).unwrap()).unwrap()
    };
    (
        load("weft-patch.schema.json"),
        load("weft-manifest.schema.json"),
    )
}

#[test]
fn schemas_accept_all_fixture_files() {
    let (patch_schema, manifest_schema) = generate_schemas();
    let patch_validator = jsonschema::validator_for(&patch_schema).unwrap();
    let manifest_validator = jsonschema::validator_for(&manifest_schema).unwrap();

    let mut patches = 0;
    let mut manifests = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        // manifest: TOML -> JSON value
        let manifest_src = std::fs::read_to_string(dir.join("weft.toml")).unwrap();
        let manifest: toml::Value = manifest_src.parse().unwrap();
        let manifest_json = serde_json::to_value(&manifest).unwrap();
        let errors: Vec<String> = manifest_validator
            .iter_errors(&manifest_json)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "manifest in {dir:?}: {errors:?}");
        manifests += 1;

        let patches_dir = dir.join("patches");
        if patches_dir.is_dir() {
            for patch in std::fs::read_dir(&patches_dir).unwrap() {
                let path = patch.unwrap().path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let value: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                let errors: Vec<String> = patch_validator
                    .iter_errors(&value)
                    .map(|e| e.to_string())
                    .collect();
                assert!(errors.is_empty(), "{path:?}: {errors:?}");
                patches += 1;
            }
        }
    }
    assert!(manifests >= 1 && patches >= 2, "fixtures actually covered");
}

#[test]
fn patch_schema_rejects_malformed_ops() {
    let (patch_schema, _) = generate_schemas();
    let validator = jsonschema::validator_for(&patch_schema).unwrap();

    for (label, bad) in [
        (
            "unknown op kind",
            serde_json::json!({"ops": [{"op": "explode", "path": "x"}]}),
        ),
        (
            "create without content",
            serde_json::json!({"ops": [{"op": "create_file", "path": "x"}]}),
        ),
        (
            "bad segment key",
            serde_json::json!({"ops": [{"op": "create_file", "path": "x", "content": [[{"bogus": "y"}]]}]}),
        ),
        (
            "stray top-level key",
            serde_json::json!({"ops": [], "wehn": "typo"}),
        ),
    ] {
        assert!(!validator.is_valid(&bad), "schema accepted {label}: {bad}");
    }

    // `ops` is optional by design: action patches carry only hooks.
    let action_patch = serde_json::json!({
        "when": "deploy_to_vercel",
        "hooks": [{
            "id": "deploy", "phase": "post", "effect": "deploy",
            "label": "Deploy", "action": "vercel deploy --prod"
        }]
    });
    assert!(
        validator.is_valid(&action_patch),
        "schema must accept a zero-op action patch"
    );
}

#[test]
fn manifest_schema_rejects_bad_question_kind() {
    let (_, manifest_schema) = generate_schemas();
    let validator = jsonschema::validator_for(&manifest_schema).unwrap();

    let bad = serde_json::json!({
        "template": {"name": "x", "weft-version": "0.1"},
        "question": [{"id": "a", "kind": "stringg"}],
    });
    assert!(!validator.is_valid(&bad), "schema accepted bad kind");

    let choice_without_choices = serde_json::json!({
        "template": {"name": "x", "weft-version": "0.1"},
        "question": [{"id": "a", "kind": "choice"}],
    });
    assert!(
        !validator.is_valid(&choice_without_choices),
        "schema accepted choice without choices"
    );
}
