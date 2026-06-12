# SVN Fixtures and Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide repeatable local SVN fixtures and a Rust backend interface that can run with a mock backend by default and libsvn when the `svn-libsvn` feature is enabled.

**Architecture:** Tests use `svnadmin` and `svn` command-line tools for fixture creation because they are stable and easy to inspect. Runtime SVN access goes through RA/editor abstractions; the early `SvnBackend` mock is only a smoke-test adapter. `RaSession`, `do_update`, `do_switch`, auth prompts, and editor traits are required before fetch or dcommit can claim compatibility.

**Tech Stack:** Rust 1.95, Subversion CLI for tests, optional `subversion` crate behind `svn-libsvn`, `tempfile`.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/svn/mod.rs`
- Create: `crates/git-svn-rs-core/src/svn/types.rs`
- Create: `crates/git-svn-rs-core/src/svn/mock.rs`
- Create: `crates/git-svn-rs-core/src/svn/libsvn.rs`
- Create: `crates/git-svn-rs-core/src/svn/ra.rs`
- Create: `crates/git-svn-rs-core/src/svn/editor.rs`
- Create: `crates/git-svn-rs-core/src/svn/auth.rs`
- Create: `crates/git-svn-rs-core/tests/support/mod.rs`
- Create: `crates/git-svn-rs-core/tests/support/svn_fixture.rs`
- Test: `crates/git-svn-rs-core/tests/svn_fixture.rs`
- Test: `crates/git-svn-rs-core/tests/svn_backend_mock.rs`

## Task 1: Create SVN Domain Types and Backend Trait

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/svn/mod.rs`
- Create: `crates/git-svn-rs-core/src/svn/types.rs`
- Create: `crates/git-svn-rs-core/src/svn/mock.rs`

- [ ] **Step 1: Add optional libsvn feature**

Modify `crates/git-svn-rs-core/Cargo.toml`:

```toml
[features]
default = []
svn-libsvn = ["dep:subversion"]

[dependencies]
clap.workspace = true
fancy-regex.workspace = true
hex.workspace = true
subversion = { version = "0.1.10", optional = true }
thiserror.workspace = true
url.workspace = true
```

- [ ] **Step 2: Export `svn` module**

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
pub mod svn;
```

- [ ] **Step 3: Define backend trait and revision types**

Create `crates/git-svn-rs-core/src/svn/mod.rs`:

```rust
pub mod mock;
pub mod types;

#[cfg(feature = "svn-libsvn")]
pub mod libsvn;

pub use types::*;

pub trait SvnBackend {
    fn uuid(&self) -> Result<String, String>;
    fn latest_revnum(&self) -> Result<u32, String>;
    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String>;
}
```

Create `crates/git-svn-rs-core/src/svn/types.rs`:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionEvent {
    pub revision: u32,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub changed_paths: Vec<ChangedPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    pub path: String,
    pub action: ChangeAction,
    pub copy_from_path: Option<String>,
    pub copy_from_rev: Option<u32>,
    pub kind: NodeKind,
    pub properties: BTreeMap<String, String>,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeAction {
    Add,
    Modify,
    Delete,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
    Symlink,
}
```

- [ ] **Step 4: Add mock backend**

Create `crates/git-svn-rs-core/src/svn/mock.rs`:

```rust
use super::{RevisionEvent, SvnBackend};

#[derive(Debug, Clone)]
pub struct MockSvnBackend {
    uuid: String,
    revisions: Vec<RevisionEvent>,
}

impl MockSvnBackend {
    pub fn new(uuid: impl Into<String>, revisions: Vec<RevisionEvent>) -> Self {
        Self {
            uuid: uuid.into(),
            revisions,
        }
    }
}

impl SvnBackend for MockSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Ok(self.uuid.clone())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Ok(self.revisions.last().map(|r| r.revision).unwrap_or(0))
    }

    fn log(&self, start: u32, end: u32) -> Result<Vec<RevisionEvent>, String> {
        Ok(self
            .revisions
            .iter()
            .filter(|r| r.revision >= start && r.revision <= end)
            .cloned()
            .collect())
    }
}
```

- [ ] **Step 5: Verify compile**

Run:

```powershell
cargo test -p git-svn-rs-core
```

Expected: PASS.

## Task 2: Test Mock Backend

**Files:**
- Test: `crates/git-svn-rs-core/tests/svn_backend_mock.rs`

- [ ] **Step 1: Write mock backend tests**

Create `crates/git-svn-rs-core/tests/svn_backend_mock.rs`:

