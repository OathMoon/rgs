# Import Clone Fetch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `init`, `fetch`, and `clone` so SVN revisions become Git commits with compatible refs, rev maps, and `git-svn-id` footers.

**Architecture:** `init` writes Git config and metadata directories, `fetch` drives an SVN delta editor modeled after `Git::SVN::Fetcher`, and `clone` runs `init` followed by `fetch` in the target worktree. `FastImport` may remain a byte-safe Git object writing mechanism, but `SvnFetchEditor` is the required source of SVN behavior; `ImportPlanner` is only a legacy smoke-test adapter.

**Tech Stack:** Rust 1.95, Git CLI `fast-import`, mock SVN backend tests, local `svnadmin` fixture tests when available.

Production command modules must use the shared `GitCli` wrapper for Git operations. Direct `std::process::Command::new("git")` is acceptable in integration test setup only.

---

## File Structure

- Modify: `crates/git-svn-rs-core/src/git.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/fast_import.rs`
- Create: `crates/git-svn-rs-core/src/fetch_editor.rs`
- Create: `crates/git-svn-rs-core/src/import.rs`
- Create: `crates/git-svn-rs-core/src/commands/mod.rs`
- Create: `crates/git-svn-rs-core/src/commands/init.rs`
- Create: `crates/git-svn-rs-core/src/commands/fetch.rs`
- Create: `crates/git-svn-rs-core/src/commands/clone.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Create: `crates/git-svn-rs-cli/tests/support/mod.rs`
- Create: `crates/git-svn-rs-cli/tests/support/svn_fixture.rs`
- Test: `crates/git-svn-rs-core/tests/fast_import.rs`
- Test: `crates/git-svn-rs-core/tests/import_mock.rs`
- Test: `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`

## Task 1: Build Fast-Import Stream Writer

**Files:**
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/fast_import.rs`
- Test: `crates/git-svn-rs-core/tests/fast_import.rs`

- [ ] **Step 1: Export `fast_import` module**

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod fast_import;
```

- [ ] **Step 2: Write fast-import tests**

Create `crates/git-svn-rs-core/tests/fast_import.rs`:

```rust
use git_svn_rs_core::fast_import::{FastImportCommit, FastImportStream, FileChange};

#[test]
fn serializes_single_file_commit() {
    let commit = FastImportCommit {
        mark: 1,
        refname: "refs/remotes/git-svn".to_string(),
        author: "Jane Doe <jane@example.com>".to_string(),
        committer: "Jane Doe <jane@example.com>".to_string(),
        timestamp: 1_704_067_200,
        message: "add file\n\ngit-svn-id: file:///repo/trunk@2 uuid".to_string(),
        parent_mark: None,
        changes: vec![FileChange::Modify {
            path: "src/lib.rs".to_string(),
            mode: "100644".to_string(),
            content: b"pub fn answer() -> u8 { 42 }\n".to_vec(),
        }],
    };

    let stream = String::from_utf8(FastImportStream::new().commit(&commit).finish()).unwrap();

    assert!(stream.contains("commit refs/remotes/git-svn\n"));
    assert!(stream.contains("mark :1\n"));
    assert!(stream.contains("M 100644 inline src/lib.rs\n"));
    assert!(stream.contains("data 29\npub fn answer() -> u8 { 42 }\n"));
}

#[test]
fn serializes_delete_and_symlink_modes() {
    let commit = FastImportCommit {
        mark: 2,
        refname: "refs/remotes/git-svn".to_string(),
        author: "A <a@example.com>".to_string(),
        committer: "A <a@example.com>".to_string(),
        timestamp: 1,
        message: "change".to_string(),
        parent_mark: Some(1),
        changes: vec![
            FileChange::Delete { path: "old.txt".to_string() },
            FileChange::Modify {
                path: "link".to_string(),
                mode: "120000".to_string(),
                content: b"target.txt".to_vec(),
            },
        ],
    };

    let stream = String::from_utf8(FastImportStream::new().commit(&commit).finish()).unwrap();

    assert!(stream.contains("from :1\n"));
    assert!(stream.contains("D old.txt\n"));
    assert!(stream.contains("M 120000 inline link\n"));
}
```

- [ ] **Step 3: Run tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test fast_import
```

Expected: FAIL because fast-import types are missing.

- [ ] **Step 4: Implement stream writer**

