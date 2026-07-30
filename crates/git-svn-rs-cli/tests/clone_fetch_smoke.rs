use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn fetch_rejects_ambiguous_all_combinations_before_repository_access() {
    let cases: &[(&[&str], &str)] = &[
        (
            &["fetch", "named", "--fetch-all"],
            "cannot combine a remote name with --fetch-all",
        ),
        (
            &["fetch", "--parent", "--fetch-all"],
            "fetch --parent cannot be combined with --fetch-all",
        ),
    ];
    for (args, message) in cases {
        let temp = tempfile::tempdir().unwrap();
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(temp.path())
            .args(*args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(*message));
        assert!(!temp.path().join(".git").exists());
    }
}

#[test]
fn fetch_rejects_legacy_rev_db_without_creating_rev_map() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    let svn = work.join(".git/svn/git-svn");
    std::fs::create_dir_all(&svn).unwrap();
    let rev_db = svn.join(".rev_db.uuid");
    let rev_map = svn.join(".rev_map.uuid");
    std::fs::write(&rev_db, b"legacy").unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(work)
        .arg("fetch")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "legacy git-svn rev_db metadata requires migration",
        ));

    assert_eq!(std::fs::read(rev_db).unwrap(), b"legacy");
    assert!(!rev_map.exists());
}

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
    assert_eq!(
        git.run_for_test(["rev-parse", "HEAD"]).unwrap(),
        git.run_for_test(["rev-parse", "refs/remotes/git-svn"])
            .unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(work.join("src/lib.rs")).unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
    assert_eq!(
        git.run_for_test(["show", "-s", "--format=%at %ct %ai %ci", "HEAD"])
            .unwrap()
            .trim(),
        "1767312000 1767312000 2026-01-02 00:00:00 +0000 2026-01-02 00:00:00 +0000"
    );
}

#[test]
fn clone_no_checkout_leaves_an_unborn_branch_without_populating_work_tree() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work", "--no-checkout"])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    assert!(git.run_for_test(["rev-parse", "HEAD"]).is_err());
    assert!(
        git.run_for_test(["show-ref", "--verify", "refs/heads/master"])
            .is_err()
    );
    assert!(!work.join("src/lib.rs").exists());
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
fn fetch_rejects_duplicate_remote_url_without_metadata_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", "mock://repo/trunk", "work"])
        .assert()
        .success();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.svn.url",
        "mock://other/trunk",
    ])
    .unwrap();
    let config_before = std::fs::read(work.join(".git/config")).unwrap();
    let refs_before = git
        .run_for_test(["for-each-ref", "--format=%(refname) %(objectname)"])
        .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "multiple values for svn-remote.svn.url",
        ));

    assert_eq!(
        std::fs::read(work.join(".git/config")).unwrap(),
        config_before
    );
    assert_eq!(
        git.run_for_test(["for-each-ref", "--format=%(refname) %(objectname)"])
            .unwrap(),
        refs_before
    );
    assert!(!work.join(".git/svn").exists());
}

#[test]
fn fetch_rejects_unvalidated_https_before_metadata_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", "https://svn.example/repo/trunk", "work"])
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .failure()
        .stderr(predicate::str::contains("HTTPS SVN fetch"))
        .stderr(predicate::str::contains("deferred"));

    assert!(
        !work.join(".git/svn").exists(),
        "unvalidated HTTPS must fail before creating SVN metadata"
    );
}

#[test]
fn failed_fetch_does_not_advance_discovery_high_water() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", "mock://repo", "work", "--stdlayout"])
        .assert()
        .success();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.config_set(
        "svn-remote.svn.authors-file",
        work.join("missing-authors").to_str().unwrap(),
    )
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .failure();

    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.branches-maxRev")
            .unwrap(),
        None
    );
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.tags-maxRev")
            .unwrap(),
        None
    );
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.reposRoot")
            .unwrap(),
        None
    );
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.uuid").unwrap(),
        None
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

