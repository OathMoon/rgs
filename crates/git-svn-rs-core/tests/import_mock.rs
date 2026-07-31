use std::cell::RefCell;
use std::collections::BTreeMap;

use git_svn_rs_core::config::SvnRemoteConfig;
use git_svn_rs_core::git::GitCli;
use git_svn_rs_core::import::{
    ImportOptions, import_mock_revisions, import_mock_revisions_for_ref, import_ra_revisions,
    import_ra_revisions_for_ref,
};
use git_svn_rs_core::import_transaction::{
    begin_or_resume_batch, ensure_no_pending, mark_batch_mapping_completed,
};
use git_svn_rs_core::mapping::{MappingKind, RefMapping, build_single_path, build_standard_layout};
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap};
use git_svn_rs_core::svn::editor::FetchEditor;
use git_svn_rs_core::svn::mock::{MockRaSession, MockSvnBackend};
use git_svn_rs_core::svn::ra::{DirListing, RaSession, SvnNodeKind, UpdateRequest};
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
        dir.path()
            .join(".git/svn/refs/remotes/git-svn/.rev_map.mock-uuid"),
        ObjectFormat::Sha1,
    )
    .unwrap();
    assert!(rev_map.get(1).unwrap().is_some());
    assert!(rev_map.get(2).unwrap().is_some());
    assert!(git.refs_under("refs/git-svn-rs/import").unwrap().is_empty());
    assert!(!dir.path().join(".git/svn/import-journal").exists());
}

#[test]
fn mapping_collision_fails_before_publication_state_is_created() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_copy_revisions());
    let mut config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));
    config.fetch = vec![
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "trunk".to_string(),
            git_ref: "refs/remotes/origin/shared".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "branches/main".to_string(),
            git_ref: "refs/remotes/origin/shared".to_string(),
        },
    ];
    config.branches.clear();
    config.tags.clear();

    let error = import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap_err();

    assert!(error.contains("maps both SVN paths"));
    assert!(git.refs_under("refs/remotes").unwrap().is_empty());
    assert!(!dir.path().join(".git/svn/import-batch").exists());
    assert!(!dir.path().join(".git/svn/origin.shared").exists());
}

#[test]
fn candidate_ref_namespace_collision_with_existing_ref_fails_before_publication() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", revisions());
    let initial = SvnRemoteConfig::new("svn", "mock://repo/trunk", build_single_path(""));
    import_mock_revisions(
        &backend,
        &git,
        &initial,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap();
    let tip = git.rev_parse("refs/remotes/git-svn").unwrap();
    git.update_ref("refs/remotes/origin/topic/nested", tip.trim())
        .unwrap();

    let mut colliding = initial;
    colliding.fetch[0].git_ref = "refs/remotes/origin/topic".to_string();
    let error = import_mock_revisions(
        &backend,
        &git,
        &colliding,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
    )
    .unwrap_err();

    assert!(error.contains("cannot coexist"));
    assert!(!dir.path().join(".git/svn/import-batch").exists());
    assert!(!dir.path().join(".git/svn/origin.topic").exists());
}

#[test]
fn repeated_fixed_svn_path_uses_the_last_configured_destination() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", revisions());
    let mut config = SvnRemoteConfig::new("svn", "mock://repo/trunk", build_single_path(""));
    config.fetch = vec![
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: String::new(),
            git_ref: "refs/remotes/old".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: String::new(),
            git_ref: "refs/remotes/new".to_string(),
        },
    ];

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

    assert!(git.rev_parse("refs/remotes/new").is_ok());
    assert!(git.rev_parse("refs/remotes/old").is_err());
    assert!(
        dir.path()
            .join(".git/svn/refs/remotes/new/.rev_map.mock-uuid")
            .is_file()
    );
}

#[test]
fn wildcard_refnames_are_sanitized_reversibly() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new(
        "mock-uuid",
        vec![RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "space branch".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/topic name/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"topic\n".to_vec()),
            }],
        }],
    );
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(1),
        },
    )
    .unwrap();

    let refname = "refs/remotes/origin/topic%20name";
    assert_eq!(
        git.run_for_test(["show", &format!("{refname}:file.txt")])
            .unwrap(),
        "topic\n"
    );
    assert!(
        dir.path()
            .join(".git/svn/refs/remotes/origin/topic%20name/.rev_map.mock-uuid")
            .is_file()
    );
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
        dir.path()
            .join(".git/svn/refs/remotes/git-svn/.rev_map.mock-uuid"),
        ObjectFormat::Sha256,
    )
    .unwrap();
    let record = rev_map.get(2).unwrap().unwrap();
    assert_eq!(record.len(), 64);
}

