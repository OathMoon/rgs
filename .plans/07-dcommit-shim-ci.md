# Dcommit Shim CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement linear `dcommit`, optional `git-svn` shim packaging, compatibility fixtures, and Windows-first verification.

**Architecture:** `dcommit` takes local commits after the configured SVN-tracking ref, plans each Git diff with `GitDiffPlanner`, writes it through an `SvnCommitEditor` modeled after `Git::SVN::Editor`, fetches the resulting SVN revisions, and rebases unless `--no-rebase` is set. The coarse mock commit request is never a production path. The shim is a separate opt-in executable or script so default installs keep the `git-svn-rs` name.

**Tech Stack:** Rust 1.95, Git CLI `rev-list`/`diff-tree`/`show`, SVN backend commit trait, `assert_cmd`, GitHub Actions or local PowerShell CI scripts.

Production command modules must use the shared `GitCli` wrapper for Git operations so repository context, command output capture, and error propagation stay aligned with `perl/Git.pm`. Direct `std::process::Command::new("git")` is acceptable in integration test setup only.

---

## File Structure

- Modify: `crates/git-svn-rs-core/src/svn/mod.rs`
- Modify: `crates/git-svn-rs-core/src/svn/types.rs`
- Create: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Create: `crates/git-svn-rs-core/src/dcommit/diff_planner.rs`
- Create: `crates/git-svn-rs-core/src/dcommit/commit_editor.rs`
- Create: `crates/git-svn-rs-core/src/dcommit/property_mapper.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Create: `crates/git-svn-rs-shim/Cargo.toml`
- Create: `crates/git-svn-rs-shim/src/main.rs`
- Modify: `Cargo.toml`
- Create: `scripts/verify.ps1`
- Create: `.github/workflows/windows.yml`
- Test: `crates/git-svn-rs-cli/tests/dcommit_linear.rs`
- Test: `crates/git-svn-rs-cli/tests/shim.rs`

## Task 1: Add Mock Commit Backend Adapter

**Files:**
- Modify: `crates/git-svn-rs-core/src/svn/mod.rs`
- Modify: `crates/git-svn-rs-core/src/svn/types.rs`
- Test: `crates/git-svn-rs-core/tests/svn_backend_mock.rs`

- [ ] **Step 1: Add mock adapter commit types**

These types support early mock tests only. They must not become the production `dcommit` write-back path; Task 6 and Task 7 replace production write-back with `GitDiffPlanner` and `SvnCommitEditor`.

Modify `crates/git-svn-rs-core/src/svn/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnCommitRequest {
    pub base_revision: u32,
    pub message: String,
    pub changes: Vec<SvnCommitChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvnCommitChange {
    PutFile { path: String, content: Vec<u8>, executable: bool },
    DeletePath { path: String },
    CopyPath { from_path: String, from_revision: u32, to_path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnCommitResult {
    pub revision: u32,
}
```

- [ ] **Step 2: Extend mock backend trait**

Modify `crates/git-svn-rs-core/src/svn/mod.rs`:

```rust
pub trait SvnCommitBackend: SvnBackend {
    fn commit(&self, request: SvnCommitRequest) -> Result<SvnCommitResult, String>;
}
```

- [ ] **Step 3: Add mock commit test**

Append to `crates/git-svn-rs-core/tests/svn_backend_mock.rs`:

```rust
use git_svn_rs_core::svn::{SvnCommitBackend, SvnCommitChange, SvnCommitRequest};

#[test]
fn mock_commit_backend_returns_next_revision() {
    let backend = MockSvnBackend::new("uuid", vec![]);
    let result = backend.commit(SvnCommitRequest {
        base_revision: 0,
        message: "add file".to_string(),
        changes: vec![SvnCommitChange::PutFile {
            path: "trunk/file.txt".to_string(),
            content: b"hello\n".to_vec(),
            executable: false,
        }],
    }).unwrap();

    assert_eq!(result.revision, 1);
}
```

- [ ] **Step 4: Implement mock commit**

Modify `crates/git-svn-rs-core/src/svn/mock.rs` to implement `SvnCommitBackend` and return `latest_revnum() + 1` for valid non-empty change lists.

- [ ] **Step 5: Verify tests**

Run:

```powershell
cargo test -p git-svn-rs-core --test svn_backend_mock
```

Expected: PASS.

## Task 2: Plan Linear Git Commits for `dcommit`

**Files:**
- Create: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/dcommit_linear.rs`

- [ ] **Step 1: Write dry-run test**

Create `crates/git-svn-rs-cli/tests/dcommit_linear.rs`:

```rust
use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn dcommit_dry_run_lists_local_commits() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["config", "user.name", "Test User"]).status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["config", "user.email", "test@example.com"]).status().unwrap();
    std::fs::write(dir.path().join("README.md"), "base\n").unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["add", "README.md"]).status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["commit", "-m", "base"]).status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["update-ref", "refs/remotes/git-svn", "HEAD"]).status().unwrap();
    std::fs::write(dir.path().join("README.md"), "base\nlocal\n").unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["commit", "-am", "local change"]).status().unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .args(["dcommit", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("local change"));
}
```

- [ ] **Step 2: Implement dry-run planner**

Create `crates/git-svn-rs-core/src/commands/dcommit.rs`:

```rust
use crate::cli::DcommitArgs;

pub fn run(args: &DcommitArgs, cwd: &std::path::Path) -> Result<String, String> {
    let tracked_ref = resolve_tracked_ref(cwd)?;
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["rev-list", "--reverse", "--format=%s", &format!("{tracked_ref}..HEAD")])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let list = String::from_utf8_lossy(&output.stdout).to_string();
    if args.dry_run {
        return Ok(list);
    }
    Err("dcommit write-back requires SVN commit backend wiring in this phase".to_string())
}

fn resolve_tracked_ref(cwd: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["for-each-ref", "refs/remotes", "--format=%(refname)"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| !line.ends_with("/HEAD"))
        .map(|line| line.to_string())
        .ok_or_else(|| "no SVN-tracking ref found under refs/remotes".to_string())
}
```

Modify `commands/mod.rs`:

```rust
pub mod dcommit;
```

Wire `Command::Dcommit` in CLI main.

- [ ] **Step 3: Verify dry-run**

Run:

```powershell
cargo test -p git-svn-rs --test dcommit_linear dcommit_dry_run_lists_local_commits
```

Expected: PASS.

## Task 3: Add Linear Write-Back Acceptance Test

**Files:**
- Modify: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Modify: `crates/git-svn-rs-core/src/svn/libsvn.rs`
- Modify: `crates/git-svn-rs-cli/tests/support/svn_fixture.rs`
- Test: `crates/git-svn-rs-cli/tests/dcommit_linear.rs`

- [ ] **Step 1: Add end-to-end fixture test**

Append to `crates/git-svn-rs-cli/tests/dcommit_linear.rs`:

```rust
mod support;

use support::svn_fixture::{has_svn_tools, StandardSvnFixture};

#[test]
fn dcommit_writes_linear_commit_when_svn_tools_exist() {
    if !has_svn_tools() {
        eprintln!("skipping: svnadmin, svn, and svnlook are required");
        return;
    }

    let fixture = StandardSvnFixture::create().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let work = parent.path().join("work");

    Command::cargo_bin("git-svn-rs").unwrap()
        .current_dir(parent.path())
        .args(["clone", &fixture.url(), work.to_str().unwrap(), "--stdlayout"])
        .assert()
        .success();

    std::process::Command::new("git").current_dir(&work).args(["config", "user.name", "Test User"]).status().unwrap();
    std::process::Command::new("git").current_dir(&work).args(["config", "user.email", "test@example.com"]).status().unwrap();
    std::fs::write(work.join("src/lib.rs"), "pub fn answer() -> u8 { 43 }\n").unwrap();
    std::process::Command::new("git").current_dir(&work).args(["commit", "-am", "change answer"]).status().unwrap();

    Command::cargo_bin("git-svn-rs").unwrap()
        .current_dir(&work)
        .arg("dcommit")
        .assert()
        .success();

    let log = std::process::Command::new("svn")
        .args(["log", "-l", "1", &fixture.url()])
        .output()
        .unwrap();

    assert!(log.status.success());
    assert!(String::from_utf8_lossy(&log.stdout).contains("change answer"));
}
```

- [ ] **Step 2: Record production write-back algorithm**

Do not implement production write-back through the coarse `SvnCommitRequest` path. The real non-dry-run implementation is completed after Task 6 and Task 7 define `GitDiffPlanner`, `SvnCommitEditor`, `PathEnsurer`, and `PropertyMapper`. Record the command-level helpers that Task 6/7 will call:

```text
local_commits(cwd) -> Vec<String>
commit_subject(cwd, commit) -> String
commit_message(cwd, commit) -> String
diff_tree_z(cwd, parent, commit) -> Vec<DiffOp>
```

The implementation must use:

```powershell
git rev-list --reverse <configured-svn-tracking-ref>..HEAD
git show -s --format=%B <commit>
git diff-tree -z -r -C <parent> <commit>
git show <commit>:<path>
```

- [ ] **Step 3: Leave non-dry-run guarded until editor tasks land**

Until Task 6 and Task 7 are complete, non-dry-run `dcommit` must return:

```text
dcommit write-back requires GitDiffPlanner and SvnCommitEditor; complete Phase 7 required core tasks first
```

After Task 6 and Task 7 are complete, for each local commit:

```text
1. Read current max SVN revision from rev_map.
2. Parse `git diff-tree -z -r -C` with `GitDiffPlanner`.
3. Preload SVN path types using `RaSession::check_path`.
4. Build `SvnCommitEditor` operations in `D/C/R/A/M/T` order.
5. Use `PathEnsurer` for missing parent directories.
6. Use `PropertyMapper` for `svn:executable`, `svn:special`, autoprops, and manual props.
7. Close the libsvn commit editor and capture the returned SVN revision.
8. Run `fetch` for the returned revision.
9. Continue with the next local commit.
```

- [ ] **Step 4: Rebase unless disabled**

After all commits are written:

```text
if --no-rebase is false:
    run `git rebase <configured-svn-tracking-ref>`
else:
    print `dcommit complete; rebase skipped by --no-rebase`
```

- [ ] **Step 5: Verify acceptance test registration**

Run:

```powershell
cargo test -p git-svn-rs --test dcommit_linear -- --nocapture
```

Expected: dry-run test always passes. The write-back fixture test exists and either skips for missing dependencies in developer mode or fails until Task 6 and Task 7 wire the editor-driven production path.

- [ ] **Step 6: Commit**

Run:

```powershell
git add crates
git commit -m "feat: add linear dcommit"
```

## Task 4: Add Optional `git-svn` Shim

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/git-svn-rs-shim/Cargo.toml`
- Create: `crates/git-svn-rs-shim/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/shim.rs`

- [ ] **Step 1: Add shim crate to workspace**

Modify root `Cargo.toml` members:

```toml
members = [
    "crates/git-svn-rs-cli",
    "crates/git-svn-rs-core",
    "crates/git-svn-rs-shim",
]
```

Create `crates/git-svn-rs-shim/Cargo.toml`:

```toml
[package]
name = "git-svn"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "git-svn"
path = "src/main.rs"
```

- [ ] **Step 2: Implement shim process forwarding**

Create `crates/git-svn-rs-shim/src/main.rs`:

```rust
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--shim-self-test") {
        println!("git-svn shim ok");
        return ExitCode::SUCCESS;
    }

    let status = Command::new("git-svn-rs")
        .args(std::env::args().skip(1))
        .status();
    match status {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(err) => {
            eprintln!("failed to run git-svn-rs: {err}");
            ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 3: Add shim smoke test**

Create `crates/git-svn-rs-cli/tests/shim.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn shim_binary_exists_after_build() {
    let mut cmd = Command::cargo_bin("git-svn").unwrap();
    cmd.arg("--shim-self-test")
        .assert()
        .success()
        .stdout(predicates::str::contains("git-svn shim ok"));
}
```

The self-test proves the shim binary builds without depending on whether `git-svn-rs` is already on `PATH`. Packaging tests cover installed forwarding.

- [ ] **Step 4: Verify shim build**

Run:

```powershell
cargo build -p git-svn
```

Expected: `target/debug/git-svn.exe` exists on Windows.

- [ ] **Step 5: Commit**

Run:

```powershell
git add Cargo.toml crates/git-svn-rs-shim crates/git-svn-rs-cli/tests/shim.rs
git commit -m "feat: add optional git-svn shim"
```

## Task 5: Add Windows Verification Script and CI

**Files:**
- Create: `scripts/verify.ps1`
- Create: `.github/workflows/windows.yml`
- Modify: `README.md`

- [ ] **Step 1: Create verification script**

Create `scripts/verify.ps1`:

```powershell
$ErrorActionPreference = "Stop"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

- [ ] **Step 2: Create Windows workflow**

Create `.github/workflows/windows.yml`:

```yaml
name: windows

on:
  push:
  pull_request:

jobs:
  test:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Verify
        shell: pwsh
        run: ./scripts/verify.ps1
```

- [ ] **Step 3: Document dependency states**

Append to `README.md`:

```markdown
## Verification

Run on Windows:

```powershell
./scripts/verify.ps1
```

The default build does not require libsvn. Enable the real SVN backend with:

```powershell
cargo test -p git-svn-rs-core --features svn-libsvn
```
```

- [ ] **Step 4: Verify locally**

Run:

```powershell
./scripts/verify.ps1
```

Expected: formatting, clippy, tests, and build complete successfully.

- [ ] **Step 5: Commit**

Run:

```powershell
git add scripts .github README.md
git commit -m "ci: add windows verification"
```

## Self Review

- Spec coverage: linear `dcommit`, editor-driven write-back, optional shim, and Windows verification are covered.
- Placeholder scan: fixture-dependent tests include concrete fixture setup and dependency skips.
- Type consistency: `SvnCommitBackend`, `SvnCommitRequest`, `SvnCommitChange`, and `SvnCommitResult` are mock adapter types; production `dcommit` uses `GitDiffPlanner`, `SvnCommitEditor`, `PathEnsurer`, and `PropertyMapper`.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 7 acceptance gate. Do not treat them as optional optimization work.

### Task 6: Replace Coarse `SvnCommitRequest` Path With `GitDiffPlanner`

**Files:**
- Create: `crates/git-svn-rs-core/src/dcommit/diff_planner.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Test: `crates/git-svn-rs-core/tests/dcommit_diff_planner.rs`

- [ ] **Step 1: Write `Editor.pm` diff parser tests**

Create `crates/git-svn-rs-core/tests/dcommit_diff_planner.rs`:

```rust
use git_svn_rs_core::dcommit::diff_planner::{DiffOp, GitDiffPlanner};

#[test]
fn parses_zero_delimited_add_modify_delete_copy_rename_typechange() {
    let raw = concat!(
        ":000000 100644 0000000 1111111 A\0new.txt\0",
        ":100644 100644 1111111 2222222 M\0mod.txt\0",
        ":100644 000000 3333333 0000000 D\0old.txt\0",
        ":100644 100644 4444444 5555555 C100\0copy-src.txt\0copy-dst.txt\0",
        ":100644 100644 6666666 7777777 R100\0old-name.txt\0new-name.txt\0",
        ":100644 120000 8888888 9999999 T\0link.txt\0"
    );
    let ops = GitDiffPlanner::parse_diff_tree_z(raw.as_bytes()).unwrap();

    assert!(matches!(ops[0], DiffOp::Add { ref path, .. } if path == "new.txt"));
    assert!(matches!(ops[1], DiffOp::Modify { ref path, .. } if path == "mod.txt"));
    assert!(matches!(ops[2], DiffOp::Delete { ref path, .. } if path == "old.txt"));
    assert!(matches!(ops[3], DiffOp::Copy { ref from_path, ref to_path, .. } if from_path == "copy-src.txt" && to_path == "copy-dst.txt"));
    assert!(matches!(ops[4], DiffOp::Rename { ref from_path, ref to_path, .. } if from_path == "old-name.txt" && to_path == "new-name.txt"));
    assert!(matches!(ops[5], DiffOp::TypeChange { ref path, .. } if path == "link.txt"));
}
```

- [ ] **Step 2: Implement diff planner**

Create `crates/git-svn-rs-core/src/dcommit/diff_planner.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOp {
    Add { path: String, mode: String, oid: String },
    Modify { path: String, old_mode: String, new_mode: String, old_oid: String, new_oid: String },
    Delete { path: String, old_mode: String, old_oid: String },
    Copy { from_path: String, to_path: String, mode: String, from_oid: String, to_oid: String },
    Rename { from_path: String, to_path: String, old_mode: String, new_mode: String, old_oid: String, new_oid: String },
    TypeChange { path: String, old_mode: String, new_mode: String, old_oid: String, new_oid: String },
}

pub struct GitDiffPlanner;

impl GitDiffPlanner {
    pub fn parse_diff_tree_z(raw: &[u8]) -> Result<Vec<DiffOp>, String> {
        let fields = raw.split(|b| *b == 0).filter(|f| !f.is_empty()).map(|f| {
            String::from_utf8(f.to_vec()).map_err(|e| e.to_string())
        }).collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        let mut i = 0;
        while i < fields.len() {
            let meta = &fields[i];
            i += 1;
            let parts = meta.split_whitespace().collect::<Vec<_>>();
            if parts.len() != 5 || !parts[0].starts_with(':') {
                return Err(format!("invalid diff-tree metadata: {meta}"));
            }
            let old_mode = parts[0].trim_start_matches(':').to_string();
            let new_mode = parts[1].to_string();
            let old_oid = parts[2].to_string();
            let new_oid = parts[3].to_string();
            let status = parts[4];
            match status.chars().next().unwrap() {
                'A' => {
                    let path = fields.get(i).ok_or("missing add path")?.clone();
                    i += 1;
                    out.push(DiffOp::Add { path, mode: new_mode, oid: new_oid });
                }
                'M' => {
                    let path = fields.get(i).ok_or("missing modify path")?.clone();
                    i += 1;
                    out.push(DiffOp::Modify { path, old_mode, new_mode, old_oid, new_oid });
                }
                'D' => {
                    let path = fields.get(i).ok_or("missing delete path")?.clone();
                    i += 1;
                    out.push(DiffOp::Delete { path, old_mode, old_oid });
                }
                'C' => {
                    let from_path = fields.get(i).ok_or("missing copy source")?.clone();
                    let to_path = fields.get(i + 1).ok_or("missing copy destination")?.clone();
                    i += 2;
                    out.push(DiffOp::Copy { from_path, to_path, mode: new_mode, from_oid: old_oid, to_oid: new_oid });
                }
                'R' => {
                    let from_path = fields.get(i).ok_or("missing rename source")?.clone();
                    let to_path = fields.get(i + 1).ok_or("missing rename destination")?.clone();
                    i += 2;
                    out.push(DiffOp::Rename { from_path, to_path, old_mode, new_mode, old_oid, new_oid });
                }
                'T' => {
                    let path = fields.get(i).ok_or("missing typechange path")?.clone();
                    i += 1;
                    out.push(DiffOp::TypeChange { path, old_mode, new_mode, old_oid, new_oid });
                }
                other => return Err(format!("unsupported diff status: {other}")),
            }
        }
        Ok(out)
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod dcommit {
    pub mod diff_planner;
}
```

- [ ] **Step 3: Replace dry-run source**

Modify `commands::dcommit::run` so `--dry-run` resolves the configured SVN-tracking ref, then uses `git rev-list --reverse <tracked-ref>..HEAD` plus `GitDiffPlanner` summaries, not only `--format=%s`.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test dcommit_diff_planner
cargo test -p git-svn-rs --test dcommit_linear dcommit_dry_run_lists_local_commits
```

Expected: PASS.

### Task 7: Add `SvnCommitEditor`, `PathEnsurer`, and `PropertyMapper`

**Files:**
- Create: `crates/git-svn-rs-core/src/dcommit/commit_editor.rs`
- Create: `crates/git-svn-rs-core/src/dcommit/property_mapper.rs`
- Modify: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Test: `crates/git-svn-rs-core/tests/dcommit_commit_editor.rs`

- [ ] **Step 1: Write commit editor ordering and property tests**

Create `crates/git-svn-rs-core/tests/dcommit_commit_editor.rs`:

```rust
use git_svn_rs_core::dcommit::commit_editor::{CommitEditAction, PathEnsurer, SvnCommitEditor};
use git_svn_rs_core::dcommit::property_mapper::PropertyMapper;

#[test]
fn applies_operations_in_editor_pm_order() {
    let mut editor = SvnCommitEditor::new();
    editor.queue_delete("old.txt");
    editor.queue_copy("copy-src.txt", "copy-dst.txt");
    editor.queue_rename("old-name.txt", "new-name.txt");
    editor.queue_add("new.txt", b"new\n");
    editor.queue_modify("mod.txt", b"mod\n");
    editor.queue_type_change("link.txt", b"target.txt");

    let actions = editor.planned_actions();
    assert_eq!(actions[0], CommitEditAction::Delete("old.txt".to_string()));
    assert!(matches!(actions[1], CommitEditAction::Copy { .. }));
    assert!(matches!(actions[2], CommitEditAction::Rename { .. }));
    assert!(matches!(actions[3], CommitEditAction::Add { .. }));
    assert!(matches!(actions[4], CommitEditAction::Modify { .. }));
    assert!(matches!(actions[5], CommitEditAction::TypeChange { .. }));
}

#[test]
fn property_mapper_sets_executable_and_special() {
    let mapper = PropertyMapper::new();
    assert_eq!(mapper.file_props("100644", "100755"), vec![("svn:executable".to_string(), Some("*".to_string()))]);
    assert_eq!(mapper.file_props("100644", "120000"), vec![("svn:special".to_string(), Some("*".to_string()))]);
}

#[test]
fn path_ensurer_opens_each_parent_once() {
    let mut ensurer = PathEnsurer::new();
    ensurer.ensure_path("a/b/c.txt").unwrap();
    ensurer.ensure_path("a/b/d.txt").unwrap();
    assert_eq!(ensurer.created_dirs(), &["a".to_string(), "a/b".to_string()]);
}
```

- [ ] **Step 2: Implement editor planning types**

Create `crates/git-svn-rs-core/src/dcommit/commit_editor.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitEditAction {
    Delete(String),
    Copy { from: String, to: String },
    Rename { from: String, to: String },
    Add { path: String, content: Vec<u8> },
    Modify { path: String, content: Vec<u8> },
    TypeChange { path: String, content: Vec<u8> },
}

#[derive(Default)]
pub struct SvnCommitEditor {
    deletes: Vec<String>,
    copies: Vec<(String, String)>,
    renames: Vec<(String, String)>,
    adds: Vec<(String, Vec<u8>)>,
    modifies: Vec<(String, Vec<u8>)>,
    type_changes: Vec<(String, Vec<u8>)>,
}

impl SvnCommitEditor {
    pub fn new() -> Self { Self::default() }
    pub fn queue_delete(&mut self, path: &str) { self.deletes.push(path.to_string()); }
    pub fn queue_copy(&mut self, from: &str, to: &str) { self.copies.push((from.to_string(), to.to_string())); }
    pub fn queue_rename(&mut self, from: &str, to: &str) { self.renames.push((from.to_string(), to.to_string())); }
    pub fn queue_add(&mut self, path: &str, content: &[u8]) { self.adds.push((path.to_string(), content.to_vec())); }
    pub fn queue_modify(&mut self, path: &str, content: &[u8]) { self.modifies.push((path.to_string(), content.to_vec())); }
    pub fn queue_type_change(&mut self, path: &str, content: &[u8]) { self.type_changes.push((path.to_string(), content.to_vec())); }

    pub fn planned_actions(&self) -> Vec<CommitEditAction> {
        let mut out = Vec::new();
        out.extend(self.deletes.iter().cloned().map(CommitEditAction::Delete));
        out.extend(self.copies.iter().cloned().map(|(from, to)| CommitEditAction::Copy { from, to }));
        out.extend(self.renames.iter().cloned().map(|(from, to)| CommitEditAction::Rename { from, to }));
        out.extend(self.adds.iter().cloned().map(|(path, content)| CommitEditAction::Add { path, content }));
        out.extend(self.modifies.iter().cloned().map(|(path, content)| CommitEditAction::Modify { path, content }));
        out.extend(self.type_changes.iter().cloned().map(|(path, content)| CommitEditAction::TypeChange { path, content }));
        out
    }
}

#[derive(Default)]
pub struct PathEnsurer {
    created: Vec<String>,
}

impl PathEnsurer {
    pub fn new() -> Self { Self::default() }

    pub fn ensure_path(&mut self, path: &str) -> Result<(), String> {
        let mut current = String::new();
        let parts = path.split('/').collect::<Vec<_>>();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            if !self.created.contains(&current) {
                self.created.push(current.clone());
            }
        }
        Ok(())
    }

    pub fn created_dirs(&self) -> &[String] {
        &self.created
    }
}
```

Create `crates/git-svn-rs-core/src/dcommit/property_mapper.rs`:

```rust
pub struct PropertyMapper;

impl PropertyMapper {
    pub fn new() -> Self { Self }

    pub fn file_props(&self, old_mode: &str, new_mode: &str) -> Vec<(String, Option<String>)> {
        let mut props = Vec::new();
        if !old_mode.ends_with("755") && new_mode.ends_with("755") {
            props.push(("svn:executable".to_string(), Some("*".to_string())));
        } else if old_mode.ends_with("755") && !new_mode.ends_with("755") {
            props.push(("svn:executable".to_string(), None));
        }
        if !old_mode.starts_with("120") && new_mode.starts_with("120") {
            props.push(("svn:special".to_string(), Some("*".to_string())));
        } else if old_mode.starts_with("120") && !new_mode.starts_with("120") {
            props.push(("svn:special".to_string(), None));
        }
        props
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod dcommit {
    pub mod commit_editor;
    pub mod diff_planner;
    pub mod property_mapper;
}
```

- [ ] **Step 3: Route write-back through editor**

Modify `commands::dcommit::run` so non-dry-run mode:

```text
1. Reads eligible local commits.
2. Uses GitDiffPlanner for each commit.
3. Builds SvnCommitEditor operations.
4. Uses PathEnsurer for parent directories.
5. Uses PropertyMapper for svn:executable and svn:special.
6. Calls the libsvn commit editor through the `svn-libsvn` backend.
7. Runs fetch for the returned SVN revision.
8. Rebases unless --no-rebase is set.
```

The old direct `SvnCommitRequest { changes }` path may remain only as a mock-test adapter; it must not be the production `dcommit` path.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test dcommit_commit_editor
cargo test -p git-svn-rs --test dcommit_linear -- --nocapture
```

Expected: PASS; fixture-backed write test skips only when SVN/libsvn dependencies are missing.

### Task 8: Keep v1 Scope Explicit

**Files:**
- Modify: `crates/git-svn-rs-core/src/commands/dcommit.rs`
- Test: `crates/git-svn-rs-cli/tests/dcommit_linear.rs`

- [ ] **Step 1: Add unsupported mergeinfo automation test**

Append to `crates/git-svn-rs-cli/tests/dcommit_linear.rs`:

```rust
#[test]
fn dcommit_does_not_auto_generate_mergeinfo_in_v1() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .args(["dcommit", "--mergeinfo", "/branches/foo:1-10", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("explicit mergeinfo accepted for dry-run"))
        .stdout(predicates::str::contains("automatic mergeinfo generation is not implemented in v1"));
}
```

- [ ] **Step 2: Implement explicit messaging**

Modify `dcommit` option handling so `--mergeinfo VALUE --dry-run` prints the explicit value and the v1 non-goal message. Do not infer mergeinfo from Git merge commits in v1.

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test dcommit_linear dcommit_does_not_auto_generate_mergeinfo_in_v1
```

Expected: PASS.

## References

- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Editor.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Editor.pm)
- [Ra.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Ra.pm)
- [Prompt.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Prompt.pm)
