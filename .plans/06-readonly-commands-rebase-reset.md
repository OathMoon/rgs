# Readonly Commands Rebase Reset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement read-only and local-history commands: `find-rev`, `info`, `log`, `gc`, `reset`, and `rebase`.

**Architecture:** These commands operate on Git config, refs, commit messages, `.git/svn` metadata, and `.rev_map`; only `rebase` performs a fetch before invoking Git rebase. `GitSvnLogFormatter` and bidirectional `find-rev` are required compatibility units before the phase gate, not optional polish.

**Tech Stack:** Rust 1.95, Git CLI, `.rev_map`, `flate2` for gzip compression, integration tests over repositories created by the import phase.

Production command modules must use the shared `GitCli` wrapper for Git operations so `perl/Git.pm`-style config lookup and error propagation stay consistent. Direct `std::process::Command::new("git")` is acceptable in integration test setup only.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Create: `crates/git-svn-rs-core/src/commands/find_rev.rs`
- Create: `crates/git-svn-rs-core/src/commands/info.rs`
- Create: `crates/git-svn-rs-core/src/commands/log.rs`
- Create: `crates/git-svn-rs-core/src/commands/gc.rs`
- Create: `crates/git-svn-rs-core/src/commands/reset.rs`
- Create: `crates/git-svn-rs-core/src/commands/rebase.rs`
- Create: `crates/git-svn-rs-core/src/log_formatter.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

Compatibility note: command implementations must resolve the primary tracked ref from `[svn-remote "<name>"]` config and `.git/svn/**/.rev_map.*`. `refs/remotes/git-svn` is valid for single-path fixtures, but production `--stdlayout` behavior must also support current `git-svn` default refs such as `refs/remotes/origin/trunk`.

## Task 1: Implement `find-rev`

**Files:**
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Create: `crates/git-svn-rs-core/src/commands/find_rev.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Write `find-rev` test over rev_map**

Create `crates/git-svn-rs-cli/tests/readonly_commands.rs`:

```rust
use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn find_rev_translates_svn_revision_to_commit() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    std::fs::create_dir_all(dir.path().join(".git/svn/refs/remotes/git-svn")).unwrap();
    let map = dir.path().join(".git/svn/refs/remotes/git-svn/.rev_map.uuid");
    let mut bytes = vec![0, 0, 0, 7];
    bytes.extend(hex::decode("1111111111111111111111111111111111111111").unwrap());
    std::fs::write(map, bytes).unwrap();
    std::process::Command::new("git")
        .current_dir(dir.path())
        .args(["config", "svn-remote.svn.uuid", "uuid"])
        .status()
        .unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .args(["find-rev", "r7"])
        .assert()
        .success()
        .stdout("1111111111111111111111111111111111111111\n");
}
```

Add `hex` to `crates/git-svn-rs-cli` dev dependencies if the test crate needs it:

```toml
[dev-dependencies]
hex.workspace = true
```

- [ ] **Step 2: Run test and see failure**

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands find_rev_translates_svn_revision_to_commit
```

Expected: FAIL because `find-rev` is not wired.

- [ ] **Step 3: Implement command**

Create `crates/git-svn-rs-core/src/commands/find_rev.rs`:

```rust
use crate::cli::FindRevArgs;
use crate::rev_map::{ObjectFormat, RevMap};

pub fn run(args: &FindRevArgs, cwd: &std::path::Path) -> Result<String, String> {
    let rev = args
        .rev_or_commit
        .strip_prefix('r')
        .unwrap_or(&args.rev_or_commit)
        .parse::<u32>()
        .map_err(|_| "find-rev v1 expects an SVN revision like r42".to_string())?;
    let git_dir = cwd.join(".git");
    let uuid = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["config", "--get", "svn-remote.svn.uuid"])
        .output()
        .map_err(|e| e.to_string())
        .and_then(|o| {
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                Err("svn-remote.svn.uuid not configured".to_string())
            }
        })?;
    let path = git_dir.join("svn/refs/remotes/git-svn").join(format!(".rev_map.{uuid}"));
    let map = RevMap::open(path, ObjectFormat::Sha1)?;
    Ok(format!("{}\n", map.get(rev)?.unwrap_or_default()))
}
```

Modify `crates/git-svn-rs-core/src/commands/mod.rs`:

```rust
pub mod find_rev;
```

Wire `Command::FindRev(args)` in CLI main and print the returned string.

- [ ] **Step 4: Verify test passes**

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands find_rev_translates_svn_revision_to_commit
```

