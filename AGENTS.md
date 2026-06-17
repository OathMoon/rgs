# AGENTS.md

## Project Overview

This repository is a Rust workspace for `git-svn-rs`, a staged Rust implementation and compatibility path for core `git svn` workflows.

Workspace members:

- `crates/git-svn-rs-cli`: primary `git-svn-rs` CLI binary and end-to-end CLI tests.
- `crates/git-svn-rs-core`: core implementation for CLI parsing, config/mapping, authors and filters, Git metadata, rev_maps, SVN backends, import/replay, readonly commands, dcommit foundations, diagnostics, and golden compatibility tests.
- `crates/git-svn-rs-shim`: thin `git-svn` compatibility shim that forwards to `git-svn-rs`.

Planning and handoff documents live under `.plans/`.

# Important Guidelines

- always read the current `.plans/implementation-progress-record.md` before continuing plan execution.
- always follow the Karpathy guidelines in `.ai/guildlines.md` for LLM coding and implementation work.

## Progress Record

The current condensed implementation progress record is:

- `.plans/implementation-progress-record.md`

Read this file before continuing plan execution. It records the current branch/base, recent completed work, per-phase status, remaining scope, verification evidence, and recommended next steps.

When updating progress:

- Keep the progress record concise and handoff-oriented.
- Preserve current status, completed capabilities, remaining work, verification commands, and important commit anchors.
- Avoid long per-test history such as "failed before implementation, then passed"; summarize the final verified behavior instead.
- If a new dated progress record is needed, use the same naming pattern: `.plans/implementation-progress-record-YYYY-MM-DD.md`.

## Key Plan Files

- `.plans/git-svn-rs-plan.md`: top-level implementation plan.
- `.plans/00-git-svn-rs-review-and-roadmap.md`: review and roadmap context.
- `.plans/01-foundation-cli-workspace.md` through `.plans/08-compatibility-golden-tests.md`: phase plans.

## Development Guidelines

- Follow the existing Rust workspace structure and local module boundaries.
- Keep implementation changes scoped to the relevant phase or command path.
- Prefer existing helpers and patterns in `git-svn-rs-core` over adding parallel abstractions.
- Do not silently change compatibility behavior; add or update focused tests when changing git-svn semantics.
- Fixtures and golden compatibility tests may skip when external Subversion or Perl `git-svn` tools are unavailable unless strict compatibility mode is enabled.
- The `svn-libsvn` feature currently compiles a not-linked backend shell; default builds do not require libsvn.

## Verification

Useful commands from the repository root:

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs-core --features svn-libsvn`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1`

For strict compatibility checks where external tools are expected:

- Set `GIT_SVN_RS_STRICT_COMPAT=1`, or
- Run `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1 -StrictCompat`

## Current High-Level Scope

- Completed foundation: CLI workspace, config/mapping/authors/filters, Git metadata, rev_map primitives, SVN abstractions, fixtures, mock import, and much of SVN CLI replay.
- Implemented supported flows: local `file://` and local `svn://` clone/fetch replay coverage, readonly commands for supported metadata/rev_map layouts, local/mock dcommit write-back, and broad golden artifact comparisons.
- Remaining major work: real `svn-libsvn` backend integration, remote SVN/libsvn dcommit write-back, broader remote replay validation, full `Log.pm` compatibility modes, and stricter Rust-vs-Perl golden comparisons.
