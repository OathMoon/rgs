# git-svn-rs Implementation Progress Record

Condensed handoff record for continuing the `.plans/` implementation work. Keep this file focused on current state, important results, remaining scope, verification evidence, and commit anchors.

## Current State

- Branch: `codex-execute-git-svn-rs-plans`
- Base: `master` at `1284668 Add planning documents`
- Latest implementation commit: `fdd1a43 feat: expose linked libsvn version`
- Worktree before this simplification: clean
- Overall status: Phases 1-3 are complete; Phases 4/5 have strong local SVN CLI replay support; Phase 6 readonly commands are implemented for supported metadata/rev_map layouts; Phase 7 supports mock and local `file://` dcommit write-back; Phase 8 has a broad golden compatibility harness but still needs fuller strict Rust-vs-Perl validation.

## Important Commit Anchors

- `7f531ee feat: add git-svn-rs cli surface`
- `ab52ef1 feat: add config mapping and git metadata primitives`
- `44ee04d feat: add svn abstractions and init import primitives`
- `9d84910 feat: add mock fetch import and svn fixtures`
- `97d4352 feat: extend svn fixture import coverage`
- `d898981 feat: preserve svn executable mode in cli import`
- `a3712d2 feat: preserve svn special links in cli import`
- `cb30882 test: compare file modes in golden harness`
- `4eaa751`, `3f44d65`, `a363864`, `27b0893`, `3b42f98`, `22ef18a`, `8c22aca`, `7c8bf7a`, `12b842e`, `0cb0da7`, `0eb9d48`, `15e6e03`: Phase 6 readonly/log/find-rev compatibility hardening
- `edb9161`: SHA-256 rev_map handling for import/fetch
- `298dc66`, `1ad8bf2`, `7e769ad`, `d1a3a64`, `b0c39fa`, `9279ac5`, `1694046`, `cce0cb3`, `b3fe080`, `c691d7e`: `.gitattributes`/auto-props SVN property dcommit behavior
- `1461caf`, `2241af4`, `7927615`, `7424b18`, `b4765c7`: Phase 8 golden artifact and rev_map comparison hardening
- `d92819a`: clippy compatibility fix for Rust 1.95
- `16e024f`: `svn-libsvn` build script performs a vcpkg Subversion link probe and diagnostics can report `linked` when the probe succeeds.
- `fdd1a43`: linked `svn-libsvn` builds call the native `svn_subr_version()` API for libsvn version reporting.

## Completed Capabilities

### Phase 1: CLI Workspace

- Created the Rust workspace with `git-svn-rs` CLI, `git-svn-rs-core`, and the compatibility shim crate.
- Added typed CLI parsing and coverage for `init`, `clone`, `fetch`, `rebase`, `dcommit`, `log`, `info`, `find-rev`, `gc`, `reset`, `diagnose`, and explicitly unsupported v1 commands.

### Phase 2: Config, Mapping, Authors, Filters

- Added SVN remote config serialization, single-path and standard-layout mappings, URL/path helpers, authors file parsing, include/ignore filters, `.git` exclusion, and ignore-over-include precedence.
- `clone`/`fetch` persist and honor key git-svn options including authors, auth/config, rewrite root/UUID, metadata, revision windows, ignored refs, path filters, empty-directory preservation, and placeholders.

### Phase 3: Git Metadata and RevMap

- Added `GitCli` basics, `git-svn-id` parsing/formatting, `.git/svn` metadata helpers, migration metadata inspection, and binary `.rev_map` read/write support for SHA-1 and SHA-256.
- Added rev_map locking, reset/list behavior, `sync_all`, all-zero record handling, out-of-order append rejection, gitfile/commondir discovery, and object-format-aware rev_map decoding.

### Phases 4/5: SVN Abstractions and Import Replay

