use std::fs;

use crate::cli::InitArgs;
use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::mapping::{LayoutMappings, build_from_layout_args};

pub fn run(args: InitArgs) -> Result<(), String> {
    let mappings = build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;

    let work_tree = args.path.as_deref().unwrap_or(".");
    fs::create_dir_all(work_tree).map_err(|e| e.to_string())?;

    let git = GitCli::new(work_tree);
    git.init()?;

    let config = svn_remote_config(args, mappings);
    write_svn_remote_config(&git, &config)
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
