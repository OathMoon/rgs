use git_svn_rs_core::migration::{MigrationAction, inspect_git_svn_metadata};
use tempfile::tempdir;

#[test]
fn detects_v5_rev_map() {
    let dir = tempdir().unwrap();
    let svn_dir = dir.path().join(".git/svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(svn_dir.join(".rev_map.uuid"), []).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::AlreadyV5
    );
}

#[test]
fn detects_old_rev_db_needing_migration() {
    let dir = tempdir().unwrap();
    let svn_dir = dir.path().join(".git/svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(svn_dir.join(".rev_db.uuid"), []).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::NeedsRevDbMigration
    );
}

#[test]
fn follows_gitfile_to_common_git_dir() {
    let dir = tempdir().unwrap();
    let git_dir = dir.path().join("actual.git");
    let worktree = dir.path().join("worktree");
    let svn_dir = git_dir.join("svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", git_dir.display()),
    )
    .unwrap();
    std::fs::write(svn_dir.join(".rev_map.uuid"), []).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(&worktree).unwrap(),
        MigrationAction::AlreadyV5
    );
}
