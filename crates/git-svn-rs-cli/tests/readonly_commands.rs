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
fn find_rev_maps_branch_git_commit_to_svn_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    let trunk = commit_file(
        work,
        "trunk.txt",
        "trunk\n",
        "trunk\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/trunk", &trunk]);
    write_rev_map_for_short_ref(work, "origin.trunk", &[(2, &trunk)]);

    let branch = commit_file(
        work,
        "branch.txt",
        "branch\n",
        "branch\n\ngit-svn-id: mock://repo/branches/main@3 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/main", &branch]);
    write_rev_map_for_short_ref(work, "origin.main", &[(3, &branch)]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", &branch])
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn find_rev_maps_branch_svn_revision_to_git_commit() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    let trunk = commit_file(
        work,
        "trunk.txt",
        "trunk\n",
        "trunk\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/trunk", &trunk]);
    write_rev_map_for_short_ref(work, "origin.trunk", &[(2, &trunk)]);

    let branch = commit_file(
        work,
        "branch.txt",
        "branch\n",
        "branch\n\ngit-svn-id: mock://repo/branches/main@3 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/main", &branch]);
    write_rev_map_for_short_ref(work, "origin.main", &[(3, &branch)]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r3"])
        .assert()
        .success()
        .stdout(format!("{branch}\n"));
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
fn info_url_includes_tracked_fetch_path() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/git-svn");
    let rev1 = commit_file(work, "one.txt", "one\n", "r1");
    write_rev_map(work, &[&rev1]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev1]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
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
fn log_oneline_show_commit_prints_git_commit_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--oneline", "--show-commit"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("^[0-9a-f]{40} \\| r2 \\| add trunk file\n$").unwrap());
}

#[test]
fn log_incremental_omits_svn_log_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--incremental"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 |"))
        .stdout(
            predicate::str::contains(
                "------------------------------------------------------------------------",
            )
            .not(),
        );
}

#[test]
fn log_verbose_prints_changed_paths() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Changed paths:"))
        .stdout(predicate::str::contains("A\tsrc/lib.rs"));
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
fn log_revision_range_filters_to_requested_svn_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", ":refs/remotes/git-svn");
    let rev1 = commit_file(
        work,
        "one.txt",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let rev2 = commit_file(
        work,
        "two.txt",
        "two\n",
        "second\n\ngit-svn-id: mock://repo@2 mock-uuid",
    );
    let rev3 = commit_file(
        work,
        "three.txt",
        "three\n",
        "third\n\ngit-svn-id: mock://repo@3 mock-uuid",
    );
    write_rev_map(work, &[&rev1, &rev2, &rev3]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev3]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--revision", "1:2", "--oneline"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r1 | first"))
        .stdout(predicate::str::contains("r2 | second"))
        .stdout(predicate::str::contains("r3 | third").not());
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

#[test]
fn reset_moves_tracked_ref_and_truncates_rev_map() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(work, "one.txt", "one\n", "r1");
    let rev2 = commit_file(work, "two.txt", "two\n", "r2");
    write_rev_map(work, &[&rev1, &rev2]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev2]);

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work)
        .args(["reset", "--revision", "1"])
        .assert()
        .success();

    let tracked = git_output(work, ["rev-parse", "refs/remotes/git-svn"]);
    assert_eq!(tracked.trim(), rev1);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn reset_missing_revision_fails_without_moving_tracked_ref() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(work, "one.txt", "one\n", "r1");
    write_rev_map(work, &[&rev1]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev1]);

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work)
        .args(["reset", "--revision", "9"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no Git commit found for SVN revision r9",
        ));

    let tracked = git_output(work, ["rev-parse", "refs/remotes/git-svn"]);
    assert_eq!(tracked.trim(), rev1);
}

#[test]
fn rebase_dry_run_prints_planned_actions() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work)
        .args(["rebase", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("would run fetch"))
        .stdout(predicate::str::contains(
            "would run git rebase refs/remotes/git-svn",
        ));
}

#[test]
fn rebase_fetches_and_runs_git_rebase_against_tracked_ref() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let base = commit_file(work, "base.txt", "base\n", "base");
    let upstream = commit_file(work, "upstream.txt", "upstream\n", "upstream");
    write_rev_map(work, &[&base, &upstream]);
    git(work, ["update-ref", "refs/remotes/git-svn", &upstream]);
    git(work, ["checkout", "-b", "topic", &base]);
    let topic = commit_file(work, "topic.txt", "topic\n", "topic");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work).arg("rebase").assert().success();

    let head = git_output(work, ["rev-parse", "HEAD"]);
    let merge_base = git_output(work, ["merge-base", "HEAD", "refs/remotes/git-svn"]);
    let tracked = git_output(work, ["rev-parse", "refs/remotes/git-svn"]);
    assert_ne!(head.trim(), topic);
    assert_eq!(merge_base.trim(), tracked.trim());
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

fn init_git_svn_work_tree(work: &std::path::Path) {
    init_git_svn_work_tree_with_remote(work, "mock://repo/trunk", ":refs/remotes/git-svn");
}

fn init_git_svn_work_tree_with_remote(work: &std::path::Path, url: &str, fetch: &str) {
    git(work, ["init"]);
    git(work, ["config", "user.name", "Test User"]);
    git(work, ["config", "user.email", "test@example.com"]);
    git(work, ["config", "svn-remote.svn.url", url]);
    git(work, ["config", "--add", "svn-remote.svn.fetch", fetch]);
}

fn commit_file(work: &std::path::Path, path: &str, content: &str, message: &str) -> String {
    std::fs::write(work.join(path), content).unwrap();
    git(work, ["add", path]);
    git(work, ["commit", "-m", message]);
    git_output(work, ["rev-parse", "HEAD"]).trim().to_string()
}

fn write_rev_map(work: &std::path::Path, commits: &[&str]) {
    let records = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| (index as u32 + 1, *commit))
        .collect::<Vec<_>>();
    write_rev_map_for_short_ref(work, "git-svn", &records);
}

fn write_rev_map_for_short_ref(work: &std::path::Path, short_ref: &str, records: &[(u32, &str)]) {
    let rev_map_path = work
        .join(".git")
        .join("svn")
        .join(short_ref)
        .join(".rev_map.mock-uuid");
    let mut rev_map = git_svn_rs_core::rev_map::RevMap::open(
        rev_map_path,
        git_svn_rs_core::rev_map::ObjectFormat::Sha1,
    )
    .unwrap();
    for (revision, commit) in records {
        rev_map.append(*revision, commit).unwrap();
    }
}

fn git<const N: usize>(work: &std::path::Path, args: [&str; N]) {
    let status = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn git_output<const N: usize>(work: &std::path::Path, args: [&str; N]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(work)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).to_string()
}
