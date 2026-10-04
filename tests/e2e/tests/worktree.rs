//! Sessions as git-style worktrees: several at once, each with its own stage,
//! discovered by walking up from the current directory; worktrees that live
//! anywhere and can move; adopting a directory you already have; and placing a
//! patch in the graph explicitly.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use weft_e2e::weft;

/// A template with one question, so abstraction has something to find.
fn init_template(dir: &Path) -> PathBuf {
    let tpl = dir.join("t");
    weft()
        .arg("init")
        .arg(&tpl)
        .args(["--name", "t"])
        .assert()
        .success();
    let manifest = tpl.join("weft.toml");
    let mut src = fs::read_to_string(&manifest).unwrap();
    src.push_str("\n[[question]]\nid = \"project_name\"\nkind = \"string\"\nprompt = \"Name\"\n");
    fs::write(&manifest, src).unwrap();
    tpl
}

fn new_session(tpl: &Path, name: &str) -> PathBuf {
    let out = weft()
        .args(["session", "new", name, "--template"])
        .arg(tpl)
        .args(["--answer", "project_name=Demo"])
        .assert()
        .success();
    let path = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    PathBuf::from(path.trim())
}

// ---- discovery -----------------------------------------------------------

#[test]
fn commands_run_from_inside_a_worktree_need_no_flags() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = new_session(&tpl, "init");

    fs::create_dir_all(wt.join("src")).unwrap();
    fs::write(wt.join("src/app.txt"), "hello\n").unwrap();
    fs::write(wt.join("README.md"), "readme\n").unwrap();

    // From a subdirectory, with a path relative to *there* — this is what
    // makes shell completion work.
    weft()
        .args(["add", "app.txt"])
        .current_dir(wt.join("src"))
        .assert()
        .success()
        .stderr(contains("staged 1 path(s)"));

    weft()
        .arg("status")
        .current_dir(&wt)
        .assert()
        .success()
        .stdout(contains("session `init` on `t`"))
        .stdout(contains("src/app.txt"))
        .stdout(contains("README.md"));

    // `.` means "here and below", so it stages the subdirectory only.
    weft().args(["reset"]).current_dir(&wt).assert().success();
    weft()
        .args(["add", "."])
        .current_dir(wt.join("src"))
        .assert()
        .success();
    weft()
        .args(["diff", "--staged"])
        .current_dir(&wt)
        .assert()
        .success()
        .stdout(contains("src/app.txt"))
        .stdout(contains("README.md").not());
}

#[test]
fn two_sessions_keep_separate_stages() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let a = new_session(&tpl, "alpha");
    let b = new_session(&tpl, "beta");

    fs::write(a.join("a.txt"), "a\n").unwrap();
    fs::write(b.join("b.txt"), "b\n").unwrap();

    weft()
        .args(["add", "a.txt"])
        .current_dir(&a)
        .assert()
        .success();

    // Staging in alpha left beta's index alone.
    weft()
        .arg("status")
        .current_dir(&b)
        .assert()
        .success()
        .stdout(contains("no staged changes"))
        .stdout(contains("b.txt"));

    // Both commit independently, into the same template.
    weft()
        .args(["commit", "--name", "from-alpha", "--yes"])
        .current_dir(&a)
        .assert()
        .success();
    weft()
        .args(["commit", "--name", "from-beta", "--yes"])
        .current_dir(&b)
        .assert()
        .success();
    assert!(tpl.join("patches/from-alpha.json").exists());
    assert!(tpl.join("patches/from-beta.json").exists());
}

#[test]
fn the_template_root_never_implies_a_session() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = new_session(&tpl, "alpha");
    // At the template root `README.md` names the template's own README, while
    // weft would read the worktree's.
    fs::write(tpl.join("README.md"), "template readme\n").unwrap();
    fs::write(wt.join("README.md"), "project readme\n").unwrap();

    // A single session is still not picked: the root is not a worktree.
    weft()
        .arg("status")
        .current_dir(&tpl)
        .assert()
        .failure()
        .stderr(contains("alpha"));
    weft()
        .args(["add", "README.md"])
        .current_dir(&tpl)
        .assert()
        .failure();

    // Naming the session works from anywhere, with paths relative to the
    // worktree root (git's --work-tree); the refused add staged nothing.
    weft()
        .args(["status", "--session", "alpha"])
        .current_dir(&tpl)
        .assert()
        .success()
        .stdout(contains("no staged changes"));
    weft()
        .args(["add", "README.md", "--session", "alpha"])
        .current_dir(&tpl)
        .assert()
        .success();
    weft()
        .args(["diff", "--staged", "--session", "alpha"])
        .current_dir(&tpl)
        .assert()
        .success()
        .stdout(contains("project readme"));
}

