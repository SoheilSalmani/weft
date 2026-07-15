//! Hub-ref consumption against a stub registry: resolve → download →
//! sha256 verify → cache → scaffold; offline cache hits; tamper rejection.
//! (The real registry's API is tested in the weft-hub repo.)

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

use sha2::Digest;
use weft_e2e::weft;

/// Tarball of the `hello` fixture template.
fn hello_tarball() -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello");
    let mut builder = tar::Builder::new(Vec::new());
    builder
        .append_path_with_name(root.join("weft.toml"), "weft.toml")
        .unwrap();
    builder
        .append_dir_all("patches", root.join("patches"))
        .unwrap();
    if root.join("presets").is_dir() {
        builder
            .append_dir_all("presets", root.join("presets"))
            .unwrap();
    }
    let tarball = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tarball).unwrap();
    gz.finish().unwrap()
}

/// A one-thread stub registry. `lie_about_sha` serves a wrong checksum in
/// the index (tamper simulation).
fn stub_registry(tarball: Vec<u8>, lie_about_sha: bool) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let sha = if lie_about_sha {
        "0".repeat(64)
    } else {
        sha2::Sha256::digest(&tarball)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    let handle = std::thread::spawn(move || {
        // Serve a handful of requests, then exit.
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = request
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_owned();
            let (status, content_type, body): (&str, &str, Vec<u8>) = if path
                == "/api/v1/index/acme/hello"
            {
                let index = format!(
                    r#"{{"owner":"acme","name":"hello","versions":[{{"version":"1.0.0","sha256":"{sha}","yanked":false,"published_at":"2026-07-15T00:00:00Z"}}]}}"#
                );
                ("200 OK", "application/json", index.into_bytes())
            } else if path == "/api/v1/templates/acme/hello/1.0.0/download" {
                ("200 OK", "application/gzip", tarball.clone())
            } else {
                (
                    "404 Not Found",
                    "application/json",
                    b"{\"error\":\"nope\"}".to_vec(),
                )
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        }
    });
    (format!("http://{addr}"), handle)
}

#[test]
fn scaffolds_from_a_hub_ref_and_pins_the_resolved_version() {
    let home = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let out = dest.path().join("out");
    let (url, _server) = stub_registry(hello_tarball(), false);

    weft()
        .env("HOME", home.path())
        .env("WEFT_HUB_URL", &url)
        .arg("new")
        .arg("hub:acme/hello")
        .arg(&out)
        .arg("--answer")
        .arg("project_name=Hub Demo")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(readme.contains("# Hub Demo"));
    // The state records the *resolved* hub ref, not a filesystem path.
    let state = std::fs::read_to_string(out.join(".weft/state.toml")).unwrap();
    assert!(state.contains("hub:acme/hello@1.0.0"), "{state}");
    // The cache holds the unpacked template.
    assert!(home
        .path()
        .join(".weft/hub/acme/hello/1.0.0/weft.toml")
        .is_file());
}

#[test]
fn pinned_versions_scaffold_offline_from_the_cache() {
    let home = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let (url, _server) = stub_registry(hello_tarball(), false);

    // Warm the cache online.
    weft()
        .env("HOME", home.path())
        .env("WEFT_HUB_URL", &url)
        .arg("new")
        .arg("hub:acme/hello@1.0.0")
        .arg(dest.path().join("one"))
        .arg("--answer")
        .arg("project_name=One")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // Point at a dead registry: the pinned ref must still work (cache hit).
    weft()
        .env("HOME", home.path())
        .env("WEFT_HUB_URL", "http://127.0.0.1:1")
        .arg("new")
        .arg("hub:acme/hello@1.0.0")
        .arg(dest.path().join("two"))
        .arg("--answer")
        .arg("project_name=Two")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(dest.path().join("two/README.md").is_file());
}

#[test]
fn sha256_mismatch_refuses_to_install() {
    let home = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let (url, _server) = stub_registry(hello_tarball(), true);

    weft()
        .env("HOME", home.path())
        .env("WEFT_HUB_URL", &url)
        .arg("new")
        .arg("hub:acme/hello")
        .arg(dest.path().join("out"))
        .arg("--answer")
        .arg("project_name=X")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("sha256 mismatch"));
    // Nothing landed in the cache.
    assert!(!home.path().join(".weft/hub/acme/hello/1.0.0").exists());
}

#[test]
fn missing_registry_is_a_clear_error() {
    weft()
        .env_remove("WEFT_HUB_URL")
        .arg("new")
        .arg("hub:acme/hello")
        .arg("/tmp/nope-hub")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("WEFT_HUB_URL"));
}
