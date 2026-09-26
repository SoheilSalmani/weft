//! The composed patch graph: `extends` imports a base template as-is, a
//! single include's patches are nodes the parent may depend on (and edit
//! the files of), and commit infers those dependencies.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates")
}

/// Copy the templates the monorepo/nextjs-repo fixtures reach through
/// `extends`/`include` into one tempdir (relative refs must resolve).
fn stack(dir: &Path) -> PathBuf {
    for name in ["base", "nextjs-app", "monorepo", "nextjs-repo"] {
        copy_dir(&fixtures().join(name), &dir.join(name));
    }
    dir.join("monorepo")
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

fn scaffold(template: &Path, dest: &Path, answers: &[&str]) {
    let mut cmd = weft();
    cmd.arg("new").arg(template).arg(dest);
    for a in answers {
        cmd.arg("--answer").arg(a);
    }
    cmd.arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
}

fn graph(template: &Path) -> serde_json::Value {
    let out = weft()
        .arg("graph")
        .arg(template)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap()
}

fn node<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == name)
        .unwrap_or_else(|| panic!("no node `{name}` in {doc}"))
}

#[test]
fn extends_imports_the_base_unqualified_with_identical_ids() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());
    let out = dir.path().join("out");
    scaffold(&monorepo, &out, &["workspace=acme", "use_linear=true"]);

    // The base's questions are asked by name and its patches render at the
    // root, gated by the base's own `when`.
    assert!(read(&out, "AGENTS.md").contains("- linear: link the issue in every PR."));
    assert_eq!(read(&out, ".gitignore"), "node_modules/\n.next/\n");
    assert_eq!(read(&out, "pnpm-workspace.yaml"), "packages:\n  - apps/*\n");

    // Same names, same ids as in the base itself: projects scaffolded from
    // the base see no rewrite.
    let base = graph(&dir.path().join("base"));
    let mono = graph(&monorepo);
    for name in ["skills", "linear", "gitignore"] {
        assert_eq!(node(&base, name)["id"], node(&mono, name)["id"], "{name}");
    }
    assert_eq!(mono["extends"], "../base");

    let off = dir.path().join("off");
    scaffold(&monorepo, &off, &["workspace=acme"]);
    assert!(!read(&off, "AGENTS.md").contains("linear"));
}

#[test]
fn include_nodes_render_in_their_frame_and_inherit_gates() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());

    // `glue` depends on `web/tailwind`: its hunk lands in the child's file,
    // after the child's own patches, with the parent's answers in scope.
    let out = dir.path().join("out");
    scaffold(&monorepo, &out, &["workspace=acme"]);
    assert_eq!(
        read(&out, "apps/web/next.config.ts"),
        "const config = {\n  reactStrictMode: true,\n  // tailwind: see tailwind.config.ts\n  \
         // workspace: acme\n};\nexport default config;\n"
    );
    assert!(read(&out, "apps/web/package.json").contains("\"name\": \"acme-web\""));

    // Gate inheritance: with Tailwind off the child patch is skipped, so the
    // parent hunk anchored on its line is off too — no render failure.
    let plain = dir.path().join("plain");
    scaffold(
        &monorepo,
        &plain,
        &["workspace=acme", "web.use_tailwind=false"],
    );
    assert_eq!(
        read(&plain, "apps/web/next.config.ts"),
        "const config = {\n  reactStrictMode: true,\n};\nexport default config;\n"
    );
    assert!(!plain.join("apps/web/tailwind.config.ts").exists());

    // The graph shows the child's nodes under the include, keyed by it.
    let mono = graph(&monorepo);
    let app = node(&mono, "web/app");
    assert_eq!(app["include"], "web");
    assert_eq!(app["mount"], "apps/web");
    let native = graph(&dir.path().join("nextjs-app"));
    let app_native = node(&native, "app");
    assert_ne!(
        app["id"], app_native["id"],
        "node identity is keyed by the include"
    );
    let edges = mono["edges"].as_array().unwrap();
    let glue = node(&mono, "glue");
    let tailwind = node(&mono, "web/tailwind");
    assert!(edges
        .iter()
        .any(|e| e["source"] == tailwind["id"] && e["target"] == glue["id"]));
}

#[test]
fn root_mounted_include_merges_with_the_base() {
    let dir = tempfile::tempdir().unwrap();
    stack(dir.path());
    let repo = dir.path().join("nextjs-repo");
    let out = dir.path().join("out");
    scaffold(&repo, &out, &[]);
    assert_eq!(read(&out, ".gitignore"), "node_modules/\n.next/\n");
    assert_eq!(
        read(&out, "next.config.ts"),
        "const config = {\n  // single-app repository\n  reactStrictMode: true,\n  \
         // tailwind: see tailwind.config.ts\n};\nexport default config;\n"
    );
    assert!(read(&out, "package.json").contains("\"name\": \"site\""));
    assert!(out.join("AGENTS.md").is_file());
    weft()
        .arg("check")
        .arg(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("passed all checks"));
}

