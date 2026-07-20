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
fn find_rev_supports_sha256_rev_maps_bidirectionally() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    git(work, ["init", "--object-format=sha256"]);
    git(work, ["config", "user.name", "Test User"]);
    git(work, ["config", "user.email", "test@example.com"]);
    git(work, ["config", "svn-remote.svn.url", "mock://repo/trunk"]);
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            ":refs/remotes/git-svn",
        ],
    );
    let commit = commit_file(
        work,
        "file.txt",
        "content\n",
        "message\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/git-svn", &commit]);

    let rev_map_path = work
        .join(".git")
        .join("svn")
        .join("git-svn")
        .join(".rev_map.mock-uuid");
    let mut rev_map = git_svn_rs_core::rev_map::RevMap::open(
        rev_map_path,
        git_svn_rs_core::rev_map::ObjectFormat::Sha256,
    )
    .unwrap();
    rev_map.append(2, &commit).unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout(format!("{commit}\n"));
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", &commit])
        .assert()
        .success()
        .stdout("2\n");
}

#[test]
fn find_rev_maps_branch_git_commit_to_svn_revision() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.branches",
            "branches/*:refs/remotes/origin/*",
        ],
    );
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
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.branches",
            "branches/*:refs/remotes/origin/*",
        ],
    );
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
fn find_rev_scopes_same_revision_to_head_or_explicit_treeish() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.branches",
            "branches/*:refs/remotes/origin/*",
        ],
    );
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
        "branch\n\ngit-svn-id: mock://repo/branches/main@2 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/main", &branch]);
    write_rev_map_for_short_ref(work, "origin.main", &[(2, &branch)]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout(format!("{branch}\n"));
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2", "refs/remotes/origin/trunk"])
        .assert()
        .success()
        .stdout(format!("{trunk}\n"));
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2", "missing-treeish"])
        .assert()
        .failure();
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
fn no_metadata_import_rejects_followup_operations_without_mutating_tracking_state() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work", "--no-metadata"])
        .assert()
        .success();

    let tracked_before = git_output(&work, ["rev-parse", "refs/remotes/git-svn"]);
    let rev_map_path = work
        .join(".git")
        .join("svn")
        .join("git-svn")
        .join(".rev_map.mock-uuid");
    let rev_map_before = std::fs::read(&rev_map_path).unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout(format!("{}\n", tracked_before.trim()));
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("URL: mock://repo/trunk"));

    for (command, expected) in [
        (vec!["fetch"], "fetch is unavailable"),
        (vec!["rebase"], "fetch is unavailable"),
        (vec!["log"], "log is unavailable"),
        (vec!["dcommit"], "dcommit is unavailable"),
    ] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(&work)
            .args(command)
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
    }

    assert_eq!(
        git_output(&work, ["rev-parse", "refs/remotes/git-svn"]),
        tracked_before
    );
    assert_eq!(std::fs::read(rev_map_path).unwrap(), rev_map_before);
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
fn info_url_accepts_leading_plus_in_fetch_refspec() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "+trunk:refs/remotes/git-svn");
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
fn info_url_uses_current_head_ref_in_multi_ref_layout() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            "branches/main:refs/remotes/origin/main",
        ],
    );
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
    git(work, ["checkout", "--detach", &branch]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/branches/main\n");
}

#[test]
fn info_url_resolves_branch_from_branches_mapping() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.branches",
            "+branches/*:refs/remotes/origin/*",
        ],
    );
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
    git(work, ["checkout", "--detach", &branch]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/branches/main\n");
}

#[test]
fn info_url_uses_current_head_ancestor_ref_in_multi_ref_layout() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "trunk:refs/remotes/origin/trunk");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            "branches/main:refs/remotes/origin/main",
        ],
    );
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
    git(work, ["checkout", "-b", "topic", &branch]);
    commit_file(work, "local.txt", "local\n", "local only");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/branches/main\n");
}

