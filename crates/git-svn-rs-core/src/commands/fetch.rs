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
    crate::migration::ensure_supported_git_svn_metadata(git.work_tree())?;
    validate_requested_urls_before_recovery(&git, &args)?;
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

fn validate_existing_tracking_states(
    git: &GitCli,
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
) -> Result<(), String> {
    let mappings = crate::commands::resolver::tracked_candidate_mappings(git, config)?
        .into_iter()
        .filter(|mapping| selected_ref.is_none_or(|selected_ref| mapping.git_ref == selected_ref))
        .collect();
    crate::tracking_state::validate_candidate_mappings(git, config, mappings)?;
    Ok(())
}

fn validate_requested_urls_before_recovery(git: &GitCli, args: &FetchArgs) -> Result<(), String> {
    if args.parent {
        let tracked =
            crate::commands::resolver::resolve_tracked_svn_allow_import_batch(git.work_tree())?;
        return crate::path_url::validate_fetch_url(&tracked.config.url);
    }
    let remotes = if args.fetch_all {
        svn_remote_names(git)?
    } else {
        vec![args.remote.clone().unwrap_or_else(|| "svn".to_string())]
    };
    for remote in remotes {
        let config = read_svn_remote_config(git, &remote)?;
        crate::path_url::validate_fetch_url(&config.url)?;
    }
    Ok(())
}

