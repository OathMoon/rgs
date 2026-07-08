# git-svn-rs Implementation Progress Record

Condensed handoff record for continuing the `.plans/` implementation work. Keep this file focused on current state, important results, remaining scope, verification evidence, and commit anchors.

## Current State

- Branch: `codex-execute-git-svn-rs-plans`
- Base: `master` at `1284668 Add planning documents`
- Latest implementation commit: `1eda453 feat: include libsvn child error details`
- Worktree before this update: clean after implementation commit
- Overall status: Phases 1-3 are complete; Phases 4/5 have strong local SVN CLI replay support; Phase 6 readonly commands are implemented for supported metadata/rev_map layouts; Phase 7 supports mock, local `file://`, and local `svn://` dcommit write-back; Phase 8 has a broad golden compatibility harness but still needs fuller strict Rust-vs-Perl validation.

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
- `e1416d9`: linked `svn-libsvn` builds can open a native RA session for a `file://` repository and read latest revision plus repository UUID.
- `a65b2c4`: linked `svn-libsvn` builds can read native RA log revision metadata and changed-path metadata with `svn_ra_get_log2()`.
- `8ec08d0`: linked `svn-libsvn` log reads fill file content plus selected SVN file properties (`svn:executable`, `svn:special`) with `svn_ra_get_file()`.
- `2f3f55a`: linked `svn-libsvn` log reads recursively expand copied directories with `svn_ra_get_dir2()` and materialize copied files with content/properties.
- `c1240484`: linked `LibSvnBackend` implements the read-only `RaSession` surface for `url`, `repos_root`, `check_path`, `get_dir`, and path-filtered `get_log`.
- `9f1aa8b7`: linked `LibSvnBackend` implements initial native `do_update`/`do_switch` replay by translating native RA log events into `FetchEditor` callbacks.
- `9f65b6fc`: linked `LibSvnBackend` normalizes replay callback paths and copy-from paths before passing native log-backed update/switch events to `FetchEditor`, matching existing editor/mock path conventions.
- `d4ad78b0`: linked `LibSvnBackend` test coverage validates native RA metadata, path-filtered log, and update replay over a local `svn://` repository served by `svnserve`.
- `0c8cd255`: linked `LibSvnBackend` can be constructed from `SvnRemoteConfig` and passes configured `svn-remote.*.config-dir` through `svn_config_get_config()` into native RA sessions.
- `607f821c`: linked `LibSvnBackend` creates a native auth baton with simple/username providers, default username/password parameters, and no-auth-cache support; coverage includes a username/password-protected local `svn://` repository.
- `19328574`: `fetch` now selects the linked `LibSvnBackend` for real SVN remotes when `svn-libsvn` is enabled and linked, while default and unlinked feature builds continue to use the SVN CLI backend.
- `51fb4da`: CLI real-SVN replay tests reuse the shared core `SvnServe` fixture helper, including TCP readiness checks that work for authenticated repositories.
- `9bfc284`: `SvnFetchEditor` can strip a configured SVN mapping prefix from editor callback paths before producing fast-import paths, preparing command fetch to consume RA editor callbacks directly.
- `55eb840`: `GitCli` can read a commit/ref tree as file path/mode/content records, and `TreeEntry` can convert those records into a `SvnFetchEditor` base tree for future RA editor-backed incremental fetch.
- `68ec947`: `SvnFetchEditor::from_git_ref()` can initialize an editor base tree directly from an existing Git ref, reducing remaining command-fetch glue for RA editor-backed incremental replay.
- `dd3ab38`: `FetchCommitPlan` can carry an existing Git parent ref through `SvnFetchEditor::into_commit()`, preparing editor-backed incremental fetch to parent its first fast-import commit to the current remote ref.
- `6dd17f5`: `import_ra_revisions()` can drive `SvnFetchEditor` from a `RaSession::do_update()` callback stream, write the resulting fast-import commits, and update rev_maps for a standard-layout mapping.
- `2fb35a8`: linked `svn-libsvn` command fetch now routes real SVN imports through `RaSession::do_update()` and `SvnFetchEditor`, with per-mapping revision filtering and branch-copy source trees loaded from source rev_map commits.
- `5e20caed`: RA editor-backed import now applies configured path filters before/after editor replay and preserves empty-directory placeholders, bringing the full linked `clone_fetch_real_svn` suite green in the configured vcpkg/libsvn environment.
- `6777536`: linked libsvn log-backed replay now emits explicit `svn:executable`/`svn:special` removals when current file properties no longer contain them, preventing stale Git modes while the true delta editor integration is still pending.
- `f3f814c`: linked libsvn fetch backend construction now applies command-line `--username` overrides even when no command-line `--password` is provided, matching the SVN CLI backend's auth option precedence.
- `67bc4c8`: linked libsvn fetch backend construction now applies command-line `--config-dir` overrides over persisted config, matching the SVN CLI backend and ensuring native RA sessions use the requested runtime config directory.
- `d53164c`: linked libsvn fetch backend construction now applies command-line `--password` even when no username override or persisted username is available, matching the SVN CLI backend's password option plumbing.
- `2456bfc5`: dcommit SVN CLI write-back now supports local `svn://` repositories served by `svnserve`, with regression coverage in default and linked `svn-libsvn` builds.
- `771a734e`: dcommit SVN CLI write-back now applies persisted and command-line SVN auth/config options (`--username`, `--config-dir`, `--no-auth-cache`) to checkout, working-copy edits, property changes, and commit.
- `be9e8dd8`: `dcommit` accepts command-line `--password`, passes it to SVN CLI write-back commands without persisting it to git config, and has end-to-end coverage for an `svnserve` repository with anonymous reads and authenticated writes.
- `b0573e7`: `clone`/`fetch` pass command-line SVN auth/config overrides, including non-persisted `--password`, through SVN CLI and linked libsvn backends; SVN CLI backend commands now run non-interactively to avoid auth hangs.
- `ecd26c54`: `dcommit` reuses command-line SVN auth/config overrides for the post-commit fetch that syncs the written SVN revision back into Git/rev_map, covering authenticated local `svn://` repositories where reads require credentials.
- `d6376f69`: `dcommit` post-commit fetch now clears user-supplied `-r`/`--revision` limits before syncing the just-written SVN revision back into Git/rev_map, while retaining auth/config overrides.
- `47b5e0d`: linked libsvn `RaSession::get_dir()` now returns user directory properties from native `svn_ra_get_dir2()` while filtering internal `svn:entry:*` metadata.
- `5dabd83`: SVN CLI and linked libsvn log reads now preserve `svn:needs-lock` file properties in `ChangedPath.properties`.
- `a4c508a`: SVN CLI and linked libsvn log reads now preserve textual SVN file properties (`svn:eol-style`, `svn:mime-type`, `svn:keywords`) in `ChangedPath.properties`.
- `ed49baa`: readonly `gc` compresses `unhandled.log`, removes stale `index` files, and preserves rev_map lock cleanup; `log --verbose` renders SVN-style changed paths with leading repository paths and rename source paths.
- `315f72c`: linked libsvn log-backed replay emits removal callbacks for all supported file properties (`svn:executable`, `svn:special`, `svn:eol-style`, `svn:mime-type`, `svn:keywords`, `svn:needs-lock`) so editor-backed fetch can clear stale metadata.
- `4e597f6`: Phase 8 golden config artifacts now include optional metadata/auth/config keys while continuing to exclude passwords, and refspec normalization strips only one force-update `+`.
- `a5f99cc`: Phase 8 golden rev_map artifacts now include the UUID from `.rev_map.<uuid>` filenames in record and byte-length snapshots.
- `917309c`: Phase 8 standard golden fixture now exercises textual SVN file properties (`svn:eol-style`, `svn:mime-type`, `svn:keywords`) plus `svn:needs-lock`.
- `0ec6461`: Rust-only stdlayout golden artifacts now capture ref-tip tree contents and assert branch/tag copies retain deleted files while trunk reflects later deletes.
- `df65538`: Phase 8 golden command-output artifacts now compare `log --revision rN:rM --oneline` range output.
- `980c98e`: Phase 8 golden command-output artifacts now compare reverse `log --revision rM:rN --oneline` range output.
- `1eda453`: linked libsvn error reporting now preserves child error messages in native call failures for deeper RA/session diagnostics.

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

