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
fn named_svn_remote_drives_readonly_commands_and_ignores_unrelated_missing_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    git(work, ["init"]);
    git(work, ["config", "svn-remote.other.url", "mock://repo"]);
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.other.fetch",
            "trunk:refs/remotes/other/trunk",
        ],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["fetch", "other"])
        .assert()
        .success();
    git(
        work,
        ["checkout", "-b", "topic", "refs/remotes/other/trunk"],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/trunk\n");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r2"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("^[0-9a-f]{40}\n$").unwrap());
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--revision", "2", "--oneline"])
        .assert()
        .success()
        .stdout("r2 | add trunk file\n");

    git(
        work,
        ["config", "svn-remote.unrelated.url", "mock://unrelated"],
    );
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.unrelated.fetch",
            ":refs/remotes/unrelated",
        ],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .success()
        .stdout("mock://repo/trunk\n");
}

#[test]
fn resolver_rejects_same_distance_identity_across_named_remotes() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    git(work, ["init"]);
    git(work, ["config", "svn-remote.first.url", "mock://repo"]);
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.first.fetch",
            "trunk:refs/remotes/first/trunk",
        ],
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["fetch", "first"])
        .assert()
        .success();
    let oid = git_output(work, ["rev-parse", "refs/remotes/first/trunk"])
        .trim()
        .to_string();
    git(work, ["update-ref", "refs/remotes/second/trunk", &oid]);
    git(work, ["config", "svn-remote.second.url", "mock://repo"]);
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.second.fetch",
            "trunk:refs/remotes/second/trunk",
        ],
    );
    let second_metadata = work.join(".git/svn/refs/remotes/second/trunk");
    std::fs::create_dir_all(&second_metadata).unwrap();
    std::fs::copy(
        work.join(".git/svn/refs/remotes/first/trunk/.rev_map.mock-uuid"),
        second_metadata.join(".rev_map.mock-uuid"),
    )
    .unwrap();
    git(work, ["checkout", "-b", "topic", &oid]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["info", "--url"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous SVN tracking identity"))
        .stderr(predicate::str::contains("first/refs/remotes/first/trunk"))
        .stderr(predicate::str::contains("second/refs/remotes/second/trunk"));
}

#[test]
fn resolver_rejects_same_ref_identity_for_distinct_svn_paths() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    git(&work, ["config", "svn-remote.svn.noMetadata", "true"]);
    git(
        &work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            "branches/other:refs/remotes/git-svn",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["info", "--url"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous SVN tracking identity"));
}

#[test]
fn resolver_does_not_hide_ambiguous_rev_map_on_an_unrelated_remote() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    git(
        &work,
        ["config", "svn-remote.broken.url", "mock://unrelated"],
    );
    git(
        &work,
        [
            "config",
            "--add",
            "svn-remote.broken.fetch",
            ":refs/remotes/broken",
        ],
    );
    let broken_metadata = work.join(".git/svn/refs/remotes/broken");
    std::fs::create_dir_all(&broken_metadata).unwrap();
    let source = canonical_git_svn_metadata(&work).join(".rev_map.mock-uuid");
    std::fs::copy(&source, broken_metadata.join(".rev_map.first")).unwrap();
    std::fs::copy(&source, broken_metadata.join(".rev_map.second")).unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["info", "--url"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous .rev_map files"))
        .stderr(predicate::str::contains("broken"));
}

#[test]
fn resolver_rejects_multiple_rev_maps_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let metadata = canonical_git_svn_metadata(&work);
    let original = metadata.join(".rev_map.mock-uuid");
    let ambiguous = metadata.join(".rev_map.other-uuid");
    let original_bytes = std::fs::read(&original).unwrap();
    std::fs::write(&ambiguous, &original_bytes).unwrap();
    let ref_before = git_output(&work, ["rev-parse", "refs/remotes/git-svn"]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["info", "--url"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous .rev_map files"));

    assert_eq!(std::fs::read(original).unwrap(), original_bytes);
    assert_eq!(std::fs::read(ambiguous).unwrap(), original_bytes);
    assert_eq!(
        git_output(&work, ["rev-parse", "refs/remotes/git-svn"]),
        ref_before
    );
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

    let rev_map_path = work.join(".git/svn/git-svn/.rev_map.mock-uuid");
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
    let rev_map_path = canonical_git_svn_metadata(&work).join(".rev_map.mock-uuid");
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
        .stdout(predicate::str::contains("Repository Root: mock://repo"))
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
fn log_treeish_selects_its_tracking_identity_instead_of_head() {
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
        "trunk\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/trunk", &trunk]);
    write_rev_map_for_short_ref(work, "origin.trunk", &[(1, &trunk)]);
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
        .args(["log", "--oneline", "refs/remotes/origin/trunk"])
        .assert()
        .success()
        .stdout("r1 | trunk\n");
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
        .stdout("------------------------------------------------------------------------\n");
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
fn log_oneline_show_commit_honors_git_abbreviation_length() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    git(&work, ["config", "core.abbrev", "12"]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--oneline", "--show-commit"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("^r2 \\| [0-9a-f]{12} \\| add trunk file\n$").unwrap());
}

#[test]
fn log_applies_author_timezone_like_frozen_log_pm() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    std::fs::write(work.join("dated.txt"), "dated\n").unwrap();
    git(work, ["add", "dated.txt"]);
    let status = std::process::Command::new("git")
        .current_dir(work)
        .env("GIT_AUTHOR_DATE", "@1767225600 +0800")
        .env("GIT_COMMITTER_DATE", "@1767225600 +0800")
        .args([
            "commit",
            "-m",
            "dated\n\ngit-svn-id: mock://repo@1 mock-uuid",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let commit = git_output(work, ["rev-parse", "HEAD"]).trim().to_string();
    write_rev_map_for_short_ref(work, "git-svn", &[(1, &commit)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &commit]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .env("TZ", "UTC")
        .args(["log", "--revision", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "2026-01-01 08:00:00 +0000 (Thu, 01 Jan 2026)",
        ));
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
            predicate::str::is_match("(?m)^r2 \\| [0-9a-f]{7} \\| bob \\| .+ \\| 2 lines$")
                .unwrap(),
        )
        .stdout(predicate::str::contains("\ncommit ").not());
}

#[test]
fn log_authors_file_overrides_the_persisted_mapping_for_display() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    std::fs::write(
        work.join("log-authors.txt"),
        "display-bob = bob <bob@mock-uuid>\n",
    )
    .unwrap();
    std::fs::write(
        work.join("persisted-authors.txt"),
        "persisted-bob = bob <bob@mock-uuid>\n",
    )
    .unwrap();
    git(
        &work,
        [
            "config",
            "svn-remote.svn.authors-file",
            "persisted-authors.txt",
        ],
    );

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--oneline", "-A", "log-authors.txt"])
        .assert()
        .success()
        .stdout("r2 | add trunk file\n");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "-A", "log-authors.txt", "--revision", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 | display-bob |"));
}

#[test]
fn log_non_recursive_omits_the_frozen_recursive_diff_flag() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--non-recursive", "--oneline"])
        .assert()
        .success()
        .stdout("r2 | add trunk file\n");
}

#[test]
fn log_incremental_keeps_record_separator_but_omits_trailing_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--incremental"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 |"))
        .stdout(predicate::str::starts_with(
            "------------------------------------------------------------------------\n",
        ))
        .stdout(
            predicate::str::ends_with(
                "------------------------------------------------------------------------\n",
            )
            .not(),
        );
}

