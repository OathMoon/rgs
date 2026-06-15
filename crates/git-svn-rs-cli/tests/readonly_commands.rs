use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn find_rev_maps_svn_revision_to_git_commit() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("^[0-9a-f]{40}\n$").unwrap());
}

#[test]
fn find_rev_maps_git_commit_to_svn_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let head = git_svn_rs_core::git::GitCli::new(&work)
        .run_for_test(["rev-parse", "refs/remotes/git-svn"])
        .unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .args(["find-rev", head.trim()])
        .assert()
        .success()
        .stdout("2\n");
}

#[test]
fn find_rev_before_and_after_use_nearest_tracked_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["find-rev", "--before", "r3"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("^[0-9a-f]{40}\n$").unwrap());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["find-rev", "--after", "r3"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn info_prints_tracked_url_and_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("URL: mock://repo/trunk"))
        .stdout(predicate::str::contains("Revision: 2"));
}

#[test]
fn info_url_prints_only_url() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/trunk\n");
}

#[test]
fn log_prints_svn_revisions_from_git_history() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .args(["log", "--oneline"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 | add trunk file"));
}

#[test]
fn log_revision_filters_to_requested_svn_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work)
        .args(["log", "--revision", "2", "--oneline"])
        .assert()
        .success()
        .stdout("r2 | add trunk file\n");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--revision", "1", "--oneline"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn gc_removes_stale_rev_map_lock_files() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let lock = work.join(".git/svn/git-svn/.rev_map.mock-uuid.lock");
    std::fs::write(&lock, []).unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work).arg("gc").assert().success();

    assert!(!lock.exists());
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
