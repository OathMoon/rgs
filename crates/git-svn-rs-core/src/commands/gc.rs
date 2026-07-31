use std::collections::BTreeSet;
use std::path::Path;

use crate::cli::GcArgs;
use crate::git::GitCli;
use crate::rev_map::REV_MAP_LOCK_MARKER;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use fs2::FileExt;
use sha2::{Digest, Sha256};
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
    let files = crate::filesystem::walk_files_no_symlinks(path)?;
    let mut log_directories = BTreeSet::new();
    for path in &files {
        let name = path.file_name().and_then(|name| name.to_str());
        if matches!(
            name,
            Some("unhandled.log" | "unhandled.log.gc-pending" | "unhandled.log.gz.gc-receipt")
        ) && let Some(parent) = path.parent()
        {
            log_directories.insert(parent.to_path_buf());
        }
    }
    for directory in log_directories {
        compress_unhandled_log(&directory.join("unhandled.log"))?;
    }
    for path in files {
        let name = path.file_name().and_then(|name| name.to_str());
        if name == Some("index")
            || name.is_some_and(|name| {
                name.starts_with(".git-svn-rs-gc-")
                    || matches!(
                        name,
                        "unhandled.log.gc-complete" | "unhandled.log.gz.gc-receipt-complete"
                    )
            })
        {
            remove_file(&path)?;
        } else if name.is_some_and(|name| name.starts_with(".rev_map.") && name.ends_with(".lock"))
        {
            remove_stale_rev_map_lock(&path)?;
        }
    }
    Ok(())
}

fn compress_unhandled_log(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let compressed_path = path.with_file_name("unhandled.log.gz");
    let pending_path = path.with_file_name("unhandled.log.gc-pending");
    let receipt_path = path.with_file_name("unhandled.log.gz.gc-receipt");
    for candidate in [path, &compressed_path, &pending_path, &receipt_path] {
        reject_symlink(candidate)?;
    }

    loop {
        recover_completed_log_compression(&compressed_path, &pending_path, &receipt_path)?;
        if !pending_path.exists() {
            if !path.exists() {
                return Ok(());
            }
            rename_durable(path, &pending_path, false)?;
        }

        let current = std::fs::read(&pending_path)
            .map_err(|error| format!("failed to read {}: {error}", pending_path.display()))?;
        let compressed = merged_compressed_log(&compressed_path, &current)?;
        write_atomic(&receipt_path, &encode_gc_receipt(&compressed, &current))?;
        write_atomic(&compressed_path, &compressed)?;
        finish_log_compression(&pending_path, &receipt_path, parent)?;
    }
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing to follow symlink during GC: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
    }
}

