# Git Metadata RevMap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Git plumbing wrappers, `git-svn-id` parsing/formatting, `.git/svn` metadata paths, and `.rev_map` binary compatibility.

**Architecture:** Keep Git process execution behind a `GitBackend` trait modeled after `perl/Git.pm`, and keep binary `.rev_map` logic independent from repository IO. The rev map module implements Git's current format: network-order SVN revision plus raw Git object id bytes, including lock/fsync handling and trailing all-zero record behavior before any import command depends on it.

**Tech Stack:** Rust 1.95, Git CLI, `hex`, `tempfile`, unit and integration tests.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/git.rs`
- Create: `crates/git-svn-rs-core/src/git_svn_id.rs`
- Create: `crates/git-svn-rs-core/src/metadata.rs`
- Create: `crates/git-svn-rs-core/src/migration.rs`
- Create: `crates/git-svn-rs-core/src/rev_map.rs`
- Test: `crates/git-svn-rs-core/tests/git_svn_id.rs`
- Test: `crates/git-svn-rs-core/tests/rev_map.rs`
- Test: `crates/git-svn-rs-core/tests/git_backend.rs`

## Task 1: Add Metadata Modules

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/git.rs`
- Create: `crates/git-svn-rs-core/src/git_svn_id.rs`
- Create: `crates/git-svn-rs-core/src/metadata.rs`
- Create: `crates/git-svn-rs-core/src/rev_map.rs`

- [ ] **Step 1: Add dependencies**

Modify root `Cargo.toml`:

```toml
[workspace.dependencies]
hex = "0.4"
```

Modify `crates/git-svn-rs-core/Cargo.toml`:

```toml
[dependencies]
clap.workspace = true
fancy-regex.workspace = true
hex.workspace = true
thiserror.workspace = true
url.workspace = true
```

- [ ] **Step 2: Export modules**

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod authors;
pub mod cli;
pub mod config;
pub mod error;
pub mod filters;
pub mod git;
pub mod git_svn_id;
pub mod mapping;
pub mod metadata;
pub mod rev_map;
```

- [ ] **Step 3: Create shell modules**

Create `crates/git-svn-rs-core/src/git.rs`:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GitCli {
    work_tree: PathBuf,
}

impl GitCli {
    pub fn new(work_tree: impl Into<PathBuf>) -> Self {
        Self { work_tree: work_tree.into() }
    }

    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }
}
```

Create `crates/git-svn-rs-core/src/git_svn_id.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSvnId {
    pub url: String,
    pub revision: u32,
    pub uuid: String,
}
```

Create `crates/git-svn-rs-core/src/metadata.rs`:

```rust
use std::path::{Path, PathBuf};

pub fn svn_metadata_dir(git_dir: &Path, refname: &str) -> PathBuf {
    git_dir.join("svn").join(refname)
}
```

Create `crates/git-svn-rs-core/src/rev_map.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapRecord {
    pub revision: u32,
    pub object_id_hex: String,
}
```

- [ ] **Step 4: Verify compile**

Run:

```powershell
cargo test -p git-svn-rs-core
```

Expected: PASS.

## Task 2: Implement `git-svn-id`

**Files:**
- Modify: `crates/git-svn-rs-core/src/git_svn_id.rs`
- Test: `crates/git-svn-rs-core/tests/git_svn_id.rs`

- [ ] **Step 1: Write metadata tests**

Create `crates/git-svn-rs-core/tests/git_svn_id.rs`:

```rust
use git_svn_rs_core::git_svn_id::GitSvnId;

#[test]
fn parses_git_svn_id_footer() {
    let parsed = GitSvnId::parse("git-svn-id: https://svn.example/project/trunk@42 12345678-1234-1234-1234-123456789abc").unwrap();

    assert_eq!(parsed.url, "https://svn.example/project/trunk");
    assert_eq!(parsed.revision, 42);
    assert_eq!(parsed.uuid, "12345678-1234-1234-1234-123456789abc");
}

#[test]
fn formats_git_svn_id_footer() {
    let id = GitSvnId {
        url: "file:///repo/trunk".to_string(),
        revision: 7,
        uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
    };

    assert_eq!(
        id.to_footer(),
        "git-svn-id: file:///repo/trunk@7 aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
}

#[test]
fn rejects_missing_revision_separator() {
    let err = GitSvnId::parse("git-svn-id: file:///repo/trunk 7 uuid").unwrap_err();
    assert!(err.contains("missing @revision"));
}
```