- Added SVN domain types, mock backend, RA session/fetch editor traits, auth prompt mock, `svn-libsvn` feature shell with a vcpkg Subversion link probe and native libsvn version call, fast-import writer, and `git-svn-rs init`.
- Added fixture builders and mock import/fetch planning for content, executable mode, symlink mode, copy, and delete.
- SVN CLI replay supports `file://`, local `svn://`, `svn+ssh://`, `http://`, and `https://` URL schemes, with strongest coverage around local fixtures.
- Replay preserves executable files, symlinks, deleted-path history through peg revisions, branch/tag copy parents, empty-directory placeholders, include/ignore filters, ignored refs, authors mappings, rewritten metadata, `--no-metadata`, revision ranges, and incremental fetch anchors.
- `fetch --fetch-all` enumerates configured `svn-remote.*.url` entries.

### Phase 6: Readonly Commands

- Implemented supported flows for `find-rev`, `info`, `info --url`, `log`, `log -v`, `log --incremental`, `log --oneline --show-commit`, `log --limit`, `log --revision`, `reset`, `rebase --dry-run`, and `gc`.
- Resolver behavior covers tracked SVN config, `.git/svn/<ref>/.rev_map.<uuid>`, multi-ref branch/tag layouts, current `HEAD`, and closest tracked ancestors.
- Readonly resolver accepts Perl-style leading `+` in configured fetch refspecs when deriving tracked SVN URLs.
- Readonly resolver expands configured `svn-remote.svn.branches`/`tags` wildcard mappings against existing remote refs when resolving the current tracked SVN URL.
- Log output now handles pathspec pass-through, invalid revision rejection, reverse revision ranges, `-n`/`--limit`, mutually exclusive `find-rev --before/--after`, SVN-style separators, singular/plural line counts, `--show-commit`, final non-empty `git-svn-id` footer recognition, and message line-ending preservation.

### Phase 7: Dcommit, Shim, Windows Verification

- Added dcommit diff planning, commit editor, path ensurer, property mapper, mock commit backend extensions, and tests.
- Local `file://` dcommit writes linear commits, adds, deletes, type changes, executable/symlink property changes, renames, copies, explicit `--commit-url`, explicit `--mergeinfo`, and selected SVN properties from `.gitattributes`.
- Upstream-style `svn-properties=name=value[;name=value...]` attributes are parsed, later rules override earlier ones, malformed/empty entries are ignored, and `-svn-properties`/`!svn-properties` clear prior container values without suppressing direct SVN attributes.
- Direct `.gitattributes` SVN property mapping now includes valued and boolean `svn:executable`, valued and boolean `svn:special`, valued and boolean `svn:needs-lock`, plus later-rule clearing for direct SVN attributes in local `file://` dcommit write-back.
- Local `file://` dcommit honors configured `svn-remote.svn.config-dir` for SVN write-back commands, including SVN auto-props applied during file adds.
- Default post-dcommit rebase and `--no-rebase` behavior are covered for local write-back.
- Added `git-svn` shim forwarding behavior, smoke tests, `scripts/verify.ps1`, Windows workflow, and strict compatibility mode wiring.

### Phase 8: Golden Compatibility

- Added golden compatibility harness and normalized artifact comparisons.
- Current artifacts cover tree modes/content, SVN file properties, symlink targets, empty-directory placeholders, clone success, readonly command output, `log --oneline -- path`, `reset`, `rebase --dry-run`, `gc`, config URL/fetch/UUID, and rev_map records.
- Standard fixture now includes explicit executable and special-link property intent plus an empty directory before branch/tag copies.
- Rev_map capture reads all `.rev_map.*` files under `.git/svn`, preserves zero-commit records, records raw byte lengths, keeps canonical logical source refs, and rejects unmatched or ambiguous metadata paths.
- Golden rev_map capture accepts SHA-1 and SHA-256 record widths, preferring the repository object format when available.
- Golden config artifact capture includes optional stdlayout branch and tag refspecs when present, with leading `+` normalized like fetch mappings.
- Golden optional config lookup reports unexpected `git config` failures instead of treating every nonzero optional-key lookup as a missing key.
- Rust-only stdlayout coverage validates trunk, branch, and tag ref tips against matching `git-svn-id` values and rev_map revisions.
- Perl git-svn detection avoids mistaking the `git-svn-rs` shim for a Perl comparison backend.

