//! Git-sourced templates (`<repo>[//<subdir>][@<rev>]`): scaffold from a
//! subdirectory of a repository of templates, pin the commit in state, follow
//! a branch or stay on a tag with `weft update`, retarget with `--to`, serve
//! pinned commits from the cache, and pin git includes in `weft.lock`.
//! Everything runs against local `file://` repositories.

use std::path::{Path, PathBuf};
use std::process::Command;

use weft_e2e::weft;

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "weft")
        .env("GIT_AUTHOR_EMAIL", "weft@example.com")
        .env("GIT_COMMITTER_NAME", "weft")
        .env("GIT_COMMITTER_EMAIL", "weft@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("git on PATH");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// A repository of templates: `templates/hello` (the fixture) and
/// `templates/platform`, which composes `../hello` by relative path.
/// Committed on `main` and tagged `v1`. Returns (repo dir, `file://` URL).
fn templates_repo(work: &Path) -> (PathBuf, String) {
    let repo = work.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates/hello");
    copy_dir(&fixture, &repo.join("templates/hello"));
    let platform = repo.join("templates/platform");
    std::fs::create_dir_all(platform.join("patches")).unwrap();
    std::fs::write(
        platform.join("weft.toml"),
        r#"[template]
name = "platform"
weft-version = "0.1"

[[question]]
id = "platform_name"
kind = "string"

[[include]]
name = "svc"
template = "../hello"
path = "services/hello"

[include.bind]
project_name = "platform_name"
use_docker = "False"
"#,
    )
    .unwrap();
    std::fs::write(
        platform.join("patches/base.json"),
        r##"{"ops":[{"op":"create_file","path":"PLATFORM.md","content":[["# ",{"answer":"platform_name"}]]}]}"##,
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "v1"]);
    git(&repo, &["tag", "v1"]);
    let url = format!("file://{}", repo.display());
    (repo, url)
}

/// Commit a patch adding `EXTRA.md` to `templates/hello` and tag it `v2`.
fn add_extra_patch(repo: &Path) -> String {
    std::fs::write(
        repo.join("templates/hello/patches/extra.json"),
        r##"{"depends_on":["base"],"ops":[{"op":"create_file","path":"EXTRA.md","content":[["extra"]]}]}"##,
    )
    .unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "v2"]);
    git(repo, &["tag", "v2"]);
    git(repo, &["rev-parse", "HEAD"])
}

fn state(dest: &Path) -> toml::Value {
    std::fs::read_to_string(dest.join(".weft/state.toml"))
        .unwrap()
        .parse()
        .unwrap()
}

fn new(home: &Path, spec: &str, dest: &Path, answer: &str) -> assert_cmd::assert::Assert {
    weft()
        .env("HOME", home)
        .arg("new")
        .arg(spec)
        .arg(dest)
        .arg("--answer")
        .arg(answer)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
}

fn update(home: &Path, dest: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    weft()
        .env("HOME", home)
        .arg("update")
        .arg(dest)
        .args(extra)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
}

#[test]
fn scaffolds_a_subdirectory_at_a_tag_and_pins_the_commit() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    let v1 = git(&repo, &["rev-parse", "v1"]);
    let out = work.path().join("out");

    new(
        home.path(),
        &format!("{url}//templates/hello@v1"),
        &out,
        "project_name=Git Demo",
    )
    .success();

    assert!(std::fs::read_to_string(out.join("README.md"))
        .unwrap()
        .contains("# Git Demo"));
    let st = state(&out);
    // The tracked ref is stored verbatim (no cache path), plus the commit.
    assert_eq!(
        st["state"]["template"].as_str(),
        Some(format!("{url}//templates/hello@v1").as_str())
    );
    assert_eq!(st["state"]["commit"].as_str(), Some(v1.as_str()));
    // The whole tree is exported once per commit.
    let checkouts = home.path().join(".weft/git/checkouts");
    let key = std::fs::read_dir(&checkouts)
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert!(key
        .path()
        .join(&v1)
        .join("templates/platform/weft.toml")
        .is_file());
}

#[test]
fn update_follows_a_tracked_branch_and_keeps_the_ref() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    let spec = format!("{url}//templates/hello");
    let out = work.path().join("out");
    new(home.path(), &spec, &out, "project_name=Track").success();

    let v2 = add_extra_patch(&repo);
    update(home.path(), &out, &[]).success();

    assert!(out.join("EXTRA.md").is_file());
    let st = state(&out);
    assert_eq!(st["state"]["template"].as_str(), Some(spec.as_str()));
    assert_eq!(st["state"]["commit"].as_str(), Some(v2.as_str()));

    // Nothing new → no-op, and it says so.
    update(home.path(), &out, &[])
        .success()
        .stderr(predicates::str::contains("up to date"));

    // Same commit, new answers: still a re-render, not "up to date".
    update(home.path(), &out, &["--answer", "project_name=Retracked"])
        .success()
        .stderr(predicates::str::contains("\"Track\" → \"Retracked\""));
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(readme.starts_with("# Retracked"), "{readme}");
}

