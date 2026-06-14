use assert_cmd::Command;

#[test]
fn clone_uses_mock_import_shell_for_mock_urls() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "-s", "--format=%s", "refs/remotes/git-svn"])
            .unwrap()
            .trim(),
        "add trunk file"
    );
}

#[test]
fn fetch_uses_existing_mock_remote_config() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", "mock://repo/trunk", "work"])
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
}

#[test]
fn fetch_after_clone_is_a_noop_when_mock_rev_map_is_current() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();
}