#[test]
fn log_incremental_show_commit_prints_short_commit_without_trailing_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--incremental", "--show-commit"])
        .assert()
        .success()
        .stdout(
            predicate::str::is_match("(?m)^r2 \\| [0-9a-f]{7} \\| bob \\| .+ \\| 2 lines$")
                .unwrap(),
        )
        .stdout(predicate::str::starts_with(
            "------------------------------------------------------------------------\n",
        ))
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
        .stdout(predicate::str::contains("   A src/lib.rs"));
}

#[test]
fn log_verbose_matches_frozen_omission_of_scored_renames() {
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
        .stdout(predicate::str::contains("rename"))
        .stdout(predicate::str::contains("Changed paths:").not());
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
fn log_empty_default_result_prints_frozen_separator() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--revision", "1"])
        .assert()
        .success()
        .stdout("------------------------------------------------------------------------\n");
}

#[test]
fn log_suppresses_adjacent_duplicate_svn_revisions_before_limit() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(
        work,
        "one.txt",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let duplicate = commit_file(
        work,
        "duplicate.txt",
        "duplicate\n",
        "duplicate\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let rev2 = commit_file(
        work,
        "two.txt",
        "two\n",
        "second\n\ngit-svn-id: mock://repo@2 mock-uuid",
    );
    write_rev_map_for_short_ref(work, "git-svn", &[(1, &rev1), (2, &rev2)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev2]);
    assert_ne!(duplicate, rev1);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--limit", "2", "--oneline"])
        .assert()
        .success()
        .stdout("r2 | second\nr1 | duplicate\n");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--revision", "1", "--oneline", "--show-commit"])
        .assert()
        .success()
        .stdout(format!("r1 | {} | first\n", &rev1[..7]));
}