pub(crate) fn effective_fetch_config(
    mut config: SvnRemoteConfig,
    shared: &SharedFetchArgs,
    base_revision: u32,
) -> Result<SvnRemoteConfig, String> {
    if let Some(window_size) = shared.log_window_size {
        config.log_window_size = Some(window_size);
    }
    if config.log_window_size == Some(0) {
        return Err("--log-window-size must be greater than zero".to_string());
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
    if shared.use_svm_props && !config.use_svm_props {
        reject_identity_change(base_revision, "--use-svm-props")?;
        config.use_svm_props = true;
    }
    if shared.use_svnsync_props && !config.use_svnsync_props {
        reject_identity_change(base_revision, "--use-svnsync-props")?;
        config.use_svnsync_props = true;
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
    config.validate_metadata_options()?;
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
        selected_ref: Option<&str>,
    ) -> Result<(), String> {
        match self {
            Self::Cli(backend) => {
                import_ra_revisions_for_ref(backend, git, config, options, selected_ref)?;
            }
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => {
                import_ra_revisions_for_ref(backend, git, config, options, selected_ref)?;
            }
        }
        Ok(())
    }

    fn repository_root(&self) -> Result<String, String> {
        match self {
            Self::Cli(backend) => backend.repository_root(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => Ok(RaSession::repos_root(backend).to_string()),
        }
    }

    fn rev_properties(
        &self,
        revision: u32,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
        match self {
            Self::Cli(backend) => backend.rev_properties(revision),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.rev_properties(revision),
        }
    }

    fn node_property_bytes(
        &self,
        repository_root: &str,
        path: &str,
        revision: u32,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
        match self {
            Self::Cli(backend) => backend.node_property_bytes(repository_root, path, revision),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => {
                backend.node_property_bytes_at_repository_path(repository_root, path, revision)
            }
        }
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
    let prompted = prompted_fetch_credentials(config, shared)?;
    let username = shared
        .username
        .as_ref()
        .or(config.username.as_ref())
        .or_else(|| prompted.as_ref().map(|credentials| &credentials.username));
    let password = shared
        .password
        .as_ref()
        .or_else(|| prompted.as_ref().map(|credentials| &credentials.password));
    #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
    {
        let mut backend = crate::svn::libsvn::LibSvnBackend::from_config(config);
        if let Some(config_dir) = &shared.config_dir {
            backend = backend.with_config_dir(config_dir);
        }
        if let Some(username) = username {
            backend = backend.with_username(username);
        }
        if let Some(password) = password {
            backend = if let Some(username) = username {
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
        if let Some(username) = username {
            backend = backend.with_username(username);
        }
        if let Some(password) = password {
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

fn prompted_fetch_credentials(
    config: &SvnRemoteConfig,
    shared: &SharedFetchArgs,
) -> Result<Option<Credentials>, String> {
    if shared.password.is_some() {
        return Ok(None);
    }
    if !matches!(
        svn_url_profile(&config.url),
        SvnUrlProfile::Svn | SvnUrlProfile::Http | SvnUrlProfile::Https
    ) {
        return Ok(None);
    }
    prompted_credentials(
        &config.url,
        shared.username.as_deref().or(config.username.as_deref()),
        shared
            .config_dir
            .as_deref()
            .or(config.config_dir.as_deref()),
        shared.no_auth_cache || config.no_auth_cache,
        AuthOperation::Read,
    )
}

fn verify_remote_fetch_ref_sanity(git: &GitCli) -> Result<(), String> {
    let mut keys = git.config_names_matching(r"^svn-remote\..*\.(fetch|branches|tags)$")?;
    keys.sort();
    keys.dedup();
    let mut destinations = Vec::<ConfiguredDestination>::new();
    for key in keys {
        let kind = if key.ends_with(".branches") {
            MappingKind::Branches
        } else if key.ends_with(".tags") {
            MappingKind::Tags
        } else {
            MappingKind::Fetch
        };
        for value in git.config_get_all(&key)? {
            let mapping = parse_mapping(&value, kind.clone())?;
            let owner = format!("{key}={value}");
            let remote = key
                .strip_prefix("svn-remote.")
                .and_then(|key| key.rsplit_once('.').map(|(remote, _)| remote))
                .ok_or_else(|| format!("invalid SVN remote mapping key: {key}"))?;
            let destination = ConfiguredDestination::new(remote.to_string(), mapping, owner)?;
            if let Some(previous) = destinations
                .iter()
                .find(|previous| previous.may_overlap(&destination))
            {
                let description = if previous.pattern == destination.pattern {
                    format!("remote ref {}", destination.pattern)
                } else {
                    format!(
                        "remote ref destinations {} and {}",
                        previous.pattern, destination.pattern
                    )
                };
                return Err(format!(
                    "{description} may be tracked by both {} and {}; resolve this ambiguity before fetching",
                    previous.owner, destination.owner
                ));
            }
            destinations.push(destination);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ConfiguredDestination {
    remote: String,
    owner: String,
    pattern: String,
    wildcard_depth: Option<usize>,
}

impl ConfiguredDestination {
    fn new(remote: String, mapping: RefMapping, owner: String) -> Result<Self, String> {
        let wildcard_depth = match mapping.kind {
            MappingKind::Fetch => None,
            MappingKind::Branches | MappingKind::Tags if mapping.git_ref.contains('*') => {
                Some(GlobSpec::new(&mapping.svn_path, true)?.depth())
            }
            MappingKind::Branches | MappingKind::Tags => None,
        };
        let mut pattern = crate::mapping::sanitize_refname(&mapping.git_ref)?;
        if wildcard_depth.is_some() {
            pattern = pattern.replace("%2A", "*");
        }
        Ok(Self {
            remote,
            owner,
            pattern,
            wildcard_depth,
        })
    }

    fn may_overlap(&self, other: &Self) -> bool {
        if self.remote == other.remote {
            return self.pattern == other.pattern;
        }
        if self.wildcard_depth.is_none() && other.wildcard_depth.is_none() {
            return self.pattern == other.pattern;
        }
        if self.expanded_slash_count() != other.expanded_slash_count() {
            return false;
        }

        let (self_prefix, self_suffix) = literal_edges(&self.pattern);
        let (other_prefix, other_suffix) = literal_edges(&other.pattern);
        (self_prefix.starts_with(other_prefix) || other_prefix.starts_with(self_prefix))
            && (self_suffix.ends_with(other_suffix) || other_suffix.ends_with(self_suffix))
    }

    fn expanded_slash_count(&self) -> usize {
        let literal_slashes = self.pattern.bytes().filter(|byte| *byte == b'/').count();
        let wildcard_count = self.pattern.bytes().filter(|byte| *byte == b'*').count();
        literal_slashes + wildcard_count * self.wildcard_depth.unwrap_or(1).saturating_sub(1)
    }
}

fn literal_edges(pattern: &str) -> (&str, &str) {
    let prefix = pattern
        .split_once('*')
        .map_or(pattern, |(prefix, _)| prefix);
    let suffix = pattern
        .rsplit_once('*')
        .map_or(pattern, |(_, suffix)| suffix);
    (prefix, suffix)
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

fn parse_mapping(value: &str, kind: MappingKind) -> Result<RefMapping, String> {
    let (svn_path, git_ref) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid fetch mapping: {value}"))?;
    crate::mapping::sanitize_refname(git_ref)?;
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
    selected_ref: Option<&str>,
) -> Result<u32, String> {
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    if let Some(selected_ref) = selected_ref {
        return imported_ref_revision(git, &svn_dir, selected_ref, uuid);
    }

    let mut bases = Vec::new();
    for mapping in &config.fetch {
        bases.push(imported_ref_revision(
            git,
            &svn_dir,
            &mapping.git_ref,
            uuid,
        )?);
    }
    if !config.branches.is_empty() {
        bases.push(discovery_high_water(git, config, "branches")?.unwrap_or(0));
    }
    if !config.tags.is_empty() {
        bases.push(discovery_high_water(git, config, "tags")?.unwrap_or(0));
    }
    Ok(bases.into_iter().min().unwrap_or(0))
}

fn imported_ref_revision(
    git: &GitCli,
    svn_dir: &std::path::Path,
    refname: &str,
    uuid: &str,
) -> Result<u32, String> {
    let object_format = git.object_format()?;
    let git_dir = svn_dir.parent().ok_or_else(|| {
        format!(
            "SVN metadata path has no Git directory: {}",
            svn_dir.display()
        )
    })?;
    let path =
        crate::metadata::svn_metadata_dir(git_dir, refname)?.join(format!(".rev_map.{uuid}"));
    if !path.exists() {
        return Ok(0);
    }
    Ok(RevMap::open_existing(path, object_format)?
        .max_revision(false)?
        .unwrap_or(0))
}

fn discovery_high_water(
    git: &GitCli,
    config: &SvnRemoteConfig,
    kind: &str,
) -> Result<Option<u32>, String> {
    let key = format!("svn-remote.{}.{kind}-maxRev", config.name);
    git.git_svn_metadata_get(&key)?
        .map(|value| {
            value
                .parse::<u32>()
                .map_err(|_| format!("invalid git-svn discovery high-water {key}={value}"))
        })
        .transpose()
}

fn persist_discovery_high_water(
    git: &GitCli,
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
    scanned_end: u32,
) -> Result<(), String> {
    if selected_ref.is_some() {
        return Ok(());
    }
    for (kind, configured) in [
        ("branches", !config.branches.is_empty()),
        ("tags", !config.tags.is_empty()),
    ] {
        if !configured {
            continue;
        }
        let current = discovery_high_water(git, config, kind)?.unwrap_or(0);
        if scanned_end > current {
            let key = format!("svn-remote.{}.{kind}-maxRev", config.name);
            git.git_svn_metadata_set(&key, &scanned_end.to_string())?;
        }
    }
    Ok(())
}

fn hydrate_svm_identity(
    git: &GitCli,
    config: &mut SvnRemoteConfig,
    selected_ref: Option<&str>,
    repository_root: &str,
    latest_revision: u32,
    mut read_directory_properties: impl FnMut(
        &str,
        u32,
    ) -> Result<
        std::collections::BTreeMap<String, Vec<u8>>,
        String,
    >,
) -> Result<(), String> {
    if !config.use_svm_props {
        return Ok(());
    }
    config.validate_metadata_options()?;
    config.validate_svm_cache()?;
    if config.svm_source.is_some() {
        return Ok(());
    }

    let paths = svm_discovery_paths(config, selected_ref, repository_root)?;
    for path in &paths {
        let properties = read_directory_properties(path, latest_revision)?;
        let source = properties
            .get("svm:source")
            .filter(|value| !value.is_empty());
        let uuid = properties.get("svm:uuid").filter(|value| !value.is_empty());
        let (Some(source), Some(uuid)) = (source, uuid) else {
            continue;
        };
        let source = normalize_svm_source(source)?;
        let uuid = decode_svm_property("svm:uuid", uuid)?;
        crate::config::validate_svm_uuid(&uuid)?;
        let replace = crate::path_url::add_path_to_url(repository_root, path);

        let prefix = format!("svn-remote.{}", config.name);
        let source_key = format!("{prefix}.svm-source");
        let replace_key = format!("{prefix}.svm-replace");
        let uuid_key = format!("{prefix}.svm-uuid");
        git.git_svn_metadata_set_many(&[
            (&source_key, &source),
            (&replace_key, &replace),
            (&uuid_key, &uuid),
        ])?;
        config.svm_source = Some(source);
        config.svm_replace = Some(replace);
        config.svm_uuid = Some(uuid);
        return Ok(());
    }

    let tried = paths
        .iter()
        .map(|path| {
            format!(
                "  {}",
                crate::path_url::add_path_to_url(repository_root, path)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "useSvmProps set, but failed to read SVM properties\n(svm:source, svm:uuid) from the following URLs:\n{tried}\n"
    ))
}

fn svm_discovery_paths(
    config: &SvnRemoteConfig,
    selected_ref: Option<&str>,
    repository_root: &str,
) -> Result<Vec<String>, String> {
    let session_path = crate::path_url::repository_relative_url_path(repository_root, &config.url)?;
    let mappings = config
        .fetch
        .iter()
        .chain(config.branches.iter())
        .chain(config.tags.iter())
        .filter(|mapping| selected_ref.is_none_or(|selected| mapping.git_ref == selected))
        .collect::<Vec<_>>();
    let starts = if mappings.is_empty() {
        vec![session_path.clone()]
    } else {
        mappings
            .into_iter()
            .map(|mapping| {
                let fixed = mapping
                    .svn_path
                    .split('/')
                    .take_while(|part| !part.contains('*') && !part.contains('{'))
                    .collect::<Vec<_>>()
                    .join("/");
                crate::path_url::join_paths([session_path.as_str(), fixed.as_str()])
            })
            .collect()
    };

    let mut paths = Vec::new();
    for start in starts {
        let mut current = start.trim_matches('/').to_string();
        loop {
            if !paths.contains(&current) {
                paths.push(current.clone());
            }
            let Some((parent, _)) = current.rsplit_once('/') else {
                if !current.is_empty() && !paths.iter().any(String::is_empty) {
                    paths.push(String::new());
                }
                break;
            };
            current = parent.to_string();
        }
    }
    if paths.is_empty() {
        paths.push(String::new());
    }
    Ok(paths)
}

fn decode_svm_property(name: &str, value: &[u8]) -> Result<String, String> {
    let mut value = String::from_utf8(value.to_vec())
        .map_err(|_| format!("{name} directory property is not valid UTF-8"))?;
    if value.ends_with('\n') {
        value.pop();
    }
    Ok(value)
}

fn normalize_svm_source(value: &[u8]) -> Result<String, String> {
    let mut source = decode_svm_property("svm:source", value)?;
    if let Some(bang) = source.find('!') {
        let left = source[..bang].trim_end_matches('/');
        let right = source[bang + 1..].trim_start_matches('/');
        source = format!("{left}/{right}");
    }
    while source.ends_with('/') {
        source.pop();
    }
    if let Some((scheme, rest)) = source.split_once("://")
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'+')
    {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if let Some((_, host)) = authority.rsplit_once('@') {
            source = if path.is_empty() {
                format!("{scheme}://{host}")
            } else {
                format!("{scheme}://{host}/{path}")
            };
        }
    }
    if source.is_empty() {
        return Err("svm:source directory property is empty after normalization".to_string());
    }
    Ok(source)
}

fn hydrate_svnsync_identity(
    git: &GitCli,
    config: &mut SvnRemoteConfig,
    read_revision_zero: impl FnOnce() -> Result<std::collections::BTreeMap<String, Vec<u8>>, String>,
) -> Result<(), String> {
    if !config.use_svnsync_props {
        return Ok(());
    }
    config.validate_metadata_options()?;
    config.validate_svnsync_cache()?;
    if config.svnsync_url.is_some() {
        return Ok(());
    }

    let properties = read_revision_zero()?;
    let read = |name: &str| -> Result<String, String> {
        let value = properties
            .get(name)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("useSvnsyncProps set, but failed to read svnsync property: {name}")
            })?;
        String::from_utf8(value.clone())
            .map_err(|_| format!("{name} revision property is not valid UTF-8"))
    };
    let url = read("svn:sync-from-url")?;
    let uuid = read("svn:sync-from-uuid")?;
    crate::config::validate_svnsync_identity(&url, &uuid)?;

    let prefix = format!("svn-remote.{}", config.name);
    let uuid_key = format!("{prefix}.svnsync-uuid");
    let url_key = format!("{prefix}.svnsync-url");
    git.git_svn_metadata_set_many(&[(&uuid_key, &uuid), (&url_key, &url)])?;
    config.svnsync_url = Some(url);
    config.svnsync_uuid = Some(uuid);
    Ok(())
}

fn persist_repository_identity(
    git: &GitCli,
    config: &SvnRemoteConfig,
    repos_root: &str,
    uuid: &str,
) -> Result<(), String> {
    let prefix = format!("svn-remote.{}", config.name);
    git.git_svn_metadata_set(&format!("{prefix}.reposRoot"), repos_root)?;
    git.git_svn_metadata_set(&format!("{prefix}.uuid"), uuid)
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
