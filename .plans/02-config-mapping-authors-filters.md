# Config Mapping Authors Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement git-svn-compatible configuration parsing, SVN layout mapping, authors resolution, and path/ref filtering.

**Architecture:** Keep config and mapping pure and independent from Git/SVN process execution. `GlobSpec` and path/url utilities are required compatibility units, not later cleanup. Tests feed config text and CLI option structs into domain functions and assert exact `[svn-remote]` keys, ref names, path/url normalization, and filter decisions.

**Tech Stack:** Rust 1.95, `fancy-regex`, `url`, `tempfile`, core crate unit tests.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/config.rs`
- Create: `crates/git-svn-rs-core/src/mapping.rs`
- Create: `crates/git-svn-rs-core/src/glob_spec.rs`
- Create: `crates/git-svn-rs-core/src/path_url.rs`
- Create: `crates/git-svn-rs-core/src/authors.rs`
- Create: `crates/git-svn-rs-core/src/filters.rs`
- Test: `crates/git-svn-rs-core/tests/config_mapping.rs`
- Test: `crates/git-svn-rs-core/tests/authors_filters.rs`

## Task 1: Add Dependencies and Module Shells

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/git-svn-rs-core/Cargo.toml`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Create: `crates/git-svn-rs-core/src/config.rs`
- Create: `crates/git-svn-rs-core/src/mapping.rs`
- Create: `crates/git-svn-rs-core/src/authors.rs`
- Create: `crates/git-svn-rs-core/src/filters.rs`

- [ ] **Step 1: Add workspace dependencies**

Modify root `Cargo.toml` `[workspace.dependencies]`:

```toml
fancy-regex = "0.14"
url = "2"
```

Modify `crates/git-svn-rs-core/Cargo.toml`:

```toml
[dependencies]
clap.workspace = true
fancy-regex.workspace = true
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
pub mod mapping;
```

- [ ] **Step 3: Create typed shells**

Create `crates/git-svn-rs-core/src/config.rs`:

```rust
use crate::mapping::RefMapping;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnRemoteConfig {
    pub name: String,
    pub url: String,
    pub fetch: Vec<RefMapping>,
    pub branches: Vec<RefMapping>,
    pub tags: Vec<RefMapping>,
    pub ignore_paths: Option<String>,
    pub include_paths: Option<String>,
    pub ignore_refs: Option<String>,
}
```

Create `crates/git-svn-rs-core/src/mapping.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingKind {
    Fetch,
    Branches,
    Tags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMapping {
    pub kind: MappingKind,
    pub svn_path: String,
    pub git_ref: String,
}
```

Create `crates/git-svn-rs-core/src/authors.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}
```

Create `crates/git-svn-rs-core/src/filters.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterDecision {
    Include,
    Exclude,
}
```

- [ ] **Step 4: Verify compile fails only on unused warnings if any**

Run:

```powershell
cargo test -p git-svn-rs-core
```

Expected: PASS with no missing module errors.

## Task 2: Implement Layout Mapping

Compatibility note: the current `git-svn` manual says `--stdlayout` is shorthand for trunk/branches/tags, custom `--trunk/-T`, `--branches/-b`, and `--tags/-t` values take precedence, and the default prefix for trunk/branches/tags layouts is `origin/` unless the user explicitly passes `--prefix=""`. The exact ref shape must be captured by golden tests before command wiring; single-path imports may still use `refs/remotes/git-svn`.

**Files:**
- Modify: `crates/git-svn-rs-core/src/mapping.rs`
- Test: `crates/git-svn-rs-core/tests/config_mapping.rs`

- [ ] **Step 1: Write mapping tests**

Create `crates/git-svn-rs-core/tests/config_mapping.rs`:

```rust
use git_svn_rs_core::mapping::{build_standard_layout, build_single_path, MappingKind};

#[test]
fn standard_layout_uses_current_git_svn_default_refs() {
    let mappings = build_standard_layout("");

    assert_eq!(mappings.fetch[0].kind, MappingKind::Fetch);
    assert_eq!(mappings.fetch[0].svn_path, "trunk");
    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/origin/trunk");
    assert_eq!(mappings.branches[0].svn_path, "branches/*");
    assert_eq!(mappings.branches[0].git_ref, "refs/remotes/origin/*");
    assert_eq!(mappings.tags[0].svn_path, "tags/*");
    assert_eq!(mappings.tags[0].git_ref, "refs/remotes/origin/tags/*");
}

#[test]
fn prefix_is_applied_to_standard_layout_refs() {
    let mappings = build_standard_layout("svn/");

    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/svn/trunk");
    assert_eq!(mappings.branches[0].git_ref, "refs/remotes/svn/*");
    assert_eq!(mappings.tags[0].git_ref, "refs/remotes/svn/tags/*");
}

#[test]
fn single_path_tracks_git_svn_ref() {
    let mappings = build_single_path("");

    assert_eq!(mappings.fetch[0].svn_path, "");
    assert_eq!(mappings.fetch[0].git_ref, "refs/remotes/git-svn");
    assert!(mappings.branches.is_empty());
    assert!(mappings.tags.is_empty());
}
```

