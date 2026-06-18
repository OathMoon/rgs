# git-svn-rs Implementation Progress Record - 2026-06-15

This document records the current execution state of the implementation plans in `.plans/`.
It is intentionally condensed: it keeps the current handoff state, completed capabilities,
remaining scope, verification evidence, and useful commit anchors.

## Current State

- Branch: `codex-execute-git-svn-rs-plans`
- Base: `master` at `1284668 Add planning documents`
- Latest implementation commit: `1ad8bf2 fix: ignore invalid svn-properties entries`
- Worktree before this progress-record refresh: clean
- Overall status: Phases 1-3 are complete; Phase 4/5 foundation and SVN CLI replay are substantially implemented; Phase 6 readonly commands are implemented for the supported local metadata/rev_map flows; Phase 7 has mock and local `file://` dcommit write-back; Phase 8 has a broad golden compatibility harness but still needs fuller Rust-vs-Perl coverage.

## Recent Progress

The previously pending Phase 4/5/8 follow-up batch was committed:

- `97d4352 feat: extend svn fixture import coverage`
- `d898981 feat: preserve svn executable mode in cli import`
- `a3712d2 feat: preserve svn special links in cli import`
- `cb30882 test: compare file modes in golden harness`

Additional completed work since that batch includes:

- SVN CLI replay now preserves `svn:executable`, `svn:special` symlinks, deleted-path history lookups through peg revisions, branch/tag copy parents, empty-directory placeholders, include/ignore path filters, ignored refs, revision ranges, authors mappings, rewritten metadata, and `--no-metadata`.
- SVN CLI backend accepts `file://`, `svn://`, `svn+ssh://`, `http://`, and `https://` URL schemes; local `svn://` replay is covered through `svnserve` when available.
- `clone`/`fetch` persist and honor key git-svn options including `--authors-file`, `--authors-prog`, `--log-window-size`, `--localtime`, `--username`, `--config-dir`, `--no-auth-cache`, `--rewrite-root`, `--rewrite-uuid`, `--ignore-refs`, `--ignore-paths`, `--include-paths`, `--preserve-empty-dirs`, and `--placeholder-filename`.
- `fetch --fetch-all` enumerates configured `svn-remote.*.url` entries and fetches each SVN remote.
- Incremental import anchors new commits to existing tracked refs, skips already imported mapping revisions, and uses SVN copy-from metadata as the first Git parent when possible.
- Readonly command coverage expanded for `find-rev`, `info`, `info --url`, `log`, `log -v`, `log --incremental`, `log --oneline --show-commit`, `log --limit`, `log --revision`, `reset`, `rebase --dry-run`, and `gc`.
- Phase 6 follow-up `4eaa751`: `git-svn-rs log` accepts trailing `git log` pass-through arguments such as pathspec filters (`git-svn-rs log --oneline -- path`), preserving existing git-svn formatting after Git history selection.
- Phase 6 follow-up `3f44d65`: `git-svn-rs log --revision` now rejects invalid revision filters instead of silently treating them as no filter and printing unrelated history.
- Phase 6 follow-up `a363864`: `git-svn-rs find-rev --before` and `--after` are now mutually exclusive at CLI parse time.
- Phase 6 follow-up: default non-incremental `git-svn-rs log` output now emits the trailing SVN-style separator after the last entry.
- Phase 6 follow-up `27b0893`: SVN-style log headers now use `1 line` for singular messages, count internal blank lines, and retain the one-line minimum for empty messages.
- Phase 6 follow-ups `3b42f98` and `22ef18a`: normal and incremental `log --show-commit` output now places the Git commit in the SVN-style header, with formatter and SHA-1/SHA-256-aware CLI coverage.
- `find-rev` scans all SVN rev_maps, allowing branch/tag-only revisions and commits to resolve in multi-ref layouts.
- `info --url` resolves tracked SVN URLs through fetch mappings and can use the current `HEAD` or closest tracked ancestor in multi-ref layouts.
- Local `file://` `dcommit` now writes adds, deletes, type changes, executable property changes, symlink property changes, renames, copies, explicit `--commit-url`, explicit `--mergeinfo`, and selected `.gitattributes`-driven SVN properties.
- `dcommit` default rebase behavior and `--no-rebase` behavior are covered for local `file://` write-back.
- Phase 7 follow-ups `298dc66` and `1ad8bf2`: local `file://` dcommit now accepts upstream-style `svn-properties=name=value[;name=value...]` attributes, preserves later-rule overrides, and ignores malformed or empty property entries.
- Golden compatibility artifacts now compare normalized tree modes, tree contents, symlink targets, empty-directory placeholders, clone output success, readonly command output, `log --oneline -- path`, `reset`, `rebase --dry-run`, and `gc`.
- Phase 8 follow-up `1461caf`: golden compatibility capture now records and compares `log --oneline -- src/lib.rs` pathspec output for Perl-vs-Rust supported subsets.
- Phase 8 follow-up: the standard golden fixture manifest and materialized SVN history now include an explicit empty directory before branch/tag copies, exercising the existing preserve-empty-dirs placeholder artifact comparison.
- Phase 8 follow-up: golden rev_map artifact capture now reads every `.rev_map.*` under `.git/svn` instead of only the first sorted path, so multi-ref layouts can expose branch/tag rev_map differences.
- Phase 8 follow-up: golden config artifacts now include `svn-remote.svn.uuid` alongside URL and fetch mappings.
- Phase 8 follow-up: golden rev_map artifacts now preserve zero-commit records instead of dropping them, allowing placeholder/trailing rev_map slots to be compared.
- Phase 8 follow-up `2241af4`: golden rev_map records now retain their normalized logical source refs across Perl and Rust metadata directory layouts, preventing records from different refs from being flattened together.
- Phase 8 follow-up: the declarative golden fixture manifest now records the `svn:executable` and `svn:special` property intent used by the materialized fixture.
- Windows verification support exists through `scripts/verify.ps1` and the Windows GitHub Actions workflow, including a manual strict compatibility mode.

