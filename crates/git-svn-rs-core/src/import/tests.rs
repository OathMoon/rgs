use super::{
    ImportRuntime, apply_placeholder_log, author_ident, changes_for_revision, commit_message,
    imports_initial_mapping_root, max_imported_revision, most_specific_mapping_for_path,
    rev_map_path, svn_git_timestamp, validate_mapping_ref_collisions, validate_refname_namespace,
};
use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::mapping::{MappingKind, RefMapping};
use crate::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn parses_svn_rfc3339_date_with_fractional_seconds() {
    let timestamp = svn_git_timestamp("2026-01-01T00:00:00.123456Z", false).unwrap();
    assert_eq!(timestamp.seconds, 1_767_225_600);
    assert_eq!(timestamp.offset, "+0000");
}

#[test]
fn rejects_missing_or_invalid_svn_dates() {
    assert!(svn_git_timestamp("", false).is_err());
    assert!(svn_git_timestamp("not-a-date", false).is_err());
}

#[test]
fn missing_rev_map_reports_zero_without_creating_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let git = GitCli::new(temp.path());
    git.init().unwrap();
    let path = rev_map_path(&git, "refs/remotes/git-svn", "uuid").unwrap();

    assert_eq!(
        max_imported_revision(&git, "refs/remotes/git-svn", "uuid").unwrap(),
        0
    );
    assert!(!path.exists());
    assert!(!path.parent().unwrap().exists());
}

#[test]
fn copy_source_mapping_prefers_the_most_specific_path_with_root_fallback() {
    let mappings = vec![
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: String::new(),
            git_ref: "refs/remotes/root".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "projects".to_string(),
            git_ref: "refs/remotes/projects".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "projects/trunk".to_string(),
            git_ref: "refs/remotes/trunk".to_string(),
        },
    ];

    assert_eq!(
        most_specific_mapping_for_path(&mappings, "projects/trunk/src/lib.rs")
            .unwrap()
            .git_ref,
        "refs/remotes/trunk"
    );
    assert_eq!(
        most_specific_mapping_for_path(&mappings, "unmapped/file.txt")
            .unwrap()
            .git_ref,
        "refs/remotes/root"
    );
}

#[test]
fn placeholder_log_replays_only_events_visible_at_the_rev_map_tip() {
    let log = "\
r2
  +empty_dir: trunk/empty%20directory
r3
  -empty_dir: trunk/empty%20directory
r4
  +empty_dir: trunk/empty%20directory
";
    let mut at_r2 = BTreeSet::new();
    apply_placeholder_log(log, "trunk", 2, &mut at_r2).unwrap();
    assert_eq!(at_r2, ["empty directory".to_string()].into_iter().collect());

    let mut at_r3 = BTreeSet::new();
    apply_placeholder_log(log, "trunk", 3, &mut at_r3).unwrap();
    assert!(at_r3.is_empty());

    let mut at_r4 = BTreeSet::new();
    apply_placeholder_log(log, "trunk", 4, &mut at_r4).unwrap();
    assert_eq!(at_r4, ["empty directory".to_string()].into_iter().collect());
}

#[test]
fn placeholder_log_ignores_other_mappings_and_rejects_bad_uri_encoding() {
    let mut ownership = BTreeSet::new();
    apply_placeholder_log(
        "r2\n  +empty_dir: branches/topic/empty\n",
        "trunk",
        2,
        &mut ownership,
    )
    .unwrap();
    assert!(ownership.is_empty());
    assert!(
        apply_placeholder_log(
            "r2\n  +empty_dir: trunk/bad%2\n",
            "trunk",
            2,
            &mut ownership,
        )
        .unwrap_err()
        .contains("invalid URI encoding")
    );
}

#[test]
fn default_author_identity_uses_repository_uuid() {
    assert_eq!(
        author_ident("alice", "repo-uuid", None).unwrap(),
        "alice <alice@repo-uuid>"
    );
}

