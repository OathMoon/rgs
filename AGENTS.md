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
- `.plans/09-release-hardening-and-quality.md` and `.plans/10-maintainability-and-package-readiness.md`: completed release hardening and package-readiness scope.

## Development Guidelines

- Follow the existing Rust workspace structure and local module boundaries.
- Keep implementation changes scoped to the relevant phase or command path.
- Prefer existing helpers and patterns in `git-svn-rs-core` over adding parallel abstractions.
- Do not silently change compatibility behavior; add or update focused tests when changing git-svn semantics.
- Fixtures and golden compatibility tests may skip when external Subversion or Perl `git-svn` tools are unavailable unless strict compatibility mode is enabled.
- The `svn-libsvn` feature provides a production linked RA/delta read/import backend when platform libraries are found; otherwise it reports `not-linked`. Default builds do not require libsvn. Dcommit still writes through the SVN CLI working-copy sink, including in linked builds.

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

- Phases 1–10 are complete for the declared v1 profiles, including exact frozen Perl comparisons, linked read/import, durable dcommit recovery, module splits, public API boundaries, and isolated package verification.
- Implemented profiles include CLI local `file://` and authenticated `svn://` read/write, linked read/import, and configured/loopback SSH and authenticated HTTP/HTTPS DAV fixtures. These are not general remote/platform support claims.
- Last proven protected release SHA: `ffb2e22` (run #33155854101). Follow-up `4b49af6` passes Windows #23 and Developer gate #11; the Windows strict step was skipped. A later release candidate needs its own protected same-SHA artifact.
- Local 2026-08-31 working-tree property repairs pass all nine Windows native/linked and all nine WSL strict/linked gates; the WSL default run supplies 41 golden tests and eight required summaries. These source-fingerprinted local results do not replace protected release evidence. See the progress record and dated review.
- Deferred scope: native libsvn write-back, broader remote/platform validation, automatic legacy metadata migration, full `Log.pm`, and non-core commands. Package readiness does not imply a registry publish or tag.