## Committed Completed Work

### Phase 1: Foundation CLI Workspace

Status: completed and committed.

Commit:

- `7f531ee feat: add git-svn-rs cli surface`

Key outcomes:

- Created the Cargo workspace with `git-svn-rs` CLI and `git-svn-rs-core`.
- Added typed CLI coverage for `init`, `clone`, `fetch`, `rebase`, `dcommit`, `log`, `info`, `find-rev`, `gc`, `reset`, `diagnose`, and explicit unsupported v1 commands.
- Added CLI smoke tests and parsing tests.

### Phase 2: Config, Mapping, Authors, Filters

Status: completed and committed.

Commit:

- `ab52ef1 feat: add config mapping and git metadata primitives`

Key outcomes:

- Added SVN remote config serialization, single-path and standard-layout mapping builders, URL/path helpers, authors file parsing, include/ignore filters, `.git` exclusion, and ignore-over-include precedence.
- Added compatibility constraints for glob specs and review-requested coverage.

### Phase 3: Git Metadata and RevMap

Status: completed and committed.

Commit:

- `ab52ef1 feat: add config mapping and git metadata primitives`

Key outcomes:

- Added `GitCli` basics, `git-svn-id` parser/formatter, `.git/svn` metadata helpers, migration metadata inspection, and binary `.rev_map` read/write support for SHA-1 and SHA-256.
- Added rev_map locking, reset/list behavior, `sync_all`, trailing all-zero record handling, out-of-order append rejection, and gitfile/commondir metadata discovery.

### Phase 4/5: SVN Abstractions, Init, Fixtures, Import

Status: foundation completed and committed; replay support continues through later commits.

Important commits:

- `44ee04d feat: add svn abstractions and init import primitives`
- `9d84910 feat: add mock fetch import and svn fixtures`

Key outcomes:

- Added SVN domain types, mock backend, RA session/fetch editor traits, auth prompt mock, `svn-libsvn` feature shell, fast-import stream writer, and `git-svn-rs init`.
- Added SVN CLI fixture builder with skip-by-default behavior and strict compatibility mode via `GIT_SVN_RS_STRICT_COMPAT=1`.
- Added mock import planner, mock-only `clone`/`fetch`, `GitCli::fast_import`, and fetch editor planning for content, executable mode, symlink mode, copy, and delete.
- Added real SVN CLI replay for supported URL schemes, with the strongest coverage currently around local `file://` and local `svn://` fixtures.

