use crate::cli::{FetchArgs, SharedFetchArgs};
use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::import::import_ra_revisions;
use crate::import::{ImportOptions, import_mock_revisions};
use crate::mapping::{MappingKind, RefMapping};
use crate::rev_map::RevMap;
use crate::svn::SvnBackend;
use crate::svn::cli::SvnCliBackend;
use crate::svn::mock::MockRaSession;
use crate::svn::ra::RaSession;
use std::cmp;

pub fn run(args: FetchArgs) -> Result<(), String> {
    run_in_work_tree(".", args)
}

pub fn run_in_work_tree(
    work_tree: impl Into<std::path::PathBuf>,
    args: FetchArgs,
) -> Result<(), String> {
    if args.parent {
        return Err("fetch --parent is not implemented".to_string());
    }
    let git = GitCli::new(work_tree.into());
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
    let config = read_remote_config(git, remote)?;

    if config.url.starts_with("mock://") {
        let session = MockRaSession::standard_fixture("mock-uuid");
        let base_revision = imported_base_revision(git, &config, "mock-uuid")?;
        let config = effective_fetch_config(config, shared, base_revision)?;
        let start_revision = base_revision.saturating_add(1);
        let import_options = import_options(
            start_revision,
            session.latest_revnum()?,
            shared.revision.as_deref(),
        )?;
        import_mock_revisions(
            &MockBackendFromSession(&session),
            git,
            &config,
            import_options,
        )?;
        return Ok(());
    }

    let backend = configured_backend(&config, shared)?;
    let uuid = backend.uuid()?;
    let base_revision = imported_base_revision(git, &config, &uuid)?;
    let config = effective_fetch_config(config, shared, base_revision)?;
    let start_revision = base_revision.saturating_add(1);
    let import_options = import_options(
        start_revision,
        backend.latest_revnum()?,
        shared.revision.as_deref(),
    )?;
    backend.import_revisions(git, &config, import_options)?;
    Ok(())
}

fn effective_fetch_config(
    mut config: SvnRemoteConfig,
    shared: &SharedFetchArgs,
    base_revision: u32,
) -> Result<SvnRemoteConfig, String> {
    if config.log_window_size.is_some() || shared.log_window_size.is_some() {
        return Err("--log-window-size is not implemented".to_string());
    }
    if let Some(value) = &shared.authors_file {
        config.authors_file = Some(value.clone());
    }
    if let Some(value) = &shared.authors_prog {
        config.authors_prog = Some(value.clone());
    }
    config.ignore_paths = combine_regex(config.ignore_paths, shared.ignore_paths.as_deref());
    config.include_paths = combine_regex(config.include_paths, shared.include_paths.as_deref());
    if let Some(value) = &shared.ignore_refs {
        config.ignore_refs = Some(value.clone());
    }
    if shared.localtime && !config.localtime {
        reject_identity_change(base_revision, "--localtime")?;
        config.localtime = true;
    }
    if shared.no_metadata && !config.no_metadata {
        reject_identity_change(base_revision, "--no-metadata")?;
        config.no_metadata = true;
    }
    if config.no_metadata && base_revision > 0 {
        return Err("fetch is unavailable after a --no-metadata one-shot import".to_string());
    }
    if let Some(value) = &shared.rewrite_root
        && config.rewrite_root.as_ref() != Some(value)
    {
        reject_identity_change(base_revision, "--rewrite-root")?;
        config.rewrite_root = Some(value.clone());
    }
    if let Some(value) = &shared.rewrite_uuid
        && config.rewrite_uuid.as_ref() != Some(value)
    {
        reject_identity_change(base_revision, "--rewrite-uuid")?;
        config.rewrite_uuid = Some(value.clone());
    }
    if shared.preserve_empty_dirs {
        config.preserve_empty_dirs = true;
        config.placeholder_filename = shared.placeholder_filename.clone();
    }
    Ok(config)
}

fn combine_regex(persisted: Option<String>, runtime: Option<&str>) -> Option<String> {
    match (persisted, runtime) {
        (Some(persisted), Some(runtime)) => Some(format!("(?:{persisted})|(?:{runtime})")),
        (persisted, None) => persisted,
        (None, Some(runtime)) => Some(runtime.to_string()),
    }
}

fn reject_identity_change(base_revision: u32, option: &str) -> Result<(), String> {
    if base_revision == 0 {
        Ok(())
    } else {
        Err(format!(
            "{option} cannot change after SVN history has been imported"
        ))
    }
}

enum ConfiguredBackend {
    #[cfg_attr(
        all(feature = "svn-libsvn", git_svn_rs_libsvn_linked),
        allow(dead_code)
    )]
    Cli(SvnCliBackend),
    #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
    LibSvn(crate::svn::libsvn::LibSvnBackend),
}

