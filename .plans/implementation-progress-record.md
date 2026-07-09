# git-svn-rs Implementation Progress Record

Condensed handoff record for continuing the `.plans/` implementation work. Keep this file focused on current state, important results, remaining scope, verification evidence, and commit anchors.

## Current State

- Branch: `codex-execute-git-svn-rs-plans`
- Base: `master` at `1284668 Add planning documents`
- Latest implementation commit: `1cef794 feat: bridge native file props to fetch editor`
- Worktree before this update: clean after implementation commit; progress record updated afterward
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
- `59033bf`: linked libsvn error detail formatting preserves the previous context fallback when no native error message is available.
- `c3d7043`: linked libsvn availability diagnostics no longer claim native backend API calls are unimplemented after the vcpkg link probe succeeds.
- `f7bf560`: linked libsvn `do_switch()` rejects switch URLs outside the configured repository root before invoking editor replay.
- `27d0921`: linked libsvn log-backed `do_switch()` derives the replay source path from the switch URL and maps emitted editor paths onto the requested target path.
- `edb123f`: linked libsvn `RaSession::repos_root()` reads the native repository root URL, so sessions opened on repository subpaths still report the true root and accept subpath-relative reads.
- `1d3dd39`: linked libsvn authenticated `svnserve` coverage now includes missing and wrong credential failures while preserving native `svn_ra_open5` diagnostic context.
- `b93a333`: linked libsvn log-backed update replay now handles sessions opened on repository subpaths by using session-relative native content reads while matching repository-relative changed paths.
- `d36cb2b`: linked libsvn local `svn://` coverage now validates `do_switch()` branch-copy replay in addition to metadata, log, and update replay.
- `69c2234`: linked libsvn authenticated local `svn://` coverage now validates path-filtered log reads and `do_update()` replay with explicit credentials and no auth cache.
- `d29adce`: linked libsvn local `svn://` coverage now validates directory property reads via `get_dir()` and corresponding directory-change log visibility.
- `6c2cbd5`: linked libsvn log enrichment now preserves directory properties on directory add/modify/replace changed paths, matching the existing native `get_dir()` property reads.
- `a0e4aff`: `FetchEditor` now has a default no-op directory property callback, and linked libsvn log-backed update replay emits directory property changes to editors.
- `87ffa3b`: linked libsvn coverage now validates directory property replay for sessions opened on repository subpaths, including correct editor path remapping to the subpath root.
- `e9dad9e`: linked libsvn authenticated local `svn://` coverage now validates persisted config usernames combined with runtime passwords and no-auth-cache.
- `c904de87`: linked libsvn auth baton can register a native simple prompt provider backed by `AuthPrompt`, allowing authenticated local `svn://` reads to obtain username/password from the prompt abstraction while respecting no-auth-cache.
- `06ac73c`: linked libsvn authenticated local `svn://` coverage now validates config usernames combined with prompt-supplied passwords and no-auth-cache.
- `27371cb`: linked libsvn log-backed update replay now tracks whether properties changed, compares previous/current properties for modified files/directories when native log flags are unknown, and avoids emitting unchanged file property callbacks on content-only edits while preserving property removals.
- `6e96ad59`: linked libsvn log-backed update replay now tracks whether file content changed, compares previous/current content when native log flags are unknown, and avoids emitting textdelta callbacks for property-only file edits.
- `b5e9659`: linked libsvn builds now bind the native `svn_delta_default_editor()` surface and validate the installed `svn_delta_editor_t` layout includes the `apply_textdelta_stream` tail slot needed for future `svn_ra_do_update3` integration.
- `0e57d2c`: linked libsvn builds now bind the native `svn_ra_do_update3()` reporter surface and validate a minimal `file://` repository update report can `set_path` and `finish_report` through the default delta editor.
- `d3d23ea`: linked libsvn delta-editor scaffolding now types the `set_target_revision` and `close_edit` callback slots and validates patched callbacks are invoked by a native `svn_ra_do_update3()` report.
- `9845838`: linked libsvn delta-editor scaffolding now types the `open_root` and `close_directory` callback slots and validates directory lifecycle callbacks during a native update report.
- `37b3e89`: linked libsvn delta-editor scaffolding now types the `add_file` and `close_file` callback slots and validates file lifecycle callbacks during a native update report.
- `174ce0b`: linked libsvn delta-editor scaffolding now types the `apply_textdelta` callback and window handler surface, validating native textdelta windows and the terminating null window during an update report.
- `38ee416`: linked libsvn delta-editor scaffolding now types the `open_file` callback slot and validates an incremental native update from r2 to r3 opens the modified file with the expected base revision.
- `ed05d51`: linked libsvn delta-editor scaffolding now types the `change_file_prop` callback slot and validates an incremental native update from r3 to r4 emits the `svn:needs-lock` file property callback.
- `8d698ea`: linked libsvn delta-editor scaffolding now types the `change_dir_prop` callback slot and validates an incremental native update from r4 to r5 emits the `svn:ignore` directory property callback.
- `34c3284`: linked libsvn delta-editor scaffolding now types the `delete_entry` callback slot and validates an incremental native update from r5 to r6 emits a file deletion callback.
- `9ea4376`: linked libsvn delta-editor scaffolding now types the `add_directory` callback slot and validates an incremental native update from r6 to r7 emits a directory-add callback.
- `a920dac`: linked libsvn delta-editor scaffolding now types the `open_directory` callback slot and validates an incremental native update from r7 to r8 opens the changed subdirectory.
- `57f19d3`: linked libsvn delta-editor scaffolding now types the `apply_textdelta_stream` callback slot, including the txdelta stream-open callback surface, and validates the default editor accepts and invokes a patched typed stream callback.
- `95086f8`: linked libsvn update-reporter scaffolding now types the `abort_report` callback slot and validates a native `svn_ra_do_update3()` report can be aborted without finishing.
- `a24e2e0`: linked libsvn update scaffolding now types the remaining reporter `delete_path`/`link_path` slots plus delta-editor `absent_directory`/`absent_file`/`abort_edit` slots, with coverage for patched callback invocation and native reporter exposure.
- `48bd2119`: linked libsvn native update scaffolding now binds `svn_stream_empty()` and `svn_txdelta_apply()` and validates native textdelta windows can be applied into a fulltext buffer for future `FetchEditor::apply_textdelta()` bridging.
- `8bf5258`: linked libsvn native update scaffolding now has a private adapter smoke test proving real `svn_ra_do_update3()` callbacks can drive a `FetchEditor` for an initial file add with fulltext content reconstruction.
- `1cef794`: linked libsvn native update adapter scaffolding now bridges `open_file` and `change_file_prop` callbacks into `FetchEditor`, with property-only file change coverage that avoids synthetic textdelta output.

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
- Linked libsvn native call error formatting retains a context fallback when libsvn returns no message.
- Linked libsvn availability detail now reflects that native backend API calls are available when the vcpkg link probe succeeds.
- Linked libsvn auth now supports explicit username/password defaults, persisted config username plus runtime password, and simple username/password prompting through the shared `AuthPrompt` abstraction.
- Linked libsvn auth coverage validates prompt-supplied passwords using the configured username as the prompt default.
- Linked libsvn log-backed replay is closer to native delta-editor semantics for modified paths: content-only file edits no longer synthesize unchanged property callbacks, while property removals still emit explicit removals.
- Linked libsvn log-backed replay also suppresses unchanged text callbacks for property-only file edits, while preserving textdelta callbacks for content edits.
- Linked libsvn now has the first native delta-editor FFI scaffold in place: the default editor template can be allocated and its modern `apply_textdelta_stream` slot is visible in the Rust layout.
- Linked libsvn now has the first native update-reporter FFI scaffold in place: a minimal `svn_ra_do_update3()` report can be driven against a local `file://` repository with the default delta editor.
- Linked libsvn native update scaffolding can now patch and observe delta-editor lifecycle callbacks (`set_target_revision` and `close_edit`) during a real `svn_ra_do_update3()` report.
- Linked libsvn native update scaffolding can also patch and observe directory lifecycle callbacks (`open_root` and `close_directory`) during a real `svn_ra_do_update3()` report.
- Linked libsvn native update scaffolding can also patch and observe file lifecycle callbacks (`add_file` and `close_file`) during a real `svn_ra_do_update3()` report, including the native callback path shape.
- Linked libsvn native update scaffolding can now patch and observe `apply_textdelta` and its txdelta window handler, including the terminating null window.
- Linked libsvn native update scaffolding now covers an incremental update path as well as an empty-base update path, including `open_file` callbacks for modified files.
- Linked libsvn native update scaffolding now also covers file property callbacks, including a real `svn:needs-lock` property change on an incremental update.
- Linked libsvn native update scaffolding now also covers directory property callbacks, including a real `svn:ignore` property change on an incremental update.
- Linked libsvn native update scaffolding now also covers delete-entry callbacks, including a real file deletion on an incremental update.
- Linked libsvn native update scaffolding now also covers add-directory callbacks, including a real subdirectory add on an incremental update.
- Linked libsvn native update scaffolding now also covers open-directory callbacks, including opening a changed subdirectory on an incremental update.
- Linked libsvn native delta-editor scaffolding now also types the `apply_textdelta_stream` tail callback and its txdelta stream-open callback surface; current native update coverage still uses the older window-handler `apply_textdelta` path.
- Linked libsvn native update-reporter scaffolding now also types and validates the `abort_report` callback path for an unfinished `svn_ra_do_update3()` report.
- Linked libsvn native update scaffolding now has typed Rust signatures for the remaining raw delta-editor/reporter callback slots (`absent_directory`, `absent_file`, `abort_edit`, `delete_path`, and `link_path`).
- Linked libsvn native update scaffolding can now apply native txdelta windows into a fulltext buffer, closing the main content-reconstruction prerequisite for a native delta-editor-to-`FetchEditor` adapter.
- Linked libsvn native update scaffolding now includes a private delta-editor-to-`FetchEditor` adapter smoke path for initial file adds; public `RaSession::do_update()` still uses the log-backed replay path until copy/delete/property/incremental cases are covered.
- Linked libsvn native update adapter scaffolding now also covers file property changes through native `open_file`/`change_file_prop` callbacks.
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

