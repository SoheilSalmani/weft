//! `weft lsp`: drive the language server over stdio with Content-Length
//! framing — initialize, open a broken patch, expect diagnostics; hover and
//! completion on a healthy template.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl LspClient {
    fn spawn() -> Self {
        let mut child = Command::new(weft_e2e::weft_path())
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn weft lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, message: serde_json::Value) {
        let body = serde_json::to_string(&message).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read_message(&mut self) -> serde_json::Value {
        // headers
        let mut content_length = 0usize;
        loop {
            let mut line = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                self.stdout.read_exact(&mut byte).expect("read header");
                line.push(byte[0]);
                if line.ends_with(b"\r\n") {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&line);
            let text = text.trim();
            if text.is_empty() {
                break;
            }
            if let Some(value) = text.strip_prefix("Content-Length:") {
                content_length = value.trim().parse().expect("content length");
            }
        }
        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).expect("valid JSON-RPC")
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
        loop {
            let message = self.read_message();
            if message.get("id") == Some(&serde_json::json!(id)) {
                assert!(
                    message.get("error").is_none(),
                    "LSP error: {}",
                    message["error"]
                );
                return message["result"].clone();
            }
        }
    }

    /// Read until a publishDiagnostics notification for `uri_suffix` with a
    /// non-empty diagnostics list arrives.
    fn await_diagnostics(&mut self, uri_suffix: &str) -> serde_json::Value {
        for _ in 0..50 {
            let message = self.read_message();
            if message["method"] == "textDocument/publishDiagnostics" {
                let params = &message["params"];
                let uri = params["uri"].as_str().unwrap_or_default();
                let diagnostics = params["diagnostics"].as_array().unwrap();
                if uri.ends_with(uri_suffix) && !diagnostics.is_empty() {
                    return params.clone();
                }
            }
        }
        panic!("no diagnostics for {uri_suffix}");
    }

    fn initialize(&mut self) {
        self.request(
            "initialize",
            serde_json::json!({"capabilities": {}, "processId": null, "rootUri": null}),
        );
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {},
        }));
    }

    fn did_open(&mut self, path: &Path, language: &str, text: &str) {
        let uri = format!("file://{}", path.display());
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": {"textDocument": {
                "uri": uri, "languageId": language, "version": 1, "text": text,
            }},
        }));
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn copy_fixture(to: &Path, name: &str) -> PathBuf {
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
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/templates")
        .join(name);
    let dst = to.join(name);
    copy_dir(&src, &dst);
    dst
}

/// `skills-dbt` next to the `skills-base` it extends (`../skills-base`).
fn copy_skills(to: &Path) -> PathBuf {
    copy_fixture(to, "skills-base");
    copy_fixture(to, "skills-dbt")
}

/// The 0-based line and column where `needle` first occurs in `text`.
fn position_of(text: &str, needle: &str) -> (usize, usize) {
    text.lines()
        .enumerate()
        .find_map(|(i, l)| l.find(needle).map(|c| (i, c)))
        .unwrap_or_else(|| panic!("`{needle}` not in text"))
}

fn labels(completion: &serde_json::Value) -> Vec<&str> {
    completion
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect()
}

#[test]
fn lsp_diagnostics_for_broken_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_fixture(tmp.path(), "hello");
    // Break the docker patch: reference an unknown answer.
    let patch_path = template.join("patches/docker.json");
    let broken = std::fs::read_to_string(&patch_path)
        .unwrap()
        .replace("\"package_name\"", "\"no_such_answer\"");
    std::fs::write(&patch_path, &broken).unwrap();

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&patch_path, "json", &broken);

    let params = client.await_diagnostics("docker.json");
    let messages: Vec<&str> = params["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["message"].as_str().unwrap())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("unknown answer `no_such_answer`")),
        "{messages:?}"
    );
    // the range anchors on the offending token, not line 0
    let diagnostic = &params["diagnostics"][0];
    assert!(diagnostic["range"]["start"]["line"].as_u64().unwrap() > 0);
}

#[test]
fn lsp_hover_and_definition_and_completion() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_fixture(tmp.path(), "hello");
    let patch_path = template.join("patches/docker.json");
    let text = std::fs::read_to_string(&patch_path).unwrap();

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&patch_path, "json", &text);

    // find position of package_name inside the answer segment
    let (line_no, col) = text
        .lines()
        .enumerate()
        .find_map(|(i, l)| l.find("package_name").map(|c| (i, c + 3)))
        .unwrap();
    let uri = format!("file://{}", patch_path.display());
    let pos = serde_json::json!({
        "textDocument": {"uri": uri},
        "position": {"line": line_no, "character": col},
    });

    let hover = client.request("textDocument/hover", pos.clone());
    assert!(hover["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("package_name"));

    let definition = client.request("textDocument/definition", pos.clone());
    assert!(definition["uri"].as_str().unwrap().ends_with("weft.toml"));
    // lands on the id = "package_name" line
    assert!(definition["range"]["start"]["line"].as_u64().unwrap() > 0);

    let completion = client.request("textDocument/completion", pos);
    let labels: Vec<&str> = completion
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"project_name") && labels.contains(&"use_docker"));
}