fn merged_compressed_log(compressed_path: &Path, current: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    if compressed_path.exists() {
        let file = std::fs::File::open(compressed_path)
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
    data.extend_from_slice(current);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&data).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

fn recover_completed_log_compression(
    compressed_path: &Path,
    pending_path: &Path,
    receipt_path: &Path,
) -> Result<(), String> {
    if !receipt_path.exists() {
        return Ok(());
    }
    let receipt = std::fs::read_to_string(receipt_path)
        .map_err(|error| format!("failed to read {}: {error}", receipt_path.display()))?;
    let (expected_archive, expected_pending) = decode_gc_receipt(&receipt, receipt_path)?;
    let actual = match std::fs::read(compressed_path) {
        Ok(bytes) => hex::encode(Sha256::digest(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(format!(
                "failed to read {}: {error}",
                compressed_path.display()
            ));
        }
    };
    if actual != expected_archive && !pending_path.exists() {
        return Err(format!(
            "GC receipt at {} does not match the compressed log",
            receipt_path.display()
        ));
    }
    if pending_path.exists() {
        let pending = std::fs::read(pending_path)
            .map_err(|error| format!("failed to read {}: {error}", pending_path.display()))?;
        if actual != expected_archive || hex::encode(Sha256::digest(pending)) != expected_pending {
            return Ok(());
        }
    }
    finish_log_compression(
        pending_path,
        receipt_path,
        compressed_path
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", compressed_path.display()))?,
    )
}

fn encode_gc_receipt(archive: &[u8], pending: &[u8]) -> Vec<u8> {
    format!(
        "git-svn-rs-gc-receipt-v1\narchive\t{}\npending\t{}\n",
        hex::encode(Sha256::digest(archive)),
        hex::encode(Sha256::digest(pending))
    )
    .into_bytes()
}

fn decode_gc_receipt(receipt: &str, path: &Path) -> Result<(String, String), String> {
    let mut lines = receipt.lines();
    let header = lines.next();
    let archive = lines.next().and_then(|line| line.strip_prefix("archive\t"));
    let pending = lines.next().and_then(|line| line.strip_prefix("pending\t"));
    let valid_hash = |value: &str| value.len() == 64 && hex::decode(value).is_ok();
    match (header, archive, pending, lines.next()) {
        (Some("git-svn-rs-gc-receipt-v1"), Some(archive), Some(pending), None)
            if valid_hash(archive) && valid_hash(pending) =>
        {
            Ok((archive.to_string(), pending.to_string()))
        }
        _ => Err(format!("invalid GC receipt at {}", path.display())),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".git-svn-rs-gc-")
        .tempfile_in(parent)
        .map_err(|error| format!("failed to create temporary GC file: {error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("failed to write temporary GC file: {error}"))?;
    temp.flush()
        .map_err(|error| format!("failed to flush temporary GC file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("failed to sync temporary GC file: {error}"))?;
    persist_temp_file(temp, path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync {}: {error}", path.display()))
}

#[cfg(unix)]
fn persist_temp_file(temp: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    temp.persist(path)
        .map_err(|error| format!("failed to replace {}: {}", path.display(), error.error))?;
    sync_directory(
        path.parent()
            .ok_or_else(|| format!("{} has no parent directory", path.display()))?,
    )
}

#[cfg(windows)]
fn persist_temp_file(temp: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    let (file, temporary_path) = temp
        .keep()
        .map_err(|error| format!("failed to retain temporary GC file: {}", error.error))?;
    drop(file);
    let result = rename_durable(&temporary_path, path, true);
    if result.is_err() {
        let _ = std::fs::remove_file(temporary_path);
    }
    result
}

#[cfg(not(any(unix, windows)))]
fn persist_temp_file(_temp: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    Err(format!(
        "crash-safe GC replacement is unavailable on this platform for {}",
        path.display()
    ))
}

#[cfg(unix)]
fn rename_durable(source: &Path, target: &Path, _replace: bool) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|error| {
        format!(
            "failed to rename {} as {}: {error}",
            source.display(),
            target.display()
        )
    })?;
    sync_directory(
        target
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", target.display()))?,
    )
}

#[cfg(windows)]
fn rename_durable(source: &Path, target: &Path, replace: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } == 0 {
        Err(format!(
            "failed to durably rename GC file: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn rename_durable(source: &Path, target: &Path, _replace: bool) -> Result<(), String> {
    Err(format!(
        "crash-safe GC rename is unavailable on this platform: {} -> {}",
        source.display(),
        target.display()
    ))
}

#[cfg(unix)]
fn finish_log_compression(pending: &Path, receipt: &Path, parent: &Path) -> Result<(), String> {
    remove_file(pending)?;
    sync_directory(parent)?;
    remove_file(receipt)?;
    sync_directory(parent)
}

#[cfg(windows)]
fn finish_log_compression(pending: &Path, receipt: &Path, _parent: &Path) -> Result<(), String> {
    let completed_pending = if pending.exists() {
        let completed = pending.with_file_name("unhandled.log.gc-complete");
        rename_durable(pending, &completed, true)?;
        Some(completed)
    } else {
        None
    };
    let completed_receipt = if receipt.exists() {
        let completed = receipt.with_file_name("unhandled.log.gz.gc-receipt-complete");
        rename_durable(receipt, &completed, true)?;
        Some(completed)
    } else {
        None
    };
    if let Some(path) = completed_pending {
        remove_file(&path)?;
    }
    if let Some(path) = completed_receipt {
        remove_file(&path)?;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn finish_log_compression(_pending: &Path, _receipt: &Path, _parent: &Path) -> Result<(), String> {
    Err("crash-safe GC cleanup is unavailable on this platform".to_string())
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

    fn read_compressed(path: &Path) -> Vec<u8> {
        let mut contents = Vec::new();
        GzDecoder::new(std::fs::File::open(path).unwrap())
            .read_to_end(&mut contents)
            .unwrap();
        contents
    }

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

    #[test]
    fn recovers_committed_archive_without_reappending_pending_log() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let archive = temp.path().join("unhandled.log.gz");
        let pending = temp.path().join("unhandled.log.gc-pending");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        std::fs::write(&source, b"r1\n").unwrap();
        compress_unhandled_log(&source).unwrap();
        std::fs::write(&pending, b"r2\n").unwrap();
        let committed = merged_compressed_log(&archive, b"r2\n").unwrap();
        std::fs::write(&archive, &committed).unwrap();
        std::fs::write(&receipt, encode_gc_receipt(&committed, b"r2\n")).unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\nr2\n");
        assert!(!pending.exists());
        assert!(!receipt.exists());
    }

    #[test]
    fn retries_uncommitted_archive_replacement_exactly_once() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let archive = temp.path().join("unhandled.log.gz");
        let pending = temp.path().join("unhandled.log.gc-pending");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        std::fs::write(&source, b"r1\n").unwrap();
        compress_unhandled_log(&source).unwrap();
        std::fs::write(&pending, b"r2\n").unwrap();
        let not_yet_committed = merged_compressed_log(&archive, b"r2\n").unwrap();
        std::fs::write(&receipt, encode_gc_receipt(&not_yet_committed, b"r2\n")).unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\nr2\n");
        assert!(!pending.exists());
        assert!(!receipt.exists());
    }

    #[test]
    fn stale_receipt_cannot_discard_a_different_pending_log() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("unhandled.log.gz");
        let pending = temp.path().join("unhandled.log.gc-pending");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        let compressed = merged_compressed_log(&archive, b"r1\n").unwrap();
        std::fs::write(&archive, &compressed).unwrap();
        std::fs::write(&pending, b"r2\n").unwrap();
        std::fs::write(
            &receipt,
            encode_gc_receipt(&compressed, b"old transaction\n"),
        )
        .unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\nr2\n");
        assert!(!pending.exists());
        assert!(!receipt.exists());
    }

    #[test]
    fn recovers_pending_only_and_then_processes_a_new_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let archive = temp.path().join("unhandled.log.gz");
        let pending = temp.path().join("unhandled.log.gc-pending");
        std::fs::write(&pending, b"r1\n").unwrap();
        std::fs::write(&source, b"r2\n").unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\nr2\n");
        assert!(!source.exists());
        assert!(!pending.exists());
    }

    #[test]
    fn matching_receipt_without_pending_is_cleaned_without_changing_archive() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("unhandled.log.gz");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        let compressed = merged_compressed_log(&archive, b"r1\n").unwrap();
        std::fs::write(&archive, &compressed).unwrap();
        std::fs::write(&receipt, encode_gc_receipt(&compressed, b"r1\n")).unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\n");
        assert!(!receipt.exists());
    }

    #[test]
    fn mismatching_receipt_without_pending_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("unhandled.log.gz");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        let compressed = merged_compressed_log(&archive, b"r1\n").unwrap();
        std::fs::write(&archive, &compressed).unwrap();
        std::fs::write(&receipt, encode_gc_receipt(b"not the archive", b"r1\n")).unwrap();

        let error = clean_svn_metadata(temp.path()).unwrap_err();

        assert!(error.contains("does not match the compressed log"));
        assert_eq!(std::fs::read(&archive).unwrap(), compressed);
        assert!(receipt.exists());
    }

    #[test]
    fn removes_owned_stale_temporary_files() {
        let temp = tempfile::tempdir().unwrap();
        let stale = temp.path().join(".git-svn-rs-gc-stale");
        std::fs::write(&stale, b"stale").unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert!(!stale.exists());
    }

    #[test]
    fn recovers_completed_tombstones_before_processing_new_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let archive = temp.path().join("unhandled.log.gz");
        let receipt = temp.path().join("unhandled.log.gz.gc-receipt");
        let completed = temp.path().join("unhandled.log.gc-complete");
        let compressed = merged_compressed_log(&archive, b"r1\n").unwrap();
        std::fs::write(&archive, &compressed).unwrap();
        std::fs::write(&receipt, encode_gc_receipt(&compressed, b"r1\n")).unwrap();
        std::fs::write(&completed, b"r1\n").unwrap();
        std::fs::write(&source, b"r2\n").unwrap();

        clean_svn_metadata(temp.path()).unwrap();

        assert_eq!(read_compressed(&archive), b"r1\nr2\n");
        assert!(!receipt.exists());
        assert!(!completed.exists());
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

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_archive_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let external = temp.path().join("external.gz");
        std::fs::write(&source, b"new log\n").unwrap();
        std::fs::write(&external, b"external archive\n").unwrap();
        symlink(&external, temp.path().join("unhandled.log.gz")).unwrap();

        let error = clean_svn_metadata(temp.path()).unwrap_err();

        assert!(error.contains("refusing to follow symlink"));
        assert_eq!(std::fs::read(&external).unwrap(), b"external archive\n");
        assert_eq!(std::fs::read(&source).unwrap(), b"new log\n");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_transaction_state_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let external = temp.path().join("external-pending");
        std::fs::write(&source, b"new log\n").unwrap();
        std::fs::write(&external, b"external pending\n").unwrap();
        symlink(&external, temp.path().join("unhandled.log.gc-pending")).unwrap();

        let error = clean_svn_metadata(temp.path()).unwrap_err();

        assert!(error.contains("refusing to follow symlink"));
        assert_eq!(std::fs::read(&external).unwrap(), b"external pending\n");
        assert_eq!(std::fs::read(&source).unwrap(), b"new log\n");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_receipt_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("unhandled.log");
        let external = temp.path().join("external-receipt");
        std::fs::write(&source, b"new log\n").unwrap();
        std::fs::write(&external, b"external receipt\n").unwrap();
        symlink(&external, temp.path().join("unhandled.log.gz.gc-receipt")).unwrap();

        let error = clean_svn_metadata(temp.path()).unwrap_err();

        assert!(error.contains("refusing to follow symlink"));
        assert_eq!(std::fs::read(&external).unwrap(), b"external receipt\n");
        assert_eq!(std::fs::read(&source).unwrap(), b"new log\n");
    }
}