- Added SVN domain types, mock backend, RA session/fetch editor traits, auth prompt mock, `svn-libsvn` feature shell with a vcpkg Subversion link probe, native libsvn version call, native RA metadata reads for latest revision/UUID, native RA log metadata/changed-path reads, native RA file content/property reads for changed files, native copied-directory file materialization, a read-only native `RaSession` implementation, native RA config-dir propagation, native simple/username auth baton support, initial native `do_update`/`do_switch` replay into `FetchEditor` with normalized callback/copy-from paths, fast-import writer, and `git-svn-rs init`.
- Added fixture builders and mock import/fetch planning for content, executable mode, symlink mode, copy, and delete.
- SVN CLI replay supports `file://`, local `svn://`, `svn+ssh://`, `http://`, and `https://` URL schemes, with strongest coverage around local fixtures.
- Linked libsvn coverage now includes local `file://` and local `svn://` fixture access for native RA metadata, path-filtered log, and log-backed update/switch replay.
- `clone`/`fetch` now propagate command-line SVN auth/config overrides (`--username`, `--password`, `--config-dir`, `--no-auth-cache`) to the selected SVN CLI or linked libsvn backend without persisting passwords to git config; authenticated local `svn://` clone coverage exercises both default and linked `svn-libsvn` builds.
- Linked libsvn fetch backend construction now handles `--username` as an independent command-line override, not only as a companion to `--password`.
- Linked libsvn fetch backend construction now applies command-line `--config-dir` over persisted `svn-remote.*.config-dir`, keeping native RA session config precedence aligned with the SVN CLI backend.
- Linked libsvn fetch backend construction now handles `--password` as an independent command-line override, allowing native auth baton password defaults without requiring a paired username at construction time.
- Feature-gated command fetch now routes real SVN remotes through linked `LibSvnBackend` and the RA editor-backed import path when available, falling back to the SVN CLI/log-backed path for default and unlinked feature builds.
- CLI real-SVN replay tests share the same `SvnServe` fixture helper as core libsvn tests, reducing duplicate remote-service setup and keeping readiness behavior consistent.
- `SvnFetchEditor` can now map full SVN callback paths such as `trunk/src/lib.rs` to Git-relative paths such as `src/lib.rs` via a configured path prefix.
- `GitCli::tree_files` exposes ref tree files with modes and bytes, and `TreeEntry::from_git_file` bridges those records into `SvnFetchEditor` base trees for copy/incremental replay.
- `SvnFetchEditor::from_git_ref()` now builds base-tree state directly from an existing Git ref, ready for command-level editor replay integration.
- `FetchCommitPlan` can now pass a parent Git ref into the resulting fast-import commit when no parent mark is available, matching the existing log-backed incremental import parent behavior.
- `import_ra_revisions()` now provides the first import path that consumes `RaSession::do_update()` editor callbacks directly, converts them through `SvnFetchEditor`, writes fast-import data, and records rev_map entries.
- RA editor-backed import now filters revisions per mapping before replay and can seed branch-copy editors from the copied source ref tree, allowing linked libsvn stdlayout file:// fetch to import trunk/branch history through `SvnFetchEditor`.
- RA editor-backed import applies persisted include/ignore path filters to editor callbacks and resulting fast-import changes, and it emits configured empty-directory placeholders from SVN directory change metadata.
- Linked libsvn log-backed replay now mirrors selected file property removal callbacks for `svn:executable` and `svn:special`, so incremental editor import can clear stale executable/symlink modes from the Git tree.
- Linked libsvn log-backed replay now mirrors removal callbacks for all supported file properties, including `svn:needs-lock` and textual properties, so incremental editor import can clear stale file metadata.
- Linked libsvn `RaSession::get_dir()` now exposes real user directory properties returned by native `svn_ra_get_dir2()` and filters internal `svn:entry:*` metadata out of the public listing.
- Linked libsvn native call errors now include child error-chain details instead of only the top-level best message.
- SVN CLI and linked libsvn log reads now include the supported `svn:needs-lock` file property alongside executable and special-link properties.
- SVN CLI and linked libsvn log reads now include textual SVN file properties used by the golden harness: `svn:eol-style`, `svn:mime-type`, and `svn:keywords`.
- Replay preserves executable files, symlinks, deleted-path history through peg revisions, branch/tag copy parents, empty-directory placeholders, include/ignore filters, ignored refs, authors mappings, rewritten metadata, `--no-metadata`, revision ranges, and incremental fetch anchors.
- `fetch --fetch-all` enumerates configured `svn-remote.*.url` entries.