#[test]
fn log_limit_counts_showable_svn_revisions_not_raw_git_commits() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(
        work,
        "one.txt",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let local = commit_file(work, "local.txt", "local\n", "no svn footer");
    write_rev_map_for_short_ref(work, "git-svn", &[(1, &rev1)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &local]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--limit", "1", "--oneline"])
        .assert()
        .success()
        .stdout("r1 | first\n");
}

#[test]
fn log_revision_selection_ignores_footer_not_present_in_rev_map() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(
        work,
        "one.txt",
        "one\n",
        "first\n\ngit-svn-id: mock://repo@1 mock-uuid",
    );
    let forged = commit_file(
        work,
        "forged.txt",
        "forged\n",
        "forged\n\ngit-svn-id: mock://repo@2 mock-uuid",
    );
    write_rev_map_for_short_ref(work, "git-svn", &[(1, &rev1)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &forged]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--revision", "2", "--oneline"])
        .assert()
        .success()
        .stdout("");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--revision", "1:2", "--oneline"])
        .assert()
        .success()
        .stdout("r1 | first\n");
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
fn log_color_preserves_ansi_for_explicit_and_configured_modes() {
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
    git(work, ["config", "color.diff", "false"]);

    let explicit = Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "--color", "-p"])
        .output()
        .unwrap();
    assert!(explicit.status.success());
    assert!(explicit.stdout.windows(2).any(|bytes| bytes == b"\x1b["));
    assert!(String::from_utf8_lossy(&explicit.stdout).contains("r1 |"));

    git(work, ["config", "color.diff", "always"]);
    let configured = Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["log", "-p"])
        .output()
        .unwrap();
    assert!(configured.status.success());
    assert!(configured.stdout.windows(2).any(|bytes| bytes == b"\x1b["));
    assert!(String::from_utf8_lossy(&configured.stdout).contains("r1 |"));
}

#[test]
fn log_pager_is_a_non_tty_noop_and_not_git_passthrough() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());

    let baseline = Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    let paged = Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["log", "--oneline", "--pager=definitely-not-a-command"])
        .output()
        .unwrap();

    assert!(baseline.status.success());
    assert!(paged.status.success());
    assert_eq!(paged.stdout, baseline.stdout);
    assert_eq!(paged.stderr, baseline.stderr);
}