- [ ] **Step 2: Run tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test git_svn_id
```

Expected: FAIL because `parse` and `to_footer` are missing.

- [ ] **Step 3: Implement parser and formatter**

Modify `crates/git-svn-rs-core/src/git_svn_id.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSvnId {
    pub url: String,
    pub revision: u32,
    pub uuid: String,
}

impl GitSvnId {
    pub fn parse(line: &str) -> Result<Self, String> {
        let rest = line
            .strip_prefix("git-svn-id: ")
            .ok_or_else(|| "missing git-svn-id prefix".to_string())?;
        let (url_rev, uuid) = rest
            .rsplit_once(' ')
            .ok_or_else(|| "missing uuid".to_string())?;
        let (url, rev) = url_rev
            .rsplit_once('@')
            .ok_or_else(|| "missing @revision".to_string())?;
        let revision = rev
            .parse::<u32>()
            .map_err(|_| format!("invalid revision: {rev}"))?;
        Ok(Self {
            url: url.to_string(),
            revision,
            uuid: uuid.to_string(),
        })
    }

    pub fn to_footer(&self) -> String {
        format!("git-svn-id: {}@{} {}", self.url, self.revision, self.uuid)
    }
}
```

- [ ] **Step 4: Verify tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test git_svn_id
```

Expected: PASS.

## Task 3: Implement RevMap Binary Format

**Files:**
- Modify: `crates/git-svn-rs-core/src/rev_map.rs`
- Test: `crates/git-svn-rs-core/tests/rev_map.rs`

- [ ] **Step 1: Write rev map tests**

Create `crates/git-svn-rs-core/tests/rev_map.rs`:

```rust
use git_svn_rs_core::rev_map::{ObjectFormat, RevMap, RevMapRecord};
use tempfile::tempdir;

#[test]
fn writes_sha1_records_as_24_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(1, "1111111111111111111111111111111111111111").unwrap();
    map.append(2, "2222222222222222222222222222222222222222").unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 48);
    assert_eq!(&bytes[0..4], &[0, 0, 0, 1]);
    assert_eq!(&bytes[24..28], &[0, 0, 0, 2]);
}

#[test]
fn gets_revision_by_binary_search() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(2, "2222222222222222222222222222222222222222").unwrap();
    map.append(9, "9999999999999999999999999999999999999999").unwrap();

    assert_eq!(
        map.get(9).unwrap(),
        Some("9999999999999999999999999999999999999999".to_string())
    );
    assert_eq!(map.get(4).unwrap(), None);
}

#[test]
fn all_zero_object_id_is_placeholder() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    map.append(10, "0000000000000000000000000000000000000000").unwrap();

    assert_eq!(map.get(10).unwrap(), None);
    assert_eq!(map.max_revision(false).unwrap(), Some(10));
    assert_eq!(map.max_revision(true).unwrap(), None);
}

#[test]
fn reset_truncates_after_matching_revision() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();

    let oid1 = "1111111111111111111111111111111111111111";
    let oid2 = "2222222222222222222222222222222222222222";
    map.append(1, oid1).unwrap();
    map.append(2, oid2).unwrap();
    map.reset_to(1, oid1).unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), 24);
    assert_eq!(map.get(2).unwrap(), None);
}

#[test]
fn sha256_records_are_36_bytes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha256).unwrap();

    map.append(1, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().len(), 36);
}

#[test]
fn record_type_round_trips() {
    let record = RevMapRecord {
        revision: 5,
        object_id_hex: "5555555555555555555555555555555555555555".to_string(),
    };
    assert_eq!(record.revision, 5);
}
```