### Phase 6: Readonly Commands

- Implemented supported flows for `find-rev`, `info`, `info --url`, `log`, `log -v`, `log --incremental`, `log --oneline --show-commit`, `log --limit`, `log --revision`, `reset`, `rebase --dry-run`, and `gc`.
- Resolver behavior covers tracked SVN config, `.git/svn/<ref>/.rev_map.<uuid>`, multi-ref branch/tag layouts, current `HEAD`, and closest tracked ancestors.
- Readonly resolver accepts Perl-style leading `+` in configured fetch refspecs when deriving tracked SVN URLs.
- Readonly resolver expands configured `svn-remote.svn.branches`/`tags` wildcard mappings against existing remote refs when resolving the current tracked SVN URL.
- Log output now handles pathspec pass-through, invalid revision rejection, reverse revision ranges, `-n`/`--limit`, mutually exclusive `find-rev --before/--after`, SVN-style separators, singular/plural line counts, `--show-commit`, final non-empty `git-svn-id` footer recognition, message line-ending preservation, and SVN-style verbose changed paths with leading paths plus rename sources.
- `gc` now covers the documented git-svn cleanup surface by compressing `.git/svn/**/unhandled.log`, removing `.git/svn/**/index`, and deleting stale `.rev_map.*.lock` files.

