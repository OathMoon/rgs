use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

const FORMAT_MAGIC: &str = "git-svn-rs-dcommit-journal";
const FORMAT_VERSION: u32 = 1;
const SNAPSHOT_PREFIX: &str = "dcommit-journal-g";
const SNAPSHOT_SUFFIX: &str = ".snapshot";
const LOCK_FILE: &str = "dcommit-journal.lock";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DcommitTargetIdentity {
    pub remote_id: String,
    pub repository_root_url: String,
    pub repository_uuid: String,
    pub mapping_ref: String,
    pub rev_map_path: String,
    pub commit_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryState {
    Queued,
    Ready {
        expected_base_revision: u64,
        expected_tracking_oid: String,
    },
    Submitted {
        svn_revision: u64,
    },
    FetchedVerified {
        svn_revision: u64,
        imported_oid: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    pub git_oid: String,
    pub base_oid: String,
    pub plan_fingerprint: String,
    pub message_fingerprint: String,
    pub state: EntryState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchState {
    Submitting,
    RebasePending,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DcommitJournal {
    pub target: DcommitTargetIdentity,
    pub original_base_revision: u64,
    pub original_base_oid: String,
    pub original_head: String,
    pub no_rebase: bool,
    pub config_fingerprint: String,
    /// Commits are stored oldest first.
    pub entries: Vec<JournalEntry>,
    pub batch_state: BatchState,
}

impl DcommitJournal {
    pub fn validate(&self) -> Result<(), JournalError> {
        for (name, value) in [
            ("target remote id", self.target.remote_id.as_str()),
            (
                "target repository root URL",
                self.target.repository_root_url.as_str(),
            ),
            (
                "target repository UUID",
                self.target.repository_uuid.as_str(),
            ),
            ("target mapping ref", self.target.mapping_ref.as_str()),
            ("target rev_map path", self.target.rev_map_path.as_str()),
            ("target commit URL", self.target.commit_url.as_str()),
            ("original base OID", self.original_base_oid.as_str()),
            ("original HEAD", self.original_head.as_str()),
            ("config fingerprint", self.config_fingerprint.as_str()),
        ] {
            require_nonempty(name, value)?;
        }
        validate_oid("original base OID", &self.original_base_oid)?;
        validate_oid("original HEAD", &self.original_head)?;

        let first = self.entries.first().ok_or_else(|| {
            JournalError::Invalid("dcommit journal commit queue is empty".to_owned())
        })?;
        if first.base_oid != self.original_base_oid {
            return Err(JournalError::Invalid(
                "oldest commit base OID does not match the original base OID".to_owned(),
            ));
        }
        if self.entries.last().map(|entry| entry.git_oid.as_str())
            != Some(self.original_head.as_str())
        {
            return Err(JournalError::Invalid(
                "newest queued commit does not match the original HEAD".to_owned(),
            ));
        }

        let mut saw_active = false;
        let mut saw_queued = false;
        for (index, entry) in self.entries.iter().enumerate() {
            validate_oid("entry Git OID", &entry.git_oid)?;
            validate_oid("entry base OID", &entry.base_oid)?;
            validate_fingerprint("entry plan fingerprint", &entry.plan_fingerprint)?;
            validate_fingerprint("entry message fingerprint", &entry.message_fingerprint)?;
            if index > 0 && entry.base_oid != self.entries[index - 1].git_oid {
                return Err(JournalError::Invalid(format!(
                    "entry {index} does not follow the preceding queued commit"
                )));
            }

            match &entry.state {
                EntryState::FetchedVerified {
                    svn_revision,
                    imported_oid,
                } => {
                    if saw_active || saw_queued {
                        return Err(JournalError::Invalid(
                            "fetched entries must form a prefix of the commit queue".to_owned(),
                        ));
                    }
                    require_positive_revision("fetched SVN revision", *svn_revision)?;
                    validate_oid("fetched imported OID", imported_oid)?;
                }
                EntryState::Submitted { svn_revision } => {
                    if saw_active || saw_queued {
                        return Err(JournalError::Invalid(
                            "at most one ready or submitted entry may precede queued entries"
                                .to_owned(),
                        ));
                    }
                    require_positive_revision("submitted SVN revision", *svn_revision)?;
                    saw_active = true;
                }
                EntryState::Ready {
                    expected_base_revision,
                    expected_tracking_oid,
                } => {
                    if saw_active || saw_queued {
                        return Err(JournalError::Invalid(
                            "at most one ready or submitted entry may precede queued entries"
                                .to_owned(),
                        ));
                    }
                    require_positive_revision(
                        "ready expected base revision",
                        *expected_base_revision,
                    )?;
                    validate_oid("ready expected tracking OID", expected_tracking_oid)?;
                    saw_active = true;
                }
                EntryState::Queued => saw_queued = true,
            }
        }

        if matches!(
            self.batch_state,
            BatchState::RebasePending | BatchState::Complete
        ) && self
            .entries
            .iter()
            .any(|entry| !matches!(entry.state, EntryState::FetchedVerified { .. }))
        {
            return Err(JournalError::Invalid(
                "rebase-pending and complete batches require every entry to be fetched and verified"
                    .to_owned(),
            ));
        }
        if self.no_rebase && self.batch_state == BatchState::RebasePending {
            return Err(JournalError::Invalid(
                "a no-rebase batch cannot be rebase-pending".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, JournalError> {
        self.validate()?;
        let mut lines = Vec::new();
        lines.push(format!("{FORMAT_MAGIC}\t{FORMAT_VERSION}"));
        push_string(&mut lines, "remote_id", &self.target.remote_id);
        push_string(
            &mut lines,
            "repository_root_url",
            &self.target.repository_root_url,
        );
        push_string(&mut lines, "repository_uuid", &self.target.repository_uuid);
        push_string(&mut lines, "mapping_ref", &self.target.mapping_ref);
        push_string(&mut lines, "rev_map_path", &self.target.rev_map_path);
        push_string(&mut lines, "commit_url", &self.target.commit_url);
        lines.push(format!(
            "original_base_revision\t{}",
            self.original_base_revision
        ));
        push_string(&mut lines, "original_base_oid", &self.original_base_oid);
        push_string(&mut lines, "original_head", &self.original_head);
        lines.push(format!("no_rebase\t{}", u8::from(self.no_rebase)));
        push_string(&mut lines, "config_fingerprint", &self.config_fingerprint);
        lines.push(format!(
            "batch_state\t{}",
            encode_batch_state(self.batch_state)
        ));
        lines.push(format!("entry_count\t{}", self.entries.len()));
        for entry in &self.entries {
            lines.push("entry".to_owned());
            push_string(&mut lines, "git_oid", &entry.git_oid);
            push_string(&mut lines, "base_oid", &entry.base_oid);
            push_string(&mut lines, "plan_fingerprint", &entry.plan_fingerprint);
            push_string(
                &mut lines,
                "message_fingerprint",
                &entry.message_fingerprint,
            );
            match &entry.state {
                EntryState::Queued => lines.push("state\tqueued".to_owned()),
                EntryState::Ready {
                    expected_base_revision,
                    expected_tracking_oid,
                } => lines.push(format!(
                    "state\tready\t{expected_base_revision}\t{}",
                    encode_string(expected_tracking_oid)
                )),
                EntryState::Submitted { svn_revision } => {
                    lines.push(format!("state\tsubmitted\t{svn_revision}"));
                }
                EntryState::FetchedVerified {
                    svn_revision,
                    imported_oid,
                } => lines.push(format!(
                    "state\tfetched_verified\t{svn_revision}\t{}",
                    encode_string(imported_oid)
                )),
            }
            lines.push("end_entry".to_owned());
        }
        lines.push("end".to_owned());
        Ok(format!("{}\n", lines.join("\n")).into_bytes())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, JournalError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| JournalError::Invalid("journal is not valid UTF-8".to_owned()))?;
        if !text.ends_with('\n') {
            return Err(JournalError::Invalid(
                "journal is missing its terminating newline".to_owned(),
            ));
        }
        let mut cursor = Cursor::new(text.strip_suffix('\n').unwrap_or(text));
        let header = cursor.next("format header")?;
        let (magic, version) = split_once(header, "format header")?;
        if magic != FORMAT_MAGIC {
            return Err(JournalError::Invalid("unknown journal format".to_owned()));
        }
        let version = parse_u32("format version", version)?;
        if version != FORMAT_VERSION {
            return Err(JournalError::UnsupportedVersion(version));
        }

        let target = DcommitTargetIdentity {
            remote_id: cursor.string("remote_id")?,
            repository_root_url: cursor.string("repository_root_url")?,
            repository_uuid: cursor.string("repository_uuid")?,
            mapping_ref: cursor.string("mapping_ref")?,
            rev_map_path: cursor.string("rev_map_path")?,
            commit_url: cursor.string("commit_url")?,
        };
        let original_base_revision = cursor.number("original_base_revision")?;
        let original_base_oid = cursor.string("original_base_oid")?;
        let original_head = cursor.string("original_head")?;
        let no_rebase = match cursor.value("no_rebase")? {
            "0" => false,
            "1" => true,
            _ => {
                return Err(JournalError::Invalid(
                    "no_rebase must be encoded as 0 or 1".to_owned(),
                ));
            }
        };
        let config_fingerprint = cursor.string("config_fingerprint")?;
        let batch_state = match cursor.value("batch_state")? {
            "submitting" => BatchState::Submitting,
            "rebase_pending" => BatchState::RebasePending,
            "complete" => BatchState::Complete,
            value => {
                return Err(JournalError::Invalid(format!(
                    "unknown batch state {value:?}"
                )));
            }
        };
        let entry_count = cursor.usize("entry_count")?;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            cursor.exact("entry")?;
            let git_oid = cursor.string("git_oid")?;
            let base_oid = cursor.string("base_oid")?;
            let plan_fingerprint = cursor.string("plan_fingerprint")?;
            let message_fingerprint = cursor.string("message_fingerprint")?;
            let state_line = cursor.next("entry state")?;
            let parts = state_line.split('\t').collect::<Vec<_>>();
            let state = match parts.as_slice() {
                ["state", "queued"] => EntryState::Queued,
                ["state", "ready", revision, tracking_oid] => EntryState::Ready {
                    expected_base_revision: parse_u64("expected base revision", revision)?,
                    expected_tracking_oid: decode_string(
                        "ready expected tracking OID",
                        tracking_oid,
                    )?,
                },
                ["state", "submitted", revision] => EntryState::Submitted {
                    svn_revision: parse_u64("submitted SVN revision", revision)?,
                },
                ["state", "fetched_verified", revision, imported_oid] => {
                    EntryState::FetchedVerified {
                        svn_revision: parse_u64("fetched SVN revision", revision)?,
                        imported_oid: decode_string("fetched imported OID", imported_oid)?,
                    }
                }
                _ => {
                    return Err(JournalError::Invalid(format!(
                        "invalid entry state line {state_line:?}"
                    )));
                }
            };
            cursor.exact("end_entry")?;
            entries.push(JournalEntry {
                git_oid,
                base_oid,
                plan_fingerprint,
                message_fingerprint,
                state,
            });
        }
        cursor.exact("end")?;
        if cursor.next_optional().is_some() {
            return Err(JournalError::Invalid(
                "journal contains trailing data".to_owned(),
            ));
        }

        let journal = Self {
            target,
            original_base_revision,
            original_base_oid,
            original_head,
            no_rebase,
            config_fingerprint,
            entries,
            batch_state,
        };
        journal.validate()?;
        Ok(journal)
    }
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Invalid(String),
    UnsupportedVersion(u32),
    LockHeld(PathBuf),
}

impl fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Invalid(message) => write!(formatter, "invalid dcommit journal: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported dcommit journal version {version}")
            }
            Self::LockHeld(path) => {
                write!(formatter, "dcommit journal lock exists: {}", path.display())
            }
        }
    }
}

impl std::error::Error for JournalError {}

impl From<io::Error> for JournalError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct JournalStore {
    directory: PathBuf,
}

impl JournalStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn acquire_lock(&self) -> Result<JournalLock, JournalError> {
        fs::create_dir_all(&self.directory)?;
        let path = self.directory.join(LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let registry_path = fs::canonicalize(&path)?;
        if !register_process_lock(&registry_path) {
            return Err(JournalError::LockHeld(path));
        }
        match file.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                unregister_process_lock(&registry_path);
                return Err(JournalError::LockHeld(path));
            }
            Err(error) => {
                unregister_process_lock(&registry_path);
                return Err(error.into());
            }
        }
        let mut lock = JournalLock {
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

    pub fn load(&self) -> Result<Option<DcommitJournal>, JournalError> {
        if !self.directory.exists() {
            return Ok(None);
        }
        let mut snapshots = self.snapshot_paths()?;
        snapshots.sort_unstable_by_key(|(generation, _)| *generation);
        let mut last_invalid = None;
        for (_, path) in snapshots.into_iter().rev() {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    last_invalid = Some(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            match DcommitJournal::decode(&bytes) {
                Ok(journal) => return Ok(Some(journal)),
                Err(error @ JournalError::UnsupportedVersion(_)) => return Err(error),
                Err(error) => last_invalid = Some(format!("{}: {error}", path.display())),
            }
        }
        match last_invalid {
            Some(message) => Err(JournalError::Invalid(format!(
                "no complete journal snapshot was found; latest error: {message}"
            ))),
            None => Ok(None),
        }
    }

    pub fn save(&self, lock: &JournalLock, journal: &DcommitJournal) -> Result<u64, JournalError> {
        if lock.path != self.directory.join(LOCK_FILE) || !lock.path.exists() {
            return Err(JournalError::Invalid(
                "journal save requires this store's live lock".to_owned(),
            ));
        }
        let bytes = journal.encode()?;
        let generation = self
            .snapshot_paths()?
            .into_iter()
            .map(|(generation, _)| generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| JournalError::Invalid("journal generation overflow".to_owned()))?;
        let final_path = self.snapshot_path(generation);
        if final_path.exists() {
            return Err(JournalError::Invalid(format!(
                "journal snapshot already exists: {}",
                final_path.display()
            )));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = self.directory.join(format!(
            ".{SNAPSHOT_PREFIX}{generation:020}.{}.{}.tmp",
            std::process::id(),
            nonce
        ));
        let write_result = (|| -> Result<(), JournalError> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            fs::rename(&temp_path, &final_path)?;
            sync_directory(&self.directory)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        write_result.map(|()| generation)
    }

    fn snapshot_paths(&self) -> Result<Vec<(u64, PathBuf)>, JournalError> {
        let mut snapshots = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            if let Some(generation) = snapshot_generation(&entry.file_name().to_string_lossy()) {
                snapshots.push((generation, entry.path()));
            }
        }
        Ok(snapshots)
    }

    fn snapshot_path(&self, generation: u64) -> PathBuf {
        self.directory.join(format!(
            "{SNAPSHOT_PREFIX}{generation:020}{SNAPSHOT_SUFFIX}"
        ))
    }
}

#[derive(Debug)]
pub struct JournalLock {
    path: PathBuf,
    file: fs::File,
    registry_path: PathBuf,
}

impl Drop for JournalLock {
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

fn snapshot_generation(name: &str) -> Option<u64> {
    name.strip_prefix(SNAPSHOT_PREFIX)?
        .strip_suffix(SNAPSHOT_SUFFIX)?
        .parse()
        .ok()
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), JournalError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), JournalError> {
    Ok(())
}

fn push_string(lines: &mut Vec<String>, name: &str, value: &str) {
    lines.push(format!("{name}\t{}", encode_string(value)));
}

fn encode_string(value: &str) -> String {
    hex::encode(value.as_bytes())
}

fn decode_string(name: &str, value: &str) -> Result<String, JournalError> {
    let bytes = hex::decode(value)
        .map_err(|_| JournalError::Invalid(format!("{name} is not valid hexadecimal")))?;
    String::from_utf8(bytes)
        .map_err(|_| JournalError::Invalid(format!("{name} is not valid UTF-8")))
}

fn encode_batch_state(state: BatchState) -> &'static str {
    match state {
        BatchState::Submitting => "submitting",
        BatchState::RebasePending => "rebase_pending",
        BatchState::Complete => "complete",
    }
}

fn split_once<'a>(line: &'a str, name: &str) -> Result<(&'a str, &'a str), JournalError> {
    line.split_once('\t')
        .ok_or_else(|| JournalError::Invalid(format!("missing value for {name}")))
}

fn parse_u32(name: &str, value: &str) -> Result<u32, JournalError> {
    value
        .parse()
        .map_err(|_| JournalError::Invalid(format!("{name} is not a valid u32")))
}

fn parse_u64(name: &str, value: &str) -> Result<u64, JournalError> {
    value
        .parse()
        .map_err(|_| JournalError::Invalid(format!("{name} is not a valid u64")))
}

fn require_nonempty(name: &str, value: &str) -> Result<(), JournalError> {
    if value.is_empty() {
        Err(JournalError::Invalid(format!("{name} is empty")))
    } else {
        Ok(())
    }
}

fn validate_oid(name: &str, value: &str) -> Result<(), JournalError> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(JournalError::Invalid(format!(
            "{name} must be a 40- or 64-character hexadecimal object ID"
        )));
    }
    Ok(())
}

