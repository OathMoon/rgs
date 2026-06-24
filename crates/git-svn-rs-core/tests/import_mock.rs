use std::collections::BTreeMap;

use git_svn_rs_core::config::SvnRemoteConfig;
use git_svn_rs_core::git::GitCli;
use git_svn_rs_core::import::{ImportOptions, import_mock_revisions};
use git_svn_rs_core::mapping::{build_single_path, build_standard_layout};
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};
use git_svn_rs_core::svn::mock::MockSvnBackend;
use git_svn_rs_core::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent};
use tempfile::tempdir;

#[test]
fn imports_mock_revisions_into_git_and_rev_map() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo/trunk", build_single_path(""));

    let summary = import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap();

    assert_eq!(summary.imported_revisions, vec![1, 2]);
    assert_eq!(
        git.run_for_test(["show", "-s", "--format=%s", "refs/remotes/git-svn"])
            .unwrap()
            .trim(),
        "update file"
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/git-svn:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );

    let rev_map = RevMap::open(
        dir.path().join(".git/svn/git-svn/.rev_map.mock-uuid"),
        ObjectFormat::Sha1,
    )
    .unwrap();
    assert!(rev_map.get(1).unwrap().is_some());
    assert!(rev_map.get(2).unwrap().is_some());
}

#[test]
fn imports_mock_revisions_into_sha256_git_repo() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.run_for_test(["init", "--object-format=sha256"])
        .unwrap();
    let backend = MockSvnBackend::new("mock-uuid", revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo/trunk", build_single_path(""));

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap();

    let rev_map = RevMap::open(
        dir.path().join(".git/svn/git-svn/.rev_map.mock-uuid"),
        ObjectFormat::Sha256,
    )
    .unwrap();
    let record = rev_map.get(2).unwrap().unwrap();
    assert_eq!(record.len(), 64);
}

#[test]
fn branch_copy_import_uses_source_revision_as_parent() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap();

    let trunk_commit = git
        .run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
        .unwrap();
    let branch_parent = git
        .run_for_test(["rev-parse", "refs/remotes/origin/main^"])
        .unwrap();

    assert_eq!(branch_parent.trim(), trunk_commit.trim());
}

fn revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "add file".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk/src/lib.rs".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties: BTreeMap::new(),
                content: Some(b"pub fn answer() -> u8 { 41 }\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "bob".to_string(),
            message: "update file".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk/src/lib.rs".to_string(),
                action: ChangeAction::Modify,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties: BTreeMap::new(),
                content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
            }],
        },
    ]
}

fn branch_copy_revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "add trunk".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk/src/lib.rs".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties: BTreeMap::new(),
                content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "alice".to_string(),
            message: "branch main".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![
                ChangedPath {
                    path: "/branches/main".to_string(),
                    action: ChangeAction::Add,
                    copy_from_path: Some("/trunk".to_string()),
                    copy_from_rev: Some(1),
                    kind: NodeKind::Directory,
                    properties: BTreeMap::new(),
                    content: None,
                },
                ChangedPath {
                    path: "/branches/main/src/lib.rs".to_string(),
                    action: ChangeAction::Add,
                    copy_from_path: Some("/trunk/src/lib.rs".to_string()),
                    copy_from_rev: Some(1),
                    kind: NodeKind::File,
                    properties: BTreeMap::new(),
                    content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
                },
            ],
        },
    ]
}
