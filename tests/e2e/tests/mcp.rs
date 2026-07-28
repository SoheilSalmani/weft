//! `weft mcp`: drive the stdio MCP server with hand-rolled JSON-RPC —
//! initialize, list tools, describe a template, scaffold a project.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    fn spawn(templates_dir: &Path) -> Self {
        let mut child = Command::new(weft_e2e::weft_path())
            .arg("mcp")
            .arg("--templates-dir")
            .arg(templates_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn weft mcp");
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
        let line = serde_json::to_string(&message).unwrap();
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Send a request and return the whole response message (result or error).
    fn request_raw(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).expect("read response");
            assert!(!line.is_empty(), "server closed the stream");
            let value: serde_json::Value = serde_json::from_str(&line).expect("valid JSON-RPC");
            if value.get("id") == Some(&serde_json::json!(id)) {
                return value;
            }
            // skip notifications/other traffic
        }
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let value = self.request_raw(method, params);
        assert!(
            value.get("error").is_none(),
            "JSON-RPC error: {}",
            value["error"]
        );
        value["result"].clone()
    }

    fn initialize(&mut self) -> serde_json::Value {
        let result = self.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "weft-e2e", "version": "0"},
            }),
        );
        self.send(serde_json::json!({
            "jsonrpc": "2.0", "method": "notifications/initialized",
        }));
        result
    }

    fn call_tool(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        let result = self.request(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        );
        assert_ne!(
            result.get("isError"),
            Some(&serde_json::json!(true)),
            "tool {name} errored: {result}"
        );
        result["structuredContent"].clone()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_scaffold_flow() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates");
    let mut client = McpClient::spawn(&fixtures);

    let init = client.initialize();
    assert!(init["capabilities"]["tools"].is_object());
    assert!(init["instructions"]
        .as_str()
        .unwrap()
        .contains("scaffolding"));

    let tools = client.request("tools/list", serde_json::json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "list_templates",
        "describe_template",
        "scaffold",
        "check_template",
        "session_start",
        "session_write_file",
        "session_commit",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }

    let templates = client.call_tool("list_templates", serde_json::json!({}));
    assert!(templates["templates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "hello"));

    let contract = client.call_tool(
        "describe_template",
        serde_json::json!({"template": "hello"}),
    );
    let required: Vec<&str> = contract["questions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|q| q["required"] == true)
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert_eq!(required, ["project_name"]);

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    let result = client.call_tool(
        "scaffold",
        serde_json::json!({
            "template": "hello",
            "dest": dest_path.to_str().unwrap(),
            "answers": {"project_name": "MCP App", "use_docker": false},
        }),
    );
    assert!(result["files"].as_i64().unwrap() >= 2);
    let readme = std::fs::read_to_string(dest_path.join("README.md")).unwrap();
    assert!(readme.starts_with("# MCP App"));
    // guardrail: the template's task must NOT have run
    assert!(!dest_path.join(".task-ran").exists());
}

#[test]
fn mcp_session_commit_flow() {
    // work on a copy so the fixture stays pristine
    let tmp = tempfile::tempdir().unwrap();
    let templates = tmp.path().join("templates");
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
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates"),
        &templates,
    );

    let mut client = McpClient::spawn(&templates);
    client.initialize();

    let started = client.call_tool(
        "session_start",
        serde_json::json!({
            "template": "hello",
            "answers": {"project_name": "My Shop"},
        }),
    );
    assert!(started["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "README.md"));

    client.call_tool(
        "session_write_file",
        serde_json::json!({
            "template": "hello",
            "path": "Makefile",
            "content": "serve-my-shop:\n\techo My Shop\n",
        }),
    );

    // path escape must be rejected (surfaces as a JSON-RPC error)
    let escape = client.request_raw(
        "tools/call",
        serde_json::json!({
            "name": "session_write_file",
            "arguments": {"template": "hello", "path": "../evil", "content": "x"},
        }),
    );
    assert!(escape["error"]["message"]
        .as_str()
        .unwrap()
        .contains("invalid worktree path"));

    client.call_tool(
        "session_commit",
        serde_json::json!({
            "template": "hello",
            "name": "makefile",
            "description": "adds a Makefile with a serve target",
            "tags": ["build"],
            "decisions": {"project_name": false},
        }),
    );

    // the committed patch: package_name abstracted (default accept),
    // project_name kept literal (decision false), metadata stored
    let patch: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(templates.join("hello/patches/makefile.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(patch["description"], "adds a Makefile with a serve target");
    assert_eq!(patch["tags"][0], "build");
    let json = patch.to_string();
    assert!(json.contains(r#"{"answer":"package_name"}"#));
    assert!(json.contains("My Shop"), "declined answer stays literal");

    let check = client.call_tool("check_template", serde_json::json!({"template": "hello"}));
    assert_eq!(check["ok"], true);
}