#[cfg(target_os = "linux")]
#[test]
fn log_runs_explicit_pager_when_stdout_is_a_pty() {
    use std::os::unix::fs::PermissionsExt;

    if std::process::Command::new("script")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("script(1) is unavailable; skipping PTY pager coverage");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let marker = temp.path().join("pager-invoked");
    let pager = temp.path().join("pager.sh");
    assert!(!marker.to_string_lossy().contains('\''));
    assert!(!pager.to_string_lossy().contains('\''));
    std::fs::write(
        &pager,
        format!("#!/bin/sh\n: > '{}'\nexec cat\n", marker.to_string_lossy()),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&pager).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&pager, permissions).unwrap();

    let binary = env!("CARGO_BIN_EXE_git-svn-rs");
    assert!(!binary.contains('\''));
    let command = format!("'{binary}' log --oneline --pager='{}'", pager.display());
    Command::new("script")
        .current_dir(&work)
        .args(["-qec", &command, "/dev/null"])
        .assert()
        .success()
        .stdout(predicate::str::contains("r2 |"));
    assert!(
        marker.is_file(),
        "explicit pager was not invoked under a PTY"
    );
}

#[test]
fn gc_preserves_unverifiable_legacy_rev_map_lock_files() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let lock = canonical_git_svn_metadata(&work).join(".rev_map.mock-uuid.lock");
    std::fs::write(&lock, []).unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(&work).arg("gc").assert().success();

    assert!(lock.exists());
}

#[test]
fn gc_rejects_mixed_legacy_metadata_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let svn = work.join(".git/svn/git-svn");
    std::fs::create_dir_all(&svn).unwrap();
    let rev_db = svn.join(".rev_db.uuid");
    let rev_map = svn.join(".rev_map.uuid");
    std::fs::write(&rev_db, b"legacy").unwrap();
    std::fs::write(&rev_map, b"current").unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .arg("gc")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "ambiguous mixed legacy and v5 git-svn metadata",
        ));

    assert_eq!(std::fs::read(rev_db).unwrap(), b"legacy");
    assert_eq!(std::fs::read(rev_map).unwrap(), b"current");
}

#[test]
fn gc_compresses_unhandled_log_and_removes_index_files() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let svn_dir = canonical_git_svn_metadata(&work);
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
fn gc_preserves_metadata_while_import_publication_is_pending() {
    let temp = tempfile::tempdir().unwrap();
    let work = clone_mock_repo(temp.path());
    let svn_root = work.join(".git/svn");
    let svn_dir = canonical_git_svn_metadata(&work);
    let journal = svn_root.join("import-journal");
    let unhandled = svn_dir.join("unhandled.log");
    let compressed = svn_dir.join("unhandled.log.gz");
    let index = svn_dir.join("index");
    std::fs::write(&journal, "pending\n").unwrap();
    std::fs::write(&unhandled, "property svn:ignore\n").unwrap();
    std::fs::write(&index, "stale index\n").unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("gc")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unfinished import journal"));

    assert!(journal.exists());
    assert!(unhandled.exists());
    assert!(!compressed.exists());
    assert!(index.exists());
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
fn reset_parent_uses_the_nearest_earlier_nonzero_rev_map_record() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let rev1 = commit_file(work, "one.txt", "one\n", "r1");
    let rev4 = commit_file(work, "four.txt", "four\n", "r4");
    write_rev_map_for_short_ref(work, "git-svn", &[(1, &rev1), (4, &rev4)]);
    git(work, ["update-ref", "refs/remotes/git-svn", &rev4]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["reset", "--revision", "4", "--parent"])
        .assert()
        .success();

    assert_eq!(
        git_output(work, ["rev-parse", "refs/remotes/git-svn"]).trim(),
        rev1
    );
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["find-rev", "r4"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn rebase_dry_run_prints_the_frozen_tracking_identity() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let tracked = commit_file(
        work,
        "tracked.txt",
        "tracked\n",
        "tracked\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    write_rev_map(work, &[&tracked]);
    git(work, ["update-ref", "refs/remotes/git-svn", &tracked]);

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work)
        .args(["rebase", "--dry-run", "-v", "--fetch-all"])
        .assert()
        .success()
        .stdout("Remote Branch: refs/remotes/git-svn\nSVN URL: mock://repo/trunk\n");
}

#[test]
fn rebase_dry_run_accepts_current_first_parent_tracking_identity() {
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
        "trunk\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/trunk", &trunk]);
    write_rev_map_for_short_ref(work, "origin.trunk", &[(1, &trunk)]);
    let branch = commit_file(
        work,
        "branch.txt",
        "branch\n",
        "branch\n\ngit-svn-id: mock://repo/branches/main@2 mock-uuid",
    );
    git(work, ["update-ref", "refs/remotes/origin/main", &branch]);
    write_rev_map_for_short_ref(work, "origin.main", &[(2, &branch)]);
    git(work, ["checkout", "-b", "topic", &branch]);
    commit_file(work, "local.txt", "local\n", "local");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["rebase", "--dry-run"])
        .assert()
        .success()
        .stdout("Remote Branch: refs/remotes/origin/main\nSVN URL: mock://repo/branches/main\n");
}