### Phase 7: Dcommit, Shim, Windows Verification

- Added dcommit diff planning, commit editor, path ensurer, property mapper, mock commit backend extensions, and tests.
- Local `file://` dcommit writes linear commits, adds, deletes, type changes, executable/symlink property changes, renames, copies, explicit `--commit-url`, explicit `--mergeinfo`, and selected SVN properties from `.gitattributes`.
- Local `svn://` dcommit writes a linear commit through an `svnserve` fixture by reusing the SVN CLI working-copy write-back path, then fetches the resulting SVN revision back into Git/rev_map. The dcommit temporary checkout uses a relative checkout target to avoid vcpkg SVN path-case resolution failures in Windows temp directories.
- Dcommit SVN CLI write-back now merges persisted `svn-remote.*` auth/config values with command-line `dcommit` overrides, passes `--username`, `--password`, `--config-dir`, and `--no-auth-cache` through all SVN working-copy commands, and has real auto-props coverage for command-line `--config-dir` plus authenticated local `svn://` write-back coverage.
- Dcommit's post-commit fetch now carries the same command-line SVN auth/config overrides as the write-back path, so authenticated local `svn://` repositories that require credentials for reads can complete write-back and rev_map synchronization.
- Dcommit's post-commit fetch ignores the dcommit command's `-r`/`--revision` limiter so the newly written SVN revision is still imported into the tracked ref and rev_map.
- Upstream-style `svn-properties=name=value[;name=value...]` attributes are parsed, later rules override earlier ones, malformed/empty entries are ignored, and `-svn-properties`/`!svn-properties` clear prior container values without suppressing direct SVN attributes.
- Direct `.gitattributes` SVN property mapping now includes valued and boolean `svn:executable`, valued and boolean `svn:special`, valued and boolean `svn:needs-lock`, plus later-rule clearing for direct SVN attributes in local `file://` dcommit write-back.
- Local `file://` dcommit honors configured `svn-remote.svn.config-dir` for SVN write-back commands, including SVN auto-props applied during file adds.
- Default post-dcommit rebase and `--no-rebase` behavior are covered for local write-back.
- Added `git-svn` shim forwarding behavior, smoke tests, `scripts/verify.ps1`, Windows workflow, and strict compatibility mode wiring.