Create `crates/git-svn-rs-core/src/fast_import.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Modify { path: String, mode: String, content: Vec<u8> },
    Delete { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastImportCommit {
    pub mark: u32,
    pub refname: String,
    pub author: String,
    pub committer: String,
    pub timestamp: i64,
    pub message: String,
    pub parent_mark: Option<u32>,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Default)]
pub struct FastImportStream {
    output: Vec<u8>,
}

impl FastImportStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn commit(mut self, commit: &FastImportCommit) -> Self {
        self.write_line(&format!("commit {}", commit.refname));
        self.write_line(&format!("mark :{}", commit.mark));
        self.write_line(&format!("author {} {} +0000", commit.author, commit.timestamp));
        self.write_line(&format!("committer {} {} +0000", commit.committer, commit.timestamp));
        self.write_line(&format!("data {}", commit.message.as_bytes().len()));
        self.output.extend_from_slice(commit.message.as_bytes());
        self.output.push(b'\n');
        if let Some(parent) = commit.parent_mark {
            self.write_line(&format!("from :{}", parent));
        }
        for change in &commit.changes {
            match change {
                FileChange::Modify { path, mode, content } => {
                    self.write_line(&format!("M {mode} inline {path}"));
                    self.write_line(&format!("data {}", content.len()));
                    self.output.extend_from_slice(content);
                    self.output.push(b'\n');
                }
                FileChange::Delete { path } => {
                    self.write_line(&format!("D {path}"));
                }
            }
        }
        self.output.push(b'\n');
        self
    }

    pub fn finish(self) -> Vec<u8> {
        self.output
    }

    fn write_line(&mut self, line: &str) {
        self.output.extend_from_slice(line.as_bytes());
        self.output.push(b'\n');
    }
}
```

- [ ] **Step 5: Verify tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test fast_import
```

Expected: PASS.

## Task 2: Convert SVN Revision Events to Fast-Import Commits

**Files:**
- Create: `crates/git-svn-rs-core/src/import.rs`
- Test: `crates/git-svn-rs-core/tests/import_mock.rs`

- [ ] **Step 1: Write import conversion tests**

Create `crates/git-svn-rs-core/tests/import_mock.rs`:

```rust
use std::collections::BTreeMap;
use git_svn_rs_core::import::{ImportPlanner, ImportTarget};
use git_svn_rs_core::svn::{ChangeAction, ChangedPath, NodeKind, RevisionEvent};

#[test]
fn revision_event_becomes_commit_with_footer() {
    let event = RevisionEvent {
        revision: 2,
        author: "jdoe".to_string(),
        message: "add trunk file".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        changed_paths: vec![ChangedPath {
            path: "/trunk/src/lib.rs".to_string(),
            action: ChangeAction::Add,
            copy_from_path: None,
            copy_from_rev: None,
            kind: NodeKind::File,
            properties: BTreeMap::new(),
            content: Some(b"pub fn answer() -> u8 { 42 }\n".to_vec()),
        }],
    };

    let target = ImportTarget {
        svn_path: "trunk".to_string(),
        refname: "refs/remotes/git-svn".to_string(),
        url: "file:///repo/trunk".to_string(),
        uuid: "uuid".to_string(),
    };
    let commits = ImportPlanner::new(target).plan_revision(&event, None).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].refname, "refs/remotes/git-svn");
    assert!(commits[0].message.contains("git-svn-id: file:///repo/trunk@2 uuid"));
    assert_eq!(commits[0].changes.len(), 1);
}
```

- [ ] **Step 2: Run test and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test import_mock
```

Expected: FAIL because `ImportPlanner` is missing.

- [ ] **Step 3: Implement import planner**

Create `crates/git-svn-rs-core/src/import.rs`:

```rust
use crate::fast_import::{FastImportCommit, FileChange};
use crate::git_svn_id::GitSvnId;
use crate::svn::{ChangeAction, NodeKind, RevisionEvent};

#[derive(Debug, Clone)]
pub struct ImportTarget {
    pub svn_path: String,
    pub refname: String,
    pub url: String,
    pub uuid: String,
}

pub struct ImportPlanner {
    target: ImportTarget,
}

impl ImportPlanner {
    pub fn new(target: ImportTarget) -> Self {
        Self { target }
    }

    pub fn plan_revision(&self, event: &RevisionEvent, parent_mark: Option<u32>) -> Result<Vec<FastImportCommit>, String> {
        let prefix = format!("/{}", self.target.svn_path.trim_matches('/'));
        let mut changes = Vec::new();
        for changed in &event.changed_paths {
            if !changed.path.starts_with(&prefix) {
                continue;
            }
            let rel = changed.path[prefix.len()..].trim_start_matches('/').to_string();
            if rel.is_empty() || changed.kind == NodeKind::Directory {
                continue;
            }
            match changed.action {
                ChangeAction::Delete => changes.push(FileChange::Delete { path: rel }),
                ChangeAction::Add | ChangeAction::Modify | ChangeAction::Replace => {
                    let mode = if changed.kind == NodeKind::Symlink {
                        "120000"
                    } else if changed.properties.contains_key("svn:executable") {
                        "100755"
                    } else {
                        "100644"
                    };
                    changes.push(FileChange::Modify {
                        path: rel,
                        mode: mode.to_string(),
                        content: changed.content.clone().unwrap_or_default(),
                    });
                }
            }
        }
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        let footer = GitSvnId {
            url: self.target.url.clone(),
            revision: event.revision,
            uuid: self.target.uuid.clone(),
        };
        Ok(vec![FastImportCommit {
            mark: event.revision,
            refname: self.target.refname.clone(),
            author: format!("{} <{}@localhost>", event.author, event.author),
            committer: format!("{} <{}@localhost>", event.author, event.author),
            timestamp: 0,
            message: format!("{}\n\n{}", event.message, footer.to_footer()),
            parent_mark,
            changes,
        }])
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod import;
```

- [ ] **Step 4: Verify import planner test passes**

Run:

```powershell
cargo test -p git-svn-rs-core --test import_mock
```

Expected: PASS.

## Task 3: Implement `init` Command Behavior

**Files:**
- Create: `crates/git-svn-rs-core/src/commands/mod.rs`
- Create: `crates/git-svn-rs-core/src/commands/init.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`

- [ ] **Step 1: Write init smoke test**

Create `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`:

```rust
use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn init_writes_svn_remote_config() {
    let dir = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();

    cmd.current_dir(dir.path())
        .args(["init", "file:///repo", "--stdlayout"])
        .assert()
        .success();

    let config = std::fs::read_to_string(dir.path().join(".git/config")).unwrap();
    assert!(config.contains("[svn-remote \"svn\"]"));
    assert!(config.contains("url = file:///repo"));
    assert!(config.contains("fetch = trunk:refs/remotes/origin/trunk"));
}
```

- [ ] **Step 2: Run test and see failure**

Run:

```powershell
cargo test -p git-svn-rs --test clone_fetch_smoke init_writes_svn_remote_config
```

Expected: FAIL because `init` still returns the phase 1 not-implemented error.

- [ ] **Step 3: Implement command module**

Create `crates/git-svn-rs-core/src/commands/mod.rs`:

```rust
pub mod init;
```

Create `crates/git-svn-rs-core/src/commands/init.rs`:

```rust
use crate::cli::InitArgs;
use crate::config::SvnRemoteConfig;
use crate::git::GitCli;
use crate::mapping::build_from_layout_args;

pub fn run(args: &InitArgs, cwd: &std::path::Path) -> Result<(), String> {
    let repo_dir = args
        .path
        .as_deref()
        .map(|path| cwd.join(path))
        .unwrap_or_else(|| cwd.to_path_buf());
    std::fs::create_dir_all(&repo_dir).map_err(|e| e.to_string())?;
    let git = GitCli::new(&repo_dir);
    git.init()?;
    let mappings = build_from_layout_args(
        args.layout.stdlayout,
        args.layout.trunk.as_deref(),
        &args.layout.branches,
        &args.layout.tags,
        args.layout.prefix.as_deref(),
    )?;
    let config = SvnRemoteConfig::new("svn", &args.url, mappings);
    for (key, value) in config.to_git_config_entries() {
        git.config_set(&key, &value)?;
    }
    Ok(())
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod commands;
```

- [ ] **Step 4: Wire CLI to init command**

Modify `crates/git-svn-rs-cli/src/main.rs`:

```rust
Command::Init(args) => {
    git_svn_rs_core::commands::init::run(&args, &std::env::current_dir()?)
        .map_err(anyhow::Error::msg)
}
```

- [ ] **Step 5: Verify init test passes**

Run:

```powershell
cargo test -p git-svn-rs --test clone_fetch_smoke init_writes_svn_remote_config
```