```rust
use git_svn_rs_core::svn::mock::MockSvnBackend;
use git_svn_rs_core::svn::{RevisionEvent, SvnBackend};

#[test]
fn mock_backend_filters_revision_window() {
    let backend = MockSvnBackend::new(
        "uuid",
        vec![
            RevisionEvent {
                revision: 1,
                author: "alice".to_string(),
                message: "one".to_string(),
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
            RevisionEvent {
                revision: 2,
                author: "bob".to_string(),
                message: "two".to_string(),
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                changed_paths: vec![],
            },
        ],
    );

    assert_eq!(backend.uuid().unwrap(), "uuid");
    assert_eq!(backend.latest_revnum().unwrap(), 2);
    assert_eq!(backend.log(2, 2).unwrap()[0].author, "bob");
}
```

- [ ] **Step 2: Verify mock tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test svn_backend_mock
```

Expected: PASS.

## Task 3: Add Local SVN Fixture Builder

**Files:**
- Create: `crates/git-svn-rs-core/tests/support/mod.rs`
- Create: `crates/git-svn-rs-core/tests/support/svn_fixture.rs`
- Test: `crates/git-svn-rs-core/tests/svn_fixture.rs`

- [ ] **Step 1: Write fixture tests**

Create `crates/git-svn-rs-core/tests/svn_fixture.rs`:

```rust
mod support;

use support::svn_fixture::{has_svn_tools, StandardSvnFixture};