### Phase 8: Golden Compatibility

- Added golden compatibility harness and normalized artifact comparisons.
- Current artifacts cover tree modes/content, SVN file properties, symlink targets, empty-directory placeholders, clone success, readonly command output, `log --oneline -- path`, `log --revision` single/forward/reverse range output, `reset`, `rebase --dry-run`, `gc`, config URL/fetch/UUID, and rev_map records.
- Standard fixture now includes executable, special-link, textual property, and needs-lock intent plus an empty directory before branch/tag copies.
- Rev_map capture reads all `.rev_map.*` files under `.git/svn`, preserves zero-commit records, records raw byte lengths, keeps canonical logical source refs and filename UUIDs, and rejects unmatched or ambiguous metadata paths.
- Golden rev_map capture accepts SHA-1 and SHA-256 record widths, preferring the repository object format when available.
- Golden config artifact capture includes optional stdlayout branch/tag refspecs plus metadata/auth/config keys when present, with a single leading force-update `+` normalized for refspec mappings and passwords excluded from artifacts.
- Golden optional config lookup reports unexpected `git config` failures instead of treating every nonzero optional-key lookup as a missing key.
- Rust-only stdlayout coverage validates trunk, branch, and tag ref tips against matching `git-svn-id` values and rev_map revisions.
- Rust-only stdlayout coverage also validates ref-tip tree contents across branch/tag copies and later trunk deletes.
- Perl git-svn detection avoids mistaking the `git-svn-rs` shim for a Perl comparison backend.

## Remaining Work

