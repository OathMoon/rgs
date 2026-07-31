use std::fs;
use std::path::Path;

use crate::cli::{InitArgs, LayoutArgs, SharedFetchArgs};
use crate::config::SvnRemoteConfig;
use crate::git::{GitCli, GitCommandOutput};
use crate::mapping::{LayoutMappings, build_from_layout_args};
use crate::path_url::{
    SvnUrlProfile, canonicalize_url, join_paths, repository_relative_url_path, svn_url_profile,
    validate_fetch_url,
};
use crate::svn::auth::{AuthOperation, prompted_password};
use crate::svn::cli::SvnCliBackend;

pub fn run(args: InitArgs) -> Result<(), String> {
    run_with_output(args).map(|_| ())
}

pub(crate) fn run_with_output(mut args: InitArgs) -> Result<GitCommandOutput, String> {
    validate_args(&args)?;
    build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;
    let normalization_notice =
        normalize_layout_args(&mut args.url, &mut args.layout, &args.shared)?;
    let mappings = build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;
    let mut output = run_prepared_with_output(args, mappings)?;
    if let Some(notice) = normalization_notice {
        output.stderr.push_str(&notice);
    }
    Ok(output)
}

fn validate_args(args: &InitArgs) -> Result<(), String> {
    if args.shared.revision.is_some() {
        return Err("init --revision is not supported; use clone or fetch".to_string());
    }
    if args.shared.password.is_some() {
        return Err(
            "init --password is not supported and passwords are never persisted".to_string(),
        );
    }
    if args.shared.log_window_size == Some(0) {
        return Err("--log-window-size must be greater than zero".to_string());
    }
    crate::config::MetadataOptions {
        no_metadata: args.shared.no_metadata,
        use_svm_props: false,
        use_svnsync_props: args.shared.use_svnsync_props,
        rewrite_root: args.shared.rewrite_root.clone(),
        rewrite_uuid: args.shared.rewrite_uuid.clone(),
    }
    .validate()?;
    Ok(())
}

pub(crate) fn run_prepared_with_output(
    mut args: InitArgs,
    mappings: LayoutMappings,
) -> Result<GitCommandOutput, String> {
    validate_args(&args)?;
    if let Some(authors_file) = &args.shared.authors_file {
        let current_dir = std::env::current_dir()
            .map_err(|error| format!("failed to resolve --authors-file: {error}"))?;
        args.shared.authors_file = Some(absolute_authors_file(authors_file, &current_dir)?);
    }
    let work_tree = args.path.as_deref().unwrap_or(".");
    fs::create_dir_all(work_tree).map_err(|e| e.to_string())?;

    let git = GitCli::new(work_tree);
    let output = git.init_with_output()?;

    let config = svn_remote_config(args, mappings);
    write_svn_remote_config(&git, &config)?;
    Ok(output)
}

fn absolute_authors_file(path: &str, current_dir: &Path) -> Result<String, String> {
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };
    absolute.into_os_string().into_string().map_err(|path| {
        format!(
            "--authors-file path is not valid UTF-8: {}",
            path.to_string_lossy()
        )
    })
}