#[test]
fn commit_infers_include_dependencies_from_edited_paths() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());

    // The session base is the composed render: the child's files are there
    // to anchor on.
    weft()
        .args(["session", "new", "main"])
        .arg("--template")
        .arg(&monorepo)
        .arg("--answer")
        .arg("workspace=acme")
        .assert()
        .success();
    let worktree = monorepo.join(".weft-sessions/main/worktree");
    assert!(worktree.join("apps/web/package.json").is_file());
    assert!(worktree.join("AGENTS.md").is_file());

    // Edit a file the child owns (plus a root file): the dependency on the
    // owning child node is inferred and proven by the closure replay.
    let pkg = worktree.join("apps/web/package.json");
    let edited = read(&worktree, "apps/web/package.json").replace(
        "\"private\": true",
        "\"private\": true,\n  \"workspaces\": [\"../*\"]",
    );
    std::fs::write(&pkg, edited).unwrap();
    std::fs::write(worktree.join("turbo.json"), "{ \"pipeline\": {} }\n").unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&monorepo)
        .arg("--name")
        .arg("turbo")
        .arg("--yes")
        .assert()
        .success()
        .stderr(predicates::str::contains("depending on web/app"));
    let patch: serde_json::Value =
        serde_json::from_str(&read(&monorepo, "patches/turbo.json")).unwrap();
    let deps: Vec<&str> = patch["depends_on"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert!(deps.contains(&"web/app"), "{deps:?}");
    assert!(deps.contains(&"glue"), "root leaves stay: {deps:?}");
    assert!(!deps.contains(&"web/tailwind"), "only the owner: {deps:?}");

    // The template still checks, and the patch renders in the child's frame.
    weft()
        .arg("check")
        .arg(&monorepo)
        .arg("--answer")
        .arg("workspace=x")
        .assert()
        .success()
        .stdout(predicates::str::contains("passed all checks"));
    let out = dir.path().join("out");
    scaffold(&monorepo, &out, &["workspace=beta"]);
    assert!(read(&out, "apps/web/package.json").contains("\"workspaces\": [\"../*\"]"));
    assert!(out.join("turbo.json").is_file());
}

#[test]
fn check_flags_a_mount_edit_without_a_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());
    std::fs::write(
        monorepo.join("patches/smuggle.json"),
        r#"{ "ops": [ { "op": "create_file", "path": "apps/web/.env.example", "content": ["X=1"] } ] }"#,
    )
    .unwrap();
    weft()
        .arg("check")
        .arg(&monorepo)
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "`apps/web/.env.example` is under include `web`'s mount but the patch does not depend",
        ));
    std::fs::write(
        monorepo.join("patches/smuggle.json"),
        r#"{ "depends_on": ["web/app"], "ops": [ { "op": "create_file", "path": "apps/web/.env.example", "content": ["X=1"] } ] }"#,
    )
    .unwrap();
    weft()
        .arg("check")
        .arg(&monorepo)
        .arg("--answer")
        .arg("workspace=x")
        .assert()
        .success();
}

#[test]
fn update_reconstructs_amended_child_bodies_from_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());
    let app = dir.path().join("nextjs-app");
    let proj = dir.path().join("proj");
    scaffold(&monorepo, &proj, &["workspace=acme"]);
    let snapshot = read(&proj, ".weft/base.json");
    assert!(snapshot.contains("\"instances\""), "{snapshot}");

    // Amend the child's `app` patch: every id downstream of it moves, and the
    // monorepo's `glue` (via web/tailwind) with it.
    weft()
        .args(["patch", "amend", "app"])
        .arg("--template")
        .arg(&app)
        .arg("--answer")
        .arg("app_name=demo")
        .arg("--non-interactive")
        .assert()
        .success();
    let wt = app.join(".weft-sessions/app/worktree");
    let cfg = read(&wt, "next.config.ts").replace(
        "export default config;",
        "export default config; // amended",
    );
    std::fs::write(wt.join("next.config.ts"), cfg).unwrap();
    weft()
        .arg("commit")
        .arg("--template")
        .arg(&app)
        .arg("--yes")
        .assert()
        .success();

    // The project's old side is rebuilt from the stored child bodies, so the
    // update adopts the amend instead of bailing on a vanished pinned id.
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&monorepo)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success();
    let cfg = read(&proj, "apps/web/next.config.ts");
    assert!(cfg.contains("export default config; // amended"), "{cfg}");
    assert!(cfg.contains("// workspace: acme"), "{cfg}");
    weft()
        .arg("update")
        .arg(&proj)
        .arg("--template")
        .arg(&monorepo)
        .arg("--non-interactive")
        .arg("--skip-tasks")
        .assert()
        .success()
        .stderr(predicates::str::contains("0 file(s) written"));
}

#[test]
fn hooks_run_in_their_frame_and_after_crosses_the_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let monorepo = stack(dir.path());
    let out = dir.path().join("out");
    weft()
        .arg("new")
        .arg(&monorepo)
        .arg(&out)
        .arg("--answer")
        .arg("workspace=acme")
        .arg("--non-interactive")
        .assert()
        .success();
    // The child's post-hook ran inside its mount; the parent's ran after it
    // (`after: ["web/install"]`) at the root.
    assert_eq!(read(&out, "apps/web/installed.txt"), "child\n");
    assert_eq!(read(&out, "order.txt"), "child\nparent\n");
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