#[test]
fn a_pattern_that_matches_nothing_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = new_session(&tpl, "alpha");
    fs::write(wt.join("wanted.txt"), "keep\n").unwrap();
    fs::write(wt.join("scratch.txt"), "not this\n").unwrap();

    // A typo beside a real path refuses the whole add: an empty stage would
    // make the next commit take the whole worktree.
    weft()
        .args(["add", "wanted.txt", "wantd.txt"])
        .current_dir(&wt)
        .assert()
        .failure()
        .stderr(contains("wantd.txt"));
    weft()
        .arg("status")
        .current_dir(&wt)
        .assert()
        .success()
        .stdout(contains("no staged changes"));

    // What a shell completes at the template root is not a worktree path.
    weft()
        .args(["add", ".weft-sessions/alpha/worktree/wanted.txt"])
        .args(["--session", "alpha"])
        .current_dir(&tpl)
        .assert()
        .failure();

    // Unstaging a path that matches nothing leaves the stage as it was.
    weft()
        .args(["add", "wanted.txt"])
        .current_dir(&wt)
        .assert()
        .success();
    weft()
        .args(["reset", "wantd.txt"])
        .current_dir(&wt)
        .assert()
        .failure()
        .stderr(contains("wantd.txt"));
    weft()
        .args(["diff", "--staged"])
        .current_dir(&wt)
        .assert()
        .success()
        .stdout(contains("wanted.txt"))
        .stdout(contains("scratch.txt").not());
}

#[test]
fn session_list_marks_the_one_you_are_in() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    new_session(&tpl, "alpha");
    let b = new_session(&tpl, "beta");

    weft()
        .args(["session", "list"])
        .current_dir(&b)
        .assert()
        .success()
        .stdout(contains("* beta"))
        .stdout(contains("  alpha"));
}

// ---- worktrees anywhere ---------------------------------------------------

#[test]
fn a_worktree_can_live_outside_the_template_and_move() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let away = tmp.path().join("away/docker");
    weft()
        .args(["session", "new", "docker", "--template"])
        .arg(&tpl)
        .args(["--answer", "project_name=Demo", "--path"])
        .arg(&away)
        .assert()
        .success();
    assert!(away.join(".weft/worktree.toml").is_file());

    fs::write(away.join("Dockerfile"), "FROM alpine\n").unwrap();
    weft()
        .args(["add", "Dockerfile"])
        .current_dir(&away)
        .assert()
        .success();

    // Move it, then work from the new location.
    let dest = tmp.path().join("moved/dk");
    weft()
        .args(["session", "move", "docker"])
        .arg(&dest)
        .args(["--template"])
        .arg(&tpl)
        .assert()
        .success();
    assert!(!away.exists());
    weft()
        .arg("status")
        .current_dir(&dest)
        .assert()
        .success()
        .stdout(contains("Dockerfile"));
}

#[test]
fn a_worktree_moved_with_mv_repairs_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let away = tmp.path().join("away");
    weft()
        .args(["session", "new", "s", "--template"])
        .arg(&tpl)
        .args(["--answer", "project_name=Demo", "--path"])
        .arg(&away)
        .assert()
        .success();

    let moved = tmp.path().join("moved");
    fs::rename(&away, &moved).unwrap();

    weft()
        .arg("status")
        .current_dir(&moved)
        .assert()
        .success()
        .stderr(contains("updated its record"));
    // The repair stuck: the template now points at the new place.
    weft()
        .args(["session", "path", "s", "--template"])
        .arg(&tpl)
        .assert()
        .success()
        .stdout(contains(moved.to_str().unwrap()));
}

#[test]
fn ending_a_session_removes_the_worktree_weft_created() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let wt = new_session(&tpl, "s");
    weft()
        .args(["session", "end", "s", "--template"])
        .arg(&tpl)
        .assert()
        .success();
    assert!(!wt.exists());
}

// ---- adopting ------------------------------------------------------------

/// A template with one patch, plus a project scaffolded from it.
fn template_and_project(dir: &Path) -> (PathBuf, PathBuf) {
    let tpl = init_template(dir);
    let wt = new_session(&tpl, "base");
    fs::write(wt.join("README.md"), "# Demo\n\nhello\n").unwrap();
    weft()
        .args(["commit", "--name", "readme", "--yes"])
        .current_dir(&wt)
        .assert()
        .success();

    let proj = dir.join("proj");
    weft()
        .arg("new")
        .arg(&tpl)
        .arg(&proj)
        .args(["--answer", "project_name=Trendlift", "--non-interactive"])
        .assert()
        .success();
    (tpl, proj)
}