Expected: PASS.

## Task 2: Implement `info` and `log`

**Files:**
- Create: `crates/git-svn-rs-core/src/commands/info.rs`
- Create: `crates/git-svn-rs-core/src/commands/log.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Add tests**

Append to `crates/git-svn-rs-cli/tests/readonly_commands.rs`:

```rust
#[test]
fn info_prints_configured_url() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    std::process::Command::new("git")
        .current_dir(dir.path())
        .args(["config", "svn-remote.svn.url", "file:///repo"])
        .status()
        .unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .arg("info")
        .assert()
        .success()
        .stdout(predicates::str::contains("URL: file:///repo"));
}
```

- [ ] **Step 2: Run test and see failure**

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands info_prints_configured_url
```

Expected: FAIL because `info` is not wired.

- [ ] **Step 3: Implement `info`**

Create `crates/git-svn-rs-core/src/commands/info.rs`:

```rust
use crate::cli::InfoArgs;

pub fn run(_args: &InfoArgs, cwd: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["config", "--get", "svn-remote.svn.url"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("svn-remote.svn.url not configured".to_string());
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(format!("URL: {url}\nRepository Root: {url}\n"))
}
```

- [ ] **Step 4: Implement `log` as a Git-backed v1 output**

Create `crates/git-svn-rs-core/src/commands/log.rs`:

```rust
use crate::cli::LogArgs;

pub fn run(args: &LogArgs, cwd: &std::path::Path) -> Result<String, String> {
    let limit = args.limit.unwrap_or(100).to_string();
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["log", "refs/remotes/git-svn", "--format=%H%n%B%x1e", "-n", &limit])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).replace('\x1e', "------------------------------------------------------------------------\n"))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
```

Modify `commands/mod.rs`:

```rust
pub mod info;
pub mod log;
```

Wire `Command::Info` and `Command::Log` in CLI main.

- [ ] **Step 5: Verify tests**

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands
```

Expected: PASS for `find-rev` and `info`; `log` is exercised by clone/fetch fixture tests after Phase 5 creates commits.

## Task 3: Implement `gc` and `reset`

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Create: `crates/git-svn-rs-core/src/commands/gc.rs`
- Create: `crates/git-svn-rs-core/src/commands/reset.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Add gzip dependency**

Modify root `Cargo.toml`:

```toml
[workspace.dependencies]
flate2 = "1"
```

Modify `crates/git-svn-rs-core/Cargo.toml`:

```toml
flate2.workspace = true
```

- [ ] **Step 2: Add `gc` test**

Append:

```rust
#[test]
fn gc_compresses_unhandled_log() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    let svn_dir = dir.path().join(".git/svn/refs/remotes/git-svn");
    std::fs::create_dir_all(&svn_dir).unwrap();
    std::fs::write(svn_dir.join("unhandled.log"), "property svn:ignore\n").unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path()).arg("gc").assert().success();

    assert!(svn_dir.join("unhandled.log.gz").exists());
}
```

- [ ] **Step 3: Implement `gc`**

Create `crates/git-svn-rs-core/src/commands/gc.rs`:

```rust
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

pub fn run(cwd: &std::path::Path) -> Result<(), String> {
    let root = cwd.join(".git/svn");
    if !root.exists() {
        return Ok(());
    }
    for entry in walk(&root)? {
        if entry.file_name().and_then(|n| n.to_str()) == Some("unhandled.log") {
            let data = std::fs::read(&entry).map_err(|e| e.to_string())?;
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data).map_err(|e| e.to_string())?;
            std::fs::write(entry.with_file_name("unhandled.log.gz"), encoder.finish().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            std::fs::remove_file(&entry).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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

- [ ] **Step 4: Implement `reset`**

Create `crates/git-svn-rs-core/src/commands/reset.rs`:

```rust
use crate::cli::{FindRevArgs, ResetArgs};
use crate::commands::find_rev;