## Remaining Work

- Implement the real `svn-libsvn` backend and deeper auth/libsvn integration. A previous local attempt resolved crates.io metadata but timed out downloading `bindgen`; no unverified libsvn changes were retained.
- Current `svn-libsvn` feature builds and runs a vcpkg Subversion link probe; this environment links when `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and the vcpkg `installed\x64-windows\bin` directory is on `PATH`.
- Broaden replay-backed `clone`/`fetch` validation beyond local `file://` and local `svn://`, especially remote auth/service scenarios and full RA session integration.
- Continue hardening branch/tag/copy, absent path, empty-directory, executable, symlink, and `git-svn-id` behavior against non-local SVN servers.
- Complete the remaining planned `Log.pm` compatibility modes and more complex multi-ref/rev_map resolver cases.
- Implement production remote SVN/libsvn dcommit write-back; current supported non-dry-run path is local `file://`, with remote write-back guarded.
- Expand SVN property/autoprops support and add automatic mergeinfo generation if it remains in v1 scope.
- Run and harden strict Rust-vs-Perl artifact comparisons where Perl git-svn, SVN CLI, and `svnserve` are available.
- Add more golden scenarios for remote layouts, refs/config metadata, rev_map records, command output, properties, copies, deletes, symlinks, modes, empty dirs, and `git-svn-id` footers.

## Verification Evidence

Latest full gates recorded as passing:

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs-core --features svn-libsvn`
- `cargo test -p git-svn-rs-core --features svn-libsvn libsvn -- --nocapture`
- `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- --nocapture`
- `cargo test -p git-svn-rs-core --features svn-libsvn --test diagnostics -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend backend_reports_native_version_when_linked -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test diagnostics -- --nocapture`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1`

Important targeted suites recorded as passing during this work:

- `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands info_url_resolves_branch_from_branches_mapping -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands log_short_limit_returns_latest_svn_revisions -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands log_revision_reverse_range_filters_to_requested_svn_revisions -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_honors_svn_config_auto_props_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_valued_executable_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_valued_special_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_boolean_executable_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_boolean_special_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_needs_lock_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_boolean_needs_lock_from_gitattributes_to_file_svn_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_direct_gitattributes_property_can_be_cleared_by_later_rule_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test clone_fetch_smoke -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden artifact_comparison_reports_supported_subset_mismatches -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden supported_rev_map_reads_sha256_records -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden supported_config_includes_optional_branch_and_tag_mappings -- --nocapture`
- `cargo test -p git-svn-rs-core --test compat_golden optional_config_values -- --nocapture`
- `cargo test -p git-svn-rs-core --test import_mock -- --nocapture`
- `cargo test -p git-svn-rs-core --test dcommit_diff_planner -- --nocapture`
- `cargo test -p git-svn-rs-core --test git_backend -- --nocapture`
- `cargo test -p git-svn-rs-core --test fast_import`

Compatibility notes:

- Perl comparisons skip when Perl git-svn is unavailable unless strict compatibility mode is enabled.
- `scripts/verify.ps1` passes when allowed enough runtime; one earlier short tool timeout interrupted `cargo test --workspace` and produced a transient BrokenPipe.
- Windows verification support exists through `scripts/verify.ps1` and the Windows GitHub Actions workflow, including manual strict compatibility mode.

## Recommended Next Steps

1. Retry Phase 4/7 production backend work in an environment with libsvn/APR development libraries and reliable access to the libsvn binding dependency tree.
2. Expand strict compatibility runs in environments with Perl git-svn, SVN CLI, and `svnserve`.
3. Add remaining golden edge cases around remote layouts, refs/config metadata, rev_map records, command output, and property behavior.