#[test]
fn resolver_uses_nearest_first_parent_identity_after_merge() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree_with_remote(work, "mock://repo", "branches/A:refs/remotes/origin/A");
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            "branches/B:refs/remotes/origin/B",
        ],
    );

    let branch_a = commit_file(
        work,
        "a.txt",
        "A\n",
        "branch A\n\ngit-svn-id: mock://repo/branches/A@100 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/A", &branch_a]);
    write_rev_map_for_short_ref(work, "origin.A", &[(100, &branch_a)]);

    git(work, ["checkout", "-b", "branch-b", &branch_a]);
    let branch_b = commit_file(
        work,
        "b.txt",
        "B\n",
        "branch B\n\ngit-svn-id: mock://repo/branches/B@200 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/B", &branch_b]);
    write_rev_map_for_short_ref(work, "origin.B", &[(200, &branch_b)]);

    git(work, ["checkout", "-b", "topic", &branch_a]);
    commit_file(work, "local.txt", "local\n", "local change");
    git(
        work,
        [
            "-c",
            "user.name=Test User",
            "-c",
            "user.email=test@example.com",
            "merge",
            "--no-ff",
            "refs/remotes/origin/B",
            "-m",
            "merge branch B",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/branches/A\n");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["dcommit", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not support merge commits in the local commit range",
        ));
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
fn log_uses_only_final_nonempty_git_svn_id_line_as_footer() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let commit = commit_file(
        work,
        "file.txt",
        "content\n",
        "subject\n\ngit-svn-id: mock://repo/body@99 mock-uuid\n\nbody text\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    write_rev_map_for_short_ref(work, "git-svn", &[(2, &commit)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &commit]);

    let output = Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .arg("log")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();

    assert!(output.contains("r2 |"), "{output}");
    assert!(
        output.contains("git-svn-id: mock://repo/body@99 mock-uuid"),
        "{output}"
    );
    assert!(
        !output.contains("git-svn-id: mock://repo/trunk@2 mock-uuid"),
        "{output}"
    );
}

#[test]
fn log_skips_commit_when_final_nonempty_line_is_not_git_svn_id_footer() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let commit = commit_file(
        work,
        "file.txt",
        "content\n",
        "subject\n\ngit-svn-id: mock://repo/body@99 mock-uuid\n\nfinal body text",
    );
    write_rev_map_for_short_ref(work, "git-svn", &[(2, &commit)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &commit]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .arg("log")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn log_default_ends_with_svn_log_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("log")
        .assert()
        .success()
        .stdout(predicate::str::ends_with(
            "------------------------------------------------------------------------\n",
        ));
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
        .stdout(predicate::str::is_match("^r2 \\| [0-9a-f]{40} \\| add trunk file\n$").unwrap());
}

#[test]
fn log_normal_show_commit_prints_git_commit_in_svn_header() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--show-commit"])
        .assert()
        .success()
        .stdout(
            predicate::str::is_match(
                "(?m)^r2 \\| (?:[0-9a-f]{40}|[0-9a-f]{64}) \\| bob \\| .+ \\| 1 line$",
            )
            .unwrap(),
        )
        .stdout(predicate::str::contains("\ncommit ").not());
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
fn log_incremental_show_commit_prints_commit_in_header_without_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--incremental", "--show-commit"])
        .assert()
        .success()
        .stdout(
            predicate::str::is_match(
                "(?m)^r2 \\| (?:[0-9a-f]{40}|[0-9a-f]{64}) \\| bob \\| .+ \\| 1 line$",
            )
            .unwrap(),
        )
        .stdout(
            predicate::str::contains(
                "------------------------------------------------------------------------",
            )
            .not(),
        )
        .stdout(predicate::str::contains("\ncommit ").not());
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
        .stdout(predicate::str::contains("   A /trunk/src/lib.rs"));
}

#[test]
fn log_verbose_detects_renamed_paths() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(
        work,
        "old.txt",
        "old\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    git(work, ["mv", "old.txt", "new.txt"]);
    git(
        work,
        [
            "commit",
            "-m",
            "rename\n\ngit-svn-id: mock://repo@2 mock-uuid",
        ],
    );
    let rev2 = git_output(work, ["rev-parse", "HEAD"]).trim().to_string();
    write_rev_map(work, &[&rev1, &rev2]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev2]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--verbose", "--revision", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "   R /trunk/new.txt (from /trunk/old.txt)",
        ));
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
fn log_invalid_revision_filter_fails() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--revision", "abc", "--oneline"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid SVN revision: abc"));
}