pub(crate) fn normalize_layout_args(
    url: &mut String,
    layout: &mut LayoutArgs,
    shared: &SharedFetchArgs,
) -> Result<Option<String>, String> {
    let has_layout = layout.stdlayout
        || layout.trunk.is_some()
        || !layout.branches.is_empty()
        || !layout.tags.is_empty();
    if !has_layout {
        return Ok(None);
    }
    let has_full_layout_url = layout
        .trunk
        .iter()
        .chain(&layout.branches)
        .chain(&layout.tags)
        .any(|value| url::Url::parse(value).is_ok());
    if !has_full_layout_url {
        return Ok(None);
    }

    let original_url = canonicalize_url(url);
    let trunk_url = layout
        .trunk
        .as_deref()
        .and_then(|trunk| url::Url::parse(trunk).ok())
        .map(|trunk| canonicalize_url(trunk.as_str()));
    let repository_root = match svn_url_profile(url) {
        SvnUrlProfile::Mock => canonicalize_url(url),
        SvnUrlProfile::File | SvnUrlProfile::Svn | SvnUrlProfile::Http | SvnUrlProfile::SvnSsh => {
            let mut backend = SvnCliBackend::new(url.clone())?;
            if let Some(username) = &shared.username {
                backend = backend.with_username(username);
            }
            let askpass = if shared.password.is_none()
                && matches!(
                    svn_url_profile(url),
                    SvnUrlProfile::Svn | SvnUrlProfile::Http
                ) {
                prompted_password(
                    url,
                    shared.username.as_deref(),
                    shared.config_dir.as_deref(),
                    shared.no_auth_cache,
                    AuthOperation::Read,
                )?
            } else {
                None
            };
            if let Some(password) = shared.password.as_ref().or(askpass.as_ref()) {
                backend = backend.with_password(password);
            }
            if let Some(config_dir) = &shared.config_dir {
                backend = backend.with_config_dir(config_dir);
            }
            if shared.no_auth_cache {
                backend = backend.without_auth_cache();
            }
            backend.repository_root()?
        }
        SvnUrlProfile::Https | SvnUrlProfile::Unsupported => {
            validate_fetch_url(url)?;
            unreachable!("unsupported fetch URL validation must fail")
        }
    };
    let session_path = repository_relative_url_path(&repository_root, url)?;

    let normalize_path = |value: &str, glob: bool| -> Result<String, String> {
        let mut path = if url::Url::parse(value).is_ok() {
            repository_relative_url_path(&repository_root, value)?
        } else {
            join_paths([session_path.as_str(), value])
        };
        if glob && !path.contains('*') && !path.contains('{') {
            path.push_str("/*");
        }
        Ok(path)
    };

    layout.trunk = match layout.trunk.as_deref() {
        Some(trunk) => Some(normalize_path(trunk, false)?),
        None if layout.stdlayout => Some(join_paths([session_path.as_str(), "trunk"])),
        None => None,
    };
    layout.branches = if layout.branches.is_empty() && layout.stdlayout {
        vec![normalize_path("branches", true)?]
    } else {
        layout
            .branches
            .iter()
            .map(|branch| normalize_path(branch, true))
            .collect::<Result<Vec<_>, _>>()?
    };
    layout.tags = if layout.tags.is_empty() && layout.stdlayout {
        vec![normalize_path("tags", true)?]
    } else {
        layout
            .tags
            .iter()
            .map(|tag| normalize_path(tag, true))
            .collect::<Result<Vec<_>, _>>()?
    };
    layout.stdlayout = false;
    *url = canonicalize_url(&repository_root);
    let normalization_source = if layout.trunk.is_some() {
        trunk_url.unwrap_or(original_url)
    } else {
        url.clone()
    };
    Ok((normalization_source != *url)
        .then(|| format!("Using higher level of URL: {normalization_source} => {url}\n")))
}

fn svn_remote_config(args: InitArgs, mappings: LayoutMappings) -> SvnRemoteConfig {
    let mut config = SvnRemoteConfig::new("svn", args.url, mappings);

    if let Some(value) = args.shared.ignore_paths {
        config = config.with_ignore_paths(value);
    }
    if let Some(value) = args.shared.include_paths {
        config = config.with_include_paths(value);
    }
    if let Some(value) = args.shared.ignore_refs {
        config = config.with_ignore_refs(value);
    }
    if let Some(value) = args.shared.authors_file {
        config = config.with_authors_file(value);
    }
    if let Some(value) = args.shared.authors_prog {
        config = config.with_authors_prog(value);
    }
    if let Some(value) = args.shared.log_window_size {
        config = config.with_log_window_size(value);
    }
    if args.shared.localtime {
        config = config.with_localtime();
    }
    if let Some(value) = args.shared.username {
        config = config.with_username(value);
    }
    if let Some(value) = args.shared.config_dir {
        config = config.with_config_dir(value);
    }
    if args.shared.no_auth_cache {
        config = config.without_auth_cache();
    }
    if args.shared.no_metadata {
        config = config.without_metadata();
    }
    if args.shared.use_svnsync_props {
        config = config.with_svnsync_props();
    }
    if let Some(value) = args.shared.rewrite_root {
        config = config.with_rewrite_root(value);
    }
    if let Some(value) = args.shared.rewrite_uuid {
        config = config.with_rewrite_uuid(value);
    }
    if args.shared.preserve_empty_dirs {
        config = config.with_preserve_empty_dirs(args.shared.placeholder_filename);
    }

    config
}

