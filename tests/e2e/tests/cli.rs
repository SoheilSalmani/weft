use weft_e2e::weft;

#[test]
fn version_prints() {
    weft()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("weft 0.1.0"));
}

#[test]
fn check_outside_template_fails_helpfully() {
    let dir = tempfile::tempdir().unwrap();
    weft()
        .arg("check")
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("weft template"));
}
