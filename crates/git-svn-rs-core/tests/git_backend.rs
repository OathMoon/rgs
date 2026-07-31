use git_svn_rs_core::git::GitCli;
use tempfile::tempdir;

#[test]
fn initializes_git_repo_and_reports_git_dir() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());

    git.init().unwrap();

    let git_dir = git.git_dir().unwrap();
    assert!(git_dir.ends_with(".git"));
}

#[test]
fn delete_ref_expected_uses_compare_and_swap() {
    let dir = tempfile::tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let oid = git
        .run_for_test(["hash-object", "-w", "-t", "blob", "--stdin"])
        .unwrap();
    let oid = oid.trim();
    git.update_ref("refs/git-svn-rs/test", oid).unwrap();

    assert!(
        git.delete_ref_expected(
            "refs/git-svn-rs/test",
            "1111111111111111111111111111111111111111"
        )
        .is_err()
    );
    assert!(git.rev_parse("refs/git-svn-rs/test").is_ok());
    git.delete_ref_expected("refs/git-svn-rs/test", oid)
        .unwrap();
    assert!(git.rev_parse("refs/git-svn-rs/test").is_err());
}

#[test]
fn config_set_and_get_round_trip() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    git.config_set("svn-remote.svn.url", "file:///repo")
        .unwrap();

    assert_eq!(
        git.config_get("svn-remote.svn.url").unwrap(),
        Some("file:///repo".to_string())
    );
}

#[test]
fn missing_config_returns_none() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    assert_eq!(git.config_get("svn-remote.svn.url").unwrap(), None);
}

#[test]
fn config_get_all_preserves_multi_value_entries() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    git.config_add(
        "svn-remote.svn.branches",
        "branches/*:refs/remotes/origin/*",
    )
    .unwrap();
    git.config_add(
        "svn-remote.svn.branches",
        "releases/*:refs/remotes/origin/releases/*",
    )
    .unwrap();

    assert_eq!(
        git.config_get_all("svn-remote.svn.branches").unwrap(),
        vec![
            "branches/*:refs/remotes/origin/*".to_string(),
            "releases/*:refs/remotes/origin/releases/*".to_string(),
        ]
    );
}

#[test]
fn config_get_bool_uses_git_boolean_spellings_and_rejects_invalid_values() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    assert_eq!(git.config_get_bool("test.boolean").unwrap(), None);
    for (value, expected) in [
        ("true", true),
        ("yes", true),
        ("on", true),
        ("1", true),
        ("TRUE", true),
        ("false", false),
        ("no", false),
        ("off", false),
        ("0", false),
        ("FALSE", false),
    ] {
        git.config_set("test.boolean", value).unwrap();
        assert_eq!(
            git.config_get_bool("test.boolean").unwrap(),
            Some(expected),
            "Git boolean spelling {value}"
        );
    }

    git.config_set("test.boolean", "maybe").unwrap();
    let error = git.config_get_bool("test.boolean").unwrap_err();
    assert!(error.contains("bad boolean config value"));
    assert!(error.contains("test.boolean"));
}

#[test]
fn config_get_bool_rejects_multiple_values() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    git.config_add("test.boolean", "yes").unwrap();
    git.config_add("test.boolean", "off").unwrap();

    let error = git.config_get_bool("test.boolean").unwrap_err();
    assert!(error.contains("multiple values for test.boolean"));
    assert!(error.contains("found 2"));
}

#[test]
fn git_svn_metadata_round_trips_without_polluting_repository_config() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.branches-maxRev")
            .unwrap(),
        None
    );
    assert!(!dir.path().join(".git/svn/.metadata").exists());

    git.git_svn_metadata_set("svn-remote.svn.branches-maxRev", "17")
        .unwrap();

    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.branches-maxRev")
            .unwrap(),
        Some("17".to_string())
    );
    assert_eq!(
        git.config_get("svn-remote.svn.branches-maxRev").unwrap(),
        None
    );
    let contents = std::fs::read_to_string(dir.path().join(".git/svn/.metadata")).unwrap();
    assert!(contents.starts_with("; This file is used internally by git-svn\n"));
}

#[test]
fn git_svn_metadata_read_migrates_the_legacy_internal_config_name() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let svn_dir = dir.path().join(".git/svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(
        svn_dir.join("config"),
        "[svn-remote \"svn\"]\n\tbranches-maxRev = 9\n",
    )
    .unwrap();

    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.branches-maxRev")
            .unwrap(),
        Some("9".to_string())
    );
    assert!(svn_dir.join(".metadata").exists());
    assert!(!svn_dir.join("config").exists());
}

#[test]
fn git_svn_metadata_multi_set_replaces_all_keys_or_none() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    git.git_svn_metadata_set("svn-remote.svn.marker", "before")
        .unwrap();

    let error = git
        .git_svn_metadata_set_many(&[
            ("svn-remote.svn.svnsync-uuid", "source-uuid"),
            ("invalid key", "cannot be written"),
        ])
        .unwrap_err();

    assert!(!error.is_empty());
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.marker")
            .unwrap()
            .as_deref(),
        Some("before")
    );
    assert_eq!(
        git.git_svn_metadata_get("svn-remote.svn.svnsync-uuid")
            .unwrap(),
        None
    );
}

#[test]
fn commands_do_not_mutate_process_current_directory() {
    let cwd = std::env::current_dir().unwrap();
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());

    git.init().unwrap();
    git.git_dir().unwrap();
    git.config_get("missing.key").unwrap();

    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[test]
fn failed_git_command_returns_stderr_context() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());

    let err = git.run_for_test(["rev-parse", "--git-dir"]).unwrap_err();
    assert!(err.contains("not a git repository") || err.contains("rev-parse"));
}

#[test]
fn tree_files_reads_modes_paths_and_content_from_commit() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    git.run_for_test(["config", "user.name", "Test User"])
        .unwrap();
    git.run_for_test(["config", "user.email", "test@example.com"])
        .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("README.md"), "hello\n").unwrap();
    std::fs::write(dir.path().join("src/run.sh"), "#!/bin/sh\n").unwrap();
    git.run_for_test(["add", "README.md", "src/run.sh"])
        .unwrap();
    git.run_for_test(["update-index", "--chmod=+x", "src/run.sh"])
        .unwrap();
    git.run_for_test(["commit", "-m", "base"]).unwrap();

    let files = git.tree_files("HEAD").unwrap();

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "README.md");
    assert_eq!(files[0].mode, "100644");
    assert_eq!(files[0].content, b"hello\n");
    assert_eq!(files[1].path, "src/run.sh");
    assert_eq!(files[1].mode, "100755");
    assert_eq!(files[1].content, b"#!/bin/sh\n");
}
