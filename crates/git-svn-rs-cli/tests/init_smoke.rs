use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn init_creates_git_repo_and_writes_svn_remote_config() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(temp.path())
        .args([
            "init",
            "file:///svn/repo",
            "work",
            "--stdlayout",
            "--ignore-paths",
            "^vendor/",
            "--log-window-size",
            "42",
            "--localtime",
        ])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);

    assert!(work.join(".git").is_dir());
    assert_eq!(
        git.config_get("svn-remote.svn.url").unwrap().as_deref(),
        Some("file:///svn/repo")
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.fetch").unwrap(),
        vec!["trunk:refs/remotes/origin/trunk"]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.branches").unwrap(),
        vec!["branches/*:refs/remotes/origin/*"]
    );
    assert_eq!(
        git.config_get_all("svn-remote.svn.tags").unwrap(),
        vec!["tags/*:refs/remotes/origin/tags/*"]
    );
    assert_eq!(
        git.config_get("svn-remote.svn.ignore-paths")
            .unwrap()
            .as_deref(),
        Some("^vendor/")
    );
    assert_eq!(
        git.config_get("svn-remote.svn.log-window-size")
            .unwrap()
            .as_deref(),
        Some("42")
    );
    assert_eq!(
        git.config_get("svn-remote.svn.localtime")
            .unwrap()
            .as_deref(),
        Some("true")
    );
}

#[test]
fn init_reports_invalid_layout_without_creating_repo() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(temp.path())
        .args([
            "init",
            "file:///svn/repo",
            "work",
            "--branches",
            "branches/*/teams/*",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Only one set of wildcards"));

    assert!(!work.exists());
}
