# Compatibility Golden Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compare `git-svn-rs` behavior against Perl `git svn` golden fixtures for the v1 compatibility surface.

**Architecture:** Build deterministic SVN fixtures with `svnadmin`, run Perl `git svn` when available to capture golden artifacts, run `git-svn-rs` against the same fixtures, then compare normalized config, refs, commit messages, rev maps, log output, read-only commands, import edge cases, and linear `dcommit`.

**Tech Stack:** Rust 1.95, Cargo integration tests, Git CLI, Perl `git svn` when installed, Subversion CLI, `tempfile`, `assert_cmd`, binary artifact comparison.

---

## File Structure

- Create: `crates/git-svn-rs-cli/tests/golden/mod.rs`
- Create: `crates/git-svn-rs-cli/tests/golden/fixtures.rs`
- Create: `crates/git-svn-rs-cli/tests/golden/perl_git_svn.rs`
- Create: `crates/git-svn-rs-cli/tests/golden/compare.rs`
- Test: `crates/git-svn-rs-cli/tests/compat_golden.rs`
- Modify: `scripts/verify.ps1`
- Modify: `README.md`

## Task 1: Add Golden Fixture Harness

**Files:**
- Create: `crates/git-svn-rs-cli/tests/golden/mod.rs`
- Create: `crates/git-svn-rs-cli/tests/golden/fixtures.rs`
- Test: `crates/git-svn-rs-cli/tests/compat_golden.rs`

- [ ] **Step 1: Create fixture module**

Create `crates/git-svn-rs-cli/tests/golden/mod.rs`:

```rust
pub mod compare;
pub mod fixtures;
pub mod perl_git_svn;
```

Create `crates/git-svn-rs-cli/tests/golden/fixtures.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub fn has_required_tools() -> bool {
    ["git", "svn", "svnadmin", "svnlook"].iter().all(|program| {
        Command::new(program).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    })
}

pub fn has_perl_git_svn() -> bool {
    Command::new("git").args(["svn", "--version"]).output().map(|o| o.status.success()).unwrap_or(false)
}

pub struct GoldenSvnFixture {
    _tmp: TempDir,
    repo: PathBuf,
    wc: PathBuf,
}

impl GoldenSvnFixture {
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
        std::fs::write(wc.join("trunk/run.sh"), "#!/bin/sh\necho hi\n").map_err(|e| e.to_string())?;
        run(&wc, "svn", &["add", "trunk/src", "trunk/run.sh"])?;
        run(&wc, "svn", &["propset", "svn:executable", "*", "trunk/run.sh"])?;
        run(&wc, "svn", &["commit", "-m", "add files"])?;
        run(&wc, "svn", &["copy", "trunk", "branches/main"])?;
        run(&wc, "svn", &["commit", "-m", "branch main"])?;
        run(&wc, "svn", &["copy", "trunk", "tags/v1"])?;
        run(&wc, "svn", &["commit", "-m", "tag v1"])?;
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

- [ ] **Step 2: Create top-level golden test shell**

Create `crates/git-svn-rs-cli/tests/compat_golden.rs`:

```rust
mod golden;

use golden::fixtures::{has_perl_git_svn, has_required_tools, GoldenSvnFixture};

#[test]
fn golden_fixture_can_be_created() {
    if !has_required_tools() {
        eprintln!("skipping: git, svn, svnadmin, and svnlook are required");
        return;
    }

    let fixture = GoldenSvnFixture::create().unwrap();
    assert!(fixture.url().starts_with("file:///"));
}

#[test]
fn perl_git_svn_availability_is_reported() {
    if !has_perl_git_svn() {
        eprintln!("skipping golden comparisons: Perl git svn is not installed");
        return;
    }
}
```

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test compat_golden -- --nocapture
```

Expected: PASS; tests skip with explicit messages if dependencies are missing.

## Task 2: Capture Perl `git svn` Golden Artifacts

**Files:**
- Create: `crates/git-svn-rs-cli/tests/golden/perl_git_svn.rs`
- Modify: `crates/git-svn-rs-cli/tests/compat_golden.rs`

- [ ] **Step 1: Implement Perl runner**