- [ ] **Step 2: Run mapping tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test config_mapping
```

Expected: FAIL because `build_standard_layout` and `build_single_path` are missing.

- [ ] **Step 3: Implement mapping builders**

Modify `crates/git-svn-rs-core/src/mapping.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingKind {
    Fetch,
    Branches,
    Tags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMapping {
    pub kind: MappingKind,
    pub svn_path: String,
    pub git_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutMappings {
    pub fetch: Vec<RefMapping>,
    pub branches: Vec<RefMapping>,
    pub tags: Vec<RefMapping>,
}

pub fn build_single_path(prefix: &str) -> LayoutMappings {
    LayoutMappings {
        fetch: vec![RefMapping {
            kind: MappingKind::Fetch,
            svn_path: String::new(),
            git_ref: format!("refs/remotes/{prefix}git-svn"),
        }],
        branches: Vec::new(),
        tags: Vec::new(),
    }
}

pub fn build_standard_layout(prefix: &str) -> LayoutMappings {
    let prefix = if prefix.is_empty() { "origin/" } else { prefix };
    build_standard_layout_with_prefix(prefix)
}

fn build_standard_layout_with_prefix(prefix: &str) -> LayoutMappings {
    LayoutMappings {
        fetch: vec![RefMapping {
            kind: MappingKind::Fetch,
            svn_path: "trunk".to_string(),
            git_ref: format!("refs/remotes/{prefix}trunk"),
        }],
        branches: vec![RefMapping {
            kind: MappingKind::Branches,
            svn_path: "branches/*".to_string(),
            git_ref: format!("refs/remotes/{prefix}*"),
        }],
        tags: vec![RefMapping {
            kind: MappingKind::Tags,
            svn_path: "tags/*".to_string(),
            git_ref: format!("refs/remotes/{prefix}tags/*"),
        }],
    }
}

pub fn build_from_layout_args(
    stdlayout: bool,
    trunk: Option<&str>,
    branches: &[String],
    tags: &[String],
    prefix: Option<&str>,
) -> Result<LayoutMappings, String> {
    let prefix = prefix.unwrap_or(if stdlayout || trunk.is_some() || !branches.is_empty() || !tags.is_empty() {
        "origin/"
    } else {
        ""
    });
    if stdlayout && trunk.is_none() && branches.is_empty() && tags.is_empty() {
        return Ok(build_standard_layout_with_prefix(prefix));
    }
    if trunk.is_some() || !branches.is_empty() || !tags.is_empty() {
        let mut mappings = LayoutMappings { fetch: Vec::new(), branches: Vec::new(), tags: Vec::new() };
        mappings.fetch.push(RefMapping {
            kind: MappingKind::Fetch,
            svn_path: trunk.unwrap_or("trunk").trim_matches('/').to_string(),
            git_ref: format!("refs/remotes/{prefix}trunk"),
        });
        for branch in branches {
            mappings.branches.push(RefMapping {
                kind: MappingKind::Branches,
                svn_path: branch.trim_matches('/').to_string(),
                git_ref: format!("refs/remotes/{prefix}*"),
            });
        }
        for tag in tags {
            mappings.tags.push(RefMapping {
                kind: MappingKind::Tags,
                svn_path: tag.trim_matches('/').to_string(),
                git_ref: format!("refs/remotes/{prefix}tags/*"),
            });
        }
        return Ok(mappings);
    }
    Ok(build_single_path(prefix))
}
```

- [ ] **Step 4: Verify mapping tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test config_mapping
```

Expected: PASS.

## Task 3: Implement Config Serialization

**Files:**
- Modify: `crates/git-svn-rs-core/src/config.rs`
- Modify: `crates/git-svn-rs-core/tests/config_mapping.rs`

- [ ] **Step 1: Add config serialization test**

Append to `crates/git-svn-rs-core/tests/config_mapping.rs`:

```rust
use git_svn_rs_core::config::SvnRemoteConfig;

#[test]
fn serializes_svn_remote_config_keys() {
    let mappings = build_standard_layout("svn/");
    let config = SvnRemoteConfig::new("svn", "file:///repo", mappings)
        .with_ignore_paths("^vendor/")
        .with_include_paths("^(trunk|branches/main)/");

    let entries = config.to_git_config_entries();

    assert!(entries.contains(&(
        "svn-remote.svn.url".to_string(),
        "file:///repo".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.fetch".to_string(),
        "trunk:refs/remotes/svn/trunk".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.branches".to_string(),
        "branches/*:refs/remotes/svn/*".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.tags".to_string(),
        "tags/*:refs/remotes/svn/tags/*".to_string()
    )));
    assert!(entries.contains(&(
        "svn-remote.svn.ignore-paths".to_string(),
        "^vendor/".to_string()
    )));
}
```

- [ ] **Step 2: Run test and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test config_mapping serializes_svn_remote_config_keys
```

Expected: FAIL because constructor and serialization are missing.

- [ ] **Step 3: Implement serialization**

Modify `crates/git-svn-rs-core/src/config.rs`:

```rust
use crate::mapping::{LayoutMappings, RefMapping};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvnRemoteConfig {
    pub name: String,
    pub url: String,
    pub fetch: Vec<RefMapping>,
    pub branches: Vec<RefMapping>,
    pub tags: Vec<RefMapping>,
    pub ignore_paths: Option<String>,
    pub include_paths: Option<String>,
    pub ignore_refs: Option<String>,
}

impl SvnRemoteConfig {
    pub fn new(name: impl Into<String>, url: impl Into<String>, mappings: LayoutMappings) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            fetch: mappings.fetch,
            branches: mappings.branches,
            tags: mappings.tags,
            ignore_paths: None,
            include_paths: None,
            ignore_refs: None,
        }
    }

    pub fn with_ignore_paths(mut self, value: impl Into<String>) -> Self {
        self.ignore_paths = Some(value.into());
        self
    }

    pub fn with_include_paths(mut self, value: impl Into<String>) -> Self {
        self.include_paths = Some(value.into());
        self
    }

    pub fn to_git_config_entries(&self) -> Vec<(String, String)> {
        let prefix = format!("svn-remote.{}", self.name);
        let mut entries = vec![(format!("{prefix}.url"), self.url.clone())];
        entries.extend(self.fetch.iter().map(|m| {
            (format!("{prefix}.fetch"), format!("{}:{}", m.svn_path, m.git_ref))
        }));
        entries.extend(self.branches.iter().map(|m| {
            (format!("{prefix}.branches"), format!("{}:{}", m.svn_path, m.git_ref))
        }));
        entries.extend(self.tags.iter().map(|m| {
            (format!("{prefix}.tags"), format!("{}:{}", m.svn_path, m.git_ref))
        }));
        if let Some(value) = &self.ignore_paths {
            entries.push((format!("{prefix}.ignore-paths"), value.clone()));
        }
        if let Some(value) = &self.include_paths {
            entries.push((format!("{prefix}.include-paths"), value.clone()));
        }
        if let Some(value) = &self.ignore_refs {
            entries.push((format!("{prefix}.ignore-refs"), value.clone()));
        }
        entries
    }
}
```

- [ ] **Step 4: Verify config tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test config_mapping
```