#[test]
fn a_scaffolded_project_adopts_with_no_extra_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let (tpl, proj) = template_and_project(tmp.path());

    // Write real code in the project, then link it.
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();

    weft()
        .args(["session", "adopt", ".", "-n", "promote"])
        .current_dir(&proj)
        .assert()
        .success();

    // Template, base and answers all came from `.weft/`, so only the edit shows.
    weft()
        .arg("status")
        .current_dir(&proj)
        .assert()
        .success()
        .stdout(contains("base: readme"))
        .stdout(contains("project_name = Trendlift"))
        .stdout(contains("src/main.rs"))
        .stdout(contains("README.md").not());

    // Promote it into the template.
    weft()
        .args(["add", "src/main.rs"])
        .current_dir(&proj)
        .assert()
        .success();
    weft()
        .args(["commit", "--name", "rust-main", "--yes"])
        .current_dir(&proj)
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/rust-main.json")).unwrap();
    assert!(patch.contains("src/main.rs"), "{patch}");
    assert!(
        patch.contains("\"readme\""),
        "depends on the pinned base: {patch}"
    );
}

#[test]
fn committing_never_reverts_an_adopted_projects_files() {
    let tmp = tempfile::tempdir().unwrap();
    let (_tpl, proj) = template_and_project(tmp.path());
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    // A second, uncommitted change so the session stays open after the commit.
    fs::write(proj.join("NOTES.md"), "notes\n").unwrap();
    weft()
        .args(["session", "adopt", ".", "-n", "promote"])
        .current_dir(&proj)
        .assert()
        .success();
    weft()
        .args(["add", "src/main.rs"])
        .current_dir(&proj)
        .assert()
        .success();

    // The sibling transition peels committed content back out of a worktree.
    // On an adopted project that is the author's real code, so it is refused…
    weft()
        .args(["commit", "--name", "p", "--yes", "--sibling"])
        .current_dir(&proj)
        .assert()
        .failure()
        .stderr(contains("adopted project"));

    // …and the default stacks instead, leaving the file on disk.
    weft()
        .args(["commit", "--name", "p", "--yes"])
        .current_dir(&proj)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(proj.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );
}

#[test]
fn ending_an_adopted_session_leaves_the_directory_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let (_tpl, proj) = template_and_project(tmp.path());
    fs::write(proj.join("extra.txt"), "mine\n").unwrap();
    weft()
        .args(["session", "adopt", ".", "-n", "promote"])
        .current_dir(&proj)
        .assert()
        .success();
    weft()
        .args(["session", "end", "promote", "--discard"])
        .current_dir(&proj)
        .assert()
        .success()
        .stderr(contains("left in place"));

    assert!(proj.join("extra.txt").exists());
    assert!(proj.join("README.md").exists());
    assert!(
        !proj.join(".weft/worktree.toml").exists(),
        "only the link is removed"
    );
}

#[test]
fn a_plain_directory_needs_a_template_and_honours_its_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = init_template(tmp.path());
    let plain = tmp.path().join("plain");
    fs::create_dir_all(plain.join("src")).unwrap();
    fs::create_dir_all(plain.join("docker")).unwrap();
    fs::write(plain.join("src/lib.rs"), "unrelated\n").unwrap();
    fs::write(plain.join("docker/Dockerfile"), "FROM alpine\n").unwrap();

    // Without a template there is nothing to diff against.
    weft()
        .args(["session", "adopt", ".", "-n", "dk", "--non-interactive"])
        .current_dir(&plain)
        .assert()
        .failure()
        .stderr(contains("--template"));

    weft()
        .args(["session", "adopt", ".", "-n", "dk", "--template"])
        .arg(&tpl)
        .args([
            "--answer",
            "project_name=Demo",
            "--scope",
            "docker/**",
            "--non-interactive",
        ])
        .current_dir(&plain)
        .assert()
        .success();

    // The rest of the project is invisible, not a wall of "new files".
    weft()
        .arg("status")
        .current_dir(&plain)
        .assert()
        .success()
        .stdout(contains("docker/Dockerfile"))
        .stdout(contains("src/lib.rs").not());

    // Widening the scope brings it into view.
    weft()
        .args(["session", "scope", "--add", "src/**"])
        .current_dir(&plain)
        .assert()
        .success();
    weft()
        .arg("status")
        .current_dir(&plain)
        .assert()
        .success()
        .stdout(contains("src/lib.rs"));
}

