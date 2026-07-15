use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use fs2::FileExt;

use super::journal::{BatchState, DcommitJournal, JournalError, JournalStore};

const JOURNAL_DIRECTORY: &str = "dcommit-journal";
const REPOSITORY_LOCK_FILE: &str = "dcommit.lock";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedJournal {
    pub directory: PathBuf,
    pub journal: DcommitJournal,
}

/// All durable dcommit state found below one `.git/svn` directory.
///
/// At most one journal may be unfinished. Completed journals are retained in
/// the result so callers can treat them as terminal ledgers rather than
/// silently starting the same batch again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalDiscovery {
    pub active: Option<LocatedJournal>,
    pub completed: Vec<LocatedJournal>,
}

pub fn discover_repository_journals(
    svn_root: impl AsRef<Path>,
) -> Result<Option<JournalDiscovery>, JournalRegistryError> {
    let svn_root = svn_root.as_ref();
    if !svn_root.exists() {
        return Ok(None);
    }
    let root = checked_root(svn_root)?;
    let mut directories = Vec::new();
    collect_journal_directories(&root, &root, &mut directories)?;
    directories.sort();

    let mut active = Vec::new();
    let mut completed = Vec::new();
    for directory in directories {
        let Some(journal) = JournalStore::new(&directory).load()? else {
            continue;
        };
        let located = LocatedJournal { directory, journal };
        if located.journal.batch_state == BatchState::Complete {
            completed.push(located);
        } else {
            active.push(located);
        }
    }

    if active.len() > 1 {
        return Err(JournalRegistryError::MultipleActive(
            active
                .into_iter()
                .map(|located| located.directory)
                .collect(),
        ));
    }
    let active = active.pop();
    if active.is_none() && completed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(JournalDiscovery { active, completed }))
    }
}

#[derive(Debug)]
pub struct RepositoryDcommitLock {
    path: PathBuf,
    file: fs::File,
    registry_path: PathBuf,
}

impl RepositoryDcommitLock {
    pub fn acquire(svn_root: impl AsRef<Path>) -> Result<Self, JournalRegistryError> {
        let svn_root = svn_root.as_ref();
        fs::create_dir_all(svn_root)?;
        let root = checked_root(svn_root)?;
        let path = root.join(REPOSITORY_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let registry_path = fs::canonicalize(&path)?;
        if !register_process_lock(&registry_path) {
            return Err(JournalRegistryError::LockHeld(path));
        }
        match file.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                unregister_process_lock(&registry_path);
                return Err(JournalRegistryError::LockHeld(path));
            }
            Err(error) => {
                unregister_process_lock(&registry_path);
                return Err(error.into());
            }
        }
        let mut lock = Self {
            path,
            file,
            registry_path,
        };
        lock.file.set_len(0)?;
        writeln!(lock.file, "pid={}", std::process::id())?;
        lock.file.flush()?;
        lock.file.sync_all()?;
        Ok(lock)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RepositoryDcommitLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
        unregister_process_lock(&self.registry_path);
    }
}

fn process_locks() -> &'static Mutex<HashSet<PathBuf>> {
    static LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn register_process_lock(path: &Path) -> bool {
    process_locks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(path.to_path_buf())
}

fn unregister_process_lock(path: &Path) {
    process_locks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(path);
}

#[derive(Debug)]
pub enum JournalRegistryError {
    Io(io::Error),
    Journal(JournalError),
    UnsafePath(PathBuf),
    MultipleActive(Vec<PathBuf>),
    LockHeld(PathBuf),
}

impl fmt::Display for JournalRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Journal(error) => write!(formatter, "{error}"),
            Self::UnsafePath(path) => write!(
                formatter,
                "dcommit journal path escapes the SVN metadata root: {}",
                path.display()
            ),
            Self::MultipleActive(paths) => write!(
                formatter,
                "multiple unfinished dcommit journals were found: {}",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::LockHeld(path) => {
                write!(
                    formatter,
                    "repository dcommit lock exists: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for JournalRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::UnsafePath(_) | Self::MultipleActive(_) | Self::LockHeld(_) => None,
        }
    }
}

impl From<io::Error> for JournalRegistryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<JournalError> for JournalRegistryError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

fn collect_journal_directories(
    root: &Path,
    directory: &Path,
    journals: &mut Vec<PathBuf>,
) -> Result<(), JournalRegistryError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || is_link_or_reparse(&metadata) {
            continue;
        }
        let canonical = fs::canonicalize(&path)?;
        if !canonical.starts_with(root) {
            return Err(JournalRegistryError::UnsafePath(canonical));
        }
        if entry.file_name() == JOURNAL_DIRECTORY {
            journals.push(canonical);
        } else {
            collect_journal_directories(root, &canonical, journals)?;
        }
    }
    Ok(())
}

