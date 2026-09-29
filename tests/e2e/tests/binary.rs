//! Binary file support: opaque blobs (favicons, images) flow through
//! record --exec, commit (create_binary_file ops), render, resync, diff,
//! and update's 3-way merge — byte-exact, never abstracted.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

const V1: &[u8] = &[0xff, 0xfe, 0x00, 0x89, 0x50, 0x4e];
const V2: &[u8] = &[0x00, 0x01, 0xff, 0xd8, 0xff];

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

/// Record a generated patch whose command copies `src` to `app/favicon.ico`
/// and writes one text file alongside.
fn record_icon_patch(tpl: &Path, src: &Path) {
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .arg("--exec")
        .arg(format!(
            "mkdir -p app && cp '{}' app/favicon.ico && echo 'page for My Demo' > app/page.tsx",
            src.display()
        ))
        .assert()
        .success();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(tpl)
        .arg("--name")
        .arg("shadcn")
        .arg("--yes")
        .assert()
        .success();
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    let src = dir.path().join("upstream-favicon.ico");
    std::fs::write(&src, V1).unwrap();
    record_icon_patch(&tpl, &src);
    (dir, tpl, src)
}

#[test]
fn binary_round_trips_byte_exact() {
    let (_guard, tpl, _src) = setup();

    let patch = read(&tpl, "patches/shadcn.json");
    assert!(patch.contains("\"create_binary_file\""), "patch: {patch}");
    assert!(patch.contains("\"data\""), "patch: {patch}");
    // The text sibling is still a normal abstracted create.
    assert!(
        patch.contains("\"answer\": \"project_name\""),
        "patch: {patch}"
    );

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=Other App")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        std::fs::read(dest_path.join("app/favicon.ico")).unwrap(),
        V1,
        "bytes reproduced exactly"
    );
    assert_eq!(
        read(&dest_path, "app/page.tsx"),
        "page for Other App\n",
        "text sibling abstracts as usual"
    );

    weft()
        .arg("check")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .assert()
        .success()
        .stdout(predicates::str::contains("passed all checks"));
}

#[test]
fn diff_shows_binary_opaque() {
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("hello");
    copy_dir(&hello_template(), &tpl);
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--exec")
        .arg("printf '\\377\\376\\000' > icon.bin")
        .assert()
        .success();
    weft()
        .arg("diff")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stdout(predicates::str::contains("icon.bin"))
        .stdout(predicates::str::contains("(binary file)"));
}

#[test]
fn resync_regenerates_binary() {
    let (_guard, tpl, src) = setup();

    weft()
        .arg("patch")
        .arg("resync")
        .arg("shadcn")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("up to date"));

    std::fs::write(&src, V2).unwrap();
    weft()
        .arg("patch")
        .arg("resync")
        .arg("shadcn")
        .arg("--template")
        .arg(&tpl)
        .assert()
        .success()
        .stderr(predicates::str::contains("rewritten"));

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        std::fs::read(dest_path.join("app/favicon.ico")).unwrap(),
        V2
    );
}

#[test]
fn modifying_a_binary_records_delete_plus_create() {
    let (_guard, tpl, _src) = setup();

    // Hand-record on top: replace the icon's bytes in the worktree.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success();
    std::fs::write(tpl.join(".weft-sessions/main/worktree/app/favicon.ico"), V2).unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(&tpl)
        .arg("--name")
        .arg("new-icon")
        .arg("--yes")
        .assert()
        .success();

    let patch = read(&tpl, "patches/new-icon.json");
    assert!(patch.contains("\"delete_file\""), "patch: {patch}");
    assert!(patch.contains("\"create_binary_file\""), "patch: {patch}");

    let dest = tempfile::tempdir().unwrap();
    let dest_path = dest.path().join("out");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest_path)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(
        std::fs::read(dest_path.join("app/favicon.ico")).unwrap(),
        V2
    );
}

/// Overwrite the worktree favicon with `bytes` and commit it as a new
/// patch — the template gains a binary change while existing pins survive
/// (so a pre-scaffolded project can still `update`; resync, by contrast,
/// rewrites ids and is not update-compatible).
fn add_icon_patch(tpl: &Path, name: &str, bytes: &[u8]) {
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(tpl)
        .arg("--answer")
        .arg("project_name=My Demo")
        .assert()
        .success();
    std::fs::write(
        tpl.join(".weft-sessions/main/worktree/app/favicon.ico"),
        bytes,
    )
    .unwrap();
    weft()
        .arg("commit")
        .args(["--session", "main"])
        .arg("--template")
        .arg(tpl)
        .arg("--name")
        .arg(name)
        .arg("--yes")
        .assert()
        .success();
}

#[test]
fn update_carries_binary_change_when_user_untouched() {
    let (dir, tpl, _src) = setup();
    let dest = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();

    // The template gains a patch replacing the icon; the project didn't
    // touch it → the new bytes land wholesale.
    add_icon_patch(&tpl, "refresh-icon", V2);
    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();
    assert_eq!(std::fs::read(dest.join("app/favicon.ico")).unwrap(), V2);
}

#[test]
fn update_keeps_user_binary_edit_with_note() {
    let (dir, tpl, _src) = setup();
    let dest = dir.path().join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&dest)
        .arg("--answer")
        .arg("project_name=x")
        .arg("--non-interactive")
        .assert()
        .success();

    // Both sides diverge: user edits the icon, template also changes it.
    let user_bytes: &[u8] = &[0xde, 0xad, 0xbe, 0xef];
    std::fs::write(dest.join("app/favicon.ico"), user_bytes).unwrap();
    add_icon_patch(&tpl, "refresh-icon", V2);
    weft()
        .arg("update")
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success()
        .stderr(predicates::str::contains("kept your version"));
    assert_eq!(
        std::fs::read(dest.join("app/favicon.ico")).unwrap(),
        user_bytes
    );
}
