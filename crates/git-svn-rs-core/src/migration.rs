use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationAction {
    NoGitSvnMetadata,
    AlreadyV5,
    NeedsRevDbMigration,
    NeedsLegacyLayoutMigration,
    NeedsConfigCleanup,
    MixedLayouts,
}

pub fn ensure_supported_git_svn_metadata(repo: &Path) -> Result<(), String> {
    match inspect_git_svn_metadata(repo)? {
        MigrationAction::NoGitSvnMetadata | MigrationAction::AlreadyV5 => Ok(()),
        MigrationAction::NeedsRevDbMigration => Err(
            "legacy git-svn rev_db metadata requires migration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsLegacyLayoutMigration => Err(
            "legacy git-svn v0-v2 metadata layout requires migration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsConfigCleanup => Err(
            "empty [svn-remote] configuration requires cleanup; remove empty svn-remote sections from .git/config on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::MixedLayouts => Err(
            "ambiguous mixed legacy and v5 git-svn metadata; reconcile the layout on a backup before using git-svn-rs"
                .to_string(),
        ),
    }
}

pub fn inspect_git_svn_metadata(repo: &Path) -> Result<MigrationAction, String> {
    let git_dir = resolve_git_dir(repo)?;
    let svn = git_dir.join("svn");
    let saw_empty_remote = has_empty_svn_remote(&git_dir.join("config"))?;
    let mut saw_legacy_root_layout = false;
    for entry in std::fs::read_dir(&git_dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir()
            && path.file_name().and_then(|name| name.to_str()) != Some("svn")
            && path.join("info/url").is_file()
        {
            saw_legacy_root_layout = true;
        }
    }

    if !svn.exists() {
        return if saw_legacy_root_layout {
            Ok(MigrationAction::NeedsLegacyLayoutMigration)
        } else if saw_empty_remote {
            Ok(MigrationAction::NeedsConfigCleanup)
        } else {
            Ok(MigrationAction::NoGitSvnMetadata)
        };
    }

    let mut saw_rev_db = false;
    let mut saw_rev_map = false;
    for path in walk(&svn)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        saw_rev_map |= name.starts_with(".rev_map.");
        saw_rev_db |= name.starts_with(".rev_db.");
    }

    if saw_rev_map && (saw_rev_db || saw_legacy_root_layout) {
        Ok(MigrationAction::MixedLayouts)
    } else if saw_empty_remote {
        Ok(MigrationAction::NeedsConfigCleanup)
    } else if saw_rev_map {
        Ok(MigrationAction::AlreadyV5)
    } else if saw_rev_db {
        Ok(MigrationAction::NeedsRevDbMigration)
    } else if saw_legacy_root_layout || has_legacy_svn_info_url(&svn)? {
        Ok(MigrationAction::NeedsLegacyLayoutMigration)
    } else {
        Ok(MigrationAction::NoGitSvnMetadata)
    }
}

fn has_empty_svn_remote(config: &Path) -> Result<bool, String> {
    if !config.exists() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(config).map_err(|error| error.to_string())?;
    let mut active_remote = None;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if active_remote == Some(false) {
                return Ok(true);
            }
            active_remote = if line.starts_with("[svn-remote ") && line.ends_with(']') {
                Some(false)
            } else {
                None
            };
        } else if let Some(has_identity) = active_remote.as_mut()
            && let Some((key, _)) = line.split_once('=')
            && matches!(key.trim(), "url" | "fetch")
        {
            *has_identity = true;
        }
    }
    Ok(active_remote == Some(false))
}

fn has_legacy_svn_info_url(svn: &Path) -> Result<bool, String> {
    Ok(walk(svn)?.iter().any(|path| {
        path.file_name().and_then(|name| name.to_str()) == Some("url")
            && path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some("info")
    }))
}

fn resolve_git_dir(repo: &Path) -> Result<PathBuf, String> {
    let dot_git = repo.join(".git");
    if dot_git.is_dir() {
        return Ok(dot_git);
    }

    let gitfile = std::fs::read_to_string(&dot_git).map_err(|e| e.to_string())?;
    let gitdir = gitfile
        .trim()
        .strip_prefix("gitdir:")
        .ok_or_else(|| format!("invalid gitfile: {}", dot_git.display()))?
        .trim();
    let gitdir = PathBuf::from(gitdir);
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        repo.join(gitdir)
    };

    let common_dir_file = gitdir.join("commondir");
    if common_dir_file.exists() {
        let common_dir = std::fs::read_to_string(&common_dir_file).map_err(|e| e.to_string())?;
        let common_dir = PathBuf::from(common_dir.trim());
        return Ok(if common_dir.is_absolute() {
            common_dir
        } else {
            gitdir.join(common_dir)
        });
    }

    Ok(gitdir)
}

fn walk(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}
