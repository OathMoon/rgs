use crate::cli::{FetchArgs, SharedFetchArgs};
use crate::config::{SvnRemoteConfig, read_svn_remote_config, svn_remote_names};
use crate::git::GitCli;
use crate::glob_spec::GlobSpec;
use crate::import::{ImportOptions, import_mock_revisions_for_ref, import_ra_revisions_for_ref};
use crate::mapping::{MappingKind, RefMapping};
use crate::path_url::{SvnUrlProfile, svn_url_profile};
use crate::rev_map::RevMap;
use crate::svn::SvnBackend;
use crate::svn::auth::{AuthOperation, Credentials, prompted_credentials};
use crate::svn::cli::SvnCliBackend;
use crate::svn::mock::MockRaSession;
use crate::svn::ra::RaSession;
use std::cmp;

mod mirror_identity;
mod preflight;
mod runtime;

use mirror_identity::*;
use preflight::*;
pub(crate) use runtime::effective_fetch_config;
use runtime::*;

pub fn run(args: FetchArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FetchArgs,
) -> Result<(), String> {
    if args.fetch_all && args.remote.is_some() {
        return Err("fetch cannot combine a remote name with --fetch-all".to_string());
    }
    if args.parent && args.fetch_all {
        return Err("fetch --parent cannot be combined with --fetch-all".to_string());
    }
    let work_tree = work_tree.into();
    let git = GitCli::new(work_tree);
    verify_remote_fetch_ref_sanity(&git)?;
    let migration = crate::migration::inspect_git_svn_metadata(git.work_tree())?;
    if migration != crate::migration::MigrationAction::NoGitSvnMetadata {
        crate::migration::ensure_supported_git_svn_metadata(git.work_tree())?;
    }
    validate_requested_urls_before_recovery(&git, &args)?;
    if migration == crate::migration::MigrationAction::NoGitSvnMetadata {
        crate::migration::ensure_supported_git_svn_metadata(git.work_tree())?;
    }
    crate::import_transaction::recover_pending(&git)?;
    if args.parent {
        let tracked =
            crate::commands::resolver::resolve_tracked_svn_allow_import_batch(git.work_tree())?;
        if let Some(remote) = &args.remote
            && remote != &tracked.config.name
        {
            return Err(format!(
                "fetch --parent resolved SVN remote {}, not requested remote {remote}",
                tracked.config.name
            ));
        }
        return fetch_config(&git, tracked.config, &args.shared, Some(&tracked.refname));
    }
    let remotes = if args.fetch_all {
        svn_remote_names(&git)?
    } else {
        vec![args.remote.clone().unwrap_or_else(|| "svn".to_string())]
    };

    for remote in remotes {
        fetch_remote(&git, &remote, &args.shared)?;
    }
    Ok(())
}

fn fetch_remote(git: &GitCli, remote: &str, shared: &SharedFetchArgs) -> Result<(), String> {
    let config = read_svn_remote_config(git, remote)?;
    fetch_config(git, config, shared, None)
}

pub(crate) fn run_for_tracking_identity(
    work_tree: impl Into<std::path::PathBuf>,
    config: SvnRemoteConfig,
    refname: &str,
    shared: &SharedFetchArgs,
) -> Result<(), String> {
    let work_tree = work_tree.into();
    crate::path_url::validate_fetch_url(&config.url)?;
    crate::migration::ensure_supported_git_svn_metadata(&work_tree)?;
    let git = GitCli::new(work_tree);
    verify_remote_fetch_ref_sanity(&git)?;
    crate::import_transaction::recover_pending(&git)?;
    fetch_config(&git, config, shared, Some(refname))
}

pub(crate) fn run_for_tracking_remote(
    work_tree: impl Into<std::path::PathBuf>,
    config: SvnRemoteConfig,
    shared: &SharedFetchArgs,
) -> Result<(), String> {
    let work_tree = work_tree.into();
    crate::path_url::validate_fetch_url(&config.url)?;
    crate::migration::ensure_supported_git_svn_metadata(&work_tree)?;
    let git = GitCli::new(work_tree);
    verify_remote_fetch_ref_sanity(&git)?;
    crate::import_transaction::recover_pending(&git)?;
    fetch_config(&git, config, shared, None)
}

