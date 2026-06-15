use std::path::Path;

use crate::cli::GcArgs;
use crate::git::GitCli;

pub fn run(_args: GcArgs) -> Result<(), String> {
    run_in_work_tree(".")
}

pub fn run_in_work_tree(work_tree: impl Into<std::path::PathBuf>) -> Result<(), String> {
    let git = GitCli::new(work_tree.into());
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    if !svn_dir.exists() {
        return Ok(());
    }
    remove_rev_map_locks(&svn_dir)
}

fn remove_rev_map_locks(path: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            remove_rev_map_locks(&path)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".rev_map.") && name.ends_with(".lock"))
        {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
