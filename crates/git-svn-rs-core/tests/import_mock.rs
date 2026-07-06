use std::cell::RefCell;
use std::collections::BTreeMap;

use git_svn_rs_core::config::SvnRemoteConfig;
use git_svn_rs_core::git::GitCli;
use git_svn_rs_core::import::{ImportOptions, import_mock_revisions, import_ra_revisions};
use git_svn_rs_core::mapping::{build_single_path, build_standard_layout};
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};
use git_svn_rs_core::svn::editor::FetchEditor;
use git_svn_rs_core::svn::mock::{MockRaSession, MockSvnBackend};
use git_svn_rs_core::svn::ra::{DirListing, RaSession, SvnNodeKind};
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

#[test]
fn imports_ra_session_update_into_git_and_rev_map() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let session = MockRaSession::standard_fixture("mock-uuid");
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    let summary = import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();

    assert_eq!(summary.imported_revisions, vec![2]);
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:src/lib.rs"])
            .unwrap(),
        "pub fn answer() -> u8 { 42 }\n".to_string()
    );

    let rev_map = RevMap::open(
        dir.path().join(".git/svn/origin.trunk/.rev_map.mock-uuid"),
        ObjectFormat::Sha1,
    )
    .unwrap();
    assert!(rev_map.get(2).unwrap().is_some());
}

#[test]
fn ra_import_filters_revisions_per_mapping_before_replay() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let session = PathFilteringRaSession::new();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    let summary = import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(3),
        },
    )
    .unwrap();

    assert_eq!(summary.imported_revisions, vec![2, 3]);
    assert_eq!(
        session.update_calls.borrow().as_slice(),
        [("trunk".to_string(), 2), ("branches/main".to_string(), 3)]
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/main:src/lib.rs"])
            .unwrap(),
        "pub fn branch() {}\n".to_string()
    );
    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/main^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", "refs/remotes/origin/trunk"])
            .unwrap()
            .trim()
    );
}

#[test]
fn ra_import_applies_path_filters_to_editor_changes() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let session = PathFilteringRaSession::new();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""))
        .with_ignore_paths("^trunk/run\\.sh$");

    import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();

    let trunk_tree = git
        .run_for_test(["ls-tree", "-r", "--name-only", "refs/remotes/origin/trunk"])
        .unwrap();
    assert!(trunk_tree.lines().any(|line| line == "src/lib.rs"));
    assert!(!trunk_tree.lines().any(|line| line == "run.sh"));
}

#[test]
fn ra_import_preserves_empty_directories_with_placeholder() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let session = PathFilteringRaSession::new();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""))
        .with_preserve_empty_dirs(".gitkeep");

    import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();

    assert_eq!(
        git.run_for_test(["show", "refs/remotes/origin/trunk:empty-dir/.gitkeep"])
            .unwrap(),
        String::new()
    );
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

struct PathFilteringRaSession {
    revisions: Vec<RevisionEvent>,
    update_calls: RefCell<Vec<(String, u32)>>,
}