#[test]
fn log_revision_filter_applies_before_limit() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
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
        .args(["log", "--revision", "1", "--limit", "1", "--oneline"])
        .assert()
        .success()
        .stdout("r1 | first\n");
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
        .stdout("r1 | first\nr2 | second\n");
}

#[test]
fn log_revision_reverse_range_filters_to_requested_svn_revisions() {
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
        .args(["log", "--revision", "3:1", "--oneline"])
        .assert()
        .success()
        .stdout("r3 | third\nr2 | second\nr1 | first\n");
}

#[test]
fn log_revision_range_filter_applies_before_limit() {
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
        .args(["log", "--revision", "1:2", "--limit", "1", "--oneline"])
        .assert()
        .success()
        .stdout("r2 | second\n");
}

#[test]
fn log_limit_returns_latest_svn_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
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
        .args(["log", "--limit", "1", "--oneline"])
        .assert()
        .success()
        .stdout("r3 | third\n");
}

#[test]
fn log_short_limit_returns_latest_svn_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
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
        .args(["log", "-n", "1", "--oneline"])
        .assert()
        .success()
        .stdout("r3 | third\n");
}

#[test]
fn log_limit_orders_latest_svn_revisions_newest_first() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
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
        .args(["log", "--limit", "2", "--oneline"])
        .assert()
        .success()
        .stdout("r3 | third\nr2 | second\n");
}

#[test]
fn log_oneline_pads_revisions_to_the_first_displayed_width() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev9 = commit_file(
        work,
        "nine.txt",
        "nine\n",
        "nine\n\ngit-svn-id: mock://repo@9 mock-uuid",
    );
    let rev10 = commit_file(
        work,
        "ten.txt",
        "ten\n",
        "ten\n\ngit-svn-id: mock://repo@10 mock-uuid",
    );
    write_rev_map_for_short_ref(work, "git-svn", &[(9, &rev9), (10, &rev10)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev10]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--oneline"])
        .assert()
        .success()
        .stdout("r10 | ten\nr9  | nine\n");
}

#[test]
fn log_passes_pathspec_args_to_git_log() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    std::fs::create_dir_all(work.join("src")).unwrap();
    let rev1 = commit_file(
        work,
        "src/lib.rs",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let rev2 = commit_file(
        work,
        "README.md",
        "two\n",
        "second\n\ngit-svn-id: mock://repo@2 mock-uuid",
    );
    write_rev_map(work, &[&rev1, &rev2]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev2]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--oneline", "--", "src/lib.rs"])
        .assert()
        .success()
        .stdout("r1 | first\n");
}

#[test]
fn log_passthrough_stat_keeps_each_commit_and_its_output() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
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
    write_rev_map(work, &[&rev1, &rev2]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev2]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--stat"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 |"))
        .stdout(predicate::str::contains("two.txt | 1 +"))
        .stdout(predicate::str::contains("r1 |"))
        .stdout(predicate::str::contains("one.txt | 1 +"));
}

#[test]
fn log_passthrough_patch_and_raw_output_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(
        work,
        "one.txt",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    write_rev_map(work, &[&rev1]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev1]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "-p"])
        .assert()
        .success()
        .stdout(predicate::str::contains("diff --git a/one.txt b/one.txt"));
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--raw"])
        .assert()
        .success()
        .stdout(predicate::str::contains("A\tone.txt"));
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
fn gc_compresses_unhandled_log_and_removes_index_files() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let svn_dir = work.join(".git/svn/git-svn");
    let unhandled = svn_dir.join("unhandled.log");
    let compressed = svn_dir.join("unhandled.log.gz");
    let index = svn_dir.join("index");
    std::fs::write(&unhandled, "property svn:ignore\n").unwrap();
    std::fs::write(&index, "stale index\n").unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work).arg("gc").assert().success();

    assert!(!unhandled.exists());
    assert!(compressed.exists());
    assert!(!index.exists());
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