impl ConfiguredBackend {
    #[cfg(test)]
    fn kind(&self) -> &'static str {
        match self {
            Self::Cli(_) => "svn-cli",
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(_) => "libsvn",
        }
    }

    #[cfg(test)]
    fn import_mode(&self) -> &'static str {
        match self {
            Self::Cli(_) => "ra-editor",
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(_) => "ra-editor",
        }
    }

    #[cfg(test)]
    fn configured_username(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_username(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_username(),
        }
    }

    #[cfg(test)]
    fn configured_config_dir(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_config_dir(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_config_dir(),
        }
    }

    #[cfg(test)]
    fn configured_password(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_password(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_password(),
        }
    }

    fn import_revisions(
        &self,
        git: &GitCli,
        config: &SvnRemoteConfig,
        options: ImportOptions,
    ) -> Result<(), String> {
        match self {
            Self::Cli(backend) => {
                import_ra_revisions(backend, git, config, options)?;
            }
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => {
                import_ra_revisions(backend, git, config, options)?;
            }
        }
        Ok(())
    }
}

impl SvnBackend for ConfiguredBackend {
    fn uuid(&self) -> Result<String, String> {
        match self {
            Self::Cli(backend) => SvnBackend::uuid(backend),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => SvnBackend::uuid(backend),
        }
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        match self {
            Self::Cli(backend) => SvnBackend::latest_revnum(backend),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => SvnBackend::latest_revnum(backend),
        }
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<crate::svn::RevisionEvent>, String> {
        match self {
            Self::Cli(backend) => backend.log(start, end),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.log(start, end),
        }
    }
}

fn configured_backend(
    config: &SvnRemoteConfig,
    shared: &SharedFetchArgs,
) -> Result<ConfiguredBackend, String> {
    #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
    {
        let mut backend = crate::svn::libsvn::LibSvnBackend::from_config(config);
        if let Some(config_dir) = &shared.config_dir {
            backend = backend.with_config_dir(config_dir);
        }
        if let Some(username) = &shared.username {
            backend = backend.with_username(username);
        }
        if let Some(password) = &shared.password {
            backend = if let Some(username) = shared.username.as_ref().or(config.username.as_ref())
            {
                backend.with_credentials(username, password)
            } else {
                backend.with_password(password)
            };
        }
        if shared.no_auth_cache {
            backend = backend.without_auth_cache();
        }
        Ok(ConfiguredBackend::LibSvn(backend))
    }

    #[cfg(not(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked)))]
    {
        let mut backend = SvnCliBackend::from_config(config)?;
        if let Some(username) = &shared.username {
            backend = backend.with_username(username);
        }
        if let Some(password) = &shared.password {
            backend = backend.with_password(password);
        }
        if let Some(config_dir) = &shared.config_dir {
            backend = backend.with_config_dir(config_dir);
        }
        if shared.no_auth_cache {
            backend = backend.without_auth_cache();
        }
        Ok(ConfiguredBackend::Cli(backend))
    }
}

fn svn_remote_names(git: &GitCli) -> Result<Vec<String>, String> {
    let keys = git.config_names_matching(r"^svn-remote\..*\.url$")?;
    let mut names = keys
        .into_iter()
        .filter_map(|key| {
            key.strip_prefix("svn-remote.")
                .and_then(|value| value.strip_suffix(".url"))
                .map(|value| value.to_string())
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

fn import_options(
    next_revision: u32,
    head_revision: u32,
    revision: Option<&str>,
) -> Result<ImportOptions, String> {
    let Some(revision) = revision else {
        return Ok(ImportOptions {
            start_revision: next_revision,
            end_revision: None,
        });
    };
    let base_revision = next_revision.saturating_sub(1);
    let range = parse_revision_argument(revision)?.resolve(base_revision, head_revision);
    Ok(ImportOptions {
        start_revision: cmp::max(next_revision, range.start.unwrap_or(next_revision)),
        end_revision: range.end,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RevisionRange {
    start: Option<u32>,
    end: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevisionArgument {
    Number(u32),
    Range(u32, u32),
    Head,
    BaseTo(u32),
    ToHead(u32),
    BaseToHead,
}

impl RevisionArgument {
    fn resolve(self, base: u32, head: u32) -> RevisionRange {
        let (start, end) = match self {
            Self::Number(revision) => (revision, revision),
            Self::Range(start, end) => (start, end),
            Self::Head => (head, head),
            Self::BaseTo(end) => (base, end),
            Self::ToHead(start) => (start, head),
            Self::BaseToHead => (base, head),
        };
        RevisionRange {
            start: Some(start),
            end: Some(end),
        }
    }
}

fn parse_revision_argument(value: &str) -> Result<RevisionArgument, String> {
    let parsed = if value == "HEAD" {
        Some(RevisionArgument::Head)
    } else if value == "BASE:HEAD" {
        Some(RevisionArgument::BaseToHead)
    } else if let Some(end) = value.strip_prefix("BASE:") {
        parse_numeric_revision(end).map(RevisionArgument::BaseTo)
    } else if let Some(start) = value.strip_suffix(":HEAD") {
        parse_numeric_revision(start).map(RevisionArgument::ToHead)
    } else if let Some((start, end)) = value.split_once(':') {
        match (parse_numeric_revision(start), parse_numeric_revision(end)) {
            (Some(start), Some(end)) => Some(RevisionArgument::Range(start, end)),
            _ => None,
        }
    } else {
        parse_numeric_revision(value).map(RevisionArgument::Number)
    };
    parsed.ok_or_else(|| format!("revision argument: {value} not understood by git-svn"))
}

fn parse_numeric_revision(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn read_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let url = git
        .config_get(&format!("{prefix}.url"))?
        .ok_or_else(|| format!("missing {prefix}.url"))?;
    let fetch = git.config_get_all(&format!("{prefix}.fetch"))?;
    let branches = git.config_get_all(&format!("{prefix}.branches"))?;
    let tags = git.config_get_all(&format!("{prefix}.tags"))?;
    let mappings = fetch
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Fetch))
        .collect::<Result<Vec<_>, _>>()?;
    let branch_mappings = branches
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Branches))
        .collect::<Result<Vec<_>, _>>()?;
    let tag_mappings = tags
        .into_iter()
        .map(|value| parse_mapping(&value, MappingKind::Tags))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: branch_mappings,
        tags: tag_mappings,
        ignore_paths: git.config_get(&format!("{prefix}.ignore-paths"))?,
        include_paths: git.config_get(&format!("{prefix}.include-paths"))?,
        ignore_refs: git.config_get(&format!("{prefix}.ignore-refs"))?,
        authors_file: git.config_get(&format!("{prefix}.authors-file"))?,
        authors_prog: git.config_get(&format!("{prefix}.authors-prog"))?,
        log_window_size: git
            .config_get(&format!("{prefix}.log-window-size"))?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("invalid {prefix}.log-window-size: {value}"))
            })
            .transpose()?,
        localtime: git
            .config_get(&format!("{prefix}.localtime"))?
            .is_some_and(|value| value == "true"),
        username: git.config_get(&format!("{prefix}.username"))?,
        config_dir: git.config_get(&format!("{prefix}.config-dir"))?,
        no_auth_cache: git
            .config_get(&format!("{prefix}.no-auth-cache"))?
            .is_some_and(|value| value == "true"),
        no_metadata: git
            .config_get(&format!("{prefix}.noMetadata"))?
            .is_some_and(|value| value == "true"),
        rewrite_root: git.config_get(&format!("{prefix}.rewriteRoot"))?,
        rewrite_uuid: git.config_get(&format!("{prefix}.rewriteUUID"))?,
        preserve_empty_dirs: git
            .config_get(&format!("{prefix}.preserve-empty-dirs"))?
            .is_some_and(|value| value == "true"),
        placeholder_filename: git
            .config_get(&format!("{prefix}.placeholder-filename"))?
            .unwrap_or_else(|| ".gitignore".to_string()),
    })
}