#[test]
fn fetch_and_info_reject_semantically_corrupt_mock_rev_map_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    let rev_map_path = mock_rev_map_path(&work);
    let mut rev_map = std::fs::read(&rev_map_path).unwrap();
    let last_record = rev_map.len() - 24;
    rev_map[last_record..last_record + 4].copy_from_slice(&100_u32.to_be_bytes());
    std::fs::write(&rev_map_path, &rev_map).unwrap();
    let before = tracking_state_snapshot(&git, &work, &rev_map_path);

    for command in ["fetch", "info"] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(&work)
            .arg(command)
            .assert()
            .failure()
            .stderr(predicate::str::contains("corrupt SVN tracking state"));
        assert_eq!(tracking_state_snapshot(&git, &work, &rev_map_path), before);
    }
}

#[test]
fn fetch_and_info_reject_tracking_ref_tip_drift_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["clone", "mock://repo/trunk", "work"])
        .assert()
        .success();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["config", "user.name", "Test"]).unwrap();
    git.run_for_test(["config", "user.email", "test@example.com"])
        .unwrap();
    git.run_for_test(["commit", "--allow-empty", "-m", "local drift"])
        .unwrap();
    let local_oid = git.rev_parse("HEAD").unwrap();
    git.update_ref("refs/remotes/git-svn", local_oid.trim())
        .unwrap();
    let rev_map_path = mock_rev_map_path(&work);
    let before = tracking_state_snapshot(&git, &work, &rev_map_path);

    for command in ["fetch", "info"] {
        Command::cargo_bin("git-svn-rs")
            .unwrap()
            .current_dir(&work)
            .arg(command)
            .assert()
            .failure()
            .stderr(predicate::str::contains("tracking ref points to"));
        assert_eq!(tracking_state_snapshot(&git, &work, &rev_map_path), before);
    }
}

fn mock_rev_map_path(work: &std::path::Path) -> std::path::PathBuf {
    work.join(".git/svn/refs/remotes/git-svn/.rev_map.mock-uuid")
}

fn tracking_state_snapshot(
    git: &git_svn_rs_core::git::GitCli,
    work: &std::path::Path,
    rev_map_path: &std::path::Path,
) -> (String, Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        git.rev_parse("refs/remotes/git-svn")
            .unwrap()
            .trim()
            .to_string(),
        std::fs::read(rev_map_path).unwrap(),
        std::fs::read(work.join(".git/config")).unwrap(),
        std::fs::read(work.join(".git/svn/.metadata")).unwrap(),
    )
}

#[test]
fn fetch_parent_uses_the_resolved_named_remote() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    git.run_for_test(["config", "svn-remote.other.url", "mock://repo"])
        .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.other.fetch",
        "trunk:refs/remotes/other/trunk",
    ])
    .unwrap();
    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "other"])
        .assert()
        .success();
    git.run_for_test(["checkout", "-b", "topic", "refs/remotes/other/trunk"])
        .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "other", "--parent"])
        .assert()
        .success();
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/other/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n"
    );
}

#[test]
fn fetch_after_import_detects_sha256_rev_map() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init", "--object-format=sha256"])
        .unwrap();
    git.run_for_test(["config", "svn-remote.svn.url", "mock://repo/trunk"])
        .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.svn.fetch",
        ":refs/remotes/git-svn",
    ])
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();
}

#[test]
fn fetch_accepts_leading_plus_refspec_from_config() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    git.run_for_test(["config", "svn-remote.svn.url", "mock://repo"])
        .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.svn.fetch",
        "+trunk:refs/remotes/origin/trunk",
    ])
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .arg("fetch")
        .assert()
        .success();

    assert!(
        git.run_for_test(["show", "-s", "--format=%B", "refs/remotes/origin/trunk"])
            .unwrap()
            .contains("git-svn-id: mock://repo/trunk@2 mock-uuid")
    );
}

#[test]
fn fetch_all_imports_every_configured_svn_remote() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(temp.path())
        .args(["init", "mock://repo/trunk", "work"])
        .assert()
        .success();

    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["config", "svn-remote.extra.url", "mock://repo/trunk"])
        .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.extra.fetch",
        ":refs/remotes/extra",
    ])
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--fetch-all"])
        .assert()
        .success();

    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/extra:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );
}