- Continue the real `svn-libsvn` backend beyond native version, RA repository metadata, read-only `RaSession` methods, RA log metadata, changed-file content/property reads, copied-directory file materialization, config-dir propagation, basic auth baton support, and initial path-compatible log-backed `do_update`/`do_switch` replay; remaining backend work includes true libsvn delta editor integration, richer auth prompt/provider flows, remote service validation, and deeper libsvn error/session handling.
- Current `svn-libsvn` feature builds and runs a vcpkg Subversion link probe; this environment links when `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and the vcpkg `installed\x64-windows\bin` directory is on `PATH`.
- Broaden replay-backed `clone`/`fetch` validation beyond local `file://` and authenticated local `svn://`, especially non-local remote auth/service scenarios and full RA editor integration.
- Continue hardening branch/tag/copy, absent path, empty-directory, executable, symlink, and `git-svn-id` behavior against non-local SVN servers.
- Complete the remaining planned `Log.pm` compatibility modes and more complex multi-ref/rev_map resolver cases.
- Implement broader production remote SVN/libsvn dcommit write-back; current supported non-dry-run paths are mock, local `file://`, and local `svn://`, with SVN CLI username/password/config option plumbing in place, while http(s) write-back, prompt/cache integration beyond explicit command-line credentials, and richer remote/auth flows remain guarded or unvalidated.
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
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_file_repository_metadata -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_implements_ra_session_read_methods -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_drives_fetch_editor_callbacks -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_switch_drives_fetch_editor_callbacks -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_ -- --nocapture` (covers normalized replay callback paths and copy-from paths)
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_replays_local_svnserve_repository -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_clears_removed_file_properties -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_clears_removed_needs_lock_property -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_get_dir_reads_directory_properties -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_log_reads_needs_lock_file_property -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_log_reads_textual_file_properties -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_metadata_with_config_dir_from_remote_config -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_authenticated_svnserve_with_credentials -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_log_metadata_and_changed_paths -- --nocapture` (covers changed-path metadata, file content, `svn:executable`/`svn:special` file properties, and copied-directory file materialization)
- `cargo test -p git-svn-rs-core commands::fetch::tests::configured_backend_prefers_linked_libsvn_and_otherwise_uses_svn_cli`
- `cargo test -p git-svn-rs-core commands::fetch::tests::configured_backend_uses_ra_editor_import_only_when_linked_libsvn_is_available`
- `cargo test -p git-svn-rs-core svn::cli::tests::backend_command_args_include_auth_and_config_options -- --nocapture`
- `cargo test -p git-svn-rs-core --test import_mock imports_ra_session_update_into_git_and_rev_map`
- `cargo test -p git-svn-rs-core --test import_mock ra_import_filters_revisions_per_mapping_before_replay`
- `cargo test -p git-svn-rs-core --test import_mock ra_import_applies_path_filters_to_editor_changes`
- `cargo test -p git-svn-rs-core --test import_mock ra_import_preserves_empty_directories_with_placeholder`
- `cargo test -p git-svn-rs-core --test import_mock`
- `cargo test -p git-svn-rs-core --test fetch_editor`
- `cargo test -p git-svn-rs-core --test fetch_editor commit_plan_can_carry_parent_ref_for_incremental_editor_import`
- `cargo test -p git-svn-rs-core --test fetch_editor editor_can_load_base_tree_from_git_ref`
- `cargo test -p git-svn-rs-core --test git_backend --test fetch_editor`
- `cargo clippy -p git-svn-rs-core --test fetch_editor -- -D warnings`
- `cargo clippy -p git-svn-rs-core --test import_mock --test ra_session_mock --test fetch_editor -- -D warnings`
- `cargo clippy -p git-svn-rs-core --test fetch_editor --test git_backend -- -D warnings`
- `cargo clippy -p git-svn-rs-core -- -D warnings`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo clippy -p git-svn-rs-core --features svn-libsvn -- -D warnings`
- `cargo test -p git-svn-rs-core commands::dcommit::tests::dcommit_svn_options_apply_command_line_auth_overrides -- --nocapture`
- `cargo test -p git-svn-rs-core svn::cli::tests::backend_command_args_include_auth_and_config_options -- --nocapture`
- `cargo test -p git-svn-rs-core --test cli_parse parses_dcommit_auth_options -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn commands::dcommit::tests::dcommit_svn_options_apply_command_line_auth_overrides -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn svn::cli::tests::backend_command_args_include_auth_and_config_options -- --nocapture`
- `cargo test -p git-svn-rs --no-default-features --test clone_fetch_real_svn -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn commands::fetch::tests::configured_backend_prefers_linked_libsvn_and_otherwise_uses_svn_cli`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn commands::fetch::tests::configured_backend_uses_ra_editor_import_only_when_linked_libsvn_is_available`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn commands::fetch::tests::configured_backend_applies_command_line_username_without_password`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn commands::fetch::tests::configured_backend_applies_command_line_config_dir_override`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn commands::fetch::tests::configured_backend_applies_command_line_password_without_username`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn clone_stdlayout_authenticated_svn_url_imports_with_password -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn fetch_stdlayout_file_url_imports_trunk_history_after_init -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn fetch_stdlayout_svn_url_imports_branch_tag_and_copy_contents_after_init -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn clone_file_url_applies_ignore_paths_filter -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn clone_file_url_applies_include_paths_filter -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn clone_stdlayout_file_url_preserves_empty_dirs_when_requested -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn fetch_file_url_preserves_empty_dirs_from_persisted_config -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo clippy -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- -D warnings`
- `cargo fmt --check`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- --nocapture`
- `cargo clippy -p git-svn-rs-core -- -D warnings`
- With the same vcpkg environment: `cargo clippy -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- -D warnings`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test diagnostics -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test fetch_editor`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test import_mock imports_ra_session_update_into_git_and_rev_map`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1`

