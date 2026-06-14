use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationAction {
    NoGitSvnMetadata,
    AlreadyV5,
    NeedsRevDbMigration,
}

pub fn inspect_git_svn_metadata(repo: &Path) -> Result<MigrationAction, String> {
    let svn = resolve_git_dir(repo)?.join("svn");
    if !svn.exists() {
        return Ok(MigrationAction::NoGitSvnMetadata);
    }

    let mut saw_rev_db = false;
    let mut saw_rev_map = false;
    for path in walk(&svn)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        saw_rev_map |= name.starts_with(".rev_map.");
        saw_rev_db |= name.starts_with(".rev_db.");
    }

    if saw_rev_map {
        Ok(MigrationAction::AlreadyV5)
    } else if saw_rev_db {
        Ok(MigrationAction::NeedsRevDbMigration)
    } else {
        Ok(MigrationAction::NoGitSvnMetadata)
    }
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