fn validate_fingerprint(name: &str, value: &str) -> Result<(), JournalError> {
    if value.is_empty()
        || !value.len().is_multiple_of(2)
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(JournalError::Invalid(format!(
            "{name} must be non-empty hexadecimal"
        )));
    }
    Ok(())
}

fn require_positive_revision(name: &str, revision: u64) -> Result<(), JournalError> {
    if revision == 0 {
        Err(JournalError::Invalid(format!("{name} must be positive")))
    } else {
        Ok(())
    }
}

struct Cursor<'a> {
    lines: std::str::Lines<'a>,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines(),
        }
    }

    fn next(&mut self, name: &str) -> Result<&'a str, JournalError> {
        self.lines
            .next()
            .ok_or_else(|| JournalError::Invalid(format!("missing {name}")))
    }

    fn next_optional(&mut self) -> Option<&'a str> {
        self.lines.next()
    }

    fn exact(&mut self, expected: &str) -> Result<(), JournalError> {
        let actual = self.next(expected)?;
        if actual == expected {
            Ok(())
        } else {
            Err(JournalError::Invalid(format!(
                "expected {expected:?}, found {actual:?}"
            )))
        }
    }

    fn value(&mut self, expected_name: &str) -> Result<&'a str, JournalError> {
        let line = self.next(expected_name)?;
        let (name, value) = split_once(line, expected_name)?;
        if name != expected_name || value.contains('\t') {
            return Err(JournalError::Invalid(format!(
                "expected field {expected_name:?}, found {line:?}"
            )));
        }
        Ok(value)
    }

    fn string(&mut self, name: &str) -> Result<String, JournalError> {
        decode_string(name, self.value(name)?)
    }

    fn number(&mut self, name: &str) -> Result<u64, JournalError> {
        parse_u64(name, self.value(name)?)
    }

    fn usize(&mut self, name: &str) -> Result<usize, JournalError> {
        self.value(name)?
            .parse()
            .map_err(|_| JournalError::Invalid(format!("{name} is not a valid usize")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(character: char) -> String {
        character.to_string().repeat(40)
    }

    fn journal() -> DcommitJournal {
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
            original_head: oid('c'),
            no_rebase: false,
            config_fingerprint: "1010".to_owned(),
            entries: vec![
                JournalEntry {
                    git_oid: oid('b'),
                    base_oid: oid('a'),
                    plan_fingerprint: "2020".to_owned(),
                    message_fingerprint: "3030".to_owned(),
                    state: EntryState::FetchedVerified {
                        svn_revision: 41,
                        imported_oid: oid('d'),
                    },
                },
                JournalEntry {
                    git_oid: oid('c'),
                    base_oid: oid('b'),
                    plan_fingerprint: "4040".to_owned(),
                    message_fingerprint: "5050".to_owned(),
                    state: EntryState::Ready {
                        expected_base_revision: 41,
                        expected_tracking_oid: oid('d'),
                    },
                },
            ],
            batch_state: BatchState::Submitting,
        }
    }

    #[test]
    fn versioned_encoding_round_trips() {
        let journal = journal();
        let encoded = journal.encode().unwrap();
        assert_eq!(DcommitJournal::decode(&encoded).unwrap(), journal);

        let unknown = String::from_utf8(encoded)
            .unwrap()
            .replacen("\t1\n", "\t2\n", 1);
        assert!(matches!(
            DcommitJournal::decode(unknown.as_bytes()),
            Err(JournalError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn entire_queue_can_be_persisted_before_the_first_submission() {
        let mut journal = journal();
        for entry in &mut journal.entries {
            entry.state = EntryState::Queued;
        }

        let encoded = journal.encode().unwrap();
        assert_eq!(DcommitJournal::decode(&encoded).unwrap(), journal);
    }

    #[test]
    fn truncated_latest_snapshot_falls_back_to_previous_generation() {
        let temp = tempfile::tempdir().unwrap();
        let store = JournalStore::new(temp.path());
        let lock = store.acquire_lock().unwrap();
        let expected = journal();
        assert_eq!(store.save(&lock, &expected).unwrap(), 1);
        fs::write(store.snapshot_path(2), b"git-svn-rs-dcommit-journal\t1\n").unwrap();

        assert_eq!(store.load().unwrap(), Some(expected));
    }

    #[test]
    fn lock_is_exclusive_and_released_by_drop() {
        let temp = tempfile::tempdir().unwrap();
        let store = JournalStore::new(temp.path());
        let lock = store.acquire_lock().unwrap();
        assert!(matches!(
            store.acquire_lock(),
            Err(JournalError::LockHeld(_))
        ));
        drop(lock);
        store.acquire_lock().unwrap();
        assert!(temp.path().join(LOCK_FILE).exists());
    }

    #[test]
    fn preexisting_unlocked_lock_file_does_not_block_acquisition() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(LOCK_FILE), b"stale pid\n").unwrap();

        JournalStore::new(temp.path()).acquire_lock().unwrap();
    }

    #[test]
    fn state_validation_rejects_non_linear_or_impossible_batches() {
        let mut non_linear = journal();
        non_linear.entries[1].base_oid = oid('e');
        assert!(non_linear.validate().is_err());

        let mut invalid_order = journal();
        invalid_order.entries[0].state = EntryState::Queued;
        invalid_order.entries[1].state = EntryState::FetchedVerified {
            svn_revision: 41,
            imported_oid: oid('d'),
        };
        assert!(invalid_order.validate().is_err());

        let mut multiple_ready = journal();
        multiple_ready.entries[0].state = EntryState::Ready {
            expected_base_revision: 40,
            expected_tracking_oid: oid('a'),
        };
        multiple_ready.entries[1].state = EntryState::Ready {
            expected_base_revision: 41,
            expected_tracking_oid: oid('d'),
        };
        assert!(multiple_ready.validate().is_err());

        let mut invalid_tracking_oid = journal();
        invalid_tracking_oid.entries[1].state = EntryState::Ready {
            expected_base_revision: 41,
            expected_tracking_oid: "not-an-oid".to_owned(),
        };
        assert!(invalid_tracking_oid.validate().is_err());

        let mut premature_rebase = journal();
        premature_rebase.batch_state = BatchState::RebasePending;
        assert!(premature_rebase.validate().is_err());
    }
}
