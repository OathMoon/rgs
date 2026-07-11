# git-svn-rs Core Compatibility Plan v2

## Authority and Purpose

This file is the product-level contract for `git-svn-rs`. The architecture and phase order are defined in `.plans/00-git-svn-rs-review-and-roadmap.md`; implementation handoff status is recorded in `.plans/implementation-progress-record.md`.

The 2026-07-10 review in `.plans/git-svn-rs-plan-code-architecture-review-2026-07-10.md` supersedes earlier completion claims and long task recipes. A phase is complete only when its gate in the current phase plan passes; commit count, test count, parse-only CLI coverage, and dependency-skipped tests are not completion evidence.

## Product Goal

Deliver a Rust implementation of the supported core `git svn` workflow:

- read: `init`, `clone`, `fetch`, `rebase`, `log`, `info`, `find-rev`, `gc`, and `reset`;
- write: safe linear `dcommit` with `--dry-run`, `--no-rebase`, `--commit-url`, and explicit `--mergeinfo`;
- compatibility: Git config, `git-svn-id`, `.git/svn/**/.rev_map.*`, refs, commit graph, working-tree state, and user-visible output for the declared profiles;
- packaging: `git-svn-rs` by default and an opt-in `git-svn` shim.

The first compatibility release is not required to implement historical non-core commands such as branch/tag write-back, `set-tree`, `commit-diff`, property-editing commands, or automatic mergeinfo generation.

## Frozen Compatibility Baseline