#[test]
fn fixed_mapping_records_and_replaces_a_sparse_scan_marker() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));
    let initial = sparse_fixed_mapping_revisions(false);
    let backend = MockSvnBackend::new("mock-uuid", initial.clone());
    let options = ImportOptions {
        start_revision: 1,
        end_revision: Some(100),
    };

    import_mock_revisions(&backend, &git, &config, options).unwrap();

    let path = dir
        .path()
        .join(".git/svn/refs/remotes/origin/trunk/.rev_map.mock-uuid");
    let map = RevMap::open_existing(&path, ObjectFormat::Sha1).unwrap();
    let records = map.records().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].revision, 1);
    assert!(records[0].object_id_hex.bytes().any(|byte| byte != b'0'));
    assert_eq!(records[1].revision, 100);
    assert!(records[1].object_id_hex.bytes().all(|byte| byte == b'0'));
    assert_eq!(map.max_revision(false).unwrap(), Some(100));
    assert_eq!(map.max_revision(true).unwrap(), Some(1));
    let original_bytes = std::fs::read(&path).unwrap();

    import_mock_revisions(&backend, &git, &config, options).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

    let backend = MockSvnBackend::new("mock-uuid", sparse_fixed_mapping_revisions(true));
    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 101,
            end_revision: Some(101),
        },
    )
    .unwrap();

    let map = RevMap::open_existing(&path, ObjectFormat::Sha1).unwrap();
    let records = map.records().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].revision, 101);
    assert!(records[1].object_id_hex.bytes().any(|byte| byte != b'0'));
    assert_eq!(map.max_revision(true).unwrap(), Some(101));
}

#[test]
fn empty_log_window_still_records_the_fixed_mapping_scan_marker() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));
    let backend = MockSvnBackend::new("mock-uuid", Vec::new());

    let summary = import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(100),
        },
    )
    .unwrap();

    assert!(summary.imported_revisions.is_empty());
    assert!(git.rev_parse("refs/remotes/origin/trunk").is_err());
    let map = RevMap::open_existing(
        dir.path()
            .join(".git/svn/refs/remotes/origin/trunk/.rev_map.mock-uuid"),
        ObjectFormat::Sha1,
    )
    .unwrap();
    assert_eq!(map.max_revision(false).unwrap(), Some(100));
    assert_eq!(map.max_revision(true).unwrap(), None);
}

#[test]
fn branch_copy_import_uses_source_revision_as_parent() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""))
        .with_log_window_size(1);

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
fn bounded_branch_to_branch_copy_discovers_unchanged_source_mapping() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_to_branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""))
        .with_log_window_size(1);

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

    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/destination^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", "refs/remotes/origin/source"])
            .unwrap()
            .trim()
    );
}

#[test]
fn branch_copy_backfills_source_history_before_requested_start_revision() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_to_branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();

    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/destination^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", "refs/remotes/origin/source"])
            .unwrap()
            .trim()
    );
}

#[test]
fn unmapped_copy_source_reuses_auxiliary_branch_revision_ref() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", unmapped_source_copy_revisions());
    let config =
        SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout("")).with_ignore_refs("@");

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();

    let auxiliary = "refs/remotes/origin/destination@1";
    assert!(
        git.config_get_all("svn-remote.svn.fetch")
            .unwrap()
            .iter()
            .all(|mapping| !mapping.contains("destination@1"))
    );
    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/origin/destination^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", auxiliary]).unwrap().trim()
    );
}

#[test]
fn bounded_multi_level_copy_backfills_each_auxiliary_parent() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", multi_level_unmapped_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""))
        .with_log_window_size(1);

    import_mock_revisions(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(4),
        },
    )
    .unwrap();

    let destination = "refs/remotes/origin/destination";
    let middle = "refs/remotes/origin/destination@3";
    let root = "refs/remotes/origin/destination@2";
    assert_eq!(
        git.run_for_test(["rev-parse", &format!("{destination}^")])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", middle]).unwrap().trim()
    );
    assert_eq!(
        git.run_for_test(["rev-parse", &format!("{middle}^")])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", root]).unwrap().trim()
    );
}