Expected: PASS.

## Task 4: Implement Fetch and Clone Import Path

**Files:**
- Create: `crates/git-svn-rs-core/src/commands/fetch.rs`
- Create: `crates/git-svn-rs-core/src/commands/clone.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-core/src/git.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`

- [ ] **Step 1: Extend Git backend for fast-import**

Modify `crates/git-svn-rs-core/src/git.rs` with:

```rust
pub fn fast_import(&self, stream: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("git")
        .current_dir(&self.work_tree)
        .arg("fast-import")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.as_mut().unwrap().write_all(stream).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
```

- [ ] **Step 2: Add command modules**

Modify `crates/git-svn-rs-core/src/commands/mod.rs`:

```rust
pub mod clone;
pub mod fetch;
pub mod init;
```

Create `crates/git-svn-rs-core/src/commands/fetch.rs`:

```rust
use crate::cli::FetchArgs;

pub fn run(_args: &FetchArgs, _cwd: &std::path::Path) -> Result<(), String> {
    Err("fetch requires real SVN log replay; use mock import tests until libsvn fetch is wired".to_string())
}
```

Create `crates/git-svn-rs-core/src/commands/clone.rs`:

```rust
use crate::cli::CloneArgs;
use crate::commands::init;

pub fn run(args: &CloneArgs, cwd: &std::path::Path) -> Result<String, String> {
    let target = args.path.clone().unwrap_or_else(|| {
        args.url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or("svn-checkout")
            .to_string()
    });
    let init_args = crate::cli::InitArgs {
        url: args.url.clone(),
        path: Some(target.clone()),
        layout: args.layout.clone(),
        shared: args.shared.clone(),
    };
    init::run(&init_args, cwd)?;
    Err(format!("clone initialized {target}; fetch is enabled after SVN log replay is wired"))
}
```

If `LayoutArgs` and `SharedFetchArgs` do not implement `Clone`, derive it in `cli.rs`:

```rust
#[derive(Debug, Clone, Args)]
pub struct LayoutArgs { ... }

#[derive(Debug, Clone, Args)]
pub struct SharedFetchArgs { ... }
```

- [ ] **Step 3: Add mock import acceptance test**

Append to `crates/git-svn-rs-core/tests/import_mock.rs`:

```rust
use git_svn_rs_core::fast_import::FastImportStream;

#[test]
fn planned_commit_serializes_to_fast_import_stream() {
    let event = RevisionEvent {
        revision: 3,
        author: "alice".to_string(),
        message: "add readme".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        changed_paths: vec![ChangedPath {
            path: "/trunk/README.md".to_string(),
            action: ChangeAction::Add,
            copy_from_path: None,
            copy_from_rev: None,
            kind: NodeKind::File,
            properties: BTreeMap::new(),
            content: Some(b"# project\n".to_vec()),
        }],
    };
    let target = ImportTarget {
        svn_path: "trunk".to_string(),
        refname: "refs/remotes/git-svn".to_string(),
        url: "file:///repo/trunk".to_string(),
        uuid: "uuid".to_string(),
    };

    let commits = ImportPlanner::new(target).plan_revision(&event, None).unwrap();
    let stream = String::from_utf8(FastImportStream::new().commit(&commits[0]).finish()).unwrap();

    assert!(stream.contains("commit refs/remotes/git-svn"));
    assert!(stream.contains("README.md"));
}
```

- [ ] **Step 4: Wire CLI clone/fetch branches**

Modify `crates/git-svn-rs-cli/src/main.rs`:

```rust
Command::Clone(args) => {
    git_svn_rs_core::commands::clone::run(&args, &std::env::current_dir()?)
        .map_err(anyhow::Error::msg)
}
Command::Fetch(args) => {
    git_svn_rs_core::commands::fetch::run(&args, &std::env::current_dir()?)
        .map_err(anyhow::Error::msg)
}
```

- [ ] **Step 5: Verify intermediate checkpoint**

Run:

```powershell
cargo test --workspace
```

Expected: PASS; `init` writes config, import planner and fast-import stream are validated. `fetch` and `clone` return explicit phase-bound errors until the libsvn replay task is completed. This is not the Phase 5 gate; the Phase 5 gate requires the later `SvnFetchEditor` task and fixture clone test.

- [ ] **Step 6: Commit**

Run:

```powershell
git add crates
git commit -m "feat: add import planner and init command"
```

## Task 5: Replace Fetch Shell with SVN Replay

**Files:**
- Modify: `crates/git-svn-rs-core/src/commands/fetch.rs`
- Modify: `crates/git-svn-rs-core/src/commands/clone.rs`
- Modify: `crates/git-svn-rs-core/src/svn/libsvn.rs`
- Create: `crates/git-svn-rs-cli/tests/support/mod.rs`
- Create: `crates/git-svn-rs-cli/tests/support/svn_fixture.rs`
- Test: `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`

- [ ] **Step 1: Add fetch acceptance test using fixture**

Create `crates/git-svn-rs-cli/tests/support/mod.rs`:

```rust
pub mod svn_fixture;
```

Create `crates/git-svn-rs-cli/tests/support/svn_fixture.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub fn has_svn_tools() -> bool {
    Command::new("svnadmin").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
        && Command::new("svn").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
        && Command::new("svnlook").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

pub struct StandardSvnFixture {
    _tmp: TempDir,
    repo: PathBuf,
    wc: PathBuf,
}

impl StandardSvnFixture {
    pub fn create() -> Result<Self, String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let repo = tmp.path().join("repo");
        let wc = tmp.path().join("wc");
        run(tmp.path(), "svnadmin", &["create", repo.to_str().unwrap()])?;
        let url = file_url(&repo);
        run(tmp.path(), "svn", &["checkout", &url, wc.to_str().unwrap()])?;
        run(&wc, "svn", &["mkdir", "trunk", "branches", "tags"])?;
        run(&wc, "svn", &["commit", "-m", "layout"])?;
        std::fs::create_dir_all(wc.join("trunk/src")).map_err(|e| e.to_string())?;
        std::fs::write(wc.join("trunk/src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").map_err(|e| e.to_string())?;
        run(&wc, "svn", &["add", "trunk/src"])?;
        run(&wc, "svn", &["commit", "-m", "add trunk file"])?;
        Ok(Self { _tmp: tmp, repo, wc })
    }

    pub fn url(&self) -> String {
        file_url(&self.repo)
    }
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program).current_dir(cwd).args(args).output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn file_url(path: &Path) -> String {
    let raw = path.canonicalize().unwrap().to_string_lossy().replace('\\', "/");
    format!("file:///{}", raw.trim_start_matches('/'))
}
```

Add this to the top of `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`:

```rust
mod support;

use support::svn_fixture::{has_svn_tools, StandardSvnFixture};
```

Append this test to `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`:

```rust
#[test]
fn clone_fetches_standard_layout_when_svn_tools_exist() {
    if !has_svn_tools() {
        eprintln!("skipping: svnadmin, svn, and svnlook are required");
        return;
    }

    let fixture = StandardSvnFixture::create().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let work = parent.path().join("work");

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(parent.path())
        .args(["clone", &fixture.url(), work.to_str().unwrap(), "--stdlayout"])
        .assert()
        .success();

    let trunk_ref = "refs/remotes/origin/trunk";
    let log = std::process::Command::new("git")
        .current_dir(&work)
        .args(["log", trunk_ref, "--format=%B"])
        .output()
        .unwrap();

    assert!(log.status.success());
    assert!(String::from_utf8_lossy(&log.stdout).contains("git-svn-id:"));
}
```

- [ ] **Step 2: Implement real fetch loop**

Implement `commands::fetch::run` so it:

```text
1. Reads svn-remote.svn.url and all svn-remote.svn.fetch/branches/tags mappings from git config.
2. Opens `RaSession` for the URL.
3. Reads UUID and latest SVN revision.
4. Reads max imported revision from `.git/svn/<ref>/.rev_map.<uuid>` for each tracked ref resolved from config.
5. Uses `RaSession::get_log` only to discover changed revisions and matching refs.
6. Calls `RaSession::do_update` or `RaSession::do_switch` for each matched revision.
7. Drives `SvnFetchEditor` with the SVN delta callbacks.
8. Writes the resulting Git tree/commit through the Git object writer.
9. Appends rev_map records for imported commits.
```

- [ ] **Step 3: Implement clone as init plus fetch**

Modify `commands::clone::run` so it calls `init::run`, then creates a `FetchArgs` value and calls `fetch::run`.

- [ ] **Step 4: Verify file fixture clone**

Run:

```powershell
cargo test -p git-svn-rs --test clone_fetch_smoke clone_fetches_standard_layout_when_svn_tools_exist -- --nocapture
```

Expected: PASS when SVN tools and libsvn are available; otherwise the test prints the missing dependency and returns success.

- [ ] **Step 5: Commit**

Run:

```powershell
git add crates
git commit -m "feat: import svn revisions with fetch and clone"
```

## Self Review

- Spec coverage: `init`, `clone`, `fetch`, `SvnFetchEditor`, Git object writing, `git-svn-id`, and rev_map updates are staged with tests.
- Placeholder scan: temporary command errors are replaced by SVN replay work inside this phase before the phase gate is accepted.
- Type consistency: `SvnFetchEditor` consumes SVN editor callbacks, produces Git tree/index changes, and may use `FastImportStream` only as a lower-level writer.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 5 acceptance gate. Do not treat them as optional optimization work.

### Task 6: Replace `ImportPlanner` Behavior Model With `SvnFetchEditor`

**Files:**
- Create: `crates/git-svn-rs-core/src/fetch_editor.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/commands/fetch.rs`
- Test: `crates/git-svn-rs-core/tests/fetch_editor.rs`
- Test: `crates/git-svn-rs-cli/tests/clone_fetch_smoke.rs`

- [ ] **Step 1: Write Fetcher.pm behavior tests**

Create `crates/git-svn-rs-core/tests/fetch_editor.rs`:

```rust
use git_svn_rs_core::fetch_editor::{FetchEditorConfig, SvnFetchEditor};

#[test]
fn rejects_dot_git_paths_before_filters() {
    let mut editor = SvnFetchEditor::new(FetchEditorConfig::default());
    assert!(editor.add_file("trunk/.git/config", None).unwrap().is_ignored());
}

#[test]
fn executable_property_sets_git_mode_100755() {
    let mut editor = SvnFetchEditor::new(FetchEditorConfig::default());
    editor.add_file("trunk/run.sh", None).unwrap();
    editor.change_file_prop("trunk/run.sh", "svn:executable", Some("*")).unwrap();
    editor.apply_textdelta("trunk/run.sh", b"#!/bin/sh\n").unwrap();
    let result = editor.close_edit().unwrap();

    assert_eq!(result.files["run.sh"].mode, "100755");
}

#[test]
fn special_property_creates_symlink_mode() {
    let mut editor = SvnFetchEditor::new(FetchEditorConfig::default());
    editor.add_file("trunk/link", None).unwrap();
    editor.change_file_prop("trunk/link", "svn:special", Some("*")).unwrap();
    editor.apply_textdelta("trunk/link", b"link target.txt").unwrap();
    let result = editor.close_edit().unwrap();

    assert_eq!(result.files["link"].mode, "120000");
    assert_eq!(result.files["link"].content, b"target.txt");
}

#[test]
fn preserve_empty_dirs_adds_placeholder_file() {
    let mut editor = SvnFetchEditor::new(FetchEditorConfig {
        preserve_empty_dirs: true,
        placeholder_filename: ".gitignore".to_string(),
        ..Default::default()
    });
    editor.add_directory("trunk/empty", None).unwrap();
    let result = editor.close_edit().unwrap();

    assert!(result.files.contains_key("empty/.gitignore"));
    assert!(result.added_placeholders.contains(&"trunk/empty/.gitignore".to_string()));
}
```

- [ ] **Step 2: Implement fetch editor data model**

