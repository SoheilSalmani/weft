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

fn copy_fixture(to: &Path) -> PathBuf {
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
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello");
    let dst = to.join("hello");
    copy_dir(&src, &dst);
    dst
}

#[test]
fn lsp_diagnostics_for_broken_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let template = copy_fixture(tmp.path());
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
    let template = copy_fixture(tmp.path());
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
