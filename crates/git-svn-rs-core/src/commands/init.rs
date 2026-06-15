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