#[test]
fn rebase_rejects_dirty_work_tree_before_fetch_mutates_tracking_state() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let tracked = commit_file(
        work,
        "tracked.txt",
        "clean\n",
        "tracked\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    write_rev_map(work, &[&tracked]);
    git(work, ["update-ref", "refs/remotes/git-svn", &tracked]);
    let rev_map_path = work.join(".git/svn/git-svn/.rev_map.mock-uuid");
    let rev_map_before = std::fs::read(&rev_map_path).unwrap();
    std::fs::write(work.join("tracked.txt"), "dirty\n").unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .arg("rebase")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "rebase requires a clean index and work tree",
        ));

    assert_eq!(
        git_output(work, ["rev-parse", "refs/remotes/git-svn"]).trim(),
        tracked
    );
    assert_eq!(std::fs::read(rev_map_path).unwrap(), rev_map_before);
}

#[test]
fn rebase_local_skips_remote_fetch() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let missing_repository = temp.path().join("missing-svn-repository");
    let url = format!("file://{}/trunk", missing_repository.display());
    init_git_svn_work_tree_with_remote(&work, &url, ":refs/remotes/git-svn");
    let tracked = commit_file(
        &work,
        "tracked.txt",
        "tracked\n",
        &format!("tracked\n\ngit-svn-id: {url}@1 mock-uuid"),
    );
    write_rev_map(&work, &[&tracked]);
    git(&work, ["update-ref", "refs/remotes/git-svn", &tracked]);
    git(&work, ["checkout", "-b", "topic"]);
    let topic = commit_file(&work, "topic.txt", "topic\n", "topic");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["rebase", "--local", "--fetch-all"])
        .assert()
        .success();

    assert_eq!(git_output(&work, ["rev-parse", "HEAD"]).trim(), topic);
    assert!(!missing_repository.exists());
}

