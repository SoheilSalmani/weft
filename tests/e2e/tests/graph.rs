//! `weft graph`: JSON graph document and per-node diffs from the CLI.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn hello_template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello")
}

#[test]
fn graph_json_with_answers_reports_active_nodes() {
    let out = weft()
        .arg("graph")
        .arg(hello_template())
        .arg("--answer")
        .arg("project_name=x")
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid JSON");

    assert_eq!(doc["template"]["name"], "hello");
    let nodes = doc["nodes"].as_array().unwrap();
    let edges = doc["edges"].as_array().unwrap();
    assert_eq!(nodes.len(), 2);
    assert_eq!(edges.len(), 1);

    let docker = nodes.iter().find(|n| n["name"] == "docker").unwrap();
    assert_eq!(docker["active"], true, "use_docker defaults to True");
    assert_eq!(docker["when"], "use_docker");
    // the edge goes base -> docker
    let base = nodes.iter().find(|n| n["name"] == "base").unwrap();
    assert_eq!(edges[0]["source"], base["id"]);
    assert_eq!(edges[0]["target"], docker["id"]);
}

#[test]
fn preset_deactivates_gated_node() {
    let out = weft()
        .arg("graph")
        .arg(hello_template())
        .arg("--preset")
        .arg("no-docker")
        .arg("--answer")
        .arg("project_name=x")
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let docker = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "docker")
        .unwrap();
    assert_eq!(docker["active"], false);
}

#[test]
fn structural_graph_without_answers_has_null_active() {
    // No flags and project_name has no default -> structural graph.
    let out = weft()
        .arg("graph")
        .arg(hello_template())
        .arg("--json")
        .assert()
        .success();
    let doc: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert!(doc["nodes"][0]["active"].is_null());
}

#[test]
fn node_diff_from_cli() {
    weft()
        .arg("graph")
        .arg(hello_template())
        .arg("--answer")
        .arg("project_name=x")
        .arg("--diff")
        .arg("docker")
        .assert()
        .success()
        .stdout(predicates::str::contains("+FROM python:3.12-slim"))
        .stdout(predicates::str::contains("+Ships with Docker."));
}

#[test]
fn text_summary_lists_nodes_and_ops() {
    weft()
        .arg("graph")
        .arg(hello_template())
        .assert()
        .success()
        .stdout(predicates::str::contains("2 node(s), 1 edge(s)"))
        .stdout(predicates::str::contains("create_file Dockerfile"));
}