#[test]
fn a_tag_stays_put_until_retargeted_with_to() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    let out = work.path().join("out");
    new(
        home.path(),
        &format!("{url}//templates/hello@v1"),
        &out,
        "project_name=Tagged",
    )
    .success();

    let v2 = add_extra_patch(&repo);
    update(home.path(), &out, &[])
        .success()
        .stderr(predicates::str::contains("up to date"));
    assert!(!out.join("EXTRA.md").exists());

    update(home.path(), &out, &["--to", "v2"]).success();
    assert!(out.join("EXTRA.md").is_file());
    let st = state(&out);
    assert_eq!(
        st["state"]["template"].as_str(),
        Some(format!("{url}//templates/hello@v2").as_str())
    );
    assert_eq!(st["state"]["commit"].as_str(), Some(v2.as_str()));
}

#[test]
fn a_subdirectory_template_resolves_sibling_path_includes() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (_repo, url) = templates_repo(work.path());
    let out = work.path().join("out");
    new(
        home.path(),
        &format!("{url}//templates/platform@v1"),
        &out,
        "platform_name=Mega",
    )
    .success();
    assert!(std::fs::read_to_string(out.join("PLATFORM.md"))
        .unwrap()
        .contains("# Mega"));
    assert!(
        std::fs::read_to_string(out.join("services/hello/README.md"))
            .unwrap()
            .contains("# Mega")
    );
}

#[test]
fn a_wrong_subdirectory_lists_the_templates_in_the_repository() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (_repo, url) = templates_repo(work.path());
    new(
        home.path(),
        &format!("{url}//nope"),
        &work.path().join("out"),
        "project_name=X",
    )
    .failure()
    .stderr(predicates::str::contains(
        "templates/hello, templates/platform",
    ));
}

#[test]
fn a_pinned_commit_scaffolds_from_the_cache_without_the_remote() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    let v1 = git(&repo, &["rev-parse", "v1"]);
    new(
        home.path(),
        &format!("{url}//templates/hello@v1"),
        &work.path().join("one"),
        "project_name=One",
    )
    .success();

    // The remote disappears; the exported commit is still there.
    std::fs::rename(&repo, work.path().join("gone")).unwrap();
    new(
        home.path(),
        &format!("{url}//templates/hello@{v1}"),
        &work.path().join("two"),
        "project_name=Two",
    )
    .success();
    assert!(work.path().join("two/README.md").is_file());

    // A branch needs the remote — unless --offline, which uses the mirror.
    let three = work.path().join("three");
    new(
        home.path(),
        &format!("{url}//templates/hello"),
        &three,
        "project_name=Three",
    )
    .failure();
    weft()
        .env("HOME", home.path())
        .arg("new")
        .arg(format!("{url}//templates/hello"))
        .arg(&three)
        .arg("--answer")
        .arg("project_name=Three")
        .arg("--offline")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
}