#[test]
fn immutable_import_state_is_initialized_once() {
    let mut config = SvnRemoteConfig::new(
        "svn",
        "file:///repo".to_string(),
        crate::mapping::LayoutMappings {
            fetch: Vec::new(),
            branches: Vec::new(),
            tags: Vec::new(),
        },
    );
    config.include_paths = Some("^trunk/".to_string());
    config.ignore_paths = Some("^trunk/generated/".to_string());
    let authors_dir = tempfile::tempdir().unwrap();
    let authors_path = authors_dir.path().join("authors.txt");
    std::fs::write(&authors_path, "alice = Alice Example <alice@example.com>\n").unwrap();
    config.authors_file = Some(authors_path.display().to_string());

    let revision = RevisionEvent {
        revision: 1,
        author: "alice".to_string(),
        message: "change".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        changed_paths: vec![ChangedPath {
            path: "/trunk/src/lib.rs".to_string(),
            action: ChangeAction::Modify,
            copy_from_path: None,
            copy_from_rev: None,
            kind: NodeKind::File,
            properties_modified: false,
            content_modified: true,
            properties: BTreeMap::new(),
            content: Some(b"pub fn value() -> u8 { 1 }\n".to_vec()),
        }],
    };
    let runtime = ImportRuntime::default();

    crate::filters::reset_regex_compilation_count();
    for _ in 0..100 {
        assert_eq!(
            changes_for_revision(&revision, "trunk", &config, &runtime)
                .unwrap()
                .len(),
            1
        );
    }
    assert_eq!(crate::filters::regex_compilation_count(), 2);

    let first_authors = runtime.authors(&config).unwrap();
    let second_authors = runtime.authors(&config).unwrap();
    assert!(std::ptr::eq(first_authors, second_authors));
    assert_eq!(
        author_ident("alice", "repo-uuid", Some(first_authors)).unwrap(),
        "Alice Example <alice@example.com>"
    );
}

#[test]
fn metadata_commit_message_has_the_frozen_trailing_newline() {
    let config = SvnRemoteConfig::new(
        "svn",
        "file:///repo".to_string(),
        crate::mapping::LayoutMappings {
            fetch: vec![crate::mapping::RefMapping {
                kind: crate::mapping::MappingKind::Fetch,
                svn_path: "trunk".to_string(),
                git_ref: "refs/remotes/git-svn".to_string(),
            }],
            branches: Vec::new(),
            tags: Vec::new(),
        },
    );
    let revision = RevisionEvent {
        revision: 1,
        author: "alice".to_string(),
        message: "layout".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        changed_paths: Vec::new(),
    };

    assert_eq!(
        commit_message(&config, &revision, "repo-uuid", "trunk").unwrap(),
        "layout\n\ngit-svn-id: file:///repo/trunk@1 repo-uuid\n"
    );
}

#[test]
fn initial_mapping_root_addition_is_imported_as_an_empty_commit() {
    let revision = RevisionEvent {
        revision: 1,
        author: "alice".to_string(),
        message: "layout".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        changed_paths: vec![ChangedPath {
            path: "/trunk".to_string(),
            action: ChangeAction::Add,
            copy_from_path: None,
            copy_from_rev: None,
            kind: NodeKind::Directory,
            properties_modified: false,
            content_modified: false,
            properties: BTreeMap::new(),
            content: None,
        }],
    };

    assert!(imports_initial_mapping_root(&revision, "trunk", true));
    assert!(!imports_initial_mapping_root(&revision, "trunk", false));
    assert!(!imports_initial_mapping_root(
        &revision,
        "branches/main",
        true
    ));
}

#[test]
fn rejects_git_ref_file_directory_collisions() {
    let refs = vec![
        "refs/remotes/origin/topic".to_string(),
        "refs/remotes/origin/topic/nested".to_string(),
    ];

    let error = validate_refname_namespace(&refs).unwrap_err();
    assert!(error.contains("cannot coexist"));
    assert!(error.contains("refs/remotes/origin/topic"));
    assert!(error.contains("refs/remotes/origin/topic/nested"));
}

#[test]
fn rejects_distinct_svn_paths_mapped_to_the_same_ref() {
    let first = RefMapping {
        kind: MappingKind::Branches,
        svn_path: "branches/one".to_string(),
        git_ref: "refs/remotes/origin/topic".to_string(),
    };
    let second = RefMapping {
        kind: MappingKind::Tags,
        svn_path: "tags/one".to_string(),
        git_ref: first.git_ref.clone(),
    };

    let error = validate_mapping_ref_collisions(&[&first, &second]).unwrap_err();
    assert!(error.contains("maps both"));
    assert!(error.contains("branches/one"));
    assert!(error.contains("tags/one"));
}
