use super::*;

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

pub(super) fn combine_regex(persisted: Option<String>, runtime: Option<&str>) -> Option<String> {
    match (persisted, runtime) {
        (Some(persisted), Some(runtime)) => Some(format!("(?:{persisted})|(?:{runtime})")),
        (persisted, None) => persisted,
        (None, Some(runtime)) => Some(runtime.to_string()),
    }
}

pub(super) fn reject_identity_change(base_revision: u32, option: &str) -> Result<(), String> {
    if base_revision == 0 {
        Ok(())
    } else {
        Err(format!(
            "{option} cannot change after SVN history has been imported"
        ))
    }
}

pub(super) enum ConfiguredBackend {
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
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Cli(_) => "svn-cli",
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(_) => "libsvn",
        }
    }

    #[cfg(test)]
    pub(super) fn import_mode(&self) -> &'static str {
        match self {
            Self::Cli(_) => "ra-editor",
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(_) => "ra-editor",
        }
    }

    #[cfg(test)]
    pub(super) fn configured_username(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_username(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_username(),
        }
    }

    #[cfg(test)]
    pub(super) fn configured_config_dir(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_config_dir(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_config_dir(),
        }
    }

    #[cfg(test)]
    pub(super) fn configured_password(&self) -> Option<&str> {
        match self {
            Self::Cli(backend) => backend.configured_password(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.configured_password(),
        }
    }

    pub(super) fn import_revisions(
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

    pub(super) fn repository_root(&self) -> Result<String, String> {
        match self {
            Self::Cli(backend) => backend.repository_root(),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => Ok(RaSession::repos_root(backend).to_string()),
        }
    }

    pub(super) fn rev_properties(
        &self,
        revision: u32,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
        match self {
            Self::Cli(backend) => backend.rev_properties(revision),
            #[cfg(all(feature = "svn-libsvn", git_svn_rs_libsvn_linked))]
            Self::LibSvn(backend) => backend.rev_properties(revision),
        }
    }

    pub(super) fn node_property_bytes(
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

pub(super) fn configured_backend(
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

pub(super) fn prompted_fetch_credentials(
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

pub(super) fn import_options(
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
pub(super) struct RevisionRange {
    start: Option<u32>,
    end: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RevisionArgument {
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

pub(super) fn parse_revision_argument(value: &str) -> Result<RevisionArgument, String> {
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

pub(super) fn parse_numeric_revision(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

pub(super) fn parse_mapping(value: &str, kind: MappingKind) -> Result<RefMapping, String> {
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

pub(super) struct MockBackendFromSession<'a>(pub(super) &'a MockRaSession);

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
