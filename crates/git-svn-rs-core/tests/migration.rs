use git_svn_rs_core::migration::{
    MigrationAction, ensure_supported_git_svn_metadata, inspect_git_svn_metadata,
};
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

#[test]
fn detects_v0_or_v1_root_metadata_layout() {
    let dir = tempdir().unwrap();
    let info = dir.path().join(".git/legacy/info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(info.join("url"), "file:///legacy\n").unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::NeedsLegacyLayoutMigration
    );
}

#[test]
fn detects_v2_metadata_layout() {
    let dir = tempdir().unwrap();
    let info = dir.path().join(".git/svn/legacy/info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(info.join("url"), "file:///legacy\n").unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::NeedsLegacyLayoutMigration
    );
}

#[test]
fn detects_mixed_rev_db_and_rev_map_without_mutation() {
    let dir = tempdir().unwrap();
    let svn = dir.path().join(".git/svn/git-svn");
    std::fs::create_dir_all(&svn).unwrap();
    let rev_db = svn.join(".rev_db.uuid");
    let rev_map = svn.join(".rev_map.uuid");
    std::fs::write(&rev_db, b"legacy").unwrap();
    std::fs::write(&rev_map, b"current").unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::MixedLayouts
    );
    assert!(ensure_supported_git_svn_metadata(dir.path()).is_err());
    assert_eq!(std::fs::read(rev_db).unwrap(), b"legacy");
    assert_eq!(std::fs::read(rev_map).unwrap(), b"current");
}

#[test]
fn v5_layout_allows_compatibility_info_url() {
    let dir = tempdir().unwrap();
    let svn = dir.path().join(".git/svn/git-svn");
    std::fs::create_dir_all(svn.join("info")).unwrap();
    std::fs::write(svn.join("info/url"), "file:///compat\n").unwrap();
    std::fs::write(svn.join(".rev_map.uuid"), []).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::AlreadyV5
    );
}

#[test]
fn detects_empty_svn_remote_section_without_rewriting_config() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let config = dir.path().join(".git/config");
    let original = b"[core]\n\trepositoryformatversion = 0\n[svn-remote \"empty\"]\n";
    std::fs::write(&config, original).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::NeedsConfigCleanup
    );
    assert!(ensure_supported_git_svn_metadata(dir.path()).is_err());
    assert_eq!(std::fs::read(config).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn ignores_rev_db_reached_only_through_a_symlink() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let svn = dir.path().join(".git/svn");
    let current = svn.join("refs/remotes/git-svn");
    let external = dir.path().join("external");
    std::fs::create_dir_all(&current).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(current.join(".rev_map.uuid"), []).unwrap();
    std::fs::write(external.join(".rev_db.uuid"), b"external").unwrap();
    symlink(&external, svn.join("external-link")).unwrap();
    symlink(&svn, svn.join("cycle")).unwrap();

    assert_eq!(
        inspect_git_svn_metadata(dir.path()).unwrap(),
        MigrationAction::AlreadyV5
    );
    assert_eq!(
        std::fs::read(external.join(".rev_db.uuid")).unwrap(),
        b"external"
    );
}