### Phase 5: Replay and Import Compatibility

Status: substantially implemented for local SVN CLI replay.

Key outcomes:

- Persisted and honored common git-svn configuration options for authors, auth/config, metadata rewriting, revision windows, filters, empty directories, and fetch-all.
- Preserved executable files, symlinks, deleted historical paths, copied branch/tag history, empty-directory placeholders, and Git tree modes/content in replay output.
- Added local `svnserve` coverage for `svn://` clone/fetch, including standard-layout branch/tag discovery, copy-from parent preservation, and empty-directory placeholders.

### Phase 6: Readonly Commands

Status: implemented for supported metadata/rev_map flows.

Key outcomes:

- Added tracked SVN resolution from `svn-remote.svn.*` config and `.git/svn/<ref>/.rev_map.<uuid>`.
- Implemented `find-rev`, `info`, `info --url`, `log`, `gc`, `reset`, and `rebase` paths used by the current CLI/tests.
- Added log formatting for oneline output, Git commit prefixes, incremental output, verbose changed paths, rename/copy detection, revision filtering, trailing non-incremental separators, and limit handling with SVN-style newest-first ordering.
- Expanded resolver behavior so multi-ref layouts can resolve commits/revisions across branch/tag rev_maps.

### Phase 7: Dcommit, Shim, Windows Verification

Status: local/mock write-back implemented; remote/libsvn write-back remains guarded.

Key outcomes:

- Added `dcommit` foundations: diff planner, commit editor, path ensurer, property mapper, mock commit backend extensions, and associated tests.
- Implemented local `file://` SVN write-back for linear commits, add/modify/delete, type changes, executable/symlink property writes and removals, renames/copies with copy-from metadata, missing destination parent creation, explicit commit URLs, explicit mergeinfo, and selected `.gitattributes` SVN property mappings.
- Added upstream-style `.gitattributes` `svn-properties=` parsing for multiple semicolon-separated SVN properties while retaining the existing direct property forms.
- Added default post-dcommit rebase coverage and `--no-rebase` local write-back coverage.
- Added the `git-svn` shim crate, forwarding behavior, shim smoke tests, `scripts/verify.ps1`, Windows workflow, and README verification/dependency notes.

### Phase 8: Golden Compatibility

Status: harness implemented and expanded; full strict Rust-vs-Perl compatibility remains future work.

Key outcomes:

- Added golden compatibility harness skeleton and normalized artifact comparisons.
- Current artifacts cover tree modes/content, symlink targets, empty-directory placeholders, clone command success, `find-rev`, `info`, `info --url`, `log` variants, `rebase --dry-run`, `reset -rN`, and `gc`.
- The standard golden fixture now creates `trunk/empty-dir` so empty-directory placeholder comparisons are backed by a concrete SVN history edge case.
- Rev_map artifact capture now includes records from all discovered rev_map files under `.git/svn`, improving coverage for future multi-ref Perl-vs-Rust comparisons.
- Rev_map artifact capture preserves both populated and all-zero commit records.
- Rev_map artifact capture associates each record with a canonical `refs/remotes/*` source and rejects unmatched or ambiguous metadata paths.
- Config artifact capture now includes the SVN remote UUID for stricter metadata comparison.
- The standard fixture manifest records the executable and special-link SVN properties explicitly, not just the affected file contents.
- Perl git-svn detection was tightened so the `git-svn-rs` shim is not mistaken for a valid Perl comparison backend.
- Manual Windows strict compatibility mode is available through the GitHub Actions `strict_compat` input.

## Remaining Work

### Phase 4

- Implement the real `svn-libsvn` backend.
- Complete deeper auth/libsvn integration.

### Phase 5

- Broaden replay-backed `clone`/`fetch` validation beyond local `file://` and local `svn://` fixtures, especially remote auth/service scenarios.
- Finish full RA session, `SvnFetchEditor`, fast-import, metadata, and rev_map integration for real remote SVN histories.
- Continue hardening branch/tag/copy, absent path, empty-directory, executable, symlink, and `git-svn-id` compatibility against non-local SVN servers.