#[test]
fn fetch_all_rejects_duplicate_remote_ref_before_import_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    for remote in ["one", "two"] {
        git.run_for_test([
            "config",
            &format!("svn-remote.{remote}.url"),
            &format!("mock://{remote}"),
        ])
        .unwrap();
        git.run_for_test([
            "config",
            "--add",
            &format!("svn-remote.{remote}.fetch"),
            "trunk:refs/remotes/origin/trunk",
        ])
        .unwrap();
    }

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--fetch-all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "remote ref refs/remotes/origin/trunk may be tracked by both",
        ));

    assert!(
        git.run_for_test(["show-ref", "--verify", "refs/remotes/origin/trunk"])
            .is_err()
    );
    assert!(!work.join(".git/svn").exists());
}

#[test]
fn fetch_all_rejects_duplicate_wildcard_destination_before_import_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    for remote in ["one", "two"] {
        git.run_for_test([
            "config",
            &format!("svn-remote.{remote}.url"),
            &format!("file:///repository-that-must-not-be-accessed/{remote}"),
        ])
        .unwrap();
        git.run_for_test([
            "config",
            "--add",
            &format!("svn-remote.{remote}.branches"),
            "branches/*:refs/remotes/shared/*",
        ])
        .unwrap();
    }

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--fetch-all"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("remote ref refs/remotes/shared/* may be tracked by both")
                .and(predicate::str::contains(
                    "svn-remote.one.branches=branches/*:refs/remotes/shared/*",
                ))
                .and(predicate::str::contains(
                    "svn-remote.two.branches=branches/*:refs/remotes/shared/*",
                )),
        );

    assert!(
        git.run_for_test(["show-ref", "--verify", "refs/remotes/shared/main"])
            .is_err()
    );
    assert!(!work.join(".git/svn").exists());
}

#[test]
fn fetch_all_rejects_fixed_and_wildcard_destination_intersection() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    git.run_for_test([
        "config",
        "svn-remote.fixed.url",
        "file:///repository-that-must-not-be-accessed/fixed",
    ])
    .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.fixed.fetch",
        "trunk:refs/remotes/shared/main",
    ])
    .unwrap();
    git.run_for_test([
        "config",
        "svn-remote.wildcard.url",
        "file:///repository-that-must-not-be-accessed/wildcard",
    ])
    .unwrap();
    git.run_for_test([
        "config",
        "--add",
        "svn-remote.wildcard.tags",
        "tags/*:refs/remotes/shared/*",
    ])
    .unwrap();

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--fetch-all"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains(
                "remote ref destinations refs/remotes/shared/main and refs/remotes/shared/*",
            )
            .and(predicate::str::contains(
                "svn-remote.fixed.fetch=trunk:refs/remotes/shared/main",
            ))
            .and(predicate::str::contains(
                "svn-remote.wildcard.tags=tags/*:refs/remotes/shared/*",
            )),
        );

    assert!(!work.join(".git/svn").exists());
}

#[test]
fn fetch_all_allows_nonintersecting_wildcard_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    let git = git_svn_rs_core::git::GitCli::new(&work);
    git.run_for_test(["init"]).unwrap();
    for (remote, key, refspec) in [
        ("one", "branches", "branches/*:refs/remotes/one/*"),
        ("two", "tags", "tags/*:refs/remotes/two/*"),
    ] {
        git.run_for_test([
            "config",
            &format!("svn-remote.{remote}.url"),
            &format!("mock://{remote}"),
        ])
        .unwrap();
        git.run_for_test([
            "config",
            "--add",
            &format!("svn-remote.{remote}.{key}"),
            refspec,
        ])
        .unwrap();
    }

    Command::cargo_bin("git-svn-rs")
        .unwrap()
        .current_dir(&work)
        .args(["fetch", "--fetch-all"])
        .assert()
        .success();
}