#[test]
fn a_scoped_adopt_commits_one_patch_after_another_on_a_template_with_patches() {
    let tmp = tempfile::tempdir().unwrap();
    // The template already renders README.md, which the scope leaves out.
    let (tpl, _proj) = template_and_project(tmp.path());
    let plain = tmp.path().join("plain");
    fs::create_dir_all(plain.join("docker")).unwrap();
    fs::write(plain.join("docker/Dockerfile"), "FROM alpine\n").unwrap();
    fs::write(plain.join("docker/entrypoint.sh"), "exec \"$@\"\n").unwrap();
    weft()
        .args(["session", "adopt", ".", "-n", "dk", "--template"])
        .arg(&tpl)
        .args([
            "--answer",
            "project_name=Demo",
            "--scope",
            "docker/**",
            "--non-interactive",
        ])
        .current_dir(&plain)
        .assert()
        .success();
    weft()
        .args(["add", "docker/Dockerfile"])
        .current_dir(&plain)
        .assert()
        .success();

    weft()
        .args(["commit", "--name", "docker", "--yes"])
        .current_dir(&plain)
        .assert()
        .success()
        .stderr(contains("1 file(s) still uncommitted"));
    let patch = fs::read_to_string(tpl.join("patches/docker.json")).unwrap();
    assert!(patch.contains("docker/Dockerfile"), "{patch}");
    assert!(!patch.contains("README.md"), "{patch}");

    // The session stacked on `docker` and still sees only its scope.
    weft()
        .args(["commit", "--name", "entrypoint", "--yes"])
        .current_dir(&plain)
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/entrypoint.json")).unwrap();
    assert!(patch.contains("docker/entrypoint.sh"), "{patch}");
    assert!(patch.contains("\"docker\""), "stacked on docker: {patch}");
    assert!(!patch.contains("Dockerfile"), "{patch}");
}

// ---- placing the patch ----------------------------------------------------

/// Two independent patches in the base, so a third can choose its parents.
fn two_roots(dir: &Path) -> PathBuf {
    let tpl = init_template(dir);
    let wt = new_session(&tpl, "s");
    fs::write(wt.join("a.txt"), "a\n").unwrap();
    fs::write(wt.join("b.txt"), "b\n").unwrap();
    weft()
        .args(["add", "a.txt"])
        .current_dir(&wt)
        .assert()
        .success();
    weft()
        .args(["commit", "--name", "a", "--yes", "--sibling"])
        .current_dir(&wt)
        .assert()
        .success();
    weft()
        .args(["commit", "--name", "b", "--yes"])
        .current_dir(&wt)
        .assert()
        .success();
    tpl
}

#[test]
fn depends_on_places_the_patch_in_the_graph() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = two_roots(tmp.path());
    let wt = new_session(&tpl, "next");
    fs::write(wt.join("c.txt"), "c\n").unwrap();

    weft()
        .args(["commit", "--name", "c", "--yes", "--depends-on", "a"])
        .current_dir(&wt)
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/c.json")).unwrap();
    assert!(patch.contains("\"a\""), "{patch}");
    assert!(!patch.contains("\"b\""), "only the declared dep: {patch}");
}

#[test]
fn after_is_sugar_for_a_single_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = two_roots(tmp.path());
    let wt = new_session(&tpl, "next");
    fs::write(wt.join("c.txt"), "c\n").unwrap();
    weft()
        .args(["commit", "--name", "c", "--yes", "--after", "b"])
        .current_dir(&wt)
        .assert()
        .success();
    let patch = fs::read_to_string(tpl.join("patches/c.json")).unwrap();
    assert!(patch.contains("\"b\""), "{patch}");
}

#[test]
fn depends_on_rejects_a_patch_outside_the_session_base() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = two_roots(tmp.path());
    let wt = new_session(&tpl, "next");
    fs::write(wt.join("c.txt"), "c\n").unwrap();
    weft()
        .args(["commit", "--name", "c", "--yes", "--depends-on", "nope"])
        .current_dir(&wt)
        .assert()
        .failure()
        .stderr(contains("no patch by that name"));
}

#[test]
fn depends_on_rejects_a_claim_of_independence_it_cannot_honour() {
    let tmp = tempfile::tempdir().unwrap();
    let tpl = two_roots(tmp.path());
    let wt = new_session(&tpl, "next");
    // Edit b.txt, which only exists because of patch `b` — claiming to depend
    // on `a` alone would produce a patch that cannot apply.
    fs::write(wt.join("b.txt"), "b\nmore\n").unwrap();
    weft()
        .args(["commit", "--name", "c", "--yes", "--depends-on", "a"])
        .current_dir(&wt)
        .assert()
        .failure()
        .stderr(contains("does not apply with only"));
}