#[test]
fn standard_fixture_creates_trunk_branch_and_tag_revisions() {
    if !has_svn_tools() {
        eprintln!("skipping: svnadmin and svn are required");
        return;
    }

    let fixture = StandardSvnFixture::create().unwrap();

    assert!(fixture.url().starts_with("file:///"));
    assert!(fixture.latest_revision() >= 4);
}
```

- [ ] **Step 2: Create support module**

Create `crates/git-svn-rs-core/tests/support/mod.rs`:

```rust
pub mod svn_fixture;
```

- [ ] **Step 3: Implement SVN fixture helper**

Create `crates/git-svn-rs-core/tests/support/svn_fixture.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub fn has_svn_tools() -> bool {
    Command::new("svnadmin").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
        && Command::new("svn").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
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
        run(&wc, "svn", &["copy", "trunk", "branches/main"])?;
        run(&wc, "svn", &["commit", "-m", "branch main"])?;
        run(&wc, "svn", &["copy", "trunk", "tags/v1"])?;
        run(&wc, "svn", &["commit", "-m", "tag v1"])?;
        Ok(Self { _tmp: tmp, repo, wc })
    }

    pub fn url(&self) -> String {
        file_url(&self.repo)
    }

    pub fn latest_revision(&self) -> u32 {
        let output = Command::new("svnlook")
            .arg("youngest")
            .arg(&self.repo)
            .output()
            .expect("svnlook youngest should run");
        String::from_utf8_lossy(&output.stdout).trim().parse().unwrap()
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

- [ ] **Step 4: Run fixture test**

Run:

```powershell
cargo test -p git-svn-rs-core --test svn_fixture -- --nocapture
```

Expected: PASS when SVN CLI tools are installed; otherwise test prints `skipping: svnadmin and svn are required` and passes.

## Task 4: Add Feature-Gated libsvn Backend Shell

**Files:**
- Create: `crates/git-svn-rs-core/src/svn/libsvn.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`

- [ ] **Step 1: Add libsvn backend type**

Create `crates/git-svn-rs-core/src/svn/libsvn.rs`:

```rust
use super::{RevisionEvent, SvnBackend};

pub struct LibSvnBackend {
    url: String,
}

impl LibSvnBackend {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl SvnBackend for LibSvnBackend {
    fn uuid(&self) -> Result<String, String> {
        Err("libsvn uuid lookup is wired in the fetch phase".to_string())
    }

    fn latest_revnum(&self) -> Result<u32, String> {
        Err("libsvn latest revision lookup is wired in the fetch phase".to_string())
    }

    fn log(&self, _start: u32, _end: u32) -> Result<Vec<RevisionEvent>, String> {
        Err("libsvn log replay is wired in the fetch phase".to_string())
    }
}
```

- [ ] **Step 2: Update diagnostics output**

Modify `crates/git-svn-rs-cli/src/main.rs` diagnose branch:

```rust
Command::Diagnose(_) => {
    println!("git-svn-rs diagnostics");
    println!("libsvn feature: {}", if cfg!(feature = "svn-libsvn") { "enabled" } else { "disabled" });
    Ok(())
}
```

- [ ] **Step 3: Verify default build stays independent of libsvn**

Run:

```powershell
cargo test --workspace
cargo run -p git-svn-rs -- diagnose
```

Expected: tests pass and diagnostics print `libsvn feature: disabled`.

- [ ] **Step 4: Verify feature build on machines with libsvn**

Run:

```powershell
cargo test -p git-svn-rs-core --features svn-libsvn
```

Expected: PASS when libsvn development libraries are installed; if linking fails, capture the linker error in `README.md` under a Windows dependency section before continuing with `svn-libsvn` work.

- [ ] **Step 5: Commit**

Run:

```powershell
git add Cargo.toml crates/git-svn-rs-core crates/git-svn-rs-cli
git commit -m "feat: add svn backend trait and fixtures"
```

## Self Review

- Spec coverage: SVN backend abstraction, mock replay, fixture generation, and optional libsvn are in place.
- Placeholder scan: libsvn shell returns explicit phase-bound errors and diagnostics expose feature state.
- Type consistency: `SvnBackend`, `RevisionEvent`, and `ChangedPath` are the input model for import work.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 4 acceptance gate. Do not treat them as optional optimization work.

### Task 5: Split `SvnBackend` Into RA Session and Editor Interfaces

**Files:**
- Create: `crates/git-svn-rs-core/src/svn/ra.rs`
- Create: `crates/git-svn-rs-core/src/svn/editor.rs`
- Modify: `crates/git-svn-rs-core/src/svn/mod.rs`
- Modify: `crates/git-svn-rs-core/src/svn/mock.rs`
- Test: `crates/git-svn-rs-core/tests/ra_session_mock.rs`

- [ ] **Step 1: Add RA session tests**

Create `crates/git-svn-rs-core/tests/ra_session_mock.rs`:

```rust
use git_svn_rs_core::svn::mock::MockRaSession;
use git_svn_rs_core::svn::ra::{RaSession, SvnNodeKind};

#[test]
fn mock_ra_session_exposes_check_path_get_dir_and_log() {
    let session = MockRaSession::standard_fixture("uuid");
    assert_eq!(session.uuid().unwrap(), "uuid");
    assert_eq!(session.check_path("trunk/src/lib.rs", 2).unwrap(), Some(SvnNodeKind::File));
    assert!(session.get_dir("trunk", 2).unwrap().entries.contains_key("src"));
    assert_eq!(session.get_log(&["trunk"], 1, 2).unwrap().len(), 2);
}
```

- [ ] **Step 2: Define RA and editor traits**

Create `crates/git-svn-rs-core/src/svn/ra.rs`:

```rust
use std::collections::BTreeMap;
use crate::svn::RevisionEvent;
use crate::svn::editor::FetchEditor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvnNodeKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub kind: SvnNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirListing {
    pub entries: BTreeMap<String, DirEntry>,
    pub properties: BTreeMap<String, String>,
}

pub trait RaSession {
    fn url(&self) -> &str;
    fn repos_root(&self) -> &str;
    fn uuid(&self) -> Result<String, String>;
    fn latest_revnum(&self) -> Result<u32, String>;
    fn check_path(&self, path: &str, revision: u32) -> Result<Option<SvnNodeKind>, String>;
    fn get_dir(&self, path: &str, revision: u32) -> Result<DirListing, String>;
    fn get_log(&self, paths: &[&str], start: u32, end: u32) -> Result<Vec<RevisionEvent>, String>;
    fn do_update(&self, path: &str, revision: u32, editor: &mut dyn FetchEditor) -> Result<(), String>;
    fn do_switch(&self, path: &str, revision: u32, switch_url: &str, editor: &mut dyn FetchEditor) -> Result<(), String>;
}
```

Create `crates/git-svn-rs-core/src/svn/editor.rs`:

```rust
pub trait FetchEditor {
    fn open_root(&mut self, revision: u32) -> Result<(), String>;
    fn add_directory(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String>;
    fn add_file(&mut self, path: &str, copy_from: Option<(&str, u32)>) -> Result<(), String>;
    fn delete_entry(&mut self, path: &str, revision: u32) -> Result<(), String>;
    fn change_file_prop(&mut self, path: &str, name: &str, value: Option<&str>) -> Result<(), String>;
    fn apply_textdelta(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn close_edit(&mut self) -> Result<(), String>;
}

pub trait CommitEditor {
    fn ensure_path(&mut self, path: &str) -> Result<(), String>;
    fn add_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn open_file(&mut self, path: &str, content: &[u8]) -> Result<(), String>;
    fn delete_entry(&mut self, path: &str) -> Result<(), String>;
    fn change_file_prop(&mut self, path: &str, name: &str, value: Option<&str>) -> Result<(), String>;
    fn close_edit(&mut self) -> Result<u32, String>;
    fn abort_edit(&mut self) -> Result<(), String>;
}
```

Modify `crates/git-svn-rs-core/src/svn/mod.rs`:

```rust
pub mod editor;
pub mod ra;
```

- [ ] **Step 3: Provide mock RA session**

Modify `crates/git-svn-rs-core/src/svn/mock.rs` to add `MockRaSession` with `standard_fixture("uuid")`, implementing `RaSession` and returning deterministic directory/log data. Its `do_update` and `do_switch` implementations must drive a provided `FetchEditor` with repeatable add/modify/delete callbacks so Phase 5 can test `SvnFetchEditor` without libsvn.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test ra_session_mock
```

Expected: PASS.

### Task 6: Add Auth Prompt Mock

**Files:**
- Create: `crates/git-svn-rs-core/src/svn/auth.rs`
- Modify: `crates/git-svn-rs-core/src/svn/mod.rs`
- Test: `crates/git-svn-rs-core/tests/auth_prompt.rs`

- [ ] **Step 1: Add prompt tests**

Create `crates/git-svn-rs-core/tests/auth_prompt.rs`:

`Git::SVN::Prompt` defines SVN auth prompts, while `Git.pm::prompt` defines askpass and terminal fallback behavior. The mock should make these branches testable without interactive input.

```rust
use git_svn_rs_core::svn::auth::{AuthPrompt, AuthRequest, MockAuthPrompt};

#[test]
fn username_option_overrides_default_username() {
    let prompt = MockAuthPrompt::new().with_username("alice").with_password("secret");
    let creds = prompt.simple(AuthRequest {
        realm: Some("repo".to_string()),
        default_username: Some("bob".to_string()),
        may_save: true,
        no_auth_cache: true,
    }).unwrap();

    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "secret");
    assert!(!creds.may_save);
}