#[test]
fn overlapping_copy_source_mappings_use_the_most_specific_parent() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", overlapping_copy_source_revisions());
    let mut config = SvnRemoteConfig::new("svn", "mock://repo", build_single_path(""));
    config.fetch = vec![
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "projects".to_string(),
            git_ref: "refs/remotes/broad".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "projects/trunk".to_string(),
            git_ref: "refs/remotes/trunk".to_string(),
        },
        RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "branches/topic".to_string(),
            git_ref: "refs/remotes/topic".to_string(),
        },
    ];

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

    assert_eq!(
        git.run_for_test(["rev-parse", "refs/remotes/topic^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", "refs/remotes/trunk"])
            .unwrap()
            .trim()
    );
    assert_ne!(
        git.run_for_test(["rev-parse", "refs/remotes/topic^"])
            .unwrap()
            .trim(),
        git.run_for_test(["rev-parse", "refs/remotes/broad"])
            .unwrap()
            .trim()
    );
    assert_eq!(
        git.run_for_test(["show", "refs/remotes/topic:a.txt"])
            .unwrap(),
        "source\n"
    );
    assert!(
        git.run_for_test(["show", "refs/remotes/topic:trunk/a.txt"])
            .is_err()
    );
}

#[test]
fn unfinished_multi_mapping_batch_resumes_without_republishing_completed_mapping() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));
    let options = ImportOptions {
        start_revision: 1,
        end_revision: Some(2),
    };
    let trunk_ref = "refs/remotes/origin/trunk";
    let branch_ref = "refs/remotes/origin/main";

    import_mock_revisions_for_ref(&backend, &git, &config, options, Some(trunk_ref)).unwrap();
    let original_trunk = git.rev_parse(trunk_ref).unwrap();
    let batch_refs = vec![trunk_ref.to_string(), branch_ref.to_string()];
    begin_or_resume_batch(&git, "mock-uuid", &batch_refs).unwrap();
    mark_batch_mapping_completed(&git, trunk_ref).unwrap();

    import_mock_revisions(&backend, &git, &config, options).unwrap();

    assert_eq!(git.rev_parse(trunk_ref).unwrap(), original_trunk);
    assert!(git.rev_parse(branch_ref).is_ok());
    ensure_no_pending(&git).unwrap();
}

#[test]
fn unfinished_ra_batch_preserves_completed_mapping_unhandled_log_exactly_once() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let session = PathFilteringRaSession::new();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));
    let options = ImportOptions {
        start_revision: 2,
        end_revision: Some(3),
    };
    let trunk_ref = "refs/remotes/origin/trunk";
    let branch_ref = "refs/remotes/origin/main";

    import_ra_revisions_for_ref(&session, &git, &config, options, Some(trunk_ref)).unwrap();
    let log_path = dir
        .path()
        .join(".git/svn/refs/remotes/origin/trunk/unhandled.log");
    let completed_log = std::fs::read(&log_path).unwrap();
    let completed_trunk = git.rev_parse(trunk_ref).unwrap();
    let batch_refs = vec![trunk_ref.to_string(), branch_ref.to_string()];
    begin_or_resume_batch(&git, "mock-uuid", &batch_refs).unwrap();
    mark_batch_mapping_completed(&git, trunk_ref).unwrap();

    import_ra_revisions(&session, &git, &config, options).unwrap();

    assert_eq!(git.rev_parse(trunk_ref).unwrap(), completed_trunk);
    assert!(git.rev_parse(branch_ref).is_ok());
    assert_eq!(std::fs::read(log_path).unwrap(), completed_log);
    ensure_no_pending(&git).unwrap();
}

#[test]
fn selected_ref_import_keeps_other_concrete_mappings_unchanged() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let backend = MockSvnBackend::new("mock-uuid", branch_copy_revisions());
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

    let summary = import_mock_revisions_for_ref(
        &backend,
        &git,
        &config,
        ImportOptions {
            start_revision: 1,
            end_revision: Some(2),
        },
        Some("refs/remotes/origin/trunk"),
    )
    .unwrap();

    assert_eq!(summary.imported_revisions, vec![1]);
    assert!(
        git.run_for_test(["rev-parse", "--verify", "refs/remotes/origin/trunk"])
            .is_ok()
    );
    assert!(
        git.run_for_test(["rev-parse", "--verify", "refs/remotes/origin/main"])
            .is_err()
    );
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
        dir.path()
            .join(".git/svn/refs/remotes/origin/trunk/.rev_map.mock-uuid"),
        ObjectFormat::Sha1,
    )
    .unwrap();
    assert!(rev_map.get(2).unwrap().is_some());
    assert_eq!(
        std::fs::read_to_string(
            dir.path()
                .join(".git/svn/refs/remotes/origin/trunk/unhandled.log")
        )
        .unwrap(),
        "r2\n  +file_prop: trunk/src/lib.rs svn:eol-style LF\n"
    );

    let repeated = import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 2,
            end_revision: Some(2),
        },
    )
    .unwrap();
    assert!(repeated.imported_revisions.is_empty());
    assert_eq!(
        std::fs::read_to_string(
            dir.path()
                .join(".git/svn/refs/remotes/origin/trunk/unhandled.log")
        )
        .unwrap(),
        "r2\n  +file_prop: trunk/src/lib.rs svn:eol-style LF\n"
    );
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
        [
            (
                "trunk".to_string(),
                UpdateRequest {
                    target_revision: 2,
                    base_revision: None,
                },
            ),
            (
                "branches/main".to_string(),
                UpdateRequest {
                    target_revision: 3,
                    base_revision: None,
                },
            ),
        ]
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
    assert_eq!(
        std::fs::read_to_string(
            dir.path()
                .join(".git/svn/refs/remotes/origin/trunk/unhandled.log")
        )
        .unwrap(),
        concat!(
            "r2\n",
            "  +dir_prop: trunk custom:dir-prop dir%20value\n",
            "  +file_prop: trunk/src/lib.rs svn:eol-style LF\n",
            "  absent_file: trunk/private%20file\n",
            "  absent_directory: trunk/private%20directory\n",
        )
    );
}

