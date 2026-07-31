use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationAction {
    NoGitSvnMetadata,
    AlreadyV5,
    NeedsV0Migration,
    NeedsV1Migration,
    NeedsV2Migration,
    NeedsRevDbMigration,
    NeedsLegacyLayoutMigration,
    NeedsConfigIdentity,
    NeedsConfigCleanup,
    MixedLayouts,
}

pub fn ensure_supported_git_svn_metadata(repo: &Path) -> Result<(), String> {
    match inspect_git_svn_metadata(repo)? {
        MigrationAction::NoGitSvnMetadata | MigrationAction::AlreadyV5 => Ok(()),
        MigrationAction::NeedsV0Migration => Err(
            "legacy git-svn v0 metadata (.git/<id>/info/url with refs/heads/<id>-HEAD) requires migration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsV1Migration => Err(
            "legacy git-svn v1 metadata (.git/<id>/info/url with refs/remotes/<id>) requires migration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsV2Migration => Err(
            "legacy git-svn v2 metadata under .git/svn has old info/url identity but no complete svn-remote configuration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsRevDbMigration => Err(
            "legacy git-svn rev_db metadata requires migration: v3/v4 layouts need the one-way v5 rev_map conversion; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsLegacyLayoutMigration => Err(
            "legacy git-svn v0-v2 metadata layout requires migration; run the frozen Perl `git svn migrate` on a backup before using git-svn-rs"
                .to_string(),
        ),
        MigrationAction::NeedsConfigIdentity => Err(
            "git-svn metadata has history state but no complete svn-remote URL/mapping identity; restore or migrate the configuration on a backup before using git-svn-rs"
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
    let config = inspect_svn_remote_config(&git_dir.join("config"))?;
    let mut root_legacy = Vec::new();
    for entry in std::fs::read_dir(&git_dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir()
            && path.file_name().and_then(|name| name.to_str()) != Some("svn")
            && path.join("info/url").is_file()
        {
            root_legacy.push(classify_root_legacy(&git_dir, &path));
        }
    }

    if !svn.exists() {
        return if !root_legacy.is_empty() {
            Ok(collapse_root_legacy(&root_legacy))
        } else if config.has_incomplete_remote {
            Ok(MigrationAction::NeedsConfigCleanup)
        } else {
            Ok(MigrationAction::NoGitSvnMetadata)
        };
    }

    let mut saw_rev_db = false;
    let mut saw_rev_map = false;
    for path in crate::filesystem::walk_files_no_symlinks(&svn)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        saw_rev_map |= name.starts_with(".rev_map.");
        saw_rev_db |= name.starts_with(".rev_db.");
    }

    let saw_info_url = has_legacy_svn_info_url(&svn)?;
    if (saw_rev_map && saw_rev_db)
        || (!root_legacy.is_empty() && (saw_rev_map || saw_rev_db || saw_info_url))
    {
        Ok(MigrationAction::MixedLayouts)
    } else if !root_legacy.is_empty() {
        Ok(collapse_root_legacy(&root_legacy))
    } else if config.has_incomplete_remote {
        Ok(MigrationAction::NeedsConfigCleanup)
    } else if saw_rev_db {
        Ok(MigrationAction::NeedsRevDbMigration)
    } else if saw_rev_map {
        if config.has_complete_remote {
            Ok(MigrationAction::AlreadyV5)
        } else {
            Ok(MigrationAction::NeedsConfigIdentity)
        }
    } else if saw_info_url && !config.has_complete_remote {
        Ok(MigrationAction::NeedsV2Migration)
    } else {
        Ok(MigrationAction::NoGitSvnMetadata)
    }
}

#[derive(Default)]
struct ConfigEvidence {
    has_complete_remote: bool,
    has_incomplete_remote: bool,
}

fn inspect_svn_remote_config(config: &Path) -> Result<ConfigEvidence, String> {
    if !config.exists() {
        return Ok(ConfigEvidence::default());
    }
    let contents = std::fs::read_to_string(config).map_err(|error| error.to_string())?;
    let mut evidence = ConfigEvidence::default();
    let mut active_remote = None::<(bool, bool)>;
    let finish = |active: Option<(bool, bool)>, evidence: &mut ConfigEvidence| {
        if let Some((has_url, has_mapping)) = active {
            evidence.has_complete_remote |= has_url && has_mapping;
            evidence.has_incomplete_remote |= !(has_url && has_mapping);
        }
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            finish(active_remote.take(), &mut evidence);
            active_remote =
                if line.to_ascii_lowercase().starts_with("[svn-remote ") && line.ends_with(']') {
                    Some((false, false))
                } else {
                    None
                };
        } else if let Some((has_url, has_mapping)) = active_remote.as_mut()
            && let Some((key, value)) = line.split_once('=')
            && !value.trim().is_empty()
        {
            match key.trim().to_ascii_lowercase().as_str() {
                "url" => *has_url = true,
                "fetch" | "branches" | "tags" => *has_mapping = true,
                _ => {}
            }
        }
    }
    finish(active_remote, &mut evidence);
    Ok(evidence)
}

fn classify_root_legacy(git_dir: &Path, metadata_dir: &Path) -> MigrationAction {
    let Some(id) = metadata_dir.file_name() else {
        return MigrationAction::NeedsLegacyLayoutMigration;
    };
    let v0_ref = git_dir
        .join("refs/heads")
        .join(format!("{}-HEAD", id.to_string_lossy()));
    let v1_ref = git_dir.join("refs/remotes").join(id);
    match (v0_ref.is_file(), v1_ref.is_file()) {
        (true, false) => MigrationAction::NeedsV0Migration,
        (false, true) => MigrationAction::NeedsV1Migration,
        _ => MigrationAction::NeedsLegacyLayoutMigration,
    }
}

fn collapse_root_legacy(actions: &[MigrationAction]) -> MigrationAction {
    let first = &actions[0];
    if actions.iter().all(|action| action == first) {
        first.clone()
    } else {
        MigrationAction::NeedsLegacyLayoutMigration
    }
}

fn has_legacy_svn_info_url(svn: &Path) -> Result<bool, String> {
    Ok(crate::filesystem::walk_files_no_symlinks(svn)?
        .iter()
        .any(|path| {
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
