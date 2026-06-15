use assert_cmd::Command;
use predicates::prelude::*;

fn shim() -> Command {
    let _git_svn_rs = Command::cargo_bin("git-svn-rs").unwrap();
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-q", "-p", "git-svn-rs-shim", "--"]);
    cmd
}

#[test]
fn git_svn_shim_forwards_help_to_git_svn_rs() {
    shim()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-svn-rs"))
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("dcommit"));
}

#[test]
fn git_svn_shim_forwards_diagnose_to_git_svn_rs() {
    shim()
        .arg("diagnose")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-svn-rs diagnostics"))
        .stdout(predicate::str::contains("libsvn feature: disabled"));
}
