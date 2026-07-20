use std::path::Path;

use crate::cli::GcArgs;
use crate::git::GitCli;
use crate::rev_map::REV_MAP_LOCK_MARKER;
use flate2::Compression;
use flate2::write::GzEncoder;
use fs2::FileExt;
use std::io::Read;
use std::io::Write;

pub fn run(_args: GcArgs) -> Result<(), String> {
    run_in_work_tree(".")
}

pub fn run_in_work_tree(work_tree: impl Into<std::path::PathBuf>) -> Result<(), String> {
    let work_tree = work_tree.into();
    crate::migration::ensure_supported_git_svn_metadata(&work_tree)?;
    let git = GitCli::new(work_tree);
    let git_dir = git.git_dir()?;
    let svn_dir = git.work_tree().join(git_dir).join("svn");
    if !svn_dir.exists() {
        return Ok(());
    }
    clean_svn_metadata(&svn_dir)
}

fn clean_svn_metadata(path: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            clean_svn_metadata(&path)?;
            continue;
        }

        let name = path.file_name().and_then(|name| name.to_str());
        if name == Some("unhandled.log") {
            compress_unhandled_log(&path)?;
        } else if name == Some("index") {
            remove_file(&path)?;
        } else if name.is_some_and(|name| name.starts_with(".rev_map.") && name.ends_with(".lock"))
        {
            remove_stale_rev_map_lock(&path)?;
        }
    }
    Ok(())
}

fn compress_unhandled_log(path: &Path) -> Result<(), String> {
    let data = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    std::fs::write(path.with_file_name("unhandled.log.gz"), compressed)
        .map_err(|error| format!("failed to write compressed {}: {error}", path.display()))?;
    remove_file(path)
}

fn remove_stale_rev_map_lock(path: &Path) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    let mut marker = String::new();
    if file.read_to_string(&mut marker).is_err() || marker != REV_MAP_LOCK_MARKER {
        return Ok(());
    }

    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file)
                .map_err(|error| format!("failed to unlock {}: {error}", path.display()))?;
            drop(file);
            remove_file(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(error) => Err(format!(
            "failed to check whether {} is active: {error}",
            path.display()
        )),
    }
}

fn remove_file(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_managed_rev_map_lock_when_no_writer_holds_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".rev_map.uuid.lock");
        std::fs::write(&path, REV_MAP_LOCK_MARKER).unwrap();

        remove_stale_rev_map_lock(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn preserves_managed_rev_map_lock_while_writer_holds_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".rev_map.uuid.lock");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        std::fs::write(&path, REV_MAP_LOCK_MARKER).unwrap();
        file.lock_exclusive().unwrap();

        remove_stale_rev_map_lock(&path).unwrap();

        assert!(path.exists());
        FileExt::unlock(&file).unwrap();
    }

    #[test]
    fn preserves_unmarked_rev_map_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".rev_map.uuid.lock");
        std::fs::write(&path, []).unwrap();

        remove_stale_rev_map_lock(&path).unwrap();

        assert!(path.exists());
    }
}