/// A local parent composing the hello template straight from the repository.
fn write_git_parent(dir: &Path, url: &str, rev: &str) {
    std::fs::create_dir_all(dir.join("patches")).unwrap();
    std::fs::write(
        dir.join("weft.toml"),
        format!(
            r#"[template]
name = "platform"
weft-version = "0.1"

[[question]]
id = "platform_name"
kind = "string"

[[include]]
name = "svc"
template = "{url}//templates/hello@{rev}"
path = "services/hello"

[include.bind]
project_name = "platform_name"
use_docker = "False"
"#
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("patches/base.json"),
        r##"{"ops":[{"op":"create_file","path":"PLATFORM.md","content":[["# ",{"answer":"platform_name"}]]}]}"##,
    )
    .unwrap();
}

#[test]
fn git_includes_pin_a_commit_in_the_lock() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    let v1 = git(&repo, &["rev-parse", "v1"]);
    let parent = work.path().join("platform");
    write_git_parent(&parent, &url, "v1");

    // No lock yet + --frozen → refuse.
    weft()
        .env("HOME", home.path())
        .arg("new")
        .arg(&parent)
        .arg(work.path().join("out"))
        .arg("--answer")
        .arg("platform_name=Mega")
        .arg("--frozen")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("weft.lock"));

    weft()
        .env("HOME", home.path())
        .arg("lock")
        .arg(&parent)
        .assert()
        .success();
    let lock: toml::Value = std::fs::read_to_string(parent.join("weft.lock"))
        .unwrap()
        .parse()
        .unwrap();
    let entry = &lock["include"].as_array().unwrap()[0];
    assert_eq!(
        entry["ref"].as_str(),
        Some(format!("{url}//templates/hello").as_str())
    );
    assert_eq!(entry["req"].as_str(), Some("v1"));
    assert_eq!(entry["commit"].as_str(), Some(v1.as_str()));

    // The tag moves on upstream; the lock keeps the child at v1's commit.
    add_extra_patch(&repo);
    let out = work.path().join("out");
    new(
        home.path(),
        parent.to_str().unwrap(),
        &out,
        "platform_name=Mega",
    )
    .success();
    assert!(out.join("services/hello/README.md").is_file());
    assert!(!out.join("services/hello/EXTRA.md").exists());

    // `version` is for hub includes; git pins with `@rev`.
    let bad = work.path().join("bad");
    write_git_parent(&bad, &url, "v1");
    let manifest = std::fs::read_to_string(bad.join("weft.toml")).unwrap();
    std::fs::write(
        bad.join("weft.toml"),
        manifest.replace(
            "path = \"services/hello\"",
            "version = \"^1\"\npath = \"services/hello\"",
        ),
    )
    .unwrap();
    weft()
        .env("HOME", home.path())
        .arg("check")
        .arg(&bad)
        .arg("--answer")
        .arg("platform_name=x")
        .assert()
        .failure()
        .stderr(predicates::str::contains("@rev"));
}

/// A template extending a git-sourced base: every command that loads the
/// template resolves the remote base — not only `new`/`update`/`session`
/// (authoring commands used to load local paths only and failed with "a
/// remote template").
#[test]
fn authoring_commands_resolve_a_git_extends() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (_repo, url) = templates_repo(work.path());
    let child = work.path().join("child");
    std::fs::create_dir_all(child.join("patches")).unwrap();
    std::fs::write(
        child.join("weft.toml"),
        format!(
            "[template]\nname = \"child\"\nweft-version = \"0.1\"\n\
             extends = \"{url}//templates/hello@v1\"\n"
        ),
    )
    .unwrap();
    std::fs::write(
        child.join("patches/notes.json"),
        r#"{"depends_on":["base"],"ops":[{"op":"create_file","path":"NOTES.md","content":["notes"]}]}"#,
    )
    .unwrap();
    let run = |args: &[&str]| {
        weft()
            .env("HOME", home.path())
            .args(args)
            .arg("--template")
            .arg(&child)
            .assert()
    };

    let listed = run(&["patch", "ls"]).success();
    let stdout = String::from_utf8_lossy(&listed.get_output().stdout).into_owned();
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("base ") && l.contains("[inherited]")),
        "{stdout}"
    );
    assert!(stdout.lines().any(|l| l.starts_with("notes ")), "{stdout}");
    run(&["hook", "ls"])
        .success()
        .stdout(predicates::str::contains("mark-synced"));
    run(&["patch", "set", "notes", "--title", "Notes"]).success();
    assert!(std::fs::read_to_string(child.join("patches/notes.json"))
        .unwrap()
        .contains("\"title\": \"Notes\""));
}

#[test]
fn a_fetched_template_must_ship_its_own_lock() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, url) = templates_repo(work.path());
    // A composed template inside the repository, without a lock.
    write_git_parent(&repo.join("templates/composed"), &url, "v1");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "composed"]);

    new(
        home.path(),
        &format!("{url}//templates/composed"),
        &work.path().join("out"),
        "platform_name=Mega",
    )
    .failure()
    .stderr(predicates::str::contains(
        "run `weft lock` and commit weft.lock",
    ));

    // With the lock committed, it composes.
    weft()
        .env("HOME", home.path())
        .arg("lock")
        .arg(repo.join("templates/composed"))
        .assert()
        .success();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "lock"]);
    let out = work.path().join("out");
    new(
        home.path(),
        &format!("{url}//templates/composed"),
        &out,
        "platform_name=Mega",
    )
    .success();
    assert!(out.join("services/hello/README.md").is_file());
}

#[test]
fn adopting_a_git_sourced_project_needs_a_local_clone() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (_repo, url) = templates_repo(work.path());
    let out = work.path().join("out");
    new(
        home.path(),
        &format!("{url}//templates/hello@v1"),
        &out,
        "project_name=Adopt",
    )
    .success();
    weft()
        .env("HOME", home.path())
        .arg("session")
        .arg("adopt")
        .arg(&out)
        .arg("--name")
        .arg("fix")
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("--template DIR"));
}