#[test]
fn askpass_fallback_can_be_mocked_without_terminal_input() {
    let prompt = MockAuthPrompt::new().with_askpass_answer("askpass-secret");
    let creds = prompt.simple(AuthRequest {
        realm: Some("repo".to_string()),
        default_username: Some("alice".to_string()),
        may_save: true,
        no_auth_cache: false,
    }).unwrap();

    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "askpass-secret");
}
```

- [ ] **Step 2: Implement auth prompt abstraction**

Create `crates/git-svn-rs-core/src/svn/auth.rs`:

```rust
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub realm: Option<String>,
    pub default_username: Option<String>,
    pub may_save: bool,
    pub no_auth_cache: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub may_save: bool,
}

pub trait AuthPrompt {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String>;
}

#[derive(Debug, Default, Clone)]
pub struct MockAuthPrompt {
    username: Option<String>,
    password: Option<String>,
    askpass_answer: Option<String>,
}

impl MockAuthPrompt {
    pub fn new() -> Self { Self::default() }
    pub fn with_username(mut self, username: &str) -> Self { self.username = Some(username.to_string()); self }
    pub fn with_password(mut self, password: &str) -> Self { self.password = Some(password.to_string()); self }
    pub fn with_askpass_answer(mut self, answer: &str) -> Self { self.askpass_answer = Some(answer.to_string()); self }
}

impl AuthPrompt for MockAuthPrompt {
    fn simple(&self, request: AuthRequest) -> Result<Credentials, String> {
        Ok(Credentials {
            username: self.username.clone().or(request.default_username).unwrap_or_default(),
            password: self.password.clone().or(self.askpass_answer.clone()).unwrap_or_default(),
            may_save: request.may_save && !request.no_auth_cache,
        })
    }
}
```

Modify `crates/git-svn-rs-core/src/svn/mod.rs`:

```rust
pub mod auth;
```

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test auth_prompt
```

Expected: PASS.

## References

- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Ra.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Ra.pm)
- [Prompt.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Prompt.pm)
- [subversion crate](https://crates.io/crates/subversion)
- [subversion docs](https://docs.rs/subversion)
- [subversion-sys crate](https://crates.io/crates/subversion-sys)
- [subversion-sys docs](https://docs.rs/subversion-sys)
