use std::collections::BTreeMap;

use git_svn_rs_core::config::SvnRemoteConfig;
use git_svn_rs_core::git::GitCli;
use git_svn_rs_core::import::{ImportOptions, import_mock_revisions};
use git_svn_rs_core::mapping::build_single_path;
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
