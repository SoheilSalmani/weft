//! Repeatable includes as a fleet: N connector instances of one child
//! template, managed project-side (`--instance`, `weft instance add/remove`)
//! and all updated by a single `weft update`.

use std::path::{Path, PathBuf};

use weft_e2e::weft;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/templates")
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel))
        .unwrap_or_else(|e| panic!("reading {rel} in scaffold: {e}"))
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

#[test]
fn connector_fleet_lifecycle() {
    // Own copies of the fixtures so the child template can evolve.
    let tpl_root = tempfile::tempdir().unwrap();
    for name in ["workspace", "hello"] {
        copy_dir(&fixtures().join(name), &tpl_root.path().join(name));
    }
    let parent = tpl_root.path().join("workspace");

    let dest = tempfile::tempdir().unwrap();
    let out = dest.path().join("out");

    // 1. Scaffold with two connector instances (declared via --instance and
    //    via an implicit namespaced answer respectively).
    weft()
        .arg("new")
        .arg(&parent)
        .arg(&out)
        .arg("--answer")
        .arg("workspace_name=Acme")
        .arg("--instance")
        .arg("connector=github")
        .arg("--answer")
        .arg("connector.stripe.project_name=Stripe API")
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    // bind seeded github's project_name from the key; stripe's explicit
    // answer overrode the bind
    assert!(read(&out, "connectors/github/README.md").contains("# github"));
    assert!(read(&out, "connectors/stripe/README.md").contains("# Stripe API"));
    // bind forced use_docker=False for connectors
    assert!(!out.join("connectors/github/Dockerfile").exists());
    // the non-repeat include is still there
    assert!(read(&out, "services/hello/README.md").contains("# Acme Service"));
    // the foreach integration patch registered every instance in the parent
    // README, with `key` and the child answer `instance_package_name` in scope
    let readme = read(&out, "README.md");
    assert!(readme.contains("- connector: github (github)"), "{readme}");
    assert!(
        readme.contains("- connector: stripe (stripe-api)"),
        "{readme}"
    );

    // 2. Add a third connector post-scaffold.
    weft()
        .arg("instance")
        .arg("add")
        .arg("connector")
        .arg("jira")
        .arg("--dest")
        .arg(&out)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();
    assert!(read(&out, "connectors/jira/README.md").contains("# jira"));
    // the integration patch re-rendered: jira is registered too
    assert!(read(&out, "README.md").contains("- connector: jira (jira)"));

    // 3. List shows all instances.
    weft()
        .arg("instance")
        .arg("list")
        .arg("--dest")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicates::str::contains("connector=github"))
        .stdout(predicates::str::contains("connector=jira"))
        .stdout(predicates::str::contains("connector=stripe"))
        .stdout(predicates::str::contains("svc=svc"));

    // 4. Evolve the CHILD template once; a single update hits every instance.
    std::fs::write(
        tpl_root.path().join("hello/patches/extra.json"),
        r#"{
          "depends_on": ["base"],
          "ops": [{"op":"create_file","path":"EXTRA.txt","content":["fleet update"]}]
        }"#,
    )
    .unwrap();
    // local edit in one connector must survive
    let jira_readme = out.join("connectors/jira/README.md");
    let mut content = std::fs::read_to_string(&jira_readme).unwrap();
    content.push_str("Local jira note.\n");
    std::fs::write(&jira_readme, content).unwrap();

    weft()
        .arg("update")
        .arg(&out)
        .arg("--skip-tasks")
        .arg("--non-interactive")
        .assert()
        .success();

    for key in ["github", "stripe", "jira"] {
        assert_eq!(
            read(&out, &format!("connectors/{key}/EXTRA.txt")).trim(),
            "fleet update",
            "instance {key} updated"
        );
    }
    assert_eq!(
        read(&out, "services/hello/EXTRA.txt").trim(),
        "fleet update"
    );
    assert!(read(&out, "connectors/jira/README.md").contains("Local jira note."));

    // 5. Remove an instance: untouched files deleted.
    weft()
        .arg("instance")
        .arg("remove")
        .arg("connector")
        .arg("stripe")
        .arg("--dest")
        .arg(&out)
        .arg("--skip-tasks")
        .assert()
        .success();
    assert!(!out.join("connectors/stripe/README.md").exists());
    assert!(out.join("connectors/github/README.md").exists());
    // …and its registry line is gone while the others remain
    let readme = read(&out, "README.md");
    assert!(!readme.contains("stripe"), "{readme}");
    assert!(readme.contains("- connector: github (github)"), "{readme}");

    // state no longer pins stripe
    let state = read(&out, ".weft/state.toml");
    assert!(!state.contains("stripe"));
}

#[test]
fn an_added_instance_runs_its_hooks_as_on_scaffold() {
    let tpl_root = tempfile::tempdir().unwrap();
    for name in ["workspace", "hello"] {
        copy_dir(&fixtures().join(name), &tpl_root.path().join(name));
    }
    let out = tpl_root.path().join("out");
    weft()
        .arg("new")
        .arg(tpl_root.path().join("workspace"))
        .arg(&out)
        .args(["--answer", "workspace_name=Acme"])
        .args(["--instance", "connector=github"])
        .arg("--non-interactive")
        .assert()
        .success();
    std::fs::remove_file(out.join("connectors/github/.deployed")).unwrap();

    weft()
        .args(["instance", "add", "connector", "stripe", "--dest"])
        .arg(&out)
        .arg("--non-interactive")
        .assert()
        .success();

    // The child's `deploy` hook has no inputs: it runs where its instance is
    // created, as `weft new` ran it for github, and nowhere else.
    assert_eq!(read(&out, "connectors/stripe/.deployed").trim(), "deployed");
    assert!(!out.join("connectors/github/.deployed").exists());
}