fn write_svn_remote_config(git: &GitCli, config: &SvnRemoteConfig) -> Result<(), String> {
    let prefix = format!("svn-remote.{}", config.name);

    git.config_set(&format!("{prefix}.url"), &config.url)?;
    add_mappings(git, &format!("{prefix}.fetch"), &config.fetch)?;
    add_mappings(git, &format!("{prefix}.branches"), &config.branches)?;
    add_mappings(git, &format!("{prefix}.tags"), &config.tags)?;

    if let Some(value) = &config.ignore_paths {
        git.config_set(&format!("{prefix}.ignore-paths"), value)?;
    }
    if let Some(value) = &config.include_paths {
        git.config_set(&format!("{prefix}.include-paths"), value)?;
    }
    if let Some(value) = &config.ignore_refs {
        git.config_set(&format!("{prefix}.ignore-refs"), value)?;
    }
    if let Some(value) = &config.authors_file {
        git.config_set(&format!("{prefix}.authors-file"), value)?;
    }
    if let Some(value) = &config.authors_prog {
        git.config_set(&format!("{prefix}.authors-prog"), value)?;
    }
    if let Some(value) = config.log_window_size {
        git.config_set(&format!("{prefix}.log-window-size"), &value.to_string())?;
    }
    if config.localtime {
        git.config_set(&format!("{prefix}.localtime"), "true")?;
    }
    if let Some(value) = &config.username {
        git.config_set(&format!("{prefix}.username"), value)?;
    }
    if let Some(value) = &config.config_dir {
        git.config_set(&format!("{prefix}.config-dir"), value)?;
    }
    if config.no_auth_cache {
        git.config_set(&format!("{prefix}.no-auth-cache"), "true")?;
    }
    if config.no_metadata {
        git.config_set(&format!("{prefix}.noMetadata"), "true")?;
    }
    if config.use_svnsync_props {
        git.config_set(&format!("{prefix}.useSvnsyncProps"), "true")?;
    }
    if let Some(value) = &config.rewrite_root {
        git.config_set(&format!("{prefix}.rewriteRoot"), value)?;
    }
    if let Some(value) = &config.rewrite_uuid {
        git.config_set(&format!("{prefix}.rewriteUUID"), value)?;
    }
    if config.preserve_empty_dirs {
        git.config_set(&format!("{prefix}.preserve-empty-dirs"), "true")?;
        git.config_set(
            &format!("{prefix}.placeholder-filename"),
            &config.placeholder_filename,
        )?;
    }

    Ok(())
}

fn add_mappings(
    git: &GitCli,
    key: &str,
    mappings: &[crate::mapping::RefMapping],
) -> Result<(), String> {
    for mapping in mappings {
        git.config_add(key, &format!("{}:{}", mapping.svn_path, mapping.git_ref))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::absolute_authors_file;

    #[test]
    fn authors_file_is_made_absolute_from_the_invocation_directory() {
        let current_dir = tempfile::tempdir().unwrap();
        let expected = current_dir.path().join("config/authors.txt");

        assert_eq!(
            absolute_authors_file("config/authors.txt", current_dir.path()).unwrap(),
            expected.to_str().unwrap()
        );
        assert_eq!(
            absolute_authors_file(expected.to_str().unwrap(), current_dir.path()).unwrap(),
            expected.to_str().unwrap()
        );
    }
}