### Phase 6

- Complete all planned `Log.pm` compatibility modes.
- Broaden resolver behavior for complex multi-ref layouts and rev_map discovery edge cases.

### Phase 7

- Implement production remote SVN/libsvn write-back.
- Keep local `file://` dcommit as the current supported non-dry-run path; remote SVN/libsvn write-back is still guarded.
- Add automatic mergeinfo generation if it enters v1 scope.
- Expand SVN property/autoprops support beyond the current selected `.gitattributes` mappings.

### Phase 8

- Run and harden real Rust-vs-Perl artifact comparisons where Perl git-svn is available.
- Add more edge-case golden scenarios for config, refs, log output, `git-svn-id` footers, rev_map records, file modes, symlinks, copies, deletes, empty dirs, and command output.

## Verification Snapshot

Latest recorded verification:

- `cargo test -p git-svn-rs-core --test cli_parse parses_log_git_log_passthrough_args`
- `cargo test -p git-svn-rs-core --test cli_parse find_rev_before_and_after_conflict`
- `cargo test -p git-svn-rs-core --test cli_parse`
- `cargo test -p git-svn-rs --test readonly_commands log_passes_pathspec_args_to_git_log`
- `cargo test -p git-svn-rs --test readonly_commands log_invalid_revision_filter_fails`
- `cargo test -p git-svn-rs-core --test compat_golden artifact_comparison_reports_supported_subset_mismatches`
- `cargo test -p git-svn-rs --test readonly_commands`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture`
- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture`
- `cargo test -p git-svn-rs-core --test import_mock`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden standard_fixture_manifest_is_deterministic`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs-core --test compat_golden golden_fixtures::tests::supported_rev_map_collects_records_from_all_rev_maps`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs-core --test compat_golden golden_fixtures::tests::supported_config_includes_svn_remote_uuid`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs --test readonly_commands log_default_ends_with_svn_log_separator`
- `cargo test -p git-svn-rs --test readonly_commands log_incremental_omits_svn_log_separator`
- `cargo test -p git-svn-rs-core --test log_formatter`
- `cargo test -p git-svn-rs --test readonly_commands`
- `cargo test -p git-svn-rs-core --test compat_golden golden_fixtures::tests::supported_rev_map_preserves_zero_records`
- `cargo test -p git-svn-rs-core --test compat_golden golden_fixtures::tests::supported_rev_map_collects_records_from_all_rev_maps`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs-core --test compat_golden standard_fixture_manifest_records_svn_properties`
- `cargo test -p git-svn-rs-core --test compat_golden standard_fixture_manifest_is_deterministic`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (14 passed; Perl comparison skipped when Perl git-svn was unavailable)
- `cargo test -p git-svn-rs-core --test log_formatter` (5 passed)
- `cargo test -p git-svn-rs-core --test log_formatter` (7 passed)
- `cargo test -p git-svn-rs --test readonly_commands` (30 passed)
- `cargo test -p git-svn-rs --test dcommit_linear` (23 passed)
- `cargo test -p git-svn-rs-core --test svn_fixture -- --nocapture`
- `cargo clippy --all-targets --all-features -- -D warnings`

Full integration verification recorded after the larger Phase 5/6/7/8 batch:

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs-core --features svn-libsvn`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1`

Notable targeted suites that have passed during this work:

- `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture`
- `cargo test -p git-svn-rs-core --test import_mock -- --nocapture`
- `cargo test -p git-svn-rs-core --test dcommit_diff_planner -- --nocapture`
- `cargo test -p git-svn-rs-core --test git_backend -- --nocapture`
- `cargo test -p git-svn-rs-core --test fast_import`

## Recommended Next Steps

1. Commit this progress-record cleanup separately if the condensed handoff format is acceptable.
2. Continue Phase 4/7 production backend work: real `svn-libsvn` integration and remote SVN write-back.
3. Expand strict compatibility runs in environments with real Perl git-svn, SVN CLI, and `svnserve` available.
4. Add remaining golden edge cases around remote layouts, refs/config metadata, rev_map records, command output, and property behavior.