The normative upstream baseline is Git `v2.54.0`, commit [`0b13e48a3a30cdfa94e8ef842e24d6045ab3d015`](https://github.com/git/git/tree/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015).

Normative sources are pinned to that commit:

- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)
- [`git-svn.perl`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/git-svn.perl)
- [`perl/Git.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git.pm)
- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`Ra.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Ra.pm)
- [`Fetcher.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Fetcher.pm)
- [`Editor.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Editor.pm)
- [`Log.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Log.pm)
- [`Migration.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Migration.pm)
- [`Prompt.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Prompt.pm)
- [`Utils.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Utils.pm)
- [`GlobSpec.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/GlobSpec.pm)

The current [git-svn documentation](https://git-scm.com/docs/git-svn.html) and Git `master` are forward-compatibility observation sources only. Differences from the frozen baseline require a recorded compatibility decision and dedicated test.

## Status Vocabulary

Every phase and protocol profile uses one of these states:

- `not-started`: no usable implementation exists;
- `in-progress`: implementation is changing and no gate may be claimed;
- `structural-pass`: types, parsing, and isolated units exist, but user behavior is not proven;
- `behavior-pass`: declared local/integration scenarios pass without hiding semantic differences;
- `release-pass`: strict upstream comparison and every required profile pass without skips.

`complete`, `supported`, and `compatible` must not be used without naming the corresponding state and profile.

## Compatibility Profiles

| Profile | First-release target | Current status source |
|---|---|---|
| `file://` through SVN CLI | `release-pass` for single-path and standard layout read/write | progress record and Phase 5/7/8 gates |
| local `svn://` through SVN CLI | `release-pass` for explicit credentials and linear read/write | progress record and Phase 5/7/8 gates |
| linked libsvn over `file://` and local `svn://` | `behavior-pass` for RA/delta read; write requires an explicit later gate | Phase 4/5 |
| `http://` and `https://` | no general support claim until authenticated read/write profiles reach `behavior-pass`; first general compatibility release requires a documented decision | Phase 4/7 |
| `svn+ssh://` | deferred unless a repeatable fixture is added; accepted-but-unvalidated is not support | Phase 4/7 |
| `mock://` | test infrastructure only | never a release profile |

An unsupported or deferred combination must fail explicitly. Silently accepting an option or URL scheme is forbidden.

## Core Compatibility Contract

### CLI and configuration

- Every parsed option is implemented, explicitly rejected, or documented as deferred; no inert option is allowed.
- Configuration precedence is centralized and tested: command line, selected `[svn-remote]`, then global Git config where the upstream baseline uses it.
- Multiple values, full layout URLs, prefixes, authors, filters, metadata modes, auth, and revision forms follow the frozen baseline.

### Git object identity

- SVN author, full message, timestamp, UTC/local offset, parent graph, tree, modes, and `git-svn-id` determine the same normalized commit identity as the baseline.
- `clone` creates the expected local branch, resolves `HEAD`, and populates the working tree unless `--no-checkout` is set.
- Golden tests compare object IDs or a complete graph fingerprint; replacing IDs with `<commit>` is not sufficient.

### Fetch architecture

- `get_log` discovers bounded revision windows; `do_update`/`do_switch` drive the sole `SvnFetchEditor` behavior model.
- SVN CLI and libsvn may be different transport adapters, but neither may bypass the common editor contract to write Git changes directly.
- The contract includes copy/delete/modify, base/result checksums, absent paths, properties, path encoding, persistent empty-directory placeholders, filters, and follow-parent behavior.

### Dcommit safety

- The target is resolved from the nearest first-parent `git-svn-id`/rev_map identity and validated against remote, UUID, repository root, and SVN path.
- Ambiguous targets fail closed. Dirty worktrees, unsupported merge topology, and stale upstream state are rejected before the first SVN write.
- Every production adapter executes the same `DcommitPlan`; full commit messages and property deletions are preserved.
- Partial SVN success is recorded so fetch/rebase can resume without duplicate submission.

### Metadata consistency

- Read-only open and create are separate operations.
- Ref updates and rev_map changes have a documented ordering, compare-and-swap precondition, validation, and recovery path.
- Legacy metadata is migrated with backup/warning or rejected explicitly; inspection alone is not migration compatibility.

### Native safety

- No Rust panic or `unwrap()` may cross an `extern "C"` callback.
- Callback state belongs to the operation baton, not process-global mutable recorders.
- Unsafe APR/libsvn code is isolated from domain orchestration and covered by linked tests that pass under the default parallel harness.

## Architecture Constraints

- Keep the three-crate workspace: CLI, core, and opt-in shim.
- Use Git CLI plumbing through one tested wrapper; do not scatter `Command::new("git")` through production modules.
- Keep one remote/config resolver, one rev_map path implementation, one fetch behavior model, and one dcommit plan model.
- Use structured error categories for auth, unsupported capability, ambiguity, metadata corruption, partial write, and external command failure.
- Do not introduce an abstraction without at least two real implementations or a demonstrated safety/test boundary.

## Execution Order

1. Phase 1–3 contract corrections: CLI semantics, URL/config model, metadata state and resolver foundations.
2. Phase 5 correctness closure: single-path clone, real timestamps, checkout, revision options, exact object tests.
3. Phase 4/5 fetch unification: production native delta, CLI adapter convergence, complete Fetcher semantics.
4. Phase 7 safe dcommit: first-parent target, clean/linear preflight, shared plan, recovery.
5. Phase 6 metadata/read-only completion.
6. Phase 8 non-skippable strict release gate.

P0 correctness work takes precedence over additional callback, protocol, property, or command breadth.

## Verification Policy

- Developer gate: formatting, lint, unit/default workspace tests; missing external tools may be an explicit skip only here.
- Backend gate: required SVN CLI/libsvn fixtures actually run; a filtered-out or skipped test is not a pass.
- Release gate: Perl `git svn`, SVN CLI, linked libsvn, and required protocol profiles are present; no compatibility scenario may skip.
- Verification output records tool versions, frozen Git commit, executed scenarios, skipped scenarios, elapsed time, and artifact location.

## Release Definition

The first compatibility release requires all of the following:

- every Phase 1–8 behavioral gate passes;
- Phase 8 reaches `release-pass` against the frozen baseline;
- single-path and standard-layout clone/fetch compare exact commit graphs and working-tree state;
- safe linear dcommit passes wrong-target, dirty-tree, full-message, property, partial-failure, fetch, and rebase scenarios;
- required profile rows are explicitly marked `release-pass` or removed from the public support claim;
- documentation states all deferred schemes, commands, options, migration limits, and auth limits.

Current implementation status is intentionally not summarized here; use `.plans/implementation-progress-record.md` so this contract remains stable.
