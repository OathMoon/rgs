use crate::mapping::{LayoutMappings, RefMapping};

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