Create `crates/git-svn-rs-core/src/fetch_editor.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct FetchEditorConfig {
    pub path_strip: String,
    pub preserve_empty_dirs: bool,
    pub placeholder_filename: String,
}

impl Default for FetchEditorConfig {
    fn default() -> Self {
        Self {
            path_strip: "trunk".to_string(),
            preserve_empty_dirs: false,
            placeholder_filename: ".gitignore".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFile {
    pub mode: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct FetchEditResult {
    pub files: BTreeMap<String, GitFile>,
    pub deleted: BTreeSet<String>,
    pub absent_files: BTreeMap<String, Vec<String>>,
    pub absent_dirs: BTreeMap<String, Vec<String>>,
    pub unhandled_properties: Vec<String>,
    pub added_placeholders: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditAction {
    ignored: bool,
}

impl EditAction {
    pub fn is_ignored(&self) -> bool { self.ignored }
}

pub struct SvnFetchEditor {
    config: FetchEditorConfig,
    files: BTreeMap<String, GitFile>,
    directories: BTreeSet<String>,
    props: BTreeMap<String, BTreeMap<String, String>>,
    content: BTreeMap<String, Vec<u8>>,
    result: FetchEditResult,
}

impl SvnFetchEditor {
    pub fn new(config: FetchEditorConfig) -> Self {
        Self {
            config,
            files: BTreeMap::new(),
            directories: BTreeSet::new(),
            props: BTreeMap::new(),
            content: BTreeMap::new(),
            result: FetchEditResult::default(),
        }
    }

    pub fn add_directory(&mut self, path: &str, _copy_from: Option<(&str, u32)>) -> Result<EditAction, String> {
        if self.is_ignored(path) {
            return Ok(EditAction { ignored: true });
        }
        self.directories.insert(path.to_string());
        Ok(EditAction { ignored: false })
    }

    pub fn add_file(&mut self, path: &str, _copy_from: Option<(&str, u32)>) -> Result<EditAction, String> {
        if self.is_ignored(path) {
            return Ok(EditAction { ignored: true });
        }
        self.content.entry(path.to_string()).or_default();
        Ok(EditAction { ignored: false })
    }

    pub fn delete_entry(&mut self, path: &str, _revision: u32) -> Result<(), String> {
        if self.is_ignored(path) {
            return Ok(());
        }
        self.result.deleted.insert(self.git_path(path));
        Ok(())
    }

    pub fn change_file_prop(&mut self, path: &str, name: &str, value: Option<&str>) -> Result<(), String> {
        if self.is_ignored(path) {
            return Ok(());
        }
        if matches!(name, "svn:executable" | "svn:special") {
            let props = self.props.entry(path.to_string()).or_default();
            if let Some(value) = value {
                props.insert(name.to_string(), value.to_string());
            } else {
                props.remove(name);
            }
        } else {
            self.result.unhandled_properties.push(format!("{path}:{name}"));
        }
        Ok(())
    }

    pub fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String> {
        if self.is_ignored(path) {
            return Ok(());
        }
        self.content.insert(path.to_string(), content.to_vec());
        Ok(())
    }

    pub fn close_edit(mut self) -> Result<FetchEditResult, String> {
        for (path, mut content) in self.content {
            let props = self.props.get(&path);
            let mode = if props.and_then(|p| p.get("svn:special")).is_some() {
                if content.starts_with(b"link ") {
                    content = content[5..].to_vec();
                }
                "120000"
            } else if props.and_then(|p| p.get("svn:executable")).is_some() {
                "100755"
            } else {
                "100644"
            };
            self.files.insert(self.git_path(&path), GitFile { mode: mode.to_string(), content });
        }
        if self.config.preserve_empty_dirs {
            for dir in &self.directories {
                let gdir = self.git_path(dir);
                let has_child = self.files.keys().any(|p| p.starts_with(&(gdir.clone() + "/")));
                if !gdir.is_empty() && !has_child {
                    let path = format!("{gdir}/{}", self.config.placeholder_filename);
                    self.files.insert(path.clone(), GitFile { mode: "100644".to_string(), content: Vec::new() });
                    self.result.added_placeholders.insert(format!("{dir}/{}", self.config.placeholder_filename));
                }
            }
        }
        self.result.files = self.files;
        Ok(self.result)
    }

    fn is_ignored(&self, path: &str) -> bool {
        path.split('/').any(|part| part == ".git")
    }

    fn git_path(&self, path: &str) -> String {
        path.strip_prefix(&self.config.path_strip)
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_string()
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod fetch_editor;
```

- [ ] **Step 3: Route fetch through `SvnFetchEditor`**

Modify `commands::fetch::run` so the real implementation receives SVN editor callbacks from `RaSession::do_update` or `RaSession::do_switch`, drives `SvnFetchEditor`, writes the resulting Git tree/commit, then updates `.rev_map`. `ImportPlanner` tests may remain as legacy smoke tests only, but the phase gate must use `SvnFetchEditor`.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test fetch_editor
cargo test -p git-svn-rs --test clone_fetch_smoke clone_fetches_standard_layout_when_svn_tools_exist -- --nocapture
```

Expected: PASS when SVN tooling is available; otherwise fixture test prints the dependency skip.

## References

- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Fetcher.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Fetcher.pm)
- [Ra.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Ra.pm)
- [Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)