fn fetch_config(
    git: &GitCli,
    config: SvnRemoteConfig,
    shared: &SharedFetchArgs,
    selected_ref: Option<&str>,
) -> Result<(), String> {
    crate::path_url::validate_fetch_url(&config.url)?;
    config.validate_mapping_destinations()?;
    validate_existing_tracking_states(git, &config, selected_ref)?;
    if config.url.starts_with("mock://") {
        let session = MockRaSession::standard_fixture("mock-uuid");
        let base_revision = imported_base_revision(git, &config, "mock-uuid", selected_ref)?;
        let mut config = effective_fetch_config(config, shared, base_revision)?;
        hydrate_svnsync_identity(git, &mut config, || session.rev_properties(0))?;
        let start_revision = base_revision.saturating_add(1);
        let head_revision = session.latest_revnum()?;
        let import_options =
            import_options(start_revision, head_revision, shared.revision.as_deref())?;
        let scanned_end = import_options
            .end_revision
            .unwrap_or(head_revision)
            .min(head_revision);
        import_mock_revisions_for_ref(
            &MockBackendFromSession(&session),
            git,
            &config,
            import_options,
            selected_ref,
        )?;
        persist_repository_identity(git, &config, session.repos_root(), "mock-uuid")?;
        persist_discovery_high_water(git, &config, selected_ref, scanned_end)?;
        return Ok(());
    }

    let backend = configured_backend(&config, shared)?;
    let uuid = backend.uuid()?;
    let repos_root = backend.repository_root()?;
    let base_revision = imported_base_revision(git, &config, &uuid, selected_ref)?;
    let mut config = effective_fetch_config(config, shared, base_revision)?;
    let head_revision = backend.latest_revnum()?;
    hydrate_svm_identity(
        git,
        &mut config,
        selected_ref,
        &repos_root,
        head_revision,
        |path, revision| backend.node_property_bytes(&repos_root, path, revision),
    )?;
    if config.use_svm_props {
        persist_repository_identity(git, &config, &repos_root, &uuid)?;
    }
    hydrate_svnsync_identity(git, &mut config, || backend.rev_properties(0))?;
    let start_revision = base_revision.saturating_add(1);
    let import_options = import_options(start_revision, head_revision, shared.revision.as_deref())?;
    let scanned_end = import_options
        .end_revision
        .unwrap_or(head_revision)
        .min(head_revision);
    backend.import_revisions(git, &config, import_options, selected_ref)?;
    persist_repository_identity(git, &config, &repos_root, &uuid)?;
    persist_discovery_high_water(git, &config, selected_ref, scanned_end)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::build_single_path;

    #[test]
    fn configured_backend_prefers_linked_libsvn_and_otherwise_uses_svn_cli() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let backend = configured_backend(&config, &default_shared_args()).unwrap();
        let expected = if cfg!(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)) {
            "libsvn"
        } else {
            "svn-cli"
        };

        assert_eq!(backend.kind(), expected);
    }

    #[test]
    fn configured_backend_uses_ra_editor_import_for_every_backend() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let backend = configured_backend(&config, &default_shared_args()).unwrap();
        assert_eq!(backend.import_mode(), "ra-editor");
    }

    #[test]
    fn configured_backend_applies_command_line_username_without_password() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""))
            .with_username("persisted");
        let mut shared = default_shared_args();
        shared.username = Some("cli-user".to_string());

        let backend = configured_backend(&config, &shared).unwrap();

        assert_eq!(backend.configured_username(), Some("cli-user"));
    }

    #[test]
    fn configured_backend_applies_command_line_config_dir_override() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""))
            .with_config_dir("persisted-config");
        let mut shared = default_shared_args();
        shared.config_dir = Some("cli-config".to_string());

        let backend = configured_backend(&config, &shared).unwrap();

        assert_eq!(backend.configured_config_dir(), Some("cli-config"));
    }

    #[test]
    fn configured_backend_applies_command_line_password_without_username() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.password = Some("secret".to_string());

        let backend = configured_backend(&config, &shared).unwrap();

        assert_eq!(backend.configured_password(), Some("secret"));
    }

    #[test]
    fn revision_ranges_resolve_head_base_and_numeric_forms() {
        assert_eq!(
            import_options(6, 10, Some("5:HEAD")).unwrap(),
            ImportOptions {
                start_revision: 6,
                end_revision: Some(10),
            }
        );
        assert_eq!(
            import_options(6, 10, Some("BASE:8")).unwrap(),
            ImportOptions {
                start_revision: 6,
                end_revision: Some(8),
            }
        );
        assert_eq!(
            import_options(6, 10, Some("BASE:HEAD")).unwrap(),
            ImportOptions {
                start_revision: 6,
                end_revision: Some(10),
            }
        );
        assert_eq!(
            import_options(6, 10, Some("HEAD")).unwrap(),
            ImportOptions {
                start_revision: 10,
                end_revision: Some(10),
            }
        );
        assert_eq!(
            import_options(6, 10, Some("007")).unwrap(),
            ImportOptions {
                start_revision: 7,
                end_revision: Some(7),
            }
        );
    }

    #[test]
    fn invalid_revision_keyword_is_rejected() {
        for invalid in [
            "BASE",
            "HEAD:7",
            ":7",
            "7:",
            ":",
            "r7",
            " 7",
            "7 ",
            "head",
            "base:7",
            "1:2:3",
            "-1",
            "4294967296",
            "PREV:HEAD",
        ] {
            assert!(
                import_options(1, 10, Some(invalid)).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    #[test]
    fn mapping_destination_must_stay_under_refs() {
        let error = parse_mapping("trunk:../../escape", MappingKind::Fetch).unwrap_err();
        assert!(error.contains("must begin with refs/"));
    }

    #[test]
    fn base_revision_uses_slowest_rev_map_for_uuid() {
        let tmp = tempfile::tempdir().unwrap();
        let git = GitCli::new(tmp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        config.fetch = vec![
            RefMapping {
                kind: MappingKind::Fetch,
                svn_path: "trunk".to_string(),
                git_ref: "refs/remotes/origin/trunk".to_string(),
            },
            RefMapping {
                kind: MappingKind::Fetch,
                svn_path: "branches/main".to_string(),
                git_ref: "refs/remotes/origin/branch".to_string(),
            },
        ];
        let git_dir = tmp.path().join(".git/svn");
        let object_id = "11".repeat(20);
        for (short_ref, revision) in [("origin.trunk", 10), ("origin.branch", 5)] {
            let mut rev_map = RevMap::open(
                git_dir.join(short_ref).join(".rev_map.uuid"),
                crate::rev_map::ObjectFormat::Sha1,
            )
            .unwrap();
            rev_map.append(revision, &object_id).unwrap();
        }

        assert_eq!(
            imported_base_revision(&git, &config, "uuid", None).unwrap(),
            5
        );
    }

    #[test]
    fn wildcard_base_uses_persisted_discovery_high_water() {
        let tmp = tempfile::tempdir().unwrap();
        let git = GitCli::new(tmp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        config.fetch[0].git_ref = "refs/remotes/origin/trunk".to_string();
        config.branches = vec![RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/*".to_string(),
            git_ref: "refs/remotes/origin/*".to_string(),
        }];
        let mut rev_map = RevMap::open(
            tmp.path().join(".git/svn/origin.trunk/.rev_map.uuid"),
            crate::rev_map::ObjectFormat::Sha1,
        )
        .unwrap();
        rev_map.append(10, &"11".repeat(20)).unwrap();
        git.git_svn_metadata_set("svn-remote.svn.branches-maxRev", "7")
            .unwrap();

        assert_eq!(
            imported_base_revision(&git, &config, "uuid", None).unwrap(),
            7
        );
        assert_eq!(
            imported_base_revision(&git, &config, "uuid", Some("refs/remotes/origin/trunk"))
                .unwrap(),
            10
        );
    }

    #[test]
    fn discovery_high_water_is_monotonic_and_parent_fetch_does_not_advance_it() {
        let tmp = tempfile::tempdir().unwrap();
        let git = GitCli::new(tmp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        config.branches = vec![RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/*".to_string(),
            git_ref: "refs/remotes/origin/*".to_string(),
        }];
        config.tags = vec![RefMapping {
            kind: MappingKind::Tags,
            svn_path: "tags/*".to_string(),
            git_ref: "refs/remotes/origin/tags/*".to_string(),
        }];

        persist_discovery_high_water(&git, &config, None, 8).unwrap();
        persist_discovery_high_water(&git, &config, None, 5).unwrap();
        persist_discovery_high_water(&git, &config, Some("refs/remotes/origin/trunk"), 12).unwrap();

        assert_eq!(
            discovery_high_water(&git, &config, "branches").unwrap(),
            Some(8)
        );
        assert_eq!(
            discovery_high_water(&git, &config, "tags").unwrap(),
            Some(8)
        );
    }

    #[test]
    fn runtime_fetch_options_overlay_persisted_config_without_writing_it() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""))
            .with_ignore_paths("persisted")
            .with_include_paths("included");
        let mut shared = default_shared_args();
        shared.ignore_paths = Some("runtime".to_string());
        shared.include_paths = Some("more".to_string());
        shared.authors_file = Some("authors.txt".to_string());
        shared.preserve_empty_dirs = true;
        shared.placeholder_filename = ".keep".to_string();

        let effective = effective_fetch_config(config, &shared, 0).unwrap();

        assert_eq!(
            effective.ignore_paths.as_deref(),
            Some("(?:persisted)|(?:runtime)")
        );
        assert_eq!(
            effective.include_paths.as_deref(),
            Some("(?:included)|(?:more)")
        );
        assert_eq!(effective.authors_file.as_deref(), Some("authors.txt"));
        assert!(effective.preserve_empty_dirs);
        assert_eq!(effective.placeholder_filename, ".keep");
    }

    #[test]
    fn placeholder_filename_without_preserve_empty_dirs_is_a_noop() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.placeholder_filename = ".custom-empty".to_string();

        let effective = effective_fetch_config(config, &shared, 0).unwrap();

        assert!(!effective.preserve_empty_dirs);
        assert_eq!(effective.placeholder_filename, ".gitignore");
    }

    #[test]
    fn metadata_identity_overrides_fail_after_import() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.rewrite_root = Some("file:///other".to_string());

        assert!(
            effective_fetch_config(config, &shared, 1)
                .unwrap_err()
                .contains("cannot change")
        );
    }

    #[test]
    fn svnsync_props_can_only_be_enabled_before_import() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.use_svnsync_props = true;

        assert!(
            effective_fetch_config(config.clone(), &shared, 0)
                .unwrap()
                .use_svnsync_props
        );
        assert!(
            effective_fetch_config(config, &shared, 1)
                .unwrap_err()
                .contains("--use-svnsync-props cannot change")
        );
    }

    fn svm_config(url: &str, svn_path: &str) -> SvnRemoteConfig {
        let mut config = SvnRemoteConfig::new("svn", url, build_single_path(svn_path));
        config.fetch[0].svn_path = svn_path.to_string();
        config.use_svm_props = true;
        config
    }

    #[test]
    fn svm_source_parser_normalizes_bang_username_and_trailing_slashes() {
        assert_eq!(
            normalize_svm_source(b"https://user:secret@origin.example/source!/nested///\n")
                .unwrap(),
            "https://origin.example/source/nested"
        );
        assert_eq!(normalize_svm_source(b"junk///").unwrap(), "junk");
    }

    #[test]
    fn svm_identity_discovers_an_ancestor_and_caches_all_keys_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = svm_config("file:///repo", "trunk/project");
        let mut visited = Vec::new();

        hydrate_svm_identity(
            &git,
            &mut config,
            None,
            "file:///repo",
            9,
            |path, revision| {
                visited.push((path.to_string(), revision));
                Ok(match path {
                    "trunk/project" => std::collections::BTreeMap::from([(
                        "svm:source".to_string(),
                        b"partial".to_vec(),
                    )]),
                    "trunk" => std::collections::BTreeMap::from([
                        (
                            "svm:source".to_string(),
                            b"https://user@origin/source!/project///\n".to_vec(),
                        ),
                        (
                            "svm:uuid".to_string(),
                            b"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee\n".to_vec(),
                        ),
                    ]),
                    _ => std::collections::BTreeMap::new(),
                })
            },
        )
        .unwrap();

        assert_eq!(
            visited,
            vec![("trunk/project".to_string(), 9), ("trunk".to_string(), 9)]
        );
        assert_eq!(
            config.svm_source.as_deref(),
            Some("https://origin/source/project")
        );
        assert_eq!(config.svm_replace.as_deref(), Some("file:///repo/trunk"));
        assert_eq!(
            config.svm_uuid.as_deref(),
            Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        );
        for (key, expected) in [
            ("svm-source", "https://origin/source/project"),
            ("svm-replace", "file:///repo/trunk"),
            ("svm-uuid", "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"),
        ] {
            assert_eq!(
                git.git_svn_metadata_get(&format!("svn-remote.svn.{key}"))
                    .unwrap()
                    .as_deref(),
                Some(expected)
            );
            assert_eq!(
                git.config_get(&format!("svn-remote.svn.{key}")).unwrap(),
                None
            );
        }
    }

    #[test]
    fn svm_identity_walks_from_a_direct_subdirectory_to_repository_root() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = svm_config("file:///repo/trunk", "");
        let mut visited = Vec::new();

        hydrate_svm_identity(&git, &mut config, None, "file:///repo", 3, |path, _| {
            visited.push(path.to_string());
            Ok(if path.is_empty() {
                std::collections::BTreeMap::from([
                    ("svm:source".to_string(), b"source-root".to_vec()),
                    (
                        "svm:uuid".to_string(),
                        b"11111111-2222-3333-4444-555555555555".to_vec(),
                    ),
                ])
            } else {
                std::collections::BTreeMap::new()
            })
        })
        .unwrap();

        assert_eq!(visited, vec!["trunk", ""]);
        assert_eq!(config.svm_source.as_deref(), Some("source-root"));
        assert_eq!(config.svm_replace.as_deref(), Some("file:///repo"));
    }

    #[test]
    fn missing_or_partial_svm_properties_never_write_a_partial_cache() {
        for properties in [
            std::collections::BTreeMap::new(),
            std::collections::BTreeMap::from([("svm:source".to_string(), b"source".to_vec())]),
            std::collections::BTreeMap::from([(
                "svm:uuid".to_string(),
                b"11111111-2222-3333-4444-555555555555".to_vec(),
            )]),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let git = GitCli::new(temp.path());
            git.init().unwrap();
            let mut config = svm_config("file:///repo", "trunk");
            let error = hydrate_svm_identity(&git, &mut config, None, "file:///repo", 2, |_, _| {
                Ok(properties.clone())
            })
            .unwrap_err();
            assert!(error.contains("failed to read SVM properties"));
            for key in ["svm-source", "svm-replace", "svm-uuid"] {
                assert_eq!(
                    git.git_svn_metadata_get(&format!("svn-remote.svn.{key}"))
                        .unwrap(),
                    None
                );
            }
        }
    }

    #[test]
    fn invalid_svm_property_encoding_and_uuid_fail_without_cache() {
        for (properties, expected) in [
            (
                std::collections::BTreeMap::from([
                    ("svm:source".to_string(), vec![0xff]),
                    (
                        "svm:uuid".to_string(),
                        b"11111111-2222-3333-4444-555555555555".to_vec(),
                    ),
                ]),
                "svm:source directory property is not valid UTF-8",
            ),
            (
                std::collections::BTreeMap::from([
                    ("svm:source".to_string(), b"source".to_vec()),
                    ("svm:uuid".to_string(), vec![0xff]),
                ]),
                "svm:uuid directory property is not valid UTF-8",
            ),
            (
                std::collections::BTreeMap::from([
                    ("svm:source".to_string(), b"source".to_vec()),
                    ("svm:uuid".to_string(), b"not-a-uuid".to_vec()),
                ]),
                "doesn't look right - svm:uuid",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let git = GitCli::new(temp.path());
            git.init().unwrap();
            let mut config = svm_config("file:///repo", "trunk");
            let error = hydrate_svm_identity(&git, &mut config, None, "file:///repo", 2, |_, _| {
                Ok(properties.clone())
            })
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert_eq!(
                git.git_svn_metadata_get("svn-remote.svn.svm-source")
                    .unwrap(),
                None
            );
        }
    }

    #[test]
    fn cached_svm_identity_skips_directory_property_access() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = svm_config("file:///repo", "trunk").with_svm_identity(
            "source",
            "file:///repo",
            "11111111-2222-3333-4444-555555555555",
        );

        hydrate_svm_identity(&git, &mut config, None, "file:///repo", 2, |_, _| {
            panic!("cached SVM identity should skip remote property access")
        })
        .unwrap();
    }

    #[test]
    fn svnsync_identity_is_validated_then_cached_in_private_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///mirror", build_single_path(""))
            .with_svnsync_props();
        let properties = std::collections::BTreeMap::from([
            (
                "svn:sync-from-url".to_string(),
                b"https://source.example/repo".to_vec(),
            ),
            (
                "svn:sync-from-uuid".to_string(),
                b"11111111-2222-3333-4444-555555555555".to_vec(),
            ),
        ]);

        hydrate_svnsync_identity(&git, &mut config, || Ok(properties)).unwrap();

        assert_eq!(
            config.svnsync_url.as_deref(),
            Some("https://source.example/repo")
        );
        assert_eq!(
            git.git_svn_metadata_get("svn-remote.svn.svnsync-uuid")
                .unwrap()
                .as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(git.config_get("svn-remote.svn.svnsync-uuid").unwrap(), None);
    }

    #[test]
    fn partial_svnsync_identity_does_not_write_a_partial_cache() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///mirror", build_single_path(""))
            .with_svnsync_props();
        let properties = std::collections::BTreeMap::from([(
            "svn:sync-from-url".to_string(),
            b"https://source.example/repo".to_vec(),
        )]);

        let error = hydrate_svnsync_identity(&git, &mut config, || Ok(properties)).unwrap_err();

        assert!(error.contains("svn:sync-from-uuid"));
        assert_eq!(
            git.git_svn_metadata_get("svn-remote.svn.svnsync-url")
                .unwrap(),
            None
        );
        assert_eq!(
            git.git_svn_metadata_get("svn-remote.svn.svnsync-uuid")
                .unwrap(),
            None
        );
    }

    #[test]
    fn cached_svnsync_identity_skips_revision_property_access() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///mirror", build_single_path(""))
            .with_svnsync_identity("foo+bar://source", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

        hydrate_svnsync_identity(&git, &mut config, || {
            panic!("cached identity must avoid remote revision-property access")
        })
        .unwrap();
    }

    #[test]
    fn svnsync_source_url_requires_a_nonempty_target() {
        let temp = tempfile::tempdir().unwrap();
        let git = GitCli::new(temp.path());
        git.init().unwrap();
        let mut config = SvnRemoteConfig::new("svn", "file:///mirror", build_single_path(""))
            .with_svnsync_props();
        let properties = std::collections::BTreeMap::from([
            ("svn:sync-from-url".to_string(), b"foo://".to_vec()),
            (
                "svn:sync-from-uuid".to_string(),
                b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            ),
        ]);

        let error = hydrate_svnsync_identity(&git, &mut config, || Ok(properties)).unwrap_err();
        assert!(error.contains("invalid svn:sync-from-url"));
        assert_eq!(
            git.git_svn_metadata_get("svn-remote.svn.svnsync-url")
                .unwrap(),
            None
        );
    }

    #[test]
    fn log_window_size_overlays_the_persisted_fetch_config() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.log_window_size = Some(100);

        assert_eq!(
            effective_fetch_config(config, &shared, 0)
                .unwrap()
                .log_window_size,
            Some(100)
        );
    }

    fn default_shared_args() -> SharedFetchArgs {
        SharedFetchArgs {
            authors_file: None,
            authors_prog: None,
            ignore_paths: None,
            include_paths: None,
            ignore_refs: None,
            revision: None,
            log_window_size: None,
            localtime: false,
            no_metadata: false,
            use_svm_props: false,
            use_svnsync_props: false,
            rewrite_root: None,
            rewrite_uuid: None,
            username: None,
            password: None,
            config_dir: None,
            no_auth_cache: false,
            preserve_empty_dirs: false,
            placeholder_filename: ".gitignore".to_string(),
        }
    }
}
