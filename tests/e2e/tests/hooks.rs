//! Hooks: a failing pre-hook aborts before any file is written; passing
//! pre-hooks let the render proceed and post-hooks run in `after` / `before`
//! order.

use std::path::Path;

use weft_e2e::weft;

fn write_template(dir: &Path, base_json: &str) {
    std::fs::create_dir_all(dir.join("patches")).unwrap();
    std::fs::write(
        dir.join("weft.toml"),
        "[template]\nname = \"hooktmpl\"\nweft-version = \"0.1\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("patches/base.json"), base_json).unwrap();
}

#[test]
fn failing_pre_hook_aborts_before_writing() {
    let tmpl = tempfile::tempdir().unwrap();
    write_template(
        tmpl.path(),
        r#"{
          "ops": [{"op":"create_file","path":"file.txt","content":["hi"]}],
          "hooks": [
            {"id":"guard","phase":"pre","effect":"check","label":"Failing guard","action":"false"},
            {"id":"post","phase":"post","effect":"setup","label":"Marker","action":"echo ran > marker.txt"}
          ]
        }"#,
    );
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("scaffold");

    weft()
        .arg("new")
        .arg(tmpl.path())
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(predicates::str::contains("Failing guard"));

    // Nothing was written: neither the rendered file nor the post-hook marker.
    assert!(!dest.join("file.txt").exists());
    assert!(!dest.join("marker.txt").exists());
}

#[test]
fn post_hooks_run_in_after_order() {
    let tmpl = tempfile::tempdir().unwrap();
    let order = tempfile::tempdir().unwrap();
    let order_file = order.path().join("order.txt");
    let base = format!(
        r#"{{
          "ops": [{{"op":"create_file","path":"file.txt","content":["hi"]}}],
          "hooks": [
            {{"id":"guard","phase":"pre","effect":"check","label":"ok","action":"true"}},
            {{"id":"a","phase":"post","effect":"setup","label":"a","action":"echo a >> {p}"}},
            {{"id":"b","phase":"post","effect":"setup","label":"b","action":"echo b >> {p}","after":["c"]}},
            {{"id":"c","phase":"post","effect":"setup","label":"c","action":"echo c >> {p}","after":["a"]}}
          ]
        }}"#,
        p = order_file.display()
    );
    write_template(tmpl.path(), &base);
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("scaffold");

    weft()
        .arg("new")
        .arg(tmpl.path())
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();

    assert!(dest.join("file.txt").exists());
    // declaration order is a,b,c; `after` forces a -> c -> b.
    let ran = std::fs::read_to_string(&order_file).unwrap();
    assert_eq!(ran.split_whitespace().collect::<Vec<_>>(), ["a", "c", "b"]);
}

#[test]
fn extender_hook_runs_before_an_inherited_hook() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base");
    write_template(
        &base,
        r#"{
          "ops": [{"op":"create_file","path":"file.txt","content":["hi"]}],
          "hooks": [
            {"id":"commit","phase":"post","effect":"setup","label":"commit","action":"echo commit >> order.txt"}
          ]
        }"#,
    );
    let stack = dir.path().join("stack");
    std::fs::create_dir_all(stack.join("patches")).unwrap();
    std::fs::write(
        stack.join("weft.toml"),
        "[template]\nname = \"stack\"\nweft-version = \"0.1\"\nextends = \"../base\"\n",
    )
    .unwrap();
    std::fs::write(
        stack.join("patches/stack.json"),
        r#"{
          "ops": [{"op":"create_file","path":"stack.txt","content":["s"]}],
          "hooks": [
            {"id":"sync","phase":"post","effect":"setup","label":"sync","action":"echo sync >> order.txt","before":["commit"]},
            {"id":"deps","phase":"post","effect":"setup","label":"deps","action":"echo deps >> order.txt","after":["sync"],"before":["commit"]}
          ]
        }"#,
    )
    .unwrap();
    let dest = dir.path().join("out");

    weft()
        .arg("new")
        .arg(&stack)
        .arg(&dest)
        .arg("--non-interactive")
        .assert()
        .success();

    // The base declares (and renders) `commit` first; `before` moves the
    // extender's setup hooks ahead of it.
    let ran = std::fs::read_to_string(dest.join("order.txt")).unwrap();
    assert_eq!(ran, "sync\ndeps\ncommit\n");
}
