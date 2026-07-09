use assert_cmd::Command;

fn weft() -> Command {
    Command::cargo_bin("weft").expect("weft binary built by workspace")
}

#[test]
fn version_prints() {
    weft()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("weft 0.1.0"));
}

#[test]
fn check_stub_runs() {
    weft().arg("check").assert().success();
}
