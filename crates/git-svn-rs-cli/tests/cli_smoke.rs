use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_core_commands() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("dcommit"))
        .stdout(predicate::str::contains("find-rev"));
}

#[test]
fn diagnose_prints_feature_state() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.arg("diagnose")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-svn-rs diagnostics"))
        .stdout(predicate::str::contains("libsvn feature: disabled"))
        .stdout(predicate::str::contains("libsvn link: not-compiled"));
}

#[cfg(feature = "svn-libsvn")]
#[test]
fn diagnose_prints_enabled_when_libsvn_feature_is_compiled() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.arg("diagnose")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-svn-rs diagnostics"))
        .stdout(predicate::str::contains("libsvn feature: enabled"))
        .stdout(predicate::str::contains("libsvn link: not-linked"));
}

#[test]
fn branch_is_explicitly_unsupported() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.args(["branch", "feature"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported in v1: branch"));
}

#[test]
fn inert_global_output_options_are_explicitly_rejected() {
    for (option, canonical) in [
        ("--quiet", "--quiet"),
        ("-q", "--quiet"),
        ("--verbose", "--verbose"),
        ("-v", "--verbose"),
    ] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .args([option, "diagnose"])
            .assert()
            .failure()
            .stdout("")
            .stderr(predicate::str::contains(format!(
                "global {canonical} is not supported in v1"
            )));
    }
}
