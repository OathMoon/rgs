# Foundation CLI Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a compilable Rust workspace with a `git-svn-rs` binary and a tested command-line surface.

**Architecture:** Keep CLI parsing in the binary crate and reusable behavior in `git-svn-rs-core`. Phase 1 does not talk to SVN or Git repositories; it creates typed command parsing, stable unsupported-command errors, logging flags, and a diagnostics command.

**Tech Stack:** Rust 1.95, Cargo workspace, `clap`, `anyhow`, `thiserror`, `assert_cmd`, `predicates`, `tempfile`.

---

## File Structure

- Create: `.gitignore`
- Create: `Cargo.toml`
- Create: `crates/git-svn-rs-cli/Cargo.toml`
- Create: `crates/git-svn-rs-cli/src/main.rs`
- Create: `crates/git-svn-rs-core/Cargo.toml`
- Create: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/cli.rs`
- Create: `crates/git-svn-rs-core/src/error.rs`
- Create: `crates/git-svn-rs-core/tests/cli_parse.rs`
- Create: `crates/git-svn-rs-cli/tests/cli_smoke.rs`
- Modify: `README.md`

## Task 1: Initialize Workspace

**Files:**
- Create: `.gitignore`
- Create: `Cargo.toml`
- Create: `crates/git-svn-rs-cli/Cargo.toml`
- Create: `crates/git-svn-rs-core/Cargo.toml`
- Create: `README.md`

- [ ] **Step 1: Initialize Git tracking**

Run:

```powershell
git init
git status --short --branch
```

Expected: Git reports a new branch and no tracked files yet.

- [ ] **Step 2: Create the workspace manifest**

Create `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/git-svn-rs-cli",
    "crates/git-svn-rs-core",
]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.95"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
anyhow = "1"
assert_cmd = "2"
clap = { version = "4", features = ["derive", "env"] }
predicates = "3"
tempfile = "3"
thiserror = "2"
```

- [ ] **Step 3: Create package manifests**

Create `crates/git-svn-rs-core/Cargo.toml`:

```toml
[package]
name = "git-svn-rs-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
clap.workspace = true
thiserror.workspace = true
```

Create `crates/git-svn-rs-cli/Cargo.toml`:

```toml
[package]
name = "git-svn-rs"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "git-svn-rs"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
git-svn-rs-core = { path = "../git-svn-rs-core" }

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true
```

- [ ] **Step 4: Add ignore rules and README**

Create `.gitignore`:

```gitignore
/target/
**/*.rs.bk
/.idea/
/.vscode/
```

Create `README.md`:

```markdown
# git-svn-rs

Rust implementation plan and staged replacement for the core `git svn` workflow.

The default command is `git-svn-rs`. A `git-svn` compatibility shim is planned as an explicit opt-in package step.
```

- [ ] **Step 5: Verify workspace metadata**

Run:

```powershell
cargo metadata --no-deps
```

Expected: JSON includes package names `git-svn-rs` and `git-svn-rs-core`.

- [ ] **Step 6: Commit**

Run:

```powershell
git add .gitignore Cargo.toml README.md crates
git commit -m "chore: create git-svn-rs workspace"
```

## Task 2: Add Typed CLI Surface

**Files:**
- Create: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/cli.rs`
- Create: `crates/git-svn-rs-core/src/error.rs`
- Create: `crates/git-svn-rs-cli/src/main.rs`
- Test: `crates/git-svn-rs-core/tests/cli_parse.rs`
- Test: `crates/git-svn-rs-cli/tests/cli_smoke.rs`

- [ ] **Step 1: Write CLI parsing tests**

Create `crates/git-svn-rs-core/tests/cli_parse.rs`:

```rust
use clap::Parser;
use git_svn_rs_core::cli::{Cli, Command};

#[test]
fn parses_clone_with_standard_layout() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "clone",
        "file:///tmp/repo",
        "work",
        "--stdlayout",
        "--authors-file",
        "authors.txt",
    ]);

    match cli.command {
        Command::Clone(args) => {
            assert_eq!(args.url, "file:///tmp/repo");
            assert_eq!(args.path.as_deref(), Some("work"));
            assert!(args.stdlayout);
            assert_eq!(args.authors_file.as_deref(), Some("authors.txt"));
        }
        other => panic!("expected clone, got {other:?}"),
    }
}

#[test]
fn parses_dcommit_dry_run_commit_url() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "dcommit",
        "--dry-run",
        "--commit-url",
        "https://svn.example/write",
    ]);

    match cli.command {
        Command::Dcommit(args) => {
            assert!(args.dry_run);
            assert_eq!(args.commit_url.as_deref(), Some("https://svn.example/write"));
        }
        other => panic!("expected dcommit, got {other:?}"),
    }
}

#[test]
fn parses_dcommit_explicit_mergeinfo() {
    let cli = Cli::parse_from([
        "git-svn-rs",
        "dcommit",
        "--mergeinfo",
        "/branches/foo:1-10",
        "--dry-run",
    ]);

    match cli.command {
        Command::Dcommit(args) => {
            assert!(args.dry_run);
            assert_eq!(args.mergeinfo.as_deref(), Some("/branches/foo:1-10"));
        }
        other => panic!("expected dcommit, got {other:?}"),
    }
}

#[test]
fn parses_known_unsupported_command() {
    let cli = Cli::parse_from(["git-svn-rs", "branch", "feature"]);
    assert!(matches!(cli.command, Command::Unsupported(_)));
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```powershell
cargo test -p git-svn-rs-core --test cli_parse
```

Expected: FAIL because `git_svn_rs_core::cli` does not exist.

- [ ] **Step 3: Implement CLI types**

Create `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod cli;
pub mod error;
```

Create `crates/git-svn-rs-core/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitSvnError {
    #[error("unsupported in v1: {0}")]
    UnsupportedCommand(String),
}
```

Create `crates/git-svn-rs-core/src/cli.rs`:

```rust
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "git-svn-rs", version, about = "Rust replacement for core git-svn workflows")]
pub struct Cli {
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init(InitArgs),
    Clone(CloneArgs),
    Fetch(FetchArgs),
    Rebase(RebaseArgs),
    Dcommit(DcommitArgs),
    Log(LogArgs),
    Info(InfoArgs),
    #[command(name = "find-rev")]
    FindRev(FindRevArgs),
    Gc(GcArgs),
    Reset(ResetArgs),
    Diagnose(DiagnoseArgs),
    Branch(UnsupportedArgs),
    Tag(UnsupportedArgs),
    #[command(name = "set-tree")]
    SetTree(UnsupportedArgs),
    Propget(UnsupportedArgs),
    Propset(UnsupportedArgs),
    Proplist(UnsupportedArgs),
    #[command(name = "show-ignore")]
    ShowIgnore(UnsupportedArgs),
    #[command(name = "show-externals")]
    ShowExternals(UnsupportedArgs),
    #[command(external_subcommand)]
    Unsupported(Vec<String>),
}

#[derive(Debug, Args)]
pub struct LayoutArgs {
    #[arg(short = 's', long)]
    pub stdlayout: bool,
    #[arg(short = 'T', long)]
    pub trunk: Option<String>,
    #[arg(short = 'b', long)]
    pub branches: Vec<String>,
    #[arg(short = 't', long)]
    pub tags: Vec<String>,
    #[arg(long)]
    pub prefix: Option<String>,
}

#[derive(Debug, Args)]
pub struct SharedFetchArgs {
    #[arg(short = 'A', long = "authors-file")]
    pub authors_file: Option<String>,
    #[arg(long = "authors-prog")]
    pub authors_prog: Option<String>,
    #[arg(long = "ignore-paths")]
    pub ignore_paths: Option<String>,
    #[arg(long = "include-paths")]
    pub include_paths: Option<String>,
    #[arg(long = "ignore-refs")]
    pub ignore_refs: Option<String>,
    #[arg(short = 'r', long = "revision")]
    pub revision: Option<String>,
    #[arg(long = "log-window-size")]
    pub log_window_size: Option<u32>,
    #[arg(long)]
    pub localtime: bool,
    #[arg(long = "no-metadata")]
    pub no_metadata: bool,
    #[arg(long = "rewrite-root")]
    pub rewrite_root: Option<String>,
    #[arg(long = "rewrite-uuid")]
    pub rewrite_uuid: Option<String>,
    #[arg(long)]
    pub username: Option<String>,
    #[arg(long = "config-dir")]
    pub config_dir: Option<String>,
    #[arg(long = "no-auth-cache")]
    pub no_auth_cache: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    pub url: String,
    pub path: Option<String>,
    #[command(flatten)]
    pub layout: LayoutArgs,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct CloneArgs {
    pub url: String,
    pub path: Option<String>,
    #[command(flatten)]
    pub layout: LayoutArgs,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
    #[arg(long = "no-checkout")]
    pub no_checkout: bool,
}

#[derive(Debug, Args)]
pub struct FetchArgs {
    pub remote: Option<String>,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
    #[arg(long = "fetch-all", alias = "all")]
    pub fetch_all: bool,
    #[arg(short = 'p', long = "parent")]
    pub parent: bool,
}

#[derive(Debug, Args)]
pub struct RebaseArgs {
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
    #[arg(short = 'm', long = "merge")]
    pub merge: bool,
    #[arg(short = 's', long = "strategy")]
    pub strategy: Option<String>,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct DcommitArgs {
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,
    #[arg(long = "commit-url")]
    pub commit_url: Option<String>,
    #[arg(long = "mergeinfo")]
    pub mergeinfo: Option<String>,
    #[arg(long = "no-rebase")]
    pub no_rebase: bool,
    #[command(flatten)]
    pub shared: SharedFetchArgs,
}

#[derive(Debug, Args)]
pub struct LogArgs {
    #[arg(short = 'r', long = "revision")]
    pub revision: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
    #[arg(short = 'v', long)]
    pub verbose: bool,
    #[arg(long)]
    pub incremental: bool,
    #[arg(long)]
    pub oneline: bool,
    #[arg(long = "show-commit")]
    pub show_commit: bool,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    #[arg(long)]
    pub url: bool,
}

#[derive(Debug, Args)]
pub struct FindRevArgs {
    pub rev_or_commit: String,
    #[arg(short = 'B', long = "before")]
    pub before: bool,
    #[arg(short = 'A', long = "after")]
    pub after: bool,
}

#[derive(Debug, Args)]
pub struct GcArgs {}

#[derive(Debug, Args)]
pub struct ResetArgs {
    #[arg(short = 'r', long = "revision")]
    pub revision: String,
    #[arg(short = 'p', long = "parent")]
    pub parent: bool,
}

#[derive(Debug, Args)]
pub struct DiagnoseArgs {}

#[derive(Debug, Args)]
pub struct UnsupportedArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}
```

- [ ] **Step 4: Add binary entry point**

Create `crates/git-svn-rs-cli/src/main.rs`:

```rust
use anyhow::{bail, Result};
use clap::Parser;
use git_svn_rs_core::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Diagnose(_) => {
            println!("git-svn-rs diagnostics");
            println!("libsvn feature: disabled");
            Ok(())
        }
        Command::Unsupported(args) => {
            let name = args.first().cloned().unwrap_or_else(|| "unknown".to_string());
            bail!("unsupported in v1: {name}")
        }
        Command::Branch(_) => bail!("unsupported in v1: branch"),
        Command::Tag(_) => bail!("unsupported in v1: tag"),
        Command::SetTree(_) => bail!("unsupported in v1: set-tree"),
        Command::Propget(_) => bail!("unsupported in v1: propget"),
        Command::Propset(_) => bail!("unsupported in v1: propset"),
        Command::Proplist(_) => bail!("unsupported in v1: proplist"),
        Command::ShowIgnore(_) => bail!("unsupported in v1: show-ignore"),
        Command::ShowExternals(_) => bail!("unsupported in v1: show-externals"),
        other => bail!("command parsed but not implemented in phase 1: {other:?}"),
    }
}
```

- [ ] **Step 5: Add smoke tests**

Create `crates/git-svn-rs-cli/tests/cli_smoke.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_core_commands() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("clone"))
        .stdout(predicate::str::contains("dcommit"))
        .stdout(predicate::str::contains("find-rev"));
}

#[test]
fn diagnose_prints_feature_state() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.arg("diagnose")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-svn-rs diagnostics"))
        .stdout(predicate::str::contains("libsvn feature: disabled"));
}

#[test]
fn branch_is_explicitly_unsupported() {
    let mut cmd = Command::cargo_bin("git-svn-rs").unwrap();
    cmd.args(["branch", "feature"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported in v1: branch"));
}
```

- [ ] **Step 6: Verify tests pass**

Run:

```powershell
cargo test --workspace
cargo run -p git-svn-rs -- --help
```

Expected: all tests pass, help output includes `clone`, `fetch`, `rebase`, `dcommit`, `find-rev`, `diagnose`.

- [ ] **Step 7: Commit**

Run:

```powershell
git add crates Cargo.toml README.md
git commit -m "feat: add git-svn-rs cli surface"
```

## Self Review

- Spec coverage: command names and core options are represented before business logic exists.
- Placeholder scan: unsupported v1 commands have explicit error behavior.
- Type consistency: `Cli`, `Command`, and argument structs are defined in `git-svn-rs-core::cli` and used by both tests and binary.