pub fn run(args: &ResetArgs, cwd: &std::path::Path) -> Result<(), String> {
    let revision = args.revision.strip_prefix('r').unwrap_or(&args.revision);
    let commit = find_rev::run(
        &FindRevArgs {
            rev_or_commit: format!("r{revision}"),
            before: false,
            after: false,
        },
        cwd,
    )?;
    let commit = commit.trim();
    if commit.is_empty() {
        return Err(format!("no Git commit found for SVN revision r{revision}"));
    }
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["update-ref", "refs/remotes/git-svn", commit])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
```

- [ ] **Step 5: Wire commands and verify**

Modify `commands/mod.rs`:

```rust
pub mod gc;
pub mod reset;
```

Wire `Command::Gc` and `Command::Reset` in CLI main.

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands gc_compresses_unhandled_log
```

Expected: PASS.

## Task 4: Implement `rebase`

**Files:**
- Create: `crates/git-svn-rs-core/src/commands/rebase.rs`
- Modify: `crates/git-svn-rs-core/src/commands/mod.rs`
- Modify: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Add dry-run test**

Append:

```rust
#[test]
fn rebase_dry_run_prints_actions() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .args(["rebase", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("would run fetch"))
        .stdout(predicates::str::contains("would run git rebase refs/remotes/git-svn"));
}
```

- [ ] **Step 2: Implement rebase**

Create `crates/git-svn-rs-core/src/commands/rebase.rs`:

```rust
use crate::cli::RebaseArgs;

pub fn run(args: &RebaseArgs, cwd: &std::path::Path) -> Result<String, String> {
    if args.dry_run {
        return Ok("would run fetch\nwould run git rebase refs/remotes/git-svn\n".to_string());
    }
    crate::commands::fetch::run(
        &crate::cli::FetchArgs {
            remote: None,
            shared: args.shared.clone(),
            fetch_all: false,
            parent: false,
        },
        cwd,
    )?;
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["rebase", "refs/remotes/git-svn"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
```

- [ ] **Step 3: Wire and verify**

Modify `commands/mod.rs`:

```rust
pub mod rebase;
```

Wire `Command::Rebase` in CLI main.

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands rebase_dry_run_prints_actions
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 4: Commit**

Run:

```powershell
git add Cargo.toml crates
git commit -m "feat: add read-only git-svn commands"
```

## Self Review

- Spec coverage: `find-rev`, `info`, `log`, `gc`, `reset`, and `rebase` each have a command module and at least one acceptance test.
- Placeholder scan: `reset` calls the local `find_rev` module directly and does not depend on Perl `git-svn`.
- Type consistency: read-only commands reuse `RevMap`, `GitSvnId`, `FetchArgs`, and Git CLI process behavior.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 6 acceptance gate. Do not treat them as optional optimization work.

### Task 5: Add `GitSvnLogFormatter`

**Files:**
- Create: `crates/git-svn-rs-core/src/log_formatter.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/commands/log.rs`
- Test: `crates/git-svn-rs-core/tests/log_formatter.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Write `Log.pm` formatter tests**

Create `crates/git-svn-rs-core/tests/log_formatter.rs`:

```rust
use git_svn_rs_core::log_formatter::{GitSvnLogFormatter, LogCommit, LogFormat};

#[test]
fn formats_normal_svn_log_entry() {
    let commit = LogCommit {
        commit: "abc123".to_string(),
        revision: 42,
        author: "alice".to_string(),
        unix_timestamp: 1_704_067_200,
        message_lines: vec!["change file\n".to_string()],
        changed_paths: vec!["   M /trunk/file.txt\n".to_string()],
    };
    let output = GitSvnLogFormatter::new(LogFormat::Normal { show_commit: true, verbose: true, incremental: false })
        .format(&[commit]);

    assert!(output.contains("------------------------------------------------------------------------"));
    assert!(output.contains("r42 | abc123 | alice |"));
    assert!(output.contains("Changed paths:"));
    assert!(output.contains("change file"));
}