#[test]
fn ra_import_propagates_empty_and_existing_rev_map_update_bases() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();
    let mut session = PathFilteringRaSession::new();
    let config = SvnRemoteConfig::new("svn", "mock://repo", build_standard_layout(""));

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

    session.revisions = vec![RevisionEvent {
        revision: 3,
        author: "bob".to_string(),
        message: "update trunk again".to_string(),
        timestamp: "2026-01-03T00:00:00Z".to_string(),
        changed_paths: vec![ChangedPath {
            path: "/trunk/src/lib.rs".to_string(),
            action: ChangeAction::Modify,
            copy_from_path: None,
            copy_from_rev: None,
            kind: NodeKind::File,
            properties_modified: false,
            content_modified: true,
            properties: BTreeMap::new(),
            content: Some(b"pub fn trunk_again() {}\n".to_vec()),
        }],
    }];

    import_ra_revisions(
        &session,
        &git,
        &config,
        ImportOptions {
            start_revision: 3,
            end_revision: Some(3),
        },
    )
    .unwrap();

    assert_eq!(
        session.update_calls.borrow().as_slice(),
        [
            (
                "trunk".to_string(),
                UpdateRequest {
                    target_revision: 2,
                    base_revision: None,
                },
            ),
            (
                "trunk".to_string(),
                UpdateRequest {
                    target_revision: 3,
                    base_revision: Some(2),
                },
            ),
        ]
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
                properties_modified: true,
                content_modified: true,
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
                properties_modified: true,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
            }],
        },
    ]
}

struct PathFilteringRaSession {
    revisions: Vec<RevisionEvent>,
    update_calls: RefCell<Vec<(String, UpdateRequest)>>,
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
                            properties_modified: true,
                            content_modified: true,
                            properties: BTreeMap::new(),
                            content: Some(b"pub fn trunk() {}\n".to_vec()),
                        },
                        ChangedPath {
                            path: "/trunk/run.sh".to_string(),
                            action: ChangeAction::Add,
                            copy_from_path: None,
                            copy_from_rev: None,
                            kind: NodeKind::File,
                            properties_modified: true,
                            content_modified: true,
                            properties: BTreeMap::new(),
                            content: Some(b"#!/bin/sh\n".to_vec()),
                        },
                        ChangedPath {
                            path: "/trunk/empty-dir".to_string(),
                            action: ChangeAction::Add,
                            copy_from_path: None,
                            copy_from_rev: None,
                            kind: NodeKind::Directory,
                            properties_modified: true,
                            content_modified: false,
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
                        properties_modified: true,
                        content_modified: true,
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

    fn rev_properties(&self, _revision: u32) -> Result<BTreeMap<String, Vec<u8>>, String> {
        Ok(BTreeMap::new())
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
        self.drive_update(path, revision, editor)
    }

    fn do_update_from(
        &self,
        path: &str,
        request: UpdateRequest,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        self.update_calls
            .borrow_mut()
            .push((path.to_string(), request));
        self.drive_update(path, request.target_revision, editor)
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

impl PathFilteringRaSession {
    fn drive_update(
        &self,
        path: &str,
        revision: u32,
        editor: &mut dyn FetchEditor,
    ) -> Result<(), String> {
        match (path, revision) {
            ("trunk", 2) => drive_update(editor, "trunk", revision, b"pub fn trunk() {}\n"),
            ("trunk", 3) => drive_update(editor, "trunk", revision, b"pub fn trunk_again() {}\n"),
            ("branches/main", 3) => {
                drive_copy_update(editor, "branches/main", revision, b"pub fn branch() {}\n")
            }
            _ => Err(format!("unexpected update {path}@{revision}")),
        }
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
    editor.change_directory_prop(path, "custom:dir-prop", Some("dir value"))?;
    editor.add_directory(&format!("{path}/empty-dir"), None)?;
    editor.add_file(&format!("{path}/src/lib.rs"), None)?;
    editor.change_file_prop(&format!("{path}/src/lib.rs"), "svn:eol-style", Some("LF"))?;
    editor.apply_textdelta(&format!("{path}/src/lib.rs"), content)?;
    editor.add_file(&format!("{path}/run.sh"), None)?;
    editor.apply_textdelta(&format!("{path}/run.sh"), b"#!/bin/sh\n")?;
    editor.absent_file(&format!("{path}/private file"))?;
    editor.absent_directory(&format!("{path}/private directory"))?;
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
                properties_modified: true,
                content_modified: true,
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
                    properties_modified: true,
                    content_modified: false,
                    properties: BTreeMap::new(),
                    content: None,
                },
                ChangedPath {
                    path: "/branches/main/src/lib.rs".to_string(),
                    action: ChangeAction::Add,
                    copy_from_path: Some("/trunk/src/lib.rs".to_string()),
                    copy_from_rev: Some(1),
                    kind: NodeKind::File,
                    properties_modified: true,
                    content_modified: true,
                    properties: BTreeMap::new(),
                    content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
                },
            ],
        },
    ]
}