Expected: PASS.

## Task 4: Implement Authors and Filters

**Files:**
- Modify: `crates/git-svn-rs-core/src/authors.rs`
- Modify: `crates/git-svn-rs-core/src/filters.rs`
- Test: `crates/git-svn-rs-core/tests/authors_filters.rs`

- [ ] **Step 1: Write authors and filter tests**

Create `crates/git-svn-rs-core/tests/authors_filters.rs`:

```rust
use git_svn_rs_core::authors::{parse_authors_file, AuthorResolver};
use git_svn_rs_core::filters::{FilterDecision, PathFilters};

#[test]
fn parses_authors_file_lines() {
    let resolver = parse_authors_file("jdoe = Jane Doe <jane@example.com>\nsvc = Service <>\n").unwrap();

    assert_eq!(resolver.resolve("jdoe").unwrap().name, "Jane Doe");
    assert_eq!(resolver.resolve("jdoe").unwrap().email, "jane@example.com");
    assert_eq!(resolver.resolve("svc").unwrap().email, "");
}

#[test]
fn missing_author_returns_none() {
    let resolver = parse_authors_file("jdoe = Jane Doe <jane@example.com>\n").unwrap();
    assert!(resolver.resolve("unknown").is_none());
}

#[test]
fn filters_support_perl_style_negative_lookahead() {
    let filters = PathFilters::new(Some("^trunk/(?!vendor/)".to_string()), None).unwrap();

    assert_eq!(filters.decide("trunk/src/lib.rs").unwrap(), FilterDecision::Include);
    assert_eq!(filters.decide("trunk/vendor/lib.c").unwrap(), FilterDecision::Exclude);
}
```

