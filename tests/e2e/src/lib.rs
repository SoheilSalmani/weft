//! Shared helpers for the e2e integration tests.

use std::path::PathBuf;

/// Path to the `weft` binary built by `cargo test --workspace`.
///
/// `assert_cmd`'s `cargo_bin` heuristic derives the artifact dir from the
/// test executable's location, which breaks on cargo ≥1.91's separated
/// build-dir layout (and `CARGO_BIN_EXE_weft` is only set for the package
/// that owns the binary, which this one doesn't). Final binaries are always
/// uplifted to `<target-dir>/debug/`, so resolve that explicitly.
pub fn weft_path() -> PathBuf {
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target")
        });
    let bin = target_dir
        .join("debug")
        .join(format!("weft{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "weft binary not found at {}; run the e2e tests via `cargo test --workspace` \
         so weft-cli is built first",
        bin.display()
    );
    bin
}

/// An `assert_cmd` command for the `weft` binary.
pub fn weft() -> assert_cmd::Command {
    assert_cmd::Command::new(weft_path())
}
