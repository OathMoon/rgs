use crate::git::GitCli;
use crate::mapping::{LayoutMappings, MappingKind, RefMapping};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnRemoteConfig {
    pub name: String,
    pub url: String,
    pub fetch: Vec<RefMapping>,
    pub branches: Vec<RefMapping>,
    pub tags: Vec<RefMapping>,
    pub ignore_paths: Option<String>,
    pub include_paths: Option<String>,
    pub ignore_refs: Option<String>,
    pub authors_file: Option<String>,
    pub authors_prog: Option<String>,
    pub log_window_size: Option<u32>,
    pub localtime: bool,
    pub username: Option<String>,
    pub config_dir: Option<String>,
    pub no_auth_cache: bool,
    pub no_metadata: bool,
    pub rewrite_root: Option<String>,
    pub rewrite_uuid: Option<String>,
    pub preserve_empty_dirs: bool,
    pub placeholder_filename: String,
}

impl SvnRemoteConfig {
    pub fn new(name: impl Into<String>, url: impl Into<String>, mappings: LayoutMappings) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            fetch: mappings.fetch,
            branches: mappings.branches,
            tags: mappings.tags,
            ignore_paths: None,
            include_paths: None,
            ignore_refs: None,
            authors_file: None,
            authors_prog: None,
            log_window_size: None,
            localtime: false,
            username: None,
            config_dir: None,
            no_auth_cache: false,
            no_metadata: false,
            rewrite_root: None,
            rewrite_uuid: None,
            preserve_empty_dirs: false,
            placeholder_filename: ".gitignore".to_string(),
        }
    }

    pub fn with_ignore_paths(mut self, value: impl Into<String>) -> Self {
        self.ignore_paths = Some(value.into());
        self
    }

    pub fn with_include_paths(mut self, value: impl Into<String>) -> Self {
        self.include_paths = Some(value.into());
        self
    }

    pub fn with_ignore_refs(mut self, value: impl Into<String>) -> Self {
        self.ignore_refs = Some(value.into());
        self
    }

    pub fn with_authors_file(mut self, value: impl Into<String>) -> Self {
        self.authors_file = Some(value.into());
        self
    }

    pub fn with_authors_prog(mut self, value: impl Into<String>) -> Self {
        self.authors_prog = Some(value.into());
        self
    }

    pub fn with_log_window_size(mut self, value: u32) -> Self {
        self.log_window_size = Some(value);
        self
    }

    pub fn with_localtime(mut self) -> Self {
        self.localtime = true;
        self
    }

    pub fn with_username(mut self, value: impl Into<String>) -> Self {
        self.username = Some(value.into());
        self
    }

    pub fn with_config_dir(mut self, value: impl Into<String>) -> Self {
        self.config_dir = Some(value.into());
        self
    }

    pub fn without_auth_cache(mut self) -> Self {
        self.no_auth_cache = true;
        self
    }

    pub fn without_metadata(mut self) -> Self {
        self.no_metadata = true;
        self
    }

    pub fn with_rewrite_root(mut self, value: impl Into<String>) -> Self {
        self.rewrite_root = Some(value.into());
        self
    }

    pub fn with_rewrite_uuid(mut self, value: impl Into<String>) -> Self {
        self.rewrite_uuid = Some(value.into());
        self
    }

    pub fn with_preserve_empty_dirs(mut self, placeholder_filename: impl Into<String>) -> Self {
        self.preserve_empty_dirs = true;
        self.placeholder_filename = placeholder_filename.into();
        self
    }

    pub fn validate_mapping_destinations(&self) -> Result<(), String> {
        for mapping in self
            .fetch
            .iter()
            .chain(self.branches.iter())
            .chain(self.tags.iter())
        {
            crate::mapping::sanitize_refname(&mapping.git_ref)?;
        }
        Ok(())
    }

    pub fn to_git_config_entries(&self) -> Vec<(String, String)> {
        let prefix = format!("svn-remote.{}", self.name);
        let mut entries = vec![(format!("{prefix}.url"), self.url.clone())];

        entries.extend(self.fetch.iter().map(|m| {
            (
                format!("{prefix}.fetch"),
                format!("{}:{}", m.svn_path, m.git_ref),
            )
        }));
        entries.extend(self.branches.iter().map(|m| {
            (
                format!("{prefix}.branches"),
                format!("{}:{}", m.svn_path, m.git_ref),
            )
        }));
        entries.extend(self.tags.iter().map(|m| {
            (
                format!("{prefix}.tags"),
                format!("{}:{}", m.svn_path, m.git_ref),
            )
        }));

        if let Some(value) = &self.ignore_paths {
            entries.push((format!("{prefix}.ignore-paths"), value.clone()));
        }
        if let Some(value) = &self.include_paths {
            entries.push((format!("{prefix}.include-paths"), value.clone()));
        }
        if let Some(value) = &self.ignore_refs {
            entries.push((format!("{prefix}.ignore-refs"), value.clone()));
        }
        if let Some(value) = &self.authors_file {
            entries.push((format!("{prefix}.authors-file"), value.clone()));
        }
        if let Some(value) = &self.authors_prog {
            entries.push((format!("{prefix}.authors-prog"), value.clone()));
        }
        if let Some(value) = self.log_window_size {
            entries.push((format!("{prefix}.log-window-size"), value.to_string()));
        }
        if self.localtime {
            entries.push((format!("{prefix}.localtime"), "true".to_string()));
        }
        if let Some(value) = &self.username {
            entries.push((format!("{prefix}.username"), value.clone()));
        }
        if let Some(value) = &self.config_dir {
            entries.push((format!("{prefix}.config-dir"), value.clone()));
        }
        if self.no_auth_cache {
            entries.push((format!("{prefix}.no-auth-cache"), "true".to_string()));
        }
        if self.no_metadata {
            entries.push((format!("{prefix}.noMetadata"), "true".to_string()));
        }
        if let Some(value) = &self.rewrite_root {
            entries.push((format!("{prefix}.rewriteRoot"), value.clone()));
        }
        if let Some(value) = &self.rewrite_uuid {
            entries.push((format!("{prefix}.rewriteUUID"), value.clone()));
        }
        if self.preserve_empty_dirs {
            entries.push((format!("{prefix}.preserve-empty-dirs"), "true".to_string()));
            entries.push((
                format!("{prefix}.placeholder-filename"),
                self.placeholder_filename.clone(),
            ));
        }

        entries
    }
}

