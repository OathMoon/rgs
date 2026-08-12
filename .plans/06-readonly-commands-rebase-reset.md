# Phase 6: Readonly Queries, Rebase, Reset, and GC

## Objective

Implement `find-rev`, `info`, `log`, `gc`, `reset`, and `rebase` on top of one validated tracking identity and the frozen `Git::SVN::Log` behavior.

## Current State

State: `behavior-pass` for the documented readonly and maintenance subset.

Commands resolve one validated named-remote identity and fail closed on mapping,
rev_map, UUID, or pending-transaction ambiguity. Covered `find-rev`, `info`,
Log.pm formatting/ranges/pathspecs, `reset`, `rebase`, and `gc` behavior has focused
and frozen-artifact coverage. Full Log.pm compatibility and automatic legacy
migration remain explicit deferred scope.

## Normative References

- [`Log.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Log.pm)
- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`Migration.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Migration.pm)
- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)

## Shared Query Context

Every command begins from the Phase 3 tracking identity:

- selected remote name;
- canonical repository root and UUID;
- mapping path and tracking ref;
- validated rev_map path/records;
- current HEAD/optional tree-ish relationship;
- metadata mode and recovery state.

Commands must not duplicate config parsing, recursively aggregate unrelated rev_maps, or create metadata during reads.

## Required Work

### 1. Correct `find-rev` scope and selection

- `find-rev rN` searches the validated current tracking identity.
- `find-rev rN <tree-ish>` scopes to the supplied tree-ish as the baseline specifies.
- commit-to-revision resolves only through the matching ref/UUID identity.
- `--before` and `--after` traverse the relevant branch state; equal revision numbers on other refs are irrelevant.
- zero rev_map records and missing matches produce baseline-compatible empty/error output.
- ambiguous identity is an error, not path-order selection.

### 2. Complete `info`

- Report URL, repository root, UUID, and revision from the validated identity.
- `--url` emits only the URL.
- Handle branch/tag wildcard mappings and repository subpath sessions.
- Local commits after the tracked ref do not change the reported SVN identity.
- Missing, ambiguous, corrupt, or incomplete metadata yields distinct non-mutating errors.

### 3. Complete `log` selection and formatting

Implement the frozen baseline behavior for:

- default, `--oneline`, `--incremental`, `--show-commit`, and `--verbose`;
- numeric revision, forward/reverse ranges, and limit semantics;
- pathspec pass-through and changed-path rendering;
- date/time formatting using imported SVN identity;
- full/empty/multiline messages and final footer recognition;
- merged/excluded commit counting rules where in v1 scope;
- author mapping/use-log-author options only if declared by the CLI contract.

Formatting tests compare meaningful lines and fields; they do not discard authors, dates, or commit IDs merely to pass.

### 4. Make `reset` state-safe

- Validate target revision belongs to the selected identity.
- `--parent` chooses the nearest valid parent on that identity.
- Update ref and rev_map through Phase 3 state operations with expected-old checks and recovery.
- Never rewrite local branches/worktrees implicitly beyond baseline behavior.
- Reject reset while an unresolved import/dcommit journal requires recovery.

### 5. Make `rebase` safe and option-complete

- Require a clean index/worktree where the baseline requires it.
- Fetch only the current SVN parent when appropriate.
- Carry auth/config/runtime options through both fetch and rebase orchestration.
- Preserve `--dry-run`, merge/strategy options only where declared and tested.
- Avoid selecting upstream by maximum SVN revision across merged refs.

### 6. Align `gc` with produced metadata

- Compress only valid `unhandled.log` files produced by Phase 5.
- Remove stale index/temp files according to the baseline.
- Handle lock cleanup conservatively; do not delete a live lock.
- Report partial failures with paths and preserve recoverable metadata.

### 7. Enforce metadata-mode limitations

- `noMetadata` one-shot imports reject commands that the baseline cannot support after metadata removal.
- Missing rev_map rebuild behavior is tested for metadata-enabled repositories.
- Legacy layout behavior follows Phase 3 migration policy before queries run.

## Invariants

- Readonly commands never create or mutate metadata except commands whose contract explicitly does so (`gc`, `reset`, fetch within rebase).
- Same SVN revision numbers on different refs are never treated as interchangeable.
- Resolver choice follows current/explicit tree-ish first-parent identity, not highest revision.
- Output normalization does not hide wrong authors, dates, commit IDs, URLs, paths, or revision selection.

## Gates

### Structural gate

- all commands consume the shared tracking/query context;
- duplicate resolver/config/rev_map walkers are removed;
- query and formatter units compile for SHA-1 and SHA-256 repositories.

### Behavioral gate

- multi-ref fixtures with the same revision on trunk/branch/tag return the scoped result;
- optional tree-ish, before/after, zero/missing, ambiguous, SHA-256, and local-commit cases pass;
- log modes/ranges/pathspec/date/author/message cases match fixture expectations;
- reset fault/recovery and rebase dirty/auth/current-parent cases pass;
- noMetadata and legacy layout limitations are explicit and non-mutating.

### Release gate

- readonly output and state transitions match the frozen Perl baseline for every declared layout/profile;
- exact commit IDs are compared where output includes them;
- strict runs include multi-ref ambiguity and tree-ish scenarios, not only one linear trunk.

## Out of Scope

- `blame`, `proplist`, `propget`, `show-ignore`, and other commands not in the v1 contract;
- inventing output for metadata modes where the baseline explicitly cannot operate;
- supporting arbitrary Git merge histories that cannot identify one SVN first-parent identity.