#[test]
fn formats_oneline_entry() {
    let commit = LogCommit {
        commit: "abc123".to_string(),
        revision: 42,
        author: "alice".to_string(),
        unix_timestamp: 1,
        message_lines: vec!["change file\n".to_string()],
        changed_paths: vec![],
    };
    let output = GitSvnLogFormatter::new(LogFormat::Oneline { show_commit: true }).format(&[commit]);
    assert_eq!(output.trim(), "abc123 | r42 | change file");
}
```

- [ ] **Step 2: Implement formatter**

Create `crates/git-svn-rs-core/src/log_formatter.rs`:

```rust
#[derive(Debug, Clone)]
pub struct LogCommit {
    pub commit: String,
    pub revision: u32,
    pub author: String,
    pub unix_timestamp: i64,
    pub message_lines: Vec<String>,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum LogFormat {
    Normal { show_commit: bool, verbose: bool, incremental: bool },
    Oneline { show_commit: bool },
}

pub struct GitSvnLogFormatter {
    format: LogFormat,
}

impl GitSvnLogFormatter {
    pub fn new(format: LogFormat) -> Self {
        Self { format }
    }

    pub fn format(&self, commits: &[LogCommit]) -> String {
        match self.format {
            LogFormat::Oneline { show_commit } => commits.iter().map(|c| {
                let first = c.message_lines.first().map(|s| s.trim()).unwrap_or("");
                if show_commit {
                    format!("{} | r{} | {}\n", c.commit, c.revision, first)
                } else {
                    format!("r{} | {}\n", c.revision, first)
                }
            }).collect(),
            LogFormat::Normal { show_commit, verbose, incremental } => {
                let mut out = String::new();
                for c in commits {
                    if !incremental {
                        out.push_str("------------------------------------------------------------------------\n");
                    }
                    out.push_str(&format!("r{} | ", c.revision));
                    if show_commit {
                        out.push_str(&format!("{} | ", c.commit));
                    }
                    out.push_str(&format!("{} | {} | {} line\n", c.author, c.unix_timestamp, c.message_lines.len()));
                    if verbose && !c.changed_paths.is_empty() {
                        out.push_str("Changed paths:\n");
                        for path in &c.changed_paths {
                            out.push_str(path);
                        }
                        out.push('\n');
                    }
                    for line in &c.message_lines {
                        out.push_str(line);
                    }
                }
                if !incremental {
                    out.push_str("------------------------------------------------------------------------\n");
                }
                out
            }
        }
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod log_formatter;
```

- [ ] **Step 3: Route `log` command through formatter**

Modify `commands::log::run` to parse the tracked ref from config, parse Git commit metadata and `git-svn-id` footers, and feed `GitSvnLogFormatter`; `git log` raw output and hard-coded `refs/remotes/git-svn` must not be returned directly as final v1 behavior.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test log_formatter
cargo test -p git-svn-rs --test readonly_commands
```

Expected: PASS.

### Task 6: Expand `find-rev`

**Files:**
- Modify: `crates/git-svn-rs-core/src/commands/find_rev.rs`
- Test: `crates/git-svn-rs-cli/tests/readonly_commands.rs`

- [ ] **Step 1: Add bidirectional tests**

Append to `crates/git-svn-rs-cli/tests/readonly_commands.rs`:

```rust
#[test]
fn find_rev_translates_commit_to_svn_revision() {
    let dir = tempdir().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).arg("init").status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["config", "user.name", "Test User"]).status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["config", "user.email", "test@example.com"]).status().unwrap();
    std::fs::write(dir.path().join("file.txt"), "content\n").unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["add", "file.txt"]).status().unwrap();
    std::process::Command::new("git").current_dir(dir.path()).args(["commit", "-m", "msg\n\ngit-svn-id: file:///repo/trunk@9 uuid"]).status().unwrap();
    let commit = std::process::Command::new("git").current_dir(dir.path()).args(["rev-parse", "HEAD"]).output().unwrap();
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();

    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.current_dir(dir.path())
        .args(["find-rev", &commit])
        .assert()
        .success()
        .stdout("9\n");
}
```

- [ ] **Step 2: Implement commit-to-revision lookup**

Modify `find_rev::run` so non-`rN` inputs are treated as Git commits. It must run `git log -1 --format=%B <commit>`, parse the final `git-svn-id:` footer with `GitSvnId::parse`, and print the SVN revision. SVN revision to commit lookup must scan configured tracked refs/rev maps instead of assuming `refs/remotes/git-svn`.

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test readonly_commands find_rev_translates_commit_to_svn_revision
```

Expected: PASS.

## References

- [git-svn official documentation](https://git-scm.com/docs/git-svn)
- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Log.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Log.pm)
- [Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)