#[test]
fn lsp_inlay_hints_preview_rendered_values() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_fixture(tmp.path(), "hello");
    let patch_path = template.join("patches/docker.json");
    let text = std::fs::read_to_string(&patch_path).unwrap();

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&patch_path, "json", &text);

    let uri = format!("file://{}", patch_path.display());
    let line_count = text.lines().count() as u64;
    let hints = client.request(
        "textDocument/inlayHint",
        serde_json::json!({
            "textDocument": {"uri": uri},
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": line_count, "character": 0},
            },
        }),
    );
    let labels: Vec<&str> = hints
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["label"].as_str().unwrap())
        .collect();
    // package_name previews via project_name's example ("My Demo" -> my-demo)
    assert!(
        labels.iter().any(|l| l.contains("my-demo")),
        "expected a rendered preview hint, got {labels:?}"
    );
}

#[test]
fn lsp_refine_completion_offers_keys_and_inherited_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_skills(tmp.path());
    let manifest_path = template.join("weft.toml");
    // Unsaved edits: a key being typed inside `[refine.stack_skills]`, and a
    // new `[refine.` header at the end.
    let text = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("[refine.stack_skills]\n", "[refine.stack_skills]\nlo\n")
        + "\n[refine.";

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&manifest_path, "toml", &text);
    let uri = format!("file://{}", manifest_path.display());

    let key_line = text.lines().position(|l| l == "lo").unwrap();
    let keys = client.request(
        "textDocument/completion",
        serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": key_line, "character": 2},
        }),
    );
    let keys = labels(&keys);
    for key in [
        "prompt",
        "description",
        "example",
        "default",
        "lock",
        "choices",
        "blocked",
        "fixed",
    ] {
        assert!(keys.contains(&key), "missing refine key {key}: {keys:?}");
    }

    let header_line = text.lines().count() - 1;
    let ids = client.request(
        "textDocument/completion",
        serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": header_line, "character": "[refine.".len()},
        }),
    );
    let ids = labels(&ids);
    for inherited in ["project_name", "ci", "use_jira", "stack_skills"] {
        assert!(ids.contains(&inherited), "missing {inherited}: {ids:?}");
    }
    // skills-dbt's own question is not inherited, so it cannot be refined
    assert!(!ids.contains(&"package_name"), "{ids:?}");
}

#[test]
fn lsp_refine_header_goes_to_the_inherited_question() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_skills(tmp.path());
    let manifest_path = template.join("weft.toml");
    let text = std::fs::read_to_string(&manifest_path).unwrap();

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&manifest_path, "toml", &text);

    let (line, col) = position_of(&text, "[refine.stack_skills]");
    let pos = serde_json::json!({
        "textDocument": {"uri": format!("file://{}", manifest_path.display())},
        "position": {"line": line, "character": col + "[refine.".len() + 2},
    });

    let definition = client.request("textDocument/definition", pos.clone());
    let target = definition["uri"].as_str().unwrap();
    assert!(target.ends_with("skills-base/weft.toml"), "{target}");
    let base_text = std::fs::read_to_string(tmp.path().join("skills-base/weft.toml")).unwrap();
    let (id_line, _) = position_of(&base_text, "id = \"stack_skills\"");
    assert_eq!(
        definition["range"]["start"]["line"].as_u64().unwrap(),
        id_line as u64
    );

    // hover shows the effective question: who narrowed it, and what is gone
    let hover = client.request("textDocument/hover", pos);
    let hover = hover["contents"]["value"].as_str().unwrap();
    assert!(hover.contains("template `skills-dbt`"), "{hover}");
    assert!(hover.contains("`airflow`"), "{hover}");
}

#[test]
fn lsp_refine_error_anchors_on_its_table() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_skills(tmp.path());
    let manifest_path = template.join("weft.toml");
    // `project_name` is a string: it has no choices to block. Its id also
    // appears earlier, in `package_name`'s default.
    let broken = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "[refine.project_name]\n",
        "[refine.project_name]\nblocked = [\"x\"]\n",
    );
    std::fs::write(&manifest_path, &broken).unwrap();

    let mut client = LspClient::spawn();
    client.initialize();
    client.did_open(&manifest_path, "toml", &broken);

    let params = client.await_diagnostics("skills-dbt/weft.toml");
    let diagnostic = params["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| {
            d["message"]
                .as_str()
                .unwrap()
                .starts_with("refine `project_name`:")
        })
        .unwrap_or_else(|| panic!("no refine error: {params}"));
    let (header_line, _) = position_of(&broken, "[refine.project_name]");
    assert_eq!(
        diagnostic["range"]["start"]["line"].as_u64().unwrap(),
        header_line as u64
    );
}