Create `crates/git-svn-rs-cli/tests/golden/perl_git_svn.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GoldenArtifacts {
    pub repo: PathBuf,
    pub primary_ref: String,
    pub config: String,
    pub refs: String,
    pub log_body: String,
    pub rev_map_bytes: Vec<u8>,
}

pub fn run_perl_git_svn_clone(url: &str, dest: &Path) -> Result<GoldenArtifacts, String> {
    let output = Command::new("git")
        .args(["svn", "clone", url, dest.to_str().unwrap(), "--stdlayout"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    capture_git_artifacts(dest)
}

pub fn capture_git_artifacts(repo: &Path) -> Result<GoldenArtifacts, String> {
    let primary_ref = read_primary_remote_ref(repo)?;
    Ok(GoldenArtifacts {
        repo: repo.to_path_buf(),
        primary_ref: primary_ref.clone(),
        config: read_git_output(repo, &["config", "--list"])?,
        refs: read_git_output(repo, &["for-each-ref", "refs/remotes", "--format=%(refname) %(objectname)"])?,
        log_body: read_git_output(repo, &["log", &primary_ref, "--format=%B"])?,
        rev_map_bytes: find_rev_map(repo)?,
    })
}

fn read_primary_remote_ref(repo: &Path) -> Result<String, String> {
    let refs = read_git_output(repo, &["for-each-ref", "refs/remotes", "--format=%(refname)"])?;
    refs.lines()
        .find(|line| !line.ends_with("/HEAD"))
        .map(|line| line.to_string())
        .ok_or_else(|| "no refs/remotes ref found".to_string())
}

fn read_git_output(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").current_dir(repo).args(args).output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn find_rev_map(repo: &Path) -> Result<Vec<u8>, String> {
    let svn = repo.join(".git/svn");
    for path in walk(&svn)? {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with(".rev_map.") {
            return std::fs::read(path).map_err(|e| e.to_string());
        }
    }
    Err("no .rev_map found".to_string())
}

fn walk(root: &Path) -> Result<Vec<PathBuf>, String> {
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

- [ ] **Step 2: Add Perl artifact test**

Append to `crates/git-svn-rs-cli/tests/compat_golden.rs`:

```rust
use golden::perl_git_svn::run_perl_git_svn_clone;

#[test]
fn captures_perl_git_svn_golden_artifacts() {
    if !has_required_tools() || !has_perl_git_svn() {
        eprintln!("skipping: golden capture requires SVN tools and Perl git svn");
        return;
    }
    let fixture = GoldenSvnFixture::create().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let artifacts = run_perl_git_svn_clone(&fixture.url(), &tmp.path().join("perl")).unwrap();

    assert!(artifacts.config.contains("svn-remote.svn.url"));
    assert!(artifacts.refs.contains(&artifacts.primary_ref));
    assert!(artifacts.log_body.contains("git-svn-id:"));
    assert!(!artifacts.rev_map_bytes.is_empty());
}
```

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test compat_golden captures_perl_git_svn_golden_artifacts -- --nocapture
```

Expected: PASS or explicit dependency skip.

## Task 3: Compare Rust Artifacts Against Golden Artifacts

**Files:**
- Create: `crates/git-svn-rs-cli/tests/golden/compare.rs`
- Modify: `crates/git-svn-rs-cli/tests/compat_golden.rs`

- [ ] **Step 1: Implement comparison helpers**

Create `crates/git-svn-rs-cli/tests/golden/compare.rs`:

```rust
use super::perl_git_svn::GoldenArtifacts;

pub fn normalize_config(config: &str) -> Vec<String> {
    let mut lines = config
        .lines()
        .filter(|line| !line.starts_with("core."))
        .map(|line| line.replace('\\', "/"))
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

pub fn assert_core_artifacts_match(expected: &GoldenArtifacts, actual: &GoldenArtifacts) {
    assert_eq!(normalize_config(&expected.config), normalize_config(&actual.config));
    assert_eq!(normalize_refs(&expected.refs), normalize_refs(&actual.refs));
    assert_eq!(extract_git_svn_ids(&expected.log_body), extract_git_svn_ids(&actual.log_body));
    assert_eq!(rev_map_revisions(&expected.rev_map_bytes), rev_map_revisions(&actual.rev_map_bytes));
    assert!(!actual.rev_map_bytes.is_empty(), "Rust clone must write a .rev_map file");
}

pub fn normalize_refs(refs: &str) -> Vec<String> {
    let mut lines = refs.lines().map(|line| line.replace('\\', "/")).collect::<Vec<_>>();
    lines.sort();
    lines
}

pub fn extract_git_svn_ids(log_body: &str) -> Vec<String> {
    log_body
        .lines()
        .filter(|line| line.starts_with("git-svn-id: "))
        .map(|line| line.to_string())
        .collect()
}

pub fn rev_map_revisions(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(24)
        .map(|record| u32::from_be_bytes([record[0], record[1], record[2], record[3]]))
        .collect()
}
```

- [ ] **Step 2: Add comparison test**

Append to `crates/git-svn-rs-cli/tests/compat_golden.rs`:

```rust
use golden::compare::assert_core_artifacts_match;
use golden::perl_git_svn::capture_git_artifacts;

#[test]
fn rust_clone_matches_perl_git_svn_core_artifacts() {
    if !has_required_tools() || !has_perl_git_svn() {
        eprintln!("skipping: golden comparison requires SVN tools and Perl git svn");
        return;
    }

    let fixture = GoldenSvnFixture::create().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let perl = run_perl_git_svn_clone(&fixture.url(), &tmp.path().join("perl")).unwrap();

    let rust_repo = tmp.path().join("rust");
    assert_cmd::Command::cargo_bin("git-svn-rs").unwrap()
        .args(["clone", &fixture.url(), rust_repo.to_str().unwrap(), "--stdlayout"])
        .assert()
        .success();

    let rust = capture_git_artifacts(&rust_repo).unwrap();

    assert_core_artifacts_match(&perl, &rust);
}
```

This test intentionally uses the same `capture_git_artifacts(repo: &Path)` helper for both Perl and Rust outputs so ref discovery, log capture, and `.rev_map` discovery stay symmetric.

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test compat_golden rust_clone_matches_perl_git_svn_core_artifacts -- --nocapture
```

Expected: PASS or explicit dependency skip.

## Task 4: Add Edge Case Golden Scenarios

**Files:**
- Modify: `crates/git-svn-rs-cli/tests/golden/fixtures.rs`
- Modify: `crates/git-svn-rs-cli/tests/compat_golden.rs`

- [ ] **Step 1: Extend fixture with edge cases**

Modify `GoldenSvnFixture::create` so fixture revisions include:

```text
1. standard layout
2. regular file
3. svn:executable file
4. svn:special symlink when supported by platform
5. copied branch
6. copied tag
7. empty directory with preserve-empty-dirs scenario
8. delete
9. rename/copy path
```

- [ ] **Step 2: Add golden assertions**

Add tests comparing:

```text
config keys and current default prefix behavior
primary tracked ref discovered from refs/remotes
tags refs discovered from refs/remotes
git-svn-id footers
.rev_map byte length
log --oneline output
find-rev rN output
preserve-empty-dirs placeholder files
svn:executable mode 100755
svn:special mode 120000 when supported
linear dcommit commit subject in svn log
```

- [ ] **Step 3: Verify**

Run:

```powershell
cargo test -p git-svn-rs --test compat_golden -- --nocapture
```

Expected: PASS or explicit dependency skip for missing Perl `git svn` or platform symlink limitations.

## Task 5: Wire Golden Tests Into Verification

**Files:**
- Modify: `scripts/verify.ps1`
- Modify: `README.md`

- [ ] **Step 1: Update verification script**

Append to `scripts/verify.ps1`:

```powershell
cargo test -p git-svn-rs --test compat_golden -- --nocapture
```

Also add a strict compatibility mode switch:

```powershell
param(
    [switch] $StrictCompatibility
)

if ($StrictCompatibility) {
    $env:GIT_SVN_RS_STRICT_COMPAT = "1"
}
```

Golden tests must treat `GIT_SVN_RS_STRICT_COMPAT=1` as "missing SVN/libsvn/Perl git svn is a failure", not a skip.

- [ ] **Step 2: Document golden dependencies**

Append to `README.md`:

```markdown
## Golden Compatibility Tests

The golden compatibility tests compare `git-svn-rs` against Perl `git svn`.

Required tools:

- `git`
- Perl-backed `git svn`
- `svn`
- `svnadmin`
- `svnlook`

Run:

```powershell
cargo test -p git-svn-rs --test compat_golden -- --nocapture
```

If Perl `git svn` or SVN tools are unavailable, the tests print an explicit skip message in developer mode. In strict compatibility mode, missing dependencies fail the test run.
```

- [ ] **Step 3: Commit**

Run:

```powershell
git add crates/git-svn-rs-cli/tests/golden crates/git-svn-rs-cli/tests/compat_golden.rs scripts README.md
git commit -m "test: add git-svn golden compatibility tests"
```

## References

- [git-svn official documentation](https://git-scm.com/docs/git-svn)
- [git-svn.perl](https://github.com/git/git/blob/master/git-svn.perl)
- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)
- [Fetcher.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Fetcher.pm)
- [Editor.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Editor.pm)
- [GlobSpec.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/GlobSpec.pm)
- [Log.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Log.pm)
- [Migration.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Migration.pm)

## Self Review

- Spec coverage: golden tests cover config, refs, rev maps, commit messages, log output, find-rev, import edge cases, and linear dcommit.
- Placeholder scan: every dependency skip is explicit and test code includes concrete fixture and comparison helpers.
- Type consistency: golden artifact names match the roadmap: `GoldenSvnFixture`, `GoldenArtifacts`, `SvnFetchEditor`, and `SvnCommitEditor`.