#[cfg(unix)]
#[test]
fn rebase_streams_successful_git_stderr() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    init_git_svn_work_tree(&work);
    let base = commit_file(
        &work,
        "base.txt",
        "base\n",
        "base\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    let upstream = commit_file(
        &work,
        "upstream.txt",
        "upstream\n",
        "upstream\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    write_rev_map(&work, &[&base, &upstream]);
    git(&work, ["update-ref", "refs/remotes/git-svn", &upstream]);
    git(&work, ["checkout", "-b", "topic", &base]);
    commit_file(&work, "topic.txt", "topic\n", "topic");

    let original_path = std::env::var_os("PATH").expect("PATH for git fixture");
    let real_git = std::env::split_paths(&original_path)
        .map(|directory| directory.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("real git executable on PATH");
    let wrapper_directory = temp.path().join("git-wrapper");
    std::fs::create_dir(&wrapper_directory).unwrap();
    let wrapper = wrapper_directory.join("git");
    assert!(!real_git.to_string_lossy().contains('"'));
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nif [ \"$1\" = rebase ]; then\n  \"{}\" \"$@\" || exit $?\n  echo git-rebase-success-marker >&2\n  exit 0\nfi\nexec \"{}\" \"$@\"\n",
            real_git.display(),
            real_git.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&wrapper, permissions).unwrap();
    let wrapped_path = std::env::join_paths(
        std::iter::once(wrapper_directory).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .env("PATH", wrapped_path)
        .args(["rebase", "--local"])
        .assert()
        .success()
        .stderr(predicate::str::contains("git-rebase-success-marker"));
    assert_eq!(
        git_output(&work, ["merge-base", "HEAD", "refs/remotes/git-svn"]).trim(),
        upstream
    );
}

#[test]
fn rebase_merges_preserves_local_merge_topology() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    init_git_svn_work_tree(work);
    let base = commit_file(
        work,
        "base.txt",
        "base\n",
        "base\n\ngit-svn-id: mock://repo/trunk@1 mock-uuid",
    );
    let upstream = commit_file(
        work,
        "upstream.txt",
        "upstream\n",
        "upstream\n\ngit-svn-id: mock://repo/trunk@2 mock-uuid",
    );
    write_rev_map(work, &[&base, &upstream]);
    git(work, ["update-ref", "refs/remotes/git-svn", &upstream]);

    git(work, ["checkout", "-b", "side", &base]);
    commit_file(work, "side.txt", "side\n", "side");
    git(work, ["checkout", "-b", "topic", &base]);
    commit_file(work, "topic.txt", "topic\n", "topic");
    git(work, ["merge", "--no-ff", "side", "-m", "merge side"]);

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .args(["rebase", "--local", "--rebase-merges"])
        .assert()
        .success();

    assert_eq!(
        git_output(work, ["merge-base", "HEAD", "refs/remotes/git-svn"]).trim(),
        upstream
    );
    assert_eq!(
        git_output(
            work,
            [
                "rev-list",
                "--count",
                "--merges",
                "refs/remotes/git-svn..HEAD",
            ],
        )
        .trim(),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("side.txt")).unwrap(),
        "side\n"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("topic.txt")).unwrap(),
        "topic\n"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("upstream.txt")).unwrap(),
        "upstream\n"
    );
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
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.svn.fetch",
            ":refs/remotes/sibling",
        ],
    );
    git(
        work,
        [
            "config",
            "svn-remote.other.url",
            "https://unvalidated.invalid/svn",
        ],
    );
    git(
        work,
        [
            "config",
            "--add",
            "svn-remote.other.fetch",
            ":refs/remotes/other",
        ],
    );
    git(work, ["checkout", "-b", "topic", &base]);
    let topic = commit_file(work, "topic.txt", "topic\n", "topic");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(work)
        .args(["rebase", "-v", "--fetch-all", "-M", "--strategy=ort"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Changes from"));

    let head = git_output(work, ["rev-parse", "HEAD"]);
    let merge_base = git_output(work, ["merge-base", "HEAD", "refs/remotes/git-svn"]);
    let tracked = git_output(work, ["rev-parse", "refs/remotes/git-svn"]);
    assert_ne!(head.trim(), topic);
    assert_eq!(merge_base.trim(), tracked.trim());
    assert_eq!(
        git_output(work, ["rev-parse", "refs/remotes/sibling"])
            .trim()
            .len(),
        40
    );
    assert!(
        work.join(".git/svn/refs/remotes/sibling/.rev_map.mock-uuid")
            .is_file()
    );
    assert!(!work.join(".git/svn/refs/remotes/other").exists());
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

fn canonical_git_svn_metadata(work: &std::path::Path) -> std::path::PathBuf {
    work.join(".git/svn/refs/remotes/git-svn")
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