fn branch_to_branch_copy_revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "create source branch".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/source/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"source\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "alice".to_string(),
            message: "copy source branch".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/destination".to_string(),
                action: ChangeAction::Add,
                copy_from_path: Some("/branches/source".to_string()),
                copy_from_rev: Some(1),
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        },
    ]
}

fn overlapping_copy_source_revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "create nested trunk".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/projects/trunk/a.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"source\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "alice".to_string(),
            message: "copy nested trunk".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/topic".to_string(),
                action: ChangeAction::Add,
                copy_from_path: Some("/projects/trunk".to_string()),
                copy_from_rev: Some(1),
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        },
    ]
}

fn unmapped_source_copy_revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "create legacy source".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/legacy/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"legacy\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "alice".to_string(),
            message: "copy legacy into branches".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/destination".to_string(),
                action: ChangeAction::Add,
                copy_from_path: Some("/legacy".to_string()),
                copy_from_rev: Some(1),
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        },
    ]
}

fn multi_level_unmapped_copy_revisions() -> Vec<RevisionEvent> {
    vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "create legacy source".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/legacy/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"one\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 2,
            author: "alice".to_string(),
            message: "update legacy source".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/legacy/file.txt".to_string(),
                action: ChangeAction::Modify,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"two\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 3,
            author: "alice".to_string(),
            message: "first move".to_string(),
            timestamp: "2026-01-03T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/middle".to_string(),
                action: ChangeAction::Add,
                copy_from_path: Some("/legacy".to_string()),
                copy_from_rev: Some(2),
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        },
        RevisionEvent {
            revision: 4,
            author: "alice".to_string(),
            message: "second move".to_string(),
            timestamp: "2026-01-04T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/branches/destination".to_string(),
                action: ChangeAction::Add,
                copy_from_path: Some("/middle".to_string()),
                copy_from_rev: Some(3),
                kind: NodeKind::Directory,
                properties_modified: false,
                content_modified: false,
                properties: BTreeMap::new(),
                content: None,
            }],
        },
    ]
}

fn sparse_fixed_mapping_revisions(include_trunk_update: bool) -> Vec<RevisionEvent> {
    let mut revisions = vec![
        RevisionEvent {
            revision: 1,
            author: "alice".to_string(),
            message: "create trunk".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"one\n".to_vec()),
            }],
        },
        RevisionEvent {
            revision: 100,
            author: "alice".to_string(),
            message: "unrelated change".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/unrelated/file.txt".to_string(),
                action: ChangeAction::Add,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"unrelated\n".to_vec()),
            }],
        },
    ];
    if include_trunk_update {
        revisions.push(RevisionEvent {
            revision: 101,
            author: "alice".to_string(),
            message: "update trunk".to_string(),
            timestamp: "2026-01-03T00:00:00Z".to_string(),
            changed_paths: vec![ChangedPath {
                path: "/trunk/file.txt".to_string(),
                action: ChangeAction::Modify,
                copy_from_path: None,
                copy_from_rev: None,
                kind: NodeKind::File,
                properties_modified: false,
                content_modified: true,
                properties: BTreeMap::new(),
                content: Some(b"two\n".to_vec()),
            }],
        });
    }
    revisions
}