- Continue the real `svn-libsvn` backend beyond native version, RA repository metadata/root reads, read-only `RaSession` methods, RA log metadata, changed-file/directory content/property reads, copied-directory file materialization, config-dir propagation, basic auth baton support with local `svnserve` success/failure, replay, persisted-username/runtime-password coverage, prompt-backed simple credentials, local `svn://` update/switch replay validation, local `svn://` directory property/log validation, initial path-compatible log-backed `do_update`/`do_switch` replay including directory property callbacks and property-diff behavior, subpath update/property replay, `do_switch()` repository-root URL validation, switch URL source-path mapping, and initial native delta-editor/update-reporter FFI scaffolding; remaining backend work includes wiring true libsvn delta editor callbacks into `FetchEditor`, richer non-simple auth/cache/provider flows, broader remote service validation, and deeper libsvn error/session handling.
- Current `svn-libsvn` feature builds and runs a vcpkg Subversion link probe; this environment links when `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and the vcpkg `installed\x64-windows\bin` directory is on `PATH`.
- Broaden replay-backed `clone`/`fetch` validation beyond local `file://` and authenticated local `svn://`, especially non-local remote auth/service scenarios and full RA editor integration.
- Continue hardening branch/tag/copy, absent path, empty-directory, executable, symlink, and `git-svn-id` behavior against non-local SVN servers.
- Complete the remaining planned `Log.pm` compatibility modes and more complex multi-ref/rev_map resolver cases.
- Implement broader production remote SVN/libsvn dcommit write-back; current supported non-dry-run paths are mock, local `file://`, and local `svn://`, with SVN CLI username/password/config option plumbing in place, while http(s) write-back, prompt/cache integration beyond explicit command-line credentials, and richer remote/auth flows remain guarded or unvalidated.
- Expand SVN property/autoprops support and add automatic mergeinfo generation if it remains in v1 scope.
- Run and harden strict Rust-vs-Perl artifact comparisons where Perl git-svn, SVN CLI, and `svnserve` are available.
- Add more golden scenarios for remote layouts, refs/config metadata, rev_map records, command output, properties, copies, deletes, symlinks, modes, empty dirs, and `git-svn-id` footers.