impl PathFilteringRaSession {
    fn new() -> Self {
        Self {
            revisions: vec![
                RevisionEvent {
                    revision: 2,
                    author: "alice".to_string(),
                    message: "update trunk".to_string(),
                    timestamp: "2026-01-02T00:00:00Z".to_string(),
                    changed_paths: vec![
                        ChangedPath {
                            path: "/trunk/src/lib.rs".to_string(),
                            action: ChangeAction::Modify,
                            copy_from_path: None,
                            copy_from_rev: None,
                            kind: NodeKind::File,
                            properties: BTreeMap::new(),
                            content: Some(b"pub fn trunk() {}\n".to_vec()),
                        },
                        ChangedPath {
                            path: "/trunk/run.sh".to_string(),
                            action: ChangeAction::Add,
                            copy_from_path: None,
                            copy_from_rev: None,
                            kind: NodeKind::File,
                            properties: BTreeMap::new(),
                            content: Some(b"#!/bin/sh\n".to_vec()),
                        },
                        ChangedPath {
                            path: "/trunk/empty-dir".to_string(),
                            action: ChangeAction::Add,
                            copy_from_path: None,
                            copy_from_rev: None,
                            kind: NodeKind::Directory,
                            properties: BTreeMap::new(),
                            content: None,
                        },
                    ],
                },
                RevisionEvent {
                    revision: 3,
                    author: "bob".to_string(),
                    message: "add branch".to_string(),
                    timestamp: "2026-01-03T00:00:00Z".to_string(),
                    changed_paths: vec![ChangedPath {
                        path: "/branches/main/src/lib.rs".to_string(),
                        action: ChangeAction::Add,
                        copy_from_path: Some("/trunk/src/lib.rs".to_string()),
                        copy_from_rev: Some(2),
                        kind: NodeKind::File,
                        properties: BTreeMap::new(),
                        content: Some(b"pub fn branch() {}\n".to_vec()),
                    }],
                },
            ],
            update_calls: RefCell::new(Vec::new()),
        }
    }
}

impl RaSession for PathFilteringRaSession {
    fn url(&self) -> &str {
        "mock://repo"
    }

    fn repos_root(&self) -> &str {
        "mock://repo"
    }

    fn uuid(&self) -> Result<String, String> {
        Ok("mock-uuid".to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Ok(3)
    }

    fn check_path(&self, _path: &str, _revision: u32) -> Result<Option<SvnNodeKind>, String> {
        Ok(None)
    }

    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String> {
        Err(format!("unexpected get_dir {path}@{revision}"))
    }

    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        if paths == ["branches/main"] && start < 3 {
            return Err("branch path did not exist at requested start revision".to_string());
        }

        Ok(self
            .revisions
            .iter()
            .filter(|revision| revision.revision >= start && revision.revision <= end)
            .filter(|revision| {
                paths.is_empty()
                    || revision.changed_paths.iter().any(|changed_path| {
                        paths.iter().any(|path| {
                            changed_path
                                .path
                                .trim_matches('/')
                                .starts_with(path.trim_matches('/'))
                        })
                    })
            })
            .cloned()
            .collect())
    }

    fn do_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        match (path, revision) {
            ("trunk", 2) => {
                self.update_calls
                    .borrow_mut()
                    .push((path.to_string(), revision));
                drive_update(editor, "trunk", revision, b"pub fn trunk() {}\n")
            }
            ("branches/main", 3) => {
                self.update_calls
                    .borrow_mut()
                    .push((path.to_string(), revision));
                drive_copy_update(editor, "branches/main", revision, b"pub fn branch() {}\n")
            }
            _ => Err(format!("unexpected update {path}@{revision}")),
        }
    }

    fn do_switch(
        &self,
        path: &str,
        revision: u32,
        _switch_url: &str,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        self.do_update(path, revision, editor)
    }
}

fn drive_update(
    editor: &mut dyn FetchEditor,
    path: &str,
    revision: u32,
    content: &[u8],
) -> Result<(), String> {
    editor.open_root(revision)?;
    editor.add_directory(path, None)?;
    editor.add_directory(&format!("{path}/empty-dir"), None)?;
    editor.add_file(&format!("{path}/src/lib.rs"), None)?;
    editor.apply_textdelta(&format!("{path}/src/lib.rs"), content)?;
    editor.add_file(&format!("{path}/run.sh"), None)?;
    editor.apply_textdelta(&format!("{path}/run.sh"), b"#!/bin/sh\n")?;
    editor.close_edit()
}

fn drive_copy_update(
    editor: &mut dyn FetchEditor,
    path: &str,
    revision: u32,
    content: &[u8],
) -> Result<(), String> {
    editor.open_root(revision)?;
    editor.add_directory(path, Some(("trunk", 2)))?;
    editor.add_file(&format!("{path}/src/lib.rs"), Some(("trunk/src/lib.rs", 2)))?;
    editor.apply_textdelta(&format!("{path}/src/lib.rs"), content)?;
    editor.close_edit()
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
