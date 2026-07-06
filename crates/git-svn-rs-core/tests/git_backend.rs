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
