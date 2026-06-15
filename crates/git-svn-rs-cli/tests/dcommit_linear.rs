use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn dcommit_dry_run_lists_local_commits_after_tracked_svn_ref() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    make_commit(&work, "local-one.txt", "one\n", "local one");
    make_commit(&work, "local-two.txt", "two\n", "local two");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dcommit dry-run"))
        .stdout(predicate::str::contains("mock://repo/trunk"))
        .stdout(predicate::str::contains("refs/remotes/git-svn"))
        .stdout(predicate::str::contains("r2"))
        .stdout(predicate::str::contains(
            "Would commit 2 local Git commit(s)",
        ))
        .stdout(predicate::str::contains("local one"))
        .stdout(predicate::str::contains("local two"));
}

#[test]
fn dcommit_dry_run_reports_no_local_commits() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No local commits to dcommit."));
}

#[test]
fn dcommit_without_dry_run_is_guarded_until_write_back_lands() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    make_commit(&work, "local.txt", "local\n", "local change");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("dcommit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("dcommit without --dry-run"))
        .stderr(predicate::str::contains("not implemented"));
}

#[test]
fn dcommit_mergeinfo_reports_v1_scope_message() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["dcommit", "--dry-run", "--mergeinfo", "/branches/main:1-2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--mergeinfo"))
        .stderr(predicate::str::contains("v1"))
        .stderr(predicate::str::contains("not implemented"));
}

fn clone_mock_repo(parent: &std::path::Path) -> std::path::PathBuf {
    let work = parent.join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(parent)
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();
    work
}

fn make_commit(work: &std::path::Path, path: &str, content: &str, message: &str) {
    std::fs::write(work.join(path), content).unwrap();
    run_git(work, &["add", path]);
    run_git(
        work,
        &[
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            message,
        ],
    );
}

fn run_git(work: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}