- [ ] **Step 2: Run tests and see failure**

Run:

```powershell
cargo test -p git-svn-rs-core --test authors_filters
```

Expected: FAIL because parsing and filtering functions are missing.

- [ ] **Step 3: Implement authors parser**

Modify `crates/git-svn-rs-core/src/authors.rs`:

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorResolver {
    by_login: BTreeMap<String, Author>,
}

impl AuthorResolver {
    pub fn resolve(&self, login: &str) -> Option<&Author> {
        self.by_login.get(login)
    }
}

pub fn parse_authors_file(input: &str) -> Result<AuthorResolver, String> {
    let mut resolver = AuthorResolver::default();
    for (idx, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (login, rest) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid authors line {}: missing '='", idx + 1))?;
        let rest = rest.trim();
        let start = rest
            .rfind('<')
            .ok_or_else(|| format!("invalid authors line {}: missing '<'", idx + 1))?;
        let end = rest
            .rfind('>')
            .ok_or_else(|| format!("invalid authors line {}: missing '>'", idx + 1))?;
        if end < start {
            return Err(format!("invalid authors line {}: malformed email", idx + 1));
        }
        resolver.by_login.insert(
            login.trim().to_string(),
            Author {
                name: rest[..start].trim().to_string(),
                email: rest[start + 1..end].trim().to_string(),
            },
        );
    }
    Ok(resolver)
}
```

- [ ] **Step 4: Implement filters with `fancy-regex`**

Modify `crates/git-svn-rs-core/src/filters.rs`:

```rust
use fancy_regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterDecision {
    Include,
    Exclude,
}

pub struct PathFilters {
    include: Option<Regex>,
    ignore: Option<Regex>,
}

impl PathFilters {
    pub fn new(include: Option<String>, ignore: Option<String>) -> Result<Self, String> {
        Ok(Self {
            include: include.map(|p| Regex::new(&p)).transpose().map_err(|e| e.to_string())?,
            ignore: ignore.map(|p| Regex::new(&p)).transpose().map_err(|e| e.to_string())?,
        })
    }

    pub fn decide(&self, path: &str) -> Result<FilterDecision, String> {
        if let Some(ignore) = &self.ignore {
            if ignore.is_match(path).map_err(|e| e.to_string())? {
                return Ok(FilterDecision::Exclude);
            }
        }
        if let Some(include) = &self.include {
            return if include.is_match(path).map_err(|e| e.to_string())? {
                Ok(FilterDecision::Include)
            } else {
                Ok(FilterDecision::Exclude)
            };
        }
        Ok(FilterDecision::Include)
    }
}
```

- [ ] **Step 5: Verify tests pass**

Run:

```powershell
cargo test -p git-svn-rs-core --test authors_filters
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add Cargo.toml crates/git-svn-rs-core
git commit -m "feat: add svn config mapping and filters"
```

## Self Review

- Spec coverage: standard layout, custom prefixes, config entries, authors files, and Perl-style filtering are covered.
- Placeholder scan: every data type and test used in this phase is defined here.
- Type consistency: `SvnRemoteConfig`, `RefMapping`, `AuthorResolver`, and `PathFilters` are reused by later plans.

## Required Compatibility Core Tasks From Roadmap

Tasks in this section are part of the Phase 2 acceptance gate. Do not treat them as optional optimization work.

### Task 5: Replace Ad Hoc Mapping With `GlobSpec`

**Files:**
- Create: `crates/git-svn-rs-core/src/glob_spec.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/mapping.rs`
- Test: `crates/git-svn-rs-core/tests/glob_spec.rs`
- Test: `crates/git-svn-rs-core/tests/config_mapping.rs`

- [ ] **Step 1: Add `GlobSpec.pm` golden tests**

Create `crates/git-svn-rs-core/tests/glob_spec.rs`:

```rust
use git_svn_rs_core::glob_spec::GlobSpec;