fn checked_root(path: &Path) -> Result<PathBuf, JournalRegistryError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || is_link_or_reparse(&metadata) {
        return Err(JournalRegistryError::UnsafePath(path.to_path_buf()));
    }
    Ok(fs::canonicalize(path)?)
}

#[cfg(windows)]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcommit::journal::{DcommitTargetIdentity, EntryState, JournalEntry, JournalLock};

    fn oid(character: char) -> String {
        character.to_string().repeat(40)
    }

    fn journal(state: BatchState) -> DcommitJournal {
        let entry_state = if state == BatchState::Submitting {
            EntryState::Ready {
                expected_base_revision: 40,
                expected_tracking_oid: oid('a'),
            }
        } else {
            EntryState::FetchedVerified {
                svn_revision: 41,
                imported_oid: oid('c'),
            }
        };
        DcommitJournal {
            target: DcommitTargetIdentity {
                remote_id: "svn".to_owned(),
                repository_root_url: "https://example.invalid/repos/project".to_owned(),
                repository_uuid: "12345678-1234-1234-1234-123456789abc".to_owned(),
                mapping_ref: "refs/remotes/origin/trunk".to_owned(),
                rev_map_path: ".git/svn/refs/remotes/origin/trunk/.rev_map.uuid".to_owned(),
                commit_url: "https://example.invalid/repos/project/trunk".to_owned(),
            },
            original_base_revision: 40,
            original_base_oid: oid('a'),
            original_head: oid('b'),
            no_rebase: false,
            config_fingerprint: "1010".to_owned(),
            entries: vec![JournalEntry {
                git_oid: oid('b'),
                base_oid: oid('a'),
                plan_fingerprint: "2020".to_owned(),
                message_fingerprint: "3030".to_owned(),
                state: entry_state,
            }],
            batch_state: state,
        }
    }

    fn save(directory: &Path, journal: &DcommitJournal) {
        let store = JournalStore::new(directory);
        let lock: JournalLock = store.acquire_lock().unwrap();
        store.save(&lock, journal).unwrap();
    }

    #[test]
    fn discovery_returns_none_when_no_journal_snapshot_exists() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("refs/trunk/dcommit-journal")).unwrap();

        assert!(discover_repository_journals(temp.path()).unwrap().is_none());
    }

    #[test]
    fn discovery_surfaces_active_and_completed_ledgers() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp.path().join("refs/trunk/dcommit-journal");
        let complete = temp.path().join("refs/branches/a/dcommit-journal");
        save(&active, &journal(BatchState::Submitting));
        save(&complete, &journal(BatchState::Complete));

        let discovery = discover_repository_journals(temp.path()).unwrap().unwrap();
        assert_eq!(
            discovery.active.unwrap().directory,
            active.canonicalize().unwrap()
        );
        assert_eq!(discovery.completed.len(), 1);
        assert_eq!(
            discovery.completed[0].directory,
            complete.canonicalize().unwrap()
        );
    }

    #[test]
    fn multiple_unfinished_journals_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        save(
            &temp.path().join("refs/trunk/dcommit-journal"),
            &journal(BatchState::Submitting),
        );
        save(
            &temp.path().join("refs/branches/a/dcommit-journal"),
            &journal(BatchState::RebasePending),
        );

        assert!(matches!(
            discover_repository_journals(temp.path()),
            Err(JournalRegistryError::MultipleActive(paths)) if paths.len() == 2
        ));
    }

    #[test]
    fn repository_lock_is_exclusive_and_drop_releases_it() {
        let temp = tempfile::tempdir().unwrap();
        let lock = RepositoryDcommitLock::acquire(temp.path()).unwrap();
        assert_eq!(
            lock.path(),
            &temp.path().canonicalize().unwrap().join("dcommit.lock")
        );
        assert!(matches!(
            RepositoryDcommitLock::acquire(temp.path()),
            Err(JournalRegistryError::LockHeld(_))
        ));
        drop(lock);
        RepositoryDcommitLock::acquire(temp.path()).unwrap();
        assert!(temp.path().join(REPOSITORY_LOCK_FILE).exists());
    }

    #[test]
    fn preexisting_unlocked_repository_lock_file_does_not_block() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(REPOSITORY_LOCK_FILE), b"stale pid\n").unwrap();

        RepositoryDcommitLock::acquire(temp.path()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn discovery_does_not_follow_symlinked_directories() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        save(
            &outside.path().join("dcommit-journal"),
            &journal(BatchState::Submitting),
        );
        symlink(outside.path(), root.path().join("outside")).unwrap();

        assert!(discover_repository_journals(root.path()).unwrap().is_none());
    }
}
