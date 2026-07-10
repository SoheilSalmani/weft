//! Shared helpers for the e2e integration tests.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Path to the `weft` binary, building it if necessary.
///
/// `assert_cmd`'s `cargo_bin` heuristic derives the artifact dir from the
/// test executable's location, and `CARGO_BIN_EXE_weft` is only set for the
/// package that owns the binary (not this one). Worse, newer cargo's
/// separated build-dir layout doesn't uplift bins to `<target-dir>/debug/`
/// during `cargo test` at all. So: resolve the uplift path explicitly and
/// run `cargo build -p weft-cli` ourselves when the binary isn't there —
/// that command always uplifts, on every cargo version.
pub fn weft_path() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let target_dir = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root.join("target"));
        let bin = target_dir
            .join("debug")
            .join(format!("weft{}", std::env::consts::EXE_SUFFIX));
        // Always build: `cargo test` compiles the bin's test harness but does
        // not necessarily (re-)uplift the executable, so an existing binary
        // can be stale. The incremental no-op build costs well under a second.
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = std::process::Command::new(cargo)
            .args(["build", "-p", "weft-cli"])
            .current_dir(&workspace_root)
            .status()
            .expect("spawning `cargo build -p weft-cli`");
        assert!(status.success(), "building the weft binary failed");
        assert!(
            bin.exists(),
            "weft binary still not found at {} after `cargo build -p weft-cli`",
            bin.display()
        );
        bin
    })
    .clone()
}

/// An `assert_cmd` command for the `weft` binary.
pub fn weft() -> assert_cmd::Command {
    assert_cmd::Command::new(weft_path())
}