#[test]
fn parses_single_star_glob_like_git_svn() {
    let spec = GlobSpec::new("branches/*", true).unwrap();
    assert_eq!(spec.left(), "branches");
    assert_eq!(spec.right(), "");
    assert_eq!(spec.depth(), 1);
    assert_eq!(spec.full_path("main"), "branches/main");
    assert!(spec.is_match("branches/main"));
    assert!(!spec.is_match("branches/main/nested"));
}

#[test]
fn supports_brace_pattern_when_pattern_ok() {
    let spec = GlobSpec::new("branches/{stable,release}", true).unwrap();
    assert_eq!(spec.left(), "branches");
    assert_eq!(spec.depth(), 1);
    assert!(spec.is_match("branches/stable"));
    assert!(spec.is_match("branches/release"));
    assert!(!spec.is_match("branches/main"));
}

#[test]
fn rejects_multiple_wildcard_groups() {
    let err = GlobSpec::new("branches/*/teams/*", true).unwrap_err();
    assert!(err.contains("Only one set of wildcards"));
}

#[test]
fn rejects_more_than_one_star_in_one_segment() {
    let err = GlobSpec::new("branches/**", true).unwrap_err();
    assert!(err.contains("Only one '*' is allowed"));
}
```

- [ ] **Step 2: Implement `GlobSpec` with the Perl constraints**

Create `crates/git-svn-rs-core/src/glob_spec.rs`:

```rust
use fancy_regex::Regex;

#[derive(Debug, Clone)]
pub struct GlobSpec {
    left: String,
    right: String,
    depth: usize,
    regex: Regex,
    glob: String,
}

impl GlobSpec {
    pub fn new(glob: &str, pattern_ok: bool) -> Result<Self, String> {
        let glob = glob.trim_end_matches('/').to_string();
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut patterns = Vec::new();
        let mut state = "left";
        let die_msg = format!("Only one set of wildcards (e.g. '*' or '*/*/*') is supported: {glob}");
        for part in glob.split('/') {
            if pattern_ok && part.contains(['{', '}']) && !(part.starts_with('{') && part.ends_with('}')) {
                return Err(format!("Invalid pattern in '{glob}': {part}"));
            }
            let stars = part.matches('*').count();
            if stars > 1 {
                return Err(format!("Only one '*' is allowed in a pattern: '{part}'"));
            }
            if let Some(pos) = part.find('*') {
                if state == "right" {
                    return Err(die_msg);
                }
                state = "pattern";
                let l = &part[..pos];
                let r = &part[pos + 1..];
                patterns.push(format!("{}[^/]*{}", regex::escape(l), regex::escape(r)));
            } else if pattern_ok && part.starts_with('{') && part.ends_with('}') {
                if state == "right" {
                    return Err(die_msg);
                }
                state = "pattern";
                let inner = &part[1..part.len() - 1];
                let alternatives = inner.split(',').map(regex::escape).collect::<Vec<_>>().join("|");
                patterns.push(format!("(?:{alternatives})"));
            } else if state == "left" {
                left.push(part.to_string());
            } else {
                state = "right";
                right.push(part.to_string());
            }
        }
        if patterns.is_empty() {
            return Err(format!("One '*' is needed in glob: '{glob}'"));
        }
        let left = left.join("/");
        let right = right.join("/");
        let middle = patterns.join("/");
        let mut pieces = Vec::new();
        if !left.is_empty() {
            pieces.push(regex::escape(&left));
        }
        pieces.push(format!("({middle})(?=/|$)"));
        if !right.is_empty() {
            pieces.push(regex::escape(&right));
        }
        let regex = Regex::new(&format!("^{}$", pieces.join("/"))).map_err(|e| e.to_string())?;
        Ok(Self { left, right, depth: patterns.len(), regex, glob })
    }

    pub fn left(&self) -> &str { &self.left }
    pub fn right(&self) -> &str { &self.right }
    pub fn depth(&self) -> usize { self.depth }
    pub fn glob(&self) -> &str { &self.glob }

    pub fn full_path(&self, path: &str) -> String {
        match (self.left.is_empty(), self.right.is_empty()) {
            (true, true) => path.to_string(),
            (false, true) => format!("{}/{}", self.left, path),
            (true, false) => format!("{}/{}", path, self.right),
            (false, false) => format!("{}/{}/{}", self.left, path, self.right),
        }
    }