Important targeted suites recorded as passing during this work:

- `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture`
- `cargo test -p git-svn-rs --test clone_fetch_real_svn clone_stdlayout_authenticated_svn_url_imports_with_password -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands -- --nocapture` (39 tests; includes SVN-style verbose log paths and `gc` unhandled/index cleanup)
- `cargo test -p git-svn-rs --test readonly_commands info_url_resolves_branch_from_branches_mapping -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands log_short_limit_returns_latest_svn_revisions -- --nocapture`
- `cargo test -p git-svn-rs --test readonly_commands log_revision_reverse_range_filters_to_requested_svn_revisions -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_revision_option_does_not_limit_post_commit_fetch_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_linear_commit_to_svnserve_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_writes_to_authenticated_svnserve_with_password_when_tools_exist -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_fetches_after_authenticated_svnserve_write_when_reads_require_auth -- --nocapture`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear dcommit_writes_linear_commit_to_svnserve_when_tools_exist -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear dcommit_fetches_after_authenticated_svnserve_write_when_reads_require_auth -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_without_dry_run_is_guarded_for_non_mock_urls -- --nocapture`
- `cargo test -p git-svn-rs --test dcommit_linear dcommit_honors_command_line_svn_config_auto_props_when_tools_exist -- --nocapture`
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
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (23 tests; includes optional config metadata/auth coverage and one-`+` refspec normalization)
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; includes rev_map filename UUID snapshots)
- `cargo clippy -p git-svn-rs-core --test compat_golden -- -D warnings`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; includes textual SVN property fixture coverage)
- `cargo fmt --check`
- `cargo clippy -p git-svn-rs-core --test compat_golden -- -D warnings`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; includes stdlayout ref-tip tree content checks)
- `cargo clippy -p git-svn-rs-core --test compat_golden -- -D warnings`
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; includes revision range log golden output)
- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; includes reverse revision range log golden output)
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::svn_call_reports_child_error_messages -- --nocapture`
- With the same vcpkg environment: `cargo clippy -p git-svn-rs-core --features svn-libsvn -- -D warnings`
- `cargo test -p git-svn-rs-core --test import_mock -- --nocapture`
- `cargo test -p git-svn-rs-core --test dcommit_diff_planner -- --nocapture`
- `cargo test -p git-svn-rs-core --test git_backend -- --nocapture`
- `cargo test -p git-svn-rs-core --test fast_import`

Compatibility notes:

- Perl comparisons skip when Perl git-svn is unavailable unless strict compatibility mode is enabled.
- `scripts/verify.ps1` passes when allowed enough runtime; one earlier short tool timeout interrupted `cargo test --workspace` and produced a transient BrokenPipe.
- Windows verification support exists through `scripts/verify.ps1` and the Windows GitHub Actions workflow, including manual strict compatibility mode.

## Recommended Next Steps

1. Continue Phase 4/7 production backend work in the configured vcpkg/libsvn environment, starting with true libsvn delta editor integration and deeper auth/config/session handling.
2. Expand strict compatibility runs in environments with Perl git-svn, SVN CLI, and `svnserve`.
3. Add remaining golden edge cases around remote layouts, refs/config metadata, rev_map records, command output, and property behavior.
