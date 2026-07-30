use std::path::Path;

use crate::cli::GcArgs;
use crate::git::GitCli;
use crate::rev_map::REV_MAP_LOCK_MARKER;
use flate2::Compression;
use flate2::read::GzDecoder;
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
    crate::import_transaction::with_exclusive_lock(&git, || {
        crate::import_transaction::ensure_no_pending(&git)?;
        clean_svn_metadata(&svn_dir)
    })
}

fn clean_svn_metadata(path: &Path) -> Result<(), String> {
    for path in crate::filesystem::walk_files_no_symlinks(path)? {
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
    let current = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let compressed_path = path.with_file_name("unhandled.log.gz");
    let mut data = Vec::new();
    if compressed_path.exists() {
        let file = std::fs::File::open(&compressed_path)
            .map_err(|error| format!("failed to read {}: {error}", compressed_path.display()))?;
        GzDecoder::new(file)
            .read_to_end(&mut data)
            .map_err(|error| {
                format!(
                    "failed to decompress {}: {error}",
                    compressed_path.display()
                )
            })?;
    }
    data.extend_from_slice(&current);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    std::fs::write(&compressed_path, compressed)
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

    #[test]
    fn repeated_gc_preserves_prior_unhandled_log_history() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("unhandled.log");
        std::fs::write(&path, "r1\n  +empty_dir: first\n").unwrap();
        compress_unhandled_log(&path).unwrap();
        std::fs::write(&path, "r2\n  +empty_dir: second\n").unwrap();
        compress_unhandled_log(&path).unwrap();

        let mut contents = String::new();
        GzDecoder::new(std::fs::File::open(temp.path().join("unhandled.log.gz")).unwrap())
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(
            contents,
            "r1\n  +empty_dir: first\nr2\n  +empty_dir: second\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_directories_and_cycles_without_touching_external_files() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let svn = temp.path().join("svn");
        let external = temp.path().join("external");
        std::fs::create_dir_all(&svn).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        let index = external.join("index");
        let unhandled = external.join("unhandled.log");
        std::fs::write(&index, b"external index\n").unwrap();
        std::fs::write(&unhandled, b"external log\n").unwrap();
        symlink(&external, svn.join("external-link")).unwrap();
        symlink(&svn, svn.join("cycle")).unwrap();

        clean_svn_metadata(&svn).unwrap();
        let root_link = temp.path().join("svn-root-link");
        symlink(&external, &root_link).unwrap();
        clean_svn_metadata(&root_link).unwrap();

        assert_eq!(std::fs::read(index).unwrap(), b"external index\n");
        assert_eq!(std::fs::read(unhandled).unwrap(), b"external log\n");
        assert!(!external.join("unhandled.log.gz").exists());
    }
}