pub fn svn_remote_names(git: &GitCli) -> Result<Vec<String>, String> {
    let keys = git.config_names_matching(r"^svn-remote\..*\.url$")?;
    let mut names = keys
        .into_iter()
        .filter_map(|key| {
            key.strip_prefix("svn-remote.")
                .and_then(|value| value.strip_suffix(".url"))
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn read_svn_remote_config(git: &GitCli, remote: &str) -> Result<SvnRemoteConfig, String> {
    let prefix = format!("svn-remote.{remote}");
    let read = |key: &str| read_single_config(git, &format!("{prefix}.{key}"));
    let url = read("url")?.ok_or_else(|| format!("missing {prefix}.url"))?;
    let mappings = read_mappings(git, &prefix, "fetch", MappingKind::Fetch)?;
    let branch_mappings = read_mappings(git, &prefix, "branches", MappingKind::Branches)?;
    let tag_mappings = read_mappings(git, &prefix, "tags", MappingKind::Tags)?;

    Ok(SvnRemoteConfig {
        name: remote.to_string(),
        url,
        fetch: mappings,
        branches: branch_mappings,
        tags: tag_mappings,
        ignore_paths: read("ignore-paths")?,
        include_paths: read("include-paths")?,
        ignore_refs: read("ignore-refs")?,
        authors_file: read("authors-file")?,
        authors_prog: read("authors-prog")?,
        log_window_size: read("log-window-size")?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| format!("invalid {prefix}.log-window-size: {value}"))
            })
            .transpose()?,
        localtime: read("localtime")?.is_some_and(|value| value == "true"),
        username: read("username")?,
        config_dir: read("config-dir")?,
        no_auth_cache: read("no-auth-cache")?.is_some_and(|value| value == "true"),
        no_metadata: read("noMetadata")?.is_some_and(|value| value == "true"),
        rewrite_root: read("rewriteRoot")?,
        rewrite_uuid: read("rewriteUUID")?,
        preserve_empty_dirs: read("preserve-empty-dirs")?.is_some_and(|value| value == "true"),
        placeholder_filename: read("placeholder-filename")?
            .unwrap_or_else(|| ".gitignore".to_string()),
    })
}

fn read_single_config(git: &GitCli, key: &str) -> Result<Option<String>, String> {
    let values = git.config_get_all(key)?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(format!(
            "multiple values for {key}: expected one, found {}",
            values.len()
        )),
    }
}

fn read_mappings(
    git: &GitCli,
    prefix: &str,
    key: &str,
    kind: MappingKind,
) -> Result<Vec<RefMapping>, String> {
    git.config_get_all(&format!("{prefix}.{key}"))?
        .into_iter()
        .map(|value| parse_mapping(&value, kind.clone()))
        .collect()
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetadataOptions {
    pub no_metadata: bool,
    pub use_svm_props: bool,
    pub use_svnsync_props: bool,
    pub rewrite_root: Option<String>,
    pub rewrite_uuid: Option<String>,
}

impl MetadataOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.no_metadata && self.use_svm_props {
            return Err("Can't have both 'noMetadata' and 'useSvmProps' options set".to_string());
        }
        if self.no_metadata && self.use_svnsync_props {
            return Err(
                "Can't have both 'noMetadata' and 'useSvnsyncProps' options set".to_string(),
            );
        }
        if self.use_svm_props && self.use_svnsync_props {
            return Err(
                "Can't have both 'useSvmProps' and 'useSvnsyncProps' options set".to_string(),
            );
        }
        if self.use_svm_props && self.rewrite_root.is_some() {
            return Err("Can't have both 'useSvmProps' and 'rewriteRoot' options set".to_string());
        }
        if self.use_svm_props && self.rewrite_uuid.is_some() {
            return Err("Can't have both 'useSvmProps' and 'rewriteUUID' options set".to_string());
        }
        Ok(())
    }
}
