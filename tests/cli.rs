use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn bin() -> Command {
    Command::cargo_bin("spechurl").expect("spechurl binary")
}

#[test]
fn version_and_help() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("spechurl 0.1.0"));
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("check"));
    bin()
        .args(["generate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--out"));
    bin()
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--dir"));
}

#[test]
fn generate_then_check_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .args([
            "generate",
            "fixtures/petstore.yaml",
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 6 files"));
    assert!(dir.path().join("get_pets.hurl").is_file());
    assert!(dir.path().join("README.md").is_file());

    bin()
        .args([
            "check",
            "fixtures/petstore.yaml",
            "--dir",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));

    fs::write(
        dir.path().join("get_pets.hurl"),
        "GET {{base_url}}/nope\n\nHTTP 200\n",
    )
    .unwrap();
    bin()
        .args([
            "check",
            "fixtures/petstore.yaml",
            "--dir",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("out of sync"));
}

#[test]
fn missing_spec_exits_2() {
    bin()
        .args(["generate", "missing.yaml"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("could not read"));
}