- [ ] **Step 2: Run tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test rev_map
```

Expected: FAIL because `RevMap`, `ObjectFormat`, and methods are missing.

- [ ] **Step 3: Implement rev map**

Modify `crates/git-svn-rs-core/src/rev_map.rs`:

```rust
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub fn object_bytes(self) -> usize {
        match self {
            ObjectFormat::Sha1 => 20,
            ObjectFormat::Sha256 => 32,
        }
    }

    pub fn hex_len(self) -> usize {
        self.object_bytes() * 2
    }

    pub fn record_size(self) -> u64 {
        4 + self.object_bytes() as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevMapRecord {
    pub revision: u32,
    pub object_id_hex: String,
}

pub struct RevMap {
    path: PathBuf,
    format: ObjectFormat,
}

impl RevMap {
    pub fn open(path: impl AsRef<Path>, format: ObjectFormat) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self { path, format })
    }

    pub fn append(&mut self, revision: u32, object_id_hex: &str) -> Result<(), String> {
        if object_id_hex.len() != self.format.hex_len() {
            return Err(format!("object id must be {} hex chars", self.format.hex_len()));
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        file.write_all(&revision.to_be_bytes()).map_err(|e| e.to_string())?;
        let raw = hex::decode(object_id_hex).map_err(|e| e.to_string())?;
        file.write_all(&raw).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, revision: u32) -> Result<Option<String>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let record_size = self.format.record_size();
        if size % record_size != 0 {
            return Err(format!("inconsistent rev_map size: {size}"));
        }
        let mut low = 0_i64;
        let mut high = (size / record_size) as i64 - 1;
        while low <= high {
            let mid = (low + high) / 2;
            file.seek(SeekFrom::Start(mid as u64 * record_size)).map_err(|e| e.to_string())?;
            let record = self.read_record(&mut file)?;
            match record.revision.cmp(&revision) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid - 1,
                std::cmp::Ordering::Equal => {
                    if record.object_id_hex.chars().all(|c| c == '0') {
                        return Ok(None);
                    }
                    return Ok(Some(record.object_id_hex));
                }
            }
        }
        Ok(None)
    }

    pub fn max_revision(&self, require_commit: bool) -> Result<Option<u32>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let record_size = self.format.record_size();
        if size == 0 {
            return Ok(None);
        }
        file.seek(SeekFrom::Start(size - record_size)).map_err(|e| e.to_string())?;
        let record = self.read_record(&mut file)?;
        if require_commit && record.object_id_hex.chars().all(|c| c == '0') {
            return Ok(None);
        }
        Ok(Some(record.revision))
    }

    pub fn reset_to(&mut self, revision: u32, object_id_hex: &str) -> Result<(), String> {
        match self.get(revision)? {
            Some(found) if found == object_id_hex => {
                let size = self.format.record_size();
                let records_to_keep = self.position_of(revision)? + 1;
                let file = OpenOptions::new().write(true).open(&self.path).map_err(|e| e.to_string())?;
                file.set_len(records_to_keep * size).map_err(|e| e.to_string())?;
                Ok(())
            }
            Some(found) => Err(format!("revision {revision} maps to {found}, not {object_id_hex}")),
            None => Err(format!("revision {revision} not found")),
        }
    }

    fn position_of(&self, revision: u32) -> Result<u64, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let record_size = self.format.record_size();
        for index in 0..(size / record_size) {
            file.seek(SeekFrom::Start(index * record_size)).map_err(|e| e.to_string())?;
            if self.read_record(&mut file)?.revision == revision {
                return Ok(index);
            }
        }
        Err(format!("revision {revision} not found"))
    }

    fn read_record(&self, file: &mut File) -> Result<RevMapRecord, String> {
        let mut rev = [0_u8; 4];
        file.read_exact(&mut rev).map_err(|e| e.to_string())?;
        let mut oid = vec![0_u8; self.format.object_bytes()];
        file.read_exact(&mut oid).map_err(|e| e.to_string())?;
        Ok(RevMapRecord {
            revision: u32::from_be_bytes(rev),
            object_id_hex: hex::encode(oid),
        })
    }
}
```

- [ ] **Step 4: Verify rev map tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test rev_map
```

Expected: PASS.

## Task 4: Implement Git CLI Backend Basics

**Files:**
- Modify: `crates/git-svn-rs-core/src/git.rs`
- Test: `crates/git-svn-rs-core/tests/git_backend.rs`

- [ ] **Step 1: Write Git backend tests**

These tests must cover `perl/Git.pm`-style wrapper behavior: repository discovery from a working directory, config not-found returning `None`, multiple values remaining accessible when needed later, stderr/error propagation for failed commands, and no hidden process-wide current-directory mutation.

Create `crates/git-svn-rs-core/tests/git_backend.rs`:

```rust
use git_svn_rs_core::git::GitCli;
use tempfile::tempdir;

#[test]
fn initializes_git_repo_and_reports_git_dir() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());

    git.init().unwrap();

    let git_dir = git.git_dir().unwrap();
    assert!(git_dir.ends_with(".git"));
}

#[test]
fn config_set_and_get_round_trip() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    git.config_set("svn-remote.svn.url", "file:///repo").unwrap();

    assert_eq!(git.config_get("svn-remote.svn.url").unwrap(), Some("file:///repo".to_string()));
}

#[test]
fn config_get_all_preserves_multi_value_entries() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());
    git.init().unwrap();

    git.config_add("svn-remote.svn.branches", "branches/*:refs/remotes/origin/*").unwrap();
    git.config_add("svn-remote.svn.branches", "releases/*:refs/remotes/origin/releases/*").unwrap();

    assert_eq!(
        git.config_get_all("svn-remote.svn.branches").unwrap(),
        vec![
            "branches/*:refs/remotes/origin/*".to_string(),
            "releases/*:refs/remotes/origin/releases/*".to_string(),
        ]
    );
}

#[test]
fn failed_git_command_returns_stderr_context() {
    let dir = tempdir().unwrap();
    let git = GitCli::new(dir.path());

    let err = git.run_for_test(["rev-parse", "--git-dir"]).unwrap_err();
    assert!(err.contains("not a git repository") || err.contains("rev-parse"));
}
```

- [ ] **Step 2: Run tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test git_backend
```

Expected: FAIL because methods are missing.

- [ ] **Step 3: Implement Git process calls**

Modify `crates/git-svn-rs-core/src/git.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitCli {
    work_tree: PathBuf,
}

impl GitCli {
    pub fn new(work_tree: impl Into<PathBuf>) -> Self {
        Self { work_tree: work_tree.into() }
    }

    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    pub fn init(&self) -> Result<(), String> {
        self.run(["init"]).map(|_| ())
    }

    pub fn git_dir(&self) -> Result<String, String> {
        self.run(["rev-parse", "--git-dir"]).map(|s| s.trim().to_string())
    }

    pub fn config_set(&self, key: &str, value: &str) -> Result<(), String> {
        self.run(["config", key, value]).map(|_| ())
    }

    pub fn config_add(&self, key: &str, value: &str) -> Result<(), String> {
        self.run(["config", "--add", key, value]).map(|_| ())
    }

    pub fn config_get(&self, key: &str) -> Result<Option<String>, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["config", "--get", key])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).trim_end().to_string()))
        } else {
            Ok(None)
        }
    }

    pub fn config_get_all(&self, key: &str) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(["config", "--get-all", key])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).lines().map(|line| line.to_string()).collect())
        } else if output.status.code() == Some(1) {
            Ok(Vec::new())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    pub fn run_for_test<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
        self.run(args)
    }

    fn run<const N: usize>(&self, args: [&str; N]) -> Result<String, String> {
        let output = Command::new("git")
            .current_dir(&self.work_tree)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}
```

- [ ] **Step 4: Verify tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test git_backend
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add Cargo.toml crates/git-svn-rs-core
git commit -m "feat: add git metadata and rev map primitives"
```

## Self Review

- Spec coverage: `git-svn-id`, `.rev_map`, and Git config plumbing are implemented before import logic depends on them.
- Placeholder scan: record sizes and byte order are explicit.
- Type consistency: `GitCli`, `GitSvnId`, `RevMap`, `RevMapRecord`, and `ObjectFormat` are available to later phases.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 3 acceptance gate. Do not treat them as optional optimization work.

### Task 5: Add `Migration.pm` Metadata Compatibility

**Files:**
- Create: `crates/git-svn-rs-core/src/migration.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/config.rs`
- Test: `crates/git-svn-rs-core/tests/migration.rs`
- Test: `crates/git-svn-rs-core/tests/metadata_options.rs`

- [ ] **Step 1: Write migration discovery tests**

Create `crates/git-svn-rs-core/tests/migration.rs`:

```rust
use git_svn_rs_core::migration::{MigrationAction, inspect_git_svn_metadata};
use tempfile::tempdir;

#[test]
fn detects_v5_rev_map() {
    let dir = tempdir().unwrap();
    let svn_dir = dir.path().join(".git/svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(svn_dir.join(".rev_map.uuid"), []).unwrap();

    assert_eq!(inspect_git_svn_metadata(dir.path()).unwrap(), MigrationAction::AlreadyV5);
}

#[test]
fn detects_old_rev_db_needing_migration() {
    let dir = tempdir().unwrap();
    let svn_dir = dir.path().join(".git/svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(svn_dir.join(".rev_db.uuid"), []).unwrap();

    assert_eq!(inspect_git_svn_metadata(dir.path()).unwrap(), MigrationAction::NeedsRevDbMigration);
}
```

- [ ] **Step 2: Implement migration inspection**