fn parse_mapping(value: &str, kind: MappingKind) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    Ok(RefMapping {
        kind,
        svn_path: svn_path.trim_start_matches('+').to_string(),
        git_ref: git_ref.to_string(),
    })
}

fn imported_base_revision(
    git: &GitCli,
    config: &SvnRemoteConfig,
    uuid: &str,
) -> Result<u32, String> {
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    let mut refnames = config
        .fetch
        .iter()
        .map(|mapping| mapping.git_ref.clone())
        .collect::<Vec<_>>();
    let remote_refs = git.refs_under("refs/remotes")?;
    for mapping in config.branches.iter().chain(config.tags.iter()) {
        let Some((prefix, suffix)) = mapping.git_ref.split_once('*') else {
            refnames.push(mapping.git_ref.clone());
            continue;
        };
        refnames.extend(
            remote_refs
                .iter()
                .filter(|refname| refname.starts_with(prefix) && refname.ends_with(suffix))
                .cloned(),
        );
    }
    refnames.sort();
    refnames.dedup();
    if refnames.is_empty() {
        return Ok(0);
    }

    let object_format = git.object_format()?;
    let mut base = u32::MAX;
    for refname in refnames {
        let short_ref = refname
            .strip_prefix("refs/remotes/")
            .unwrap_or(&refname)
            .replace('/', ".");
        let path = svn_dir.join(short_ref).join(format!(".rev_map.{uuid}"));
        if !path.exists() {
            return Ok(0);
        }
        let revision = RevMap::open(path, object_format)?
            .max_revision(false)?
            .unwrap_or(0);
        base = base.min(revision);
    }
    Ok(base)
}

struct MockBackendFromSession<'a>(&'a MockRaSession);

impl crate::svn::SvnBackend for MockBackendFromSession<'_> {
    fn uuid(&self) -> Result<String, String> {
        self.0.uuid()
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        self.0.latest_revnum()
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<crate::svn::RevisionEvent>, String> {
        self.0.get_log(&[], start, end)
    }
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

        assert_eq!(imported_base_revision(&git, &config, "uuid").unwrap(), 5);
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
    fn log_window_size_is_explicitly_rejected() {
        let config = SvnRemoteConfig::new("svn", "file:///repo", build_single_path(""));
        let mut shared = default_shared_args();
        shared.log_window_size = Some(100);

        assert_eq!(
            effective_fetch_config(config, &shared, 0).unwrap_err(),
            "--log-window-size is not implemented"
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