## Verification Evidence

Recent full gates recorded as passing:

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs-core --features svn-libsvn`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `powershell -ExecutionPolicy Bypass -File scripts\verify.ps1`

Important targeted suites recorded as passing during recent work:

- `cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (24 tests; covers optional config/auth metadata, rev_map filename UUID snapshots, textual SVN properties, stdlayout tree contents, and forward/reverse revision-range log golden output)
- `cargo clippy -p git-svn-rs-core --test compat_golden -- -D warnings`
- With `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `PATH` including `E:\vcpkg\installed\x64-windows\bin`: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::svn_call_ -- --nocapture`
- With the same vcpkg environment: `cargo clippy -p git-svn-rs-core --features svn-libsvn -- -D warnings`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend reports_feature_enabled_link_probe_state -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_switch -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_does_not_emit_unchanged_file_props -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_does_not_emit_textdelta_for_property_only_file_change -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_clears_removed_file_properties -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update_clears_removed_needs_lock_property -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_subpath_session_reports_repository_root_and_relative_paths -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_implements_ra_session_read_methods -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_rejects_authenticated_svnserve -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_authenticated_svnserve_with_credentials -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_subpath_session_do_update_replays_relative_paths -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_switches_local_svnserve_repository -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_replays_local_svnserve_repository -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_replays_authenticated_svnserve_with_credentials -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_svnserve_get_dir_reads_directory_properties_and_logs_change -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_get_dir_reads_directory_properties -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_log_reads -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_do_update -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_subpath_do_update_reports_directory_properties -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_authenticated_svnserve_with_config_username_and_runtime_password -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_prompts_for_authenticated_svnserve_credentials -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_prompts_for_authenticated_svnserve_password_with_config_username -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_reads_authenticated_svnserve_with_credentials -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn --test libsvn_backend linked_backend_rejects_authenticated_svnserve -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::default_delta_editor_exposes_textdelta_stream_slot -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::default_delta_editor_accepts_patched_textdelta_stream_callback -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_reporter_can_abort_default_delta_editor_report -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_reporter_finishes_with_default_delta_editor -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_ -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::default_delta_editor_ -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_textdelta_windows_can_be_applied_to_fulltext_buffer -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_callbacks_drive_fetch_editor_for_initial_file_add -- --nocapture`
- With the same vcpkg environment: `cargo test -p git-svn-rs-core --features svn-libsvn svn::libsvn::tests::native_update_callbacks_drive_fetch_editor_for_file_property_change -- --nocapture`
- `cargo test -p git-svn-rs-core --test libsvn_backend linked_backend_prompts_for_authenticated_svnserve_credentials -- --nocapture` (default build compiles the test target and filters out the linked-only test)
- `cargo test -p git-svn-rs-core --test auth_prompt`
- `cargo test -p git-svn-rs-core --test fetch_editor`
- `cargo test -p git-svn-rs-core --test import_mock -- --nocapture`
- With the same vcpkg environment: `cargo clippy -p git-svn-rs-core --features svn-libsvn --test libsvn_backend -- -D warnings`
- With the same vcpkg environment: `cargo clippy -p git-svn-rs-core --features svn-libsvn -- -D warnings`
- Recent focused default suites: `cargo test -p git-svn-rs --test readonly_commands -- --nocapture`, `cargo test -p git-svn-rs --test dcommit_linear -- --nocapture`, `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture`, `cargo test -p git-svn-rs-core --test import_mock -- --nocapture`, `cargo test -p git-svn-rs-core --test fetch_editor`, `cargo test -p git-svn-rs-core --test git_backend -- --nocapture`, and `cargo test -p git-svn-rs-core --test fast_import`

Compatibility notes:

- Perl comparisons skip when Perl git-svn is unavailable unless strict compatibility mode is enabled.
- vcpkg/libsvn verification requires `VCPKG_ROOT=E:\vcpkg`, `VCPKG_DEFAULT_TRIPLET=x64-windows`, `VCPKGRS_DYNAMIC=1`, and `E:\vcpkg\installed\x64-windows\bin` on `PATH`.
- Windows verification support exists through `scripts/verify.ps1` and the Windows GitHub Actions workflow, including manual strict compatibility mode.
## Recommended Next Steps

1. Continue Phase 4/7 production backend work in the configured vcpkg/libsvn environment, likely with the next slice around native `svn_ra_do_update*` delta-editor integration, broader non-local remote validation, or remaining libsvn auth/provider prompt behavior.
2. Expand strict compatibility runs in environments with Perl git-svn, SVN CLI, and `svnserve`.
3. Add remaining golden edge cases around remote layouts, refs/config metadata, rev_map records, command output, and property behavior.