Create `crates/git-svn-rs-core/src/migration.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationAction {
    NoGitSvnMetadata,
    AlreadyV5,
    NeedsRevDbMigration,
}

pub fn inspect_git_svn_metadata(repo: &std::path::Path) -> Result<MigrationAction, String> {
    let svn = repo.join(".git/svn");
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

fn walk(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
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
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod migration;
```

- [ ] **Step 3: Add metadata option conflict tests**

Create `crates/git-svn-rs-core/tests/metadata_options.rs`:

```rust
use git_svn_rs_core::config::MetadataOptions;

#[test]
fn rejects_no_metadata_with_svm_props() {
    let err = MetadataOptions {
        no_metadata: true,
        use_svm_props: true,
        use_svnsync_props: false,
        rewrite_root: None,
        rewrite_uuid: None,
    }.validate().unwrap_err();
    assert!(err.contains("noMetadata"));
    assert!(err.contains("useSvmProps"));
}

#[test]
fn rejects_svm_props_with_rewrite_root() {
    let err = MetadataOptions {
        no_metadata: false,
        use_svm_props: true,
        use_svnsync_props: false,
        rewrite_root: Some("https://mirror.example".to_string()),
        rewrite_uuid: None,
    }.validate().unwrap_err();
    assert!(err.contains("useSvmProps"));
    assert!(err.contains("rewriteRoot"));
}
```

Modify `crates/git-svn-rs-core/src/config.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetadataOptions {
    pub no_metadata: bool,
    pub use_svm_props: bool,
    pub use_svnsync_props: bool,
    pub rewrite_root: Option<String>,
    pub rewrite_uuid: Option<String>,
}

impl MetadataOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.no_metadata && self.use_svm_props {
            return Err("Can't have both 'noMetadata' and 'useSvmProps' options set".to_string());
        }
        if self.no_metadata && self.use_svnsync_props {
            return Err("Can't have both 'noMetadata' and 'useSvnsyncProps' options set".to_string());
        }
        if self.use_svm_props && self.use_svnsync_props {
            return Err("Can't have both 'useSvmProps' and 'useSvnsyncProps' options set".to_string());
        }
        if self.use_svm_props && self.rewrite_root.is_some() {
            return Err("Can't have both 'useSvmProps' and 'rewriteRoot' options set".to_string());
        }
        if self.use_svm_props && self.rewrite_uuid.is_some() {
            return Err("Can't have both 'useSvmProps' and 'rewriteUUID' options set".to_string());
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test migration
cargo test -p git-svn-rs-core --test metadata_options
```

Expected: PASS.

### Task 6: Expand RevMap Compatibility

**Files:**
- Modify: `crates/git-svn-rs-core/src/rev_map.rs`
- Test: `crates/git-svn-rs-core/tests/rev_map.rs`

- [ ] **Step 1: Add all-zero and lock behavior tests**

Append to `crates/git-svn-rs-core/tests/rev_map.rs`:

```rust
#[test]
fn max_revision_with_want_commit_uses_penultimate_when_last_is_zero() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
    map.append(4, "4444444444444444444444444444444444444444").unwrap();
    map.append(5, "0000000000000000000000000000000000000000").unwrap();

    assert_eq!(map.max_record(true).unwrap().unwrap().revision, 4);
}

#[test]
fn detects_two_trailing_zero_records_as_inconsistent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".rev_map.uuid");
    let mut map = RevMap::open(&path, ObjectFormat::Sha1).unwrap();
    map.append(4, "0000000000000000000000000000000000000000").unwrap();
    map.append(5, "0000000000000000000000000000000000000000").unwrap();

    assert!(map.max_record(true).unwrap_err().contains("inconsistent .rev_map"));
}
```

- [ ] **Step 2: Implement lock/fsync and `max_record(want_commit)`**

Modify `RevMap` so every append/reset operation creates and removes a sibling lock file, writes complete records only, calls `sync_all` before releasing the lock, and leaves a clear error if an existing lock is detected. Also implement `max_record(true)` to match `Git::SVN.pm` behavior: if the final record is all-zero, read the penultimate record; if that is also all-zero, return `inconsistent .rev_map`; if no non-zero record exists, return `None`.

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test rev_map
```

Expected: PASS.

## References

- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)
- [Migration.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Migration.pm)
- [git-svn.perl](https://github.com/git/git/blob/master/git-svn.perl)
- [raw git-svn.perl](https://raw.githubusercontent.com/git/git/master/git-svn.perl)