    pub fn is_match(&self, path: &str) -> bool {
        self.regex.is_match(path).unwrap_or(false)
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod glob_spec;
```

- [ ] **Step 3: Route branch/tag mappings through `GlobSpec`**

Modify `crates/git-svn-rs-core/src/mapping.rs` so branch and tag mappings store or validate a `GlobSpec` before producing Git ref mappings. Existing `build_standard_layout` and `build_from_layout_args` tests must still pass, custom `--trunk/--branches/--tags` values must override `--stdlayout`, and invalid multi-wildcard layouts must fail before writing Git config.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test glob_spec
cargo test -p git-svn-rs-core --test config_mapping
```

Expected: PASS.

### Task 6: Add Path/URL Utilities and Filter Precedence

**Files:**
- Create: `crates/git-svn-rs-core/src/path_url.rs`
- Modify: `crates/git-svn-rs-core/src/lib.rs`
- Modify: `crates/git-svn-rs-core/src/filters.rs`
- Test: `crates/git-svn-rs-core/tests/path_url.rs`
- Test: `crates/git-svn-rs-core/tests/authors_filters.rs`

- [ ] **Step 1: Add `Utils.pm` compatibility tests**

Create `crates/git-svn-rs-core/tests/path_url.rs`:

```rust
use git_svn_rs_core::path_url::{add_path_to_url, canonicalize_path, canonicalize_url, join_paths};

#[test]
fn canonicalize_path_collapses_dotdot_and_slashes() {
    assert_eq!(canonicalize_path("./trunk/../branches//main/"), "branches/main");
}

#[test]
fn canonicalize_url_preserves_scheme_and_host() {
    assert_eq!(
        canonicalize_url("https://svn.example/repo/./trunk/../branches/main/"),
        "https://svn.example/repo/branches/main"
    );
}

#[test]
fn joins_non_empty_path_segments() {
    assert_eq!(join_paths(["", "trunk", "src", ""]), "trunk/src");
}

#[test]
fn adds_path_to_url_without_double_slashes() {
    assert_eq!(add_path_to_url("https://svn.example/repo/", "/trunk"), "https://svn.example/repo/trunk");
}
```

- [ ] **Step 2: Implement path/url helpers**

Create `crates/git-svn-rs-core/src/path_url.rs`:

```rust
pub fn canonicalize_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.replace('\\', "/").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn canonicalize_url(url: &str) -> String {
    if let Some((scheme_host, path)) = url.split_once("://") {
        if let Some((host, rest)) = path.split_once('/') {
            return format!("{scheme_host}://{host}/{}", canonicalize_path(rest));
        }
    }
    canonicalize_path(url)
}

pub fn join_paths<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    canonicalize_path(&parts.into_iter().filter_map(|p| {
        let p = p.as_ref().trim_matches('/');
        (!p.is_empty()).then(|| p.to_string())
    }).collect::<Vec<_>>().join("/"))
}

pub fn add_path_to_url(url: &str, path: &str) -> String {
    let base = url.trim_end_matches('/');
    let path = canonicalize_path(path.trim_start_matches('/'));
    if path.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{path}")
    }
}
```

Modify `crates/git-svn-rs-core/src/lib.rs`:

```rust
pub mod path_url;
```

- [ ] **Step 3: Lock filter precedence**

Append to `crates/git-svn-rs-core/tests/authors_filters.rs`:

```rust
#[test]
fn filters_always_reject_dot_git_paths() {
    let filters = PathFilters::new(None, None).unwrap();
    assert_eq!(filters.decide("trunk/.git/config").unwrap(), FilterDecision::Exclude);
}

#[test]
fn ignore_wins_over_include() {
    let filters = PathFilters::new(Some("^trunk/".to_string()), Some("^trunk/vendor/".to_string())).unwrap();
    assert_eq!(filters.decide("trunk/vendor/lib.c").unwrap(), FilterDecision::Exclude);
    assert_eq!(filters.decide("trunk/src/lib.rs").unwrap(), FilterDecision::Include);
}
```

Modify `PathFilters::decide` so `.git` paths are excluded before regex checks and ignore regex wins over include regex.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p git-svn-rs-core --test path_url
cargo test -p git-svn-rs-core --test authors_filters
```

Expected: PASS.

## References

- [git-svn official documentation](https://git-scm.com/docs/git-svn)
- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [GlobSpec.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/GlobSpec.pm)
- [Utils.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Utils.pm)
