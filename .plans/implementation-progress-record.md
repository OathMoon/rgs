# git-svn-rs Implementation Progress Record

Last audited: 2026-07-27
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `8405e79 fix: make import ref-map publication recoverable`
Latest implementation commit: `8405e79 fix: make import ref-map publication recoverable`

This is a concise handoff record. Product requirements live in `.plans/git-svn-rs-plan.md`; architecture/order live in `.plans/00-git-svn-rs-review-and-roadmap.md`; the evidence behind the status correction lives in `.plans/git-svn-rs-plan-code-architecture-review-2026-07-10.md`.

## Status Vocabulary

- `not-started`
- `in-progress`
- `structural-pass`
- `behavior-pass`
- `release-pass`

Do not use unqualified “complete” or “supported”. Developer tests that skip external compatibility checks cannot produce `release-pass`.

## Current Overall State

The repository is a substantial preview implementation with useful local fixtures and compatibility primitives. It is not yet a core `git svn` replacement and is not release-compatible with the frozen Git `v2.54.0` baseline.

| Phase | State | Current evidence | Main gap |
|---|---|---|---|
| 1 workspace/CLI | `structural-pass` | workspace, CLI, shim, diagnose, unsupported commands | remaining inert options and global verbosity contract |
| 2 config/mapping | `structural-pass` | basic config/glob/authors/filter/layout units, CLI subdirectory session paths | full layout URLs and metadata runtime semantics |
| 3 metadata/rev_map | `structural-pass` | SHA-1/SHA-256 rev_map, non-creating reads, managed OS locks/fsync/reset, gitfile discovery, explicit legacy/multi-UUID rejection, recoverable multi-mapping publication batch including unhandled-log append | remote/mapping ambiguity |
| 4 SVN adapters | `in-progress` | CLI/libsvn share the RA editor contract; native update/switch/checksums/errors | auth profiles, binary properties, broader remote validation |
| 5 import/clone/fetch | `in-progress` | local replay, unhandled metadata, timestamps, checkout, strict revision forms, bounded log windows, parent-scoped fetch, explicitly owned incremental placeholders, journaled multi-mapping publication, frozen-compatible auxiliary follow-parent refs, and persistent branch/tag discovery high-water | remaining Fetcher/follow-parent semantics |
| 6 readonly | `in-progress` | identity-scoped find-rev, explicit noMetadata/legacy limits, recoverable reset, conservative gc, SVN-style Log output, and current-parent rebase fetch | merge/strategy compatibility and external exactness |
| 7 dcommit | `in-progress` | production working-copy sink runs through the durable coordinator; strict file/svn writes, submitted recovery, and dirty no-write state match frozen Perl | unsupported remote decisions and broader recovery faults |
| 8 golden/release | `behavior-pass` | frozen Perl 2.54.0 strict reads, local writes, recovery final state, and dirty preflight artifacts match exactly; required Linux workflow is defined | first hosted CI execution |

## Validated Capabilities

### Foundation

- Rust workspace with CLI, core library, and opt-in `git-svn` shim.
- Typed core command surface and explicit v1 unsupported commands.
- Basic Git CLI wrapper, config serialization, `GlobSpec`, authors, filters, URL helpers, and metadata option conflict checks.
- Binary rev_map primitives for SHA-1/SHA-256, zero records, explicit create versus non-creating reads, append order, locks, fsync, reset, gitfile, and commondir.
- Frozen v0-v5 metadata layouts are inspected before fetch/query/gc mutations. Rev_db, v0-v2 roots, empty svn-remote sections, and mixed legacy/v5 layouts receive actionable non-mutating rejection; v5 compatibility `info/url` and gitfile/commondir discovery remain accepted.
- Resolver rev_map discovery is deterministic: zero candidates report missing, one is selected, and multiple UUID candidates fail closed with sorted paths without changing refs or metadata.

### Local read/import preview

- Standard-layout `file://` and local `svn://` clone/fetch fixtures cover trunk, branches, tags, copies, deletes, modes, symlinks, filters, authors, rewrite metadata, revision ranges, empty placeholders, and incremental anchors.
- Default SVN CLI and linked libsvn builds now use the same `RaSession`/`FetchEditor` import coordinator. CLI log enrichment materializes self-contained deltas while preserving copy ancestry and explicit base revisions.
- Linked build reads RA metadata/log/file/directory properties and routes production `do_update` through `svn_ra_do_update3` into `SvnFetchEditor` with explicit base-revision reports.
- Native update covers initial/incremental content, copy-only files, properties and property deletion, deletes, nested directories, repository subpaths via RA reparenting, file/svn transport, and authenticated svnserve.
- Unknown file/directory properties and absent nodes are collected in the common editor and appended after successful import using the frozen git-svn `unhandled.log` ordering and URI encoding. CLI `proplist --xml --verbose` discovers arbitrary textual properties and base-relative removals; encoded binary properties fail explicitly.
- Native editor failures and panics are operation-owned and converted to immediate `svn_error_t` cancellation errors; linked regression verifies libsvn stops before `close_edit`.
- SVN CLI subdirectory sessions now separate repository-root content URLs from session-relative changed paths; real `file://.../trunk` clone coverage passes.
- Import parses SVN RFC3339 dates (including fractional seconds), writes author/committer epoch and offset, and supports historical local offsets for `--localtime`.
- Clone materializes the primary tracking ref into the initial local branch and worktree; `--no-checkout` resolves the branch without populating files.
- Each imported mapping is first written to a unique internal staging ref, then published through a durable repository-level batch journal. The journal records the UUID, complete mapping set, and completed mappings; restart resumes only unfinished mappings without republishing finished refs or rev_map suffixes. Recovery accepts only expected old/target ref identities and retains evidence on concurrent movement or metadata mismatch.
- Per-mapping publication includes the exact `unhandled.log` append before the repository batch marks that mapping complete. RA restart coverage proves a completed mapping's metadata is neither omitted nor appended twice.
- Empty-directory reconciliation now uses explicit Perl-compatible `+empty_dir`/`-empty_dir` ownership events in each mapping's transactional `unhandled.log`. Only generated placeholders are removed; real same-named SVN files survive later siblings. Ownership replays only through the rev_map tip, propagates across copies, survives reset and repeated GC archives, and real incremental file-SVN coverage exercises empty/non-empty round trips.
- `fetch --parent` resolves the current first-parent tracking identity and imports only that ref. Configured/runtime `--log-window-size` replays monotonic bounded ranges while retaining standard-layout branch/tag discovery and copy ancestry; copy dependencies discover unchanged sources, order sources before destinations, and backfill missing source history from before the requested range.
- Follow-parent now creates frozen-compatible auxiliary `branch@copyfrom_rev` refs for moved tracking roots whose source path is not an exact configured mapping. Auxiliary history begins at the copy point, repeated fetches reuse the same ref through its `git-svn-id`, and mapped-root replacement expands the root delete into valid per-file Git deletes.
- Multi-level follow-parent reaches a fixed point across newly backfilled history: each earlier copy source receives the frozen `branch@rev` ref, dependency ordering forms the complete parent chain, and one-revision log windows can discover and backfill a two-move ancestry. The resulting refs, parents, and commit OIDs match frozen Perl.
- Ancestor directory copies now enumerate wildcard descendants through the RA directory contract, synthesize mapping-root copy ancestry, and defer copy de-duplication until an exact destination mapping exists. A copied branch layout therefore retains its auxiliary `branch@rev` parent even when SVN reports only the ancestor copy.
- Branch/tag glob discovery now persists monotonic `branches-maxRev`/`tags-maxRev` values in Perl-compatible `.git/svn/.metadata`, migrates the legacy internal `config` name, and uses those scan positions instead of stale wildcard rev_map tips. Failed and parent-scoped fetches do not advance global discovery state.
- Fixed fetch mappings now publish the frozen trailing-zero rev_map scan high-water through the same durable import transaction. Repeated sparse/empty windows are byte-stable, the next real revision replaces the marker in place, zero-only publication does not create or move a ref, and selected-parent fetch scopes the marker to the selected mapping.
- `ignore-refs` now follows frozen Fetcher scope: fixed fetch mappings and generated `branch@rev` ancestry refs remain importable while wildcard branch/tag refs are filtered. Duplicate fixed destination refs across remotes fail before recovery or import mutation, and destination-side path filters retain copies materialized from excluded sources on the default SVN CLI transport.
- Successful fetch persists Perl-compatible repository `reposRoot` and UUID identity in `.git/svn/.metadata`; failed fetches leave both absent. `info` therefore reports the real SVN repository root for single-subdirectory remotes instead of mistaking the configured `/trunk` URL for the root.

### Readonly preview

- Common flows exist for `find-rev`, `info`, `log` modes/ranges/pathspecs, `gc`, `reset`, and rebase on supported metadata layouts.
- `find-rev` resolves revision queries through the current or explicit tree-ish identity and commit queries through the commit identity, reading only that rev_map. Same-numbered revisions on other refs are not flattened into the result; explicit invalid or ambiguous tree-ish scopes fail.
- `noMetadata` remains a one-shot import mode: initial import and rev_map-backed `find-rev`/`info` work, while later fetch/rebase, log, and dcommit fail before ref/rev_map or SVN mutation.
- Reset serializes with dcommit, rejects active write journals, persists the expected old/new ref identity and rev_map target, moves the ref with expected-old CAS, and recovers crashes before or after the ref update. Other resolver-backed commands fail closed while a reset journal is pending.
- Log derives the frozen SVN author from an unambiguous authors-file reverse mapping or email login fallback, formats author epochs in the local SVN date shape, preserves message whitespace, and skips leading blank lines in oneline mode.
- Log follows requested numeric range direction, emits revision-first `--oneline --show-commit`, and pads lower-width oneline revisions to the first displayed revision width.
- Log record framing preserves per-commit `--stat`, `--raw`, and patch passthrough blocks without contaminating the next commit identity.
- Rebase resolves the current first-parent SVN identity before fetch, carries runtime fetch/auth options to that identity only, rejects dirty worktrees before mutation, and leaves unrelated concrete mappings unchanged.
- Rev_map writers now publish a versioned lock marker while holding an OS-level exclusive lock. GC removes only marked, unlockable crash leftovers; active and unknown legacy locks are preserved, and cleanup errors include paths.
- The remaining readonly flows are not yet fully compatible because merge/strategy behavior, migration policy, and strict external comparison remain incomplete.

### Local write preview

- Mock write-back executes the shared typed `DcommitPlan` through `SvnCommitEditor`, including rename/copy operations.
- Local `file://` and local authenticated `svn://` write-back use an SVN working copy driven by the shared typed plan and cover file/property/copy/move cases, post-fetch, and rebase.
- Typed raw diffs preserve A/M/D/C/R/T modes, object IDs, similarity, and both paths. The common plan builder materializes final copy/move content, symlink encoding, mode-property set/delete operations, mergeinfo, and stable raw metadata; editor operation failures abort without closing.
- The versioned dcommit journal stores the whole oldest-first queue with `Queued`/`Ready`/`Submitted`/`FetchedVerified` ordering, atomic generation snapshots, corruption fallback, and crash-safe OS advisory plus in-process locks. A pure coordinator persists each transition, verifies stable SHA-256 plan/message fingerprints, checks the remote head before submit, advances later plan/copy bases from verified imports, resumes `Submitted` without duplicate submission, and records terminal rebase/no-rebase states.
- Repository-wide discovery scans `.git/svn/**/dcommit-journal`, rejects multiple active journals, retains completed ledgers, and holds a `.git/svn/dcommit.lock`. Production resumes matching file/svn active journals and fails closed on target/config mismatch, multiple active journals, mock active state, or completed-ledger commit overlap.
- `JournalStorePersistence` keeps a live journal lock and tracks snapshot generations. Disk-restart tests destroy and reload coordinator state to prove submitted commits are not resubmitted, fetch verification is retryable, and rebase-pending recovery avoids the sink.
- `PreparedDcommit` construction now deterministically derives the oldest-first journal queue and production fingerprints. `.gitattributes` SVN-property resolution is a pure plan-enrichment step shared by the working-copy path, and invalid UTF-8 remains an explicit error.
- Working-copy write-back now executes the shared editor plan through the durable coordinator. Production persists the queue before write, checks the target path head, records submitted revisions before exact-revision fetch, verifies rev_map/ref/footer identity, reconstructs active journals from durable fingerprints, and runs final rebase/no-rebase transitions.
- Post-fetch verification projects each queued `DcommitPlan` from the original imported base and compares the imported tree after normalizing SVN EOL, executable, symlink, and keyword semantics. It therefore detects path/content/mode divergence without incorrectly requiring byte identity with the original local Git commit.
- Non-dry-run dcommit rejects a dirty index/worktree before any state that may submit. Explicit commit URLs use the target path's last-changed revision and verify that target even when the configured mapping cannot import the submitted revision.
- Every production write target is queried for its repository UUID before journal creation, checkout, or submission. A cross-repository `--commit-url` fails closed even when its revision number happens to match the tracked repository.
- Journal format v2 persists `SubmissionInFlight` before invoking the commit sink. A submit-command error, process restart, or failure to persist the returned revision therefore refuses automatic retry instead of risking a duplicate SVN revision; v1 journals remain readable.
- `dcommit --adopt-revision REV` reconciles an in-flight file/svn submission only after the normal exact-revision fetch and plan-projected tree verification succeeds. Failed verification leaves the durable state in-flight; commit-URL overrides are rejected because that path cannot yet prove exact imported-tree identity.
- The working-copy sink preserves real filesystem kind across executable and `svn:special` transitions: new symlinks are materialized before `svn add`, link-to-file replacement resets inherited symlink mode, and file-to-link/link-target updates retain the intended node kind.
- Frozen `file://` and password-protected `svn://` dcommit comparisons prove identical r7 author/date/full message, A/M/D/move copy ancestry, node tree, contents, properties, exact imported ref/graph/rev_map OIDs, and no-rebase HEAD/index/worktree state. Existing `svn:needs-lock` files are made writable only in the temporary commit working copy; file-RA ignores explicit username like Perl, and SVN log messages remove only Git format terminators.

### Golden infrastructure

- Deterministic SVN fixture and artifact capture/comparison infrastructure exists.
- Current artifacts cover exact ref tips, remote-reachable commit graph identity, rev_map object IDs, default/no-checkout clone HEAD/index/worktree state, configs, modes, properties, tree content, and readonly outputs.
- Find-rev and show-commit artifacts retain exact commit IDs, clone output retains status/stdout/stderr, and structured log artifacts retain author, local date, line count, message, and changed paths. The covered strict trunk fixture matches frozen Perl 2.54.0 exactly after normalizing only the deliberately different destination directory name.
- The same exact artifact schema now covers stdlayout trunk/branch/tag refs and direct URLs ending in `/trunk`. Multi-ref collection binds readonly/tree/property artifacts to the HEAD-matching tracking ref, while clone output includes frozen branch/tag copy progress and follow-parent diagnostics.
- The deterministic write harness hotcopies one repository behind the same URL, freezes the commit date in a post-commit hook, and retains raw dcommit output plus canonical SVN revision/path/tree/content/property and exact Git state artifacts. Recovery injection proves durable `Submitted` state resumes without r8, while staged-dirty preflight proves neither client writes r7 and Rust creates no journal.
- Each exact scenario writes a machine-readable JSON summary with frozen commit, tool versions, OS, artifact profile, and pass/fail state. CI retains complete Perl/Rust artifacts and failed summaries via an always-run upload step.

## Audited Critical Gaps

### P0

1. The strict frozen Perl 2.54.0 read, local file/authenticated-svn write, submitted-recovery, and staged-dirty no-write profiles pass and required CI is defined, but the first hosted run remains.

### P1

- Native `do_update` and `do_switch` are production-backed; MD5 base/result checksums, absent/abort callbacks, persistent unhandled metadata, and immediate native callback errors are implemented.
- Test callback recorders are operation-owned, the global serialization lock is removed, auth prompt panics are contained, and the linked default-parallel suite passes. A full production callback/error audit remains before claiming all FFI callbacks panic-free.
- Working-copy production dcommit preserves full messages and executes `DcommitPlanBuilder`/`SvnCommitEditor` through the recovery coordinator. Exact plan-projected tree verification accounts for SVN properties/keywords and `.gitattributes` mappings that intentionally differ from the original local commit.
- Dcommit target selection uses the nearest first-parent rev_map identity with footer URL/UUID/revision validation; local merge ranges and dirty pre-submit worktrees are rejected. Broader commit-URL intent/auth profile validation remains incomplete.
- The `Ready -> submit -> Submitted` persistence gap is fail-closed through a durable pre-submit in-flight marker. Automatic resubmission is prohibited when the outcome is unknown; explicit `--adopt-revision` recovery verifies the imported plan before persisting adoption.
- Rebase now fetches only the current first-parent tracking identity and fails before mutation on dirty worktrees; merge/strategy exactness and broader external comparison remain incomplete.
- Fetch-time authors/filters/localtime/metadata/empty-dir options overlay persisted config; identity-changing overrides are rejected after import. `log-window-size` and `fetch --parent` are implemented with real file-SVN regressions.
- Strict fetch revision forms (`N`, `N:M`, `HEAD`, `BASE:N`, `N:HEAD`, `BASE:HEAD`) are implemented; BASE uses the slowest configured mapping for the selected remote/UUID.
- Fetcher still lacks binary-property transport, refname/metadata-path collision handling, and linked-CLI stdlayout copy parity.
- Multi-mapping ref/rev_map publication and each mapping's `unhandled.log` append are recoverable under one repository batch.
- Migration is inspection only.

## Capability Profile State

| Profile | Audited state |
|---|---|
| `file://` + SVN CLI standard layout read | `behavior-pass` for the covered local fixture only |
| `file://` + SVN CLI single-path read | `behavior-pass` for the covered local fixture only |
| local `svn://` + SVN CLI read/write | `behavior-pass` with strict frozen write evidence for the covered explicit-credential fixture |
| linked libsvn file/svn metadata and log replay | `behavior-pass` for covered local fixtures only |
| linked libsvn true delta update/switch | `behavior-pass` for covered file/svn fixtures, including unhandled metadata and callback failure |
| HTTP(S) read | accepted/unvalidated; no support claim |
| HTTP(S) write | not implemented |
| svn+ssh | accepted/unvalidated or deferred; no support claim |
| strict frozen-baseline compatibility | `behavior-pass` for covered reads, local file/svn writes, one submitted recovery, and staged-dirty no-write; first hosted CI remains |

## Verification Evidence from 2026-07-10 Audit

Environment:

- Git `2.54.0.windows.1`
- SVN/SVNAdmin `1.14.0`
- Cargo/rustc `1.95.0`
- vcpkg Subversion available with dynamic environment settings
- Perl `git-svn` unavailable

Results:

- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS in about 429.2 seconds; strict Perl comparison explicitly skipped.
- Focused compat test with `--nocapture`: test shell PASS, output `skipping: Perl git-svn is required`.
- All-feature clippy without `VCPKGRS_DYNAMIC`: FAIL in the unlinked feature matrix due dead-code test accessors.
- All-feature clippy with documented vcpkg dynamic environment: PASS.
- `cargo test -p git-svn-rs-core --features svn-libsvn` default parallel: FAIL due shared callback recorder race, mutex poisoning, and panic across `extern "C"`.
- The first failing native test alone: PASS.
- Linked core suite with `--test-threads=1`: PASS in about 200.9 seconds, confirming test isolation rather than deterministic callback functionality as the primary failure.
- Temporary stdlayout clone reproduction: remote ref created; `HEAD` unresolved, worktree empty, SVN r2 date in 2026 but Git tip epoch `1`.
- Temporary single-subdirectory clone reproduction: FAIL on duplicated `/trunk/trunk/...` path.

Current linked evidence from 2026-07-13 supersedes the callback-race result above:

- With `VCPKGRS_DYNAMIC=1`, native callback tests pass 20/20 under the default parallel harness.
- `cargo test -p git-svn-rs-core --features svn-libsvn`: PASS under the default parallel harness after native update/switch/checksum promotion (about 189 seconds including follow-up gates).
- The linked backend integration suite passes 32/32, and the exact stdlayout ref/graph/rev_map golden collector passes through production native update.
- Linked `cargo clippy -p git-svn-rs-core --all-targets --features svn-libsvn -- -D warnings`, formatting, and diff checks pass. Default `cargo test --workspace` also passes after the new editor contract.
- After CLI/editor convergence and unhandled metadata persistence, `cargo test --workspace` passes in about 441 seconds, including real clone/fetch 23/23 and dcommit 37/37.
- With `VCPKGRS_DYNAMIC=1`, linked core passes with 33/33 backend integrations; linked clippy with `-D warnings`, formatting, and `git diff --check` pass. The failing-editor regression confirms immediate `svn_error_t` propagation and no `close_edit` continuation.
- Current Phase 7 foundation verification passes 34 core library tests, 5 commit-editor tests, 3 legacy planner tests, and 2 typed plan-builder tests. Journal tests cover whole-queue persistence, strict state ordering, roundtrip, truncated-snapshot fallback, and lock exclusion.
- The next Phase 7 slice adds 7 coordinator fault-injection tests, 4 repository journal-registry tests, typed mock rename/copy execution, and CLI fail-closed checks proving unfinished/completed journal guards leave the tracking ref and rev_map bytes unchanged.
- `cargo test --workspace` passes after the Phase 7 foundation slice (about 458 seconds). `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `git diff --check` also pass.
- On 2026-07-16 the production coordinator and projected-tree slices pass coordinator 9/9, disk restart 4/4, dcommit unit 4/4, tree projection 5/5, Git cleanliness 1/1, and the real dcommit CLI matrix 43/43. The real file-SVN recovery regression proves a submitted revision survives post-fetch failure and resumes without a duplicate SVN commit; the in-flight disk restart regression proves an unknown outcome is never resubmitted.
- On 2026-07-17 `cargo test --workspace` passes in about 556.5 seconds after scoped find-rev, noMetadata limits, and reset transactions: real clone/fetch 23/23, dcommit 43/43, readonly 42/42, core library 65, golden 25/25, and reset recovery 3/3. Workspace clippy with `-D warnings`, formatting, and `git diff --check` also pass.
- On 2026-07-20 frozen Log author/date/message alignment passes core library 68/68, formatter 9/9, readonly 42/42, and golden 26/26. `cargo test --workspace` passes in about 878.3 seconds; workspace clippy with `-D warnings`, formatting, and `git diff --check` pass.
- On 2026-07-20 current-parent rebase fetch and clean-worktree preflight pass focused core/import/CLI tests. A full `cargo test --workspace` rerun passes in about 951 seconds: real clone/fetch 23/23, dcommit 43/43, readonly 47/47, core library 68/68, import mock 9/9, formatter 11/11, and golden 26/26. All-target/all-feature clippy, formatting, diff checks, and the `svn-libsvn` backend integration suite 33/33 pass after linkage-scoping test-only accessors.
- On 2026-07-20 conservative GC lock probing passes managed stale/live/legacy unit coverage, rev_map 10/10, and GC CLI 2/2. `cargo test --workspace` passes in about 746 seconds, including real clone/fetch 23/23, dcommit 43/43, and readonly 47/47; all-target/all-feature clippy, formatting, and diff checks pass.
- On 2026-07-20 non-creating rev_map reads pass rev_map 11/11, import mock 9/9, readonly 47/47, and the full workspace in about 701 seconds, including dcommit 43/43. All-target/all-feature clippy, formatting, and diff checks pass; production `RevMap::open` calls are now confined to write coordination.
- On 2026-07-20 explicit legacy-layout policy passes migration 8/8 plus process-level fetch/gc no-mutation regressions. `cargo test --workspace` passes in about 788 seconds: clone/fetch smoke 8/8, dcommit 43/43, readonly 48/48, and all remaining suites; all-target/all-feature clippy, formatting, and diff checks pass.
- On 2026-07-20 multi-UUID rev_map ambiguity passes its process-level no-mutation regression and readonly 49/49; all-target/all-feature clippy, formatting, and diff checks pass on top of the immediately preceding full-workspace gate.
- On 2026-07-20 import publication recovery passes 4/4 focused transaction tests, import mock 9/9, clone/fetch smoke 8/8, and `cargo test --workspace` in about 889.3 seconds. The workspace gate includes real clone/fetch 23/23, dcommit 43/43, readonly 49/49, golden 26/26, rev_map 11/11, and migration 8/8; all-target/all-feature clippy, formatting, and diff checks pass.
- On 2026-07-25 Ubuntu 24.04 WSL with system Subversion/libsvn 1.14.3 reports `libsvn link: linked`. The 33-test native backend suite passes three consecutive default-parallel runs, the complete `cargo test -p git-svn-rs-core --features svn-libsvn` gate passes, and workspace all-target/all-feature clippy, formatting, and diff checks pass. Linux discovery uses pkg-config version/library-directory queries plus explicit dynamic links, avoiding false failure on unavailable private static-dependency metadata; svnserve startup is serialized only through port selection and readiness to remove a parallel fixture race.
- On 2026-07-27 WSL resolves user-local Git and Perl `git-svn` 2.54.0 with SVN 1.14.3. The strict golden comparison now runs instead of skipping. Its collector accepts Git 2.54.0 UTC `Z` timestamps, absent optional `svn-remote.svn.uuid`, generated empty-directory placeholders, and Perl numeric log revisions. The remaining failure is a substantive compatibility report rather than a collector crash. The other golden tests pass 29/29; focused clippy, formatting, and diff checks pass.
- On 2026-07-27 the strict trunk comparison was reduced to clone output only. Exact commit graph/OIDs now match after importing the mapping-root empty revision, using the frozen default `author@UUID` identity, and preserving the trailing metadata-message newline. Perl-compatible unborn `--no-checkout`, two-line Log message framing, incremental separators, seven-character `--show-commit`, and silent rebase dry-run also match. Non-output golden tests pass 29/29, import mock 9/9, clone/fetch smoke 8/8, readonly 50/50, workspace all-target clippy, formatting, and diff checks pass.
- On 2026-07-27 structured clone output carries captured Git initialization stdout/stderr, revision/OID/path progress, empty-dir warnings, and checkout identity through the command boundary. After normalizing only the deliberately different test destination directory, the complete strict golden suite passes 30/30 against frozen Perl 2.54.0. Clone/fetch smoke 8/8, workspace all-target clippy, formatting, and diff checks also pass.
- On 2026-07-27 the import journal was lifted to a repository-level multi-mapping batch. Restart coverage proves a completed trunk mapping is not republished while an unfinished branch resumes. Linux working-copy regressions cover executable and symlink add/remove/type transitions. The full workspace passes, including real clone/fetch 23/23, dcommit 43/43, readonly 50/50, strict golden 30/30, import mock 10/10, and core units 85/85; workspace clippy with `-D warnings`, formatting, and diff checks pass.
- On 2026-07-27 verified manual dcommit adoption passes coordinator 11/11 and real dcommit 44/44. The real file-SVN regression proves an in-flight revision is imported and plan-verified without creating a second SVN revision; workspace clippy, formatting, and diff checks pass.
- On 2026-07-27 real clone/fetch expands to 26/26 with bounded one-revision log windows, current-parent selective fetch, and incremental empty-directory fill/re-empty transitions. Fetch-editor 14/14 and import mock 11/11 cover final-tree reconciliation and exact-once unhandled metadata across batch restart. The full workspace, workspace clippy with `-D warnings`, formatting, and diff checks pass.
- On 2026-07-27 copy-source dependency/backfill raises real clone/fetch to 27/27 and import mock to 13/13. A real `-r 3:3` branch clone backfills trunk r1-r2 before importing the copy, preserving both copied contents and the declared trunk@1 directory parent. Full workspace and static gates pass.
- On 2026-07-27 auxiliary follow-parent compatibility raises strict golden to 31/31, real clone/fetch to 28/28, import mock to 14/14, and fetch-editor to 15/15. The frozen moved-directory case matches Perl ref tips, parent graph, commit OIDs, normal config, and `branch@rev` reuse exactly. `cargo test --workspace`, the linked `svn-libsvn` core suite (with CLI-only golden transports excluded), all-target/all-feature clippy, formatting, and diff checks pass.
- On 2026-07-27 discovery high-water compatibility raises strict golden to 32/32, real clone/fetch to 29/29, clone/fetch smoke to 9/9, Git backend to 10/10, and core units to 87/87. Initial and trunk-only incremental stdlayout fetches match frozen Perl internal maxRev values; rollback, failed-fetch, selected-parent, legacy-name migration, and normal-config isolation regressions pass. Workspace, linked core, all-target/all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 explicit placeholder ownership raises real clone/fetch to 31/31, fetch-editor to 17/17, and core units to 90/90 (118/118 linked). Perl-shaped empty-dir events preserve real same-named files, copy ownership, rev_map-tip reset semantics, and post-GC replay; repeated GC retains earlier compressed history. Workspace, linked core, strict golden 32/32, all-target/all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 multi-level follow-parent adds fixed-point auxiliary discovery after each copy-source backfill. A one-revision-window mock regression proves `destination@2 -> destination@3 -> destination`; the expanded strict frozen comparison proves the two-move ref set, parent graph, and OIDs match Perl. Import mock passes 15/15 and strict golden remains 32/32.
- On 2026-07-27 `.github/workflows/compatibility.yml` adds a non-optional Ubuntu gate that builds frozen Git commit `0b13e48` (v2.54.0), installs SVN/libsvn, enables strict compatibility, and runs workspace, linked-backend, formatting, and all-feature clippy gates. Strict linked tests now fail if the feature compiled without a real libsvn link; local strict linked coverage passes. The workflow still needs its first hosted execution before claiming the release gate.
- On 2026-07-27 the exact frozen golden suite expands to 33/33 with stdlayout. Trunk, branch, and tag refs, OIDs, parent graph, rev_maps, checkout state, content/properties, readonly behavior, and complete clone output now match Perl. Multi-ref artifact selection and branch/tag follow-parent progress were corrected; JSON scenario summaries and retained CI artifacts provide machine-readable execution evidence.
- On 2026-07-27 strict golden expands to 34/34 with a direct `file://.../trunk` clone. The exact artifact set matches Perl after persisting its internal `reposRoot`/UUID metadata and using that root in `info`; failed-fetch coverage proves repository identity is not published prematurely.
- On 2026-07-27 the post-expansion gate passes `cargo test --workspace`, strict linked `cargo test -p git-svn-rs-core --features svn-libsvn`, all-target/all-feature clippy with `-D warnings`, formatting, and `git diff --check`. The progress record remains concise at 279 lines.
- On 2026-07-27 strict golden expands to 35/35 with deterministic `file://` dcommit. The comparison exposed and fixed `svn:needs-lock` write permission, file-RA username, and trailing log-message differences; exact SVN semantics and post-write Git identities now match frozen Perl. Workspace, strict linked core, all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 strict golden expands to 36/36 with password-protected `svn://` dcommit. An isolated config/auth cache plus noninteractive prompt input proves credentials span clone, commit, and post-fetch without entering Git config; exact SVN/Git artifacts match Perl. Workspace, strict linked core, all-target/all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 strict golden expands to 38/38. A forced post-submit authors failure leaves durable `Submitted r7` while the tracking ref remains r6; restart reaches the exact uninterrupted Perl final state without r8. A staged tracked dirty index fails before SVN write on both clients, preserves exact SVN/Git/rev_map/worktree state, and creates no Rust journal. Workspace, strict linked core, all-target/all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 Fetcher edge regressions pass clone/fetch smoke 10/10, real SVN clone/fetch 32/32, import mock 15/15, and strict frozen golden 38/38. Fixed and auxiliary refs survive matching `ignore-refs`, cross-remote duplicate targets fail before metadata/ref mutation, and a copy from an excluded source into an included destination is retained on the default SVN CLI path. Workspace, strict linked core, all-target/all-feature clippy, formatting, and diff gates pass; native linked CLI stdlayout copies remain a documented Phase 5 gap.
- On 2026-07-27 ancestor-copy discovery raises real SVN clone/fetch to 33/33 and strict frozen golden to 39/39. A copied parent layout containing an unchanged branch produces the same destination/auxiliary refs, parent graph, and commit OIDs as Perl 2.54.0; import mock remains 15/15. Workspace, strict linked core, all-target/all-feature clippy, formatting, and diff gates pass.
- On 2026-07-27 transactional fixed-mapping scan markers pass rev_map 14/14, import transaction 14/14, import mock 17/17, real SVN clone/fetch 34/34, and strict frozen golden 40/40. Sparse scans match Perl records and byte lengths exactly, repeat byte-identically, and replace the final zero with the next real revision without growing the map. Workspace, strict linked core, all-target/all-feature clippy, formatting, and diff gates pass.

Previously recorded passing commands remain useful developer evidence, but the audit results above take precedence for current gate status.

## Important Commit Anchors

### Foundation and default workflows

- `7f531ee`: CLI surface.
- `ab52ef1`: config/mapping/Git metadata primitives.
- `44ee04d`, `9d84910`, `97d4352`: SVN abstractions, mock import, fixtures.
- `d898981`, `a3712d2`: executable and symlink import behavior.
- `edb9161`: SHA-256 rev_map import/fetch.

### Readonly and write-back

- `4eaa751` through `15e6e03`: readonly/log/find-rev hardening series.
- `298dc66` through `c691d7e`: attributes/auto-props local dcommit series.
- `2456bfc5`, `771a734e`, `be9e8dd8`, `ecd26c54`: local svnserve write/auth/post-fetch series.
- `9d354f6`: shared replay convergence and Phase 7 typed-plan/journal foundation.
- `e3b8cf9`: pure dcommit recovery coordinator, repository journal discovery/lock, and typed mock execution.
- `aaf0be0`: stable recovery fingerprints, crash-safe locks, journal persistence adapter, and disk-restart tests.
- `4bf9205`: production working-copy coordinator integration, post-fetch identity verification, active-journal reconstruction, and clean-worktree preflight.
- `68fabb0`: real file-SVN post-fetch failure/restart regression proving no duplicate submission.
- `87aabd7`: plan-projected post-fetch tree verification with SVN EOL, mode, symlink, and keyword normalization.
- `a29e90f`: pre-write repository UUID validation and a real two-repository wrong-target regression.
- `7e27bcf`: v2 in-flight submission journal state and fail-closed ambiguous-command/restart recovery.
- `13811f7`: identity-scoped `find-rev`, optional tree-ish syntax, and same-revision multi-ref regression.
- `7aa266d`: one-shot noMetadata enforcement for fetch/rebase/log/dcommit with rev_map-backed queries retained.
- `19ad4ef`: reset expected-old CAS, durable transaction journal, dcommit serialization, and crash recovery.
- `f97c729`: frozen SVN log author/date/message formatting and structured golden comparison.
- `293fb4c`: frozen log range direction, revision-first show-commit, and oneline width padding.
- `8876d0f0`: record-safe stat/raw/patch passthrough output for offline SVN log.
- `4858009`: current first-parent rebase resolution, selective fetch/import, clean-worktree preflight, and multi-ref state-isolation regressions.
- `0e8d795`: link-state-aware libsvn test accessors restoring the all-feature clippy gate.
- `e8ef1d4`: versioned OS-held rev_map locks and conservative GC cleanup with live/legacy preservation.
- `c191e9b`: explicit non-creating rev_map reads across resolver, fetch, reset, dcommit verification, and copy-parent lookup.
- `181e762`: actionable non-mutating v0-v5/rev_db/mixed-layout and empty svn-remote rejection policy.
- `c14d9f7`: deterministic rev_map discovery and fail-closed multi-UUID identity ambiguity.
- `4bf1ebb`, `0ee499e`: non-creating import inspection and compare-and-swap ref deletion prerequisites.
- `8405e79`: staging-ref import plus durable, idempotent CAS ref/rev_map publication recovery.

### Golden harness

- `1461caf`, `2241af4`, `7927615`, `7424b18`, `b4765c7`: artifact/rev_map comparison series.
- `4e597f6`, `a5f99cc`, `917309c`, `0ec6461`, `df65538`, `980c98e`: config, UUID, properties, tree, and log-range extensions.

### Linked libsvn

- `16e024f`, `fdd1a43`, `e1416d9`, `a65b2c4`: link/version/session/log foundation.
- `c1240484`, `9f1aa8b7`, `19328574`, `2fb35a8`, `5e20caed`: RA surface and command integration through log-backed editor replay.
- `b5e9659` through `48bd2119`: native delta/reporter/txdelta scaffolding.
- `8bf5258` through `613efa8`: private test-only delta-to-`FetchEditor` adapter.

## Corrected Next Steps

Continue in this order unless new verification changes priority:

1. Phase 8: run the hosted compatibility workflow.
2. Phase 5: implement ref/path collision handling and linked CLI stdlayout copy parity.
3. Phase 4/5/7: define binary-property transport and validate HTTP(S)/svn+ssh profiles.

## Handoff Notes

- The v2 plan rewrite is documentation-only; no business code was changed.
- The audit report is intentionally retained as evidence, while the v2 plan files are the current authority.
- An aborted linked test left two untracked `crates/git-svn-rs-core/golden-stdlayout-*` directories during the audit; they are test artifacts, not source. Cleanup was not performed after the filesystem action was denied, so verify workspace status before the next implementation commit.
- Preserve the pre-existing untracked `.codex/` directory.
- 2026-07-12 focused verification passed: SVN CLI path units (5), timestamp parser units (2), fast-import tests (3), mock clone/fetch (7), and real single-subdirectory `file://` clone (1). `cargo fmt --all` and `git diff --check` passed.
- `cargo test --workspace` passed after the URL/timestamp/checkout changes. The A/r100 first-parent versus merged B/r200 resolver regression and existing linear dcommit dry-run regression also pass.
- Exact SHA-1/SHA-256 rev_map artifact tests (including zero records), footer-based resolver disambiguation, merge preflight, and affected file/symlink dcommit regressions pass.
- Exact ref-tip/commit graph collectors and default/no-checkout clone-state collectors pass without Perl; comparison mismatch tests cover refs, graph, rev_map OIDs, and clone state. `<commit>` and unconditional `clone: success` normalization were removed.
- Strict revision parsing/BASE aggregation, runtime fetch-config overlay, identity immutability checks, and explicit init/fetch option rejection pass focused tests; `--fetch-all` with same-UUID remotes remains covered.
- Production libsvn `do_update` now uses a separate native delta module and an explicit `UpdateRequest` base revision supplied by the import coordinator. Copy-only files retain parent content, multi-component targets reparent safely, and initial/existing rev_map base propagation is tested.
- Global native callback recorders/serialization were removed. Linked default-parallel core tests pass with `VCPKGRS_DYNAMIC=1`; prompt/editor panics are contained at FFI boundaries.
- Native `do_switch` combines log copy discovery with `svn_ra_do_switch3` content deltas. Base/result MD5 checksums are validated, copy-only sources resolve mixed revisions, and callback failures return immediate libsvn errors.
- CLI and libsvn imports now share `RaSession`/`FetchEditor`. The common editor persists exact textual property/absent records to `unhandled.log`; CLI arbitrary textual properties use verbose XML proplist, while encoded binary properties remain an explicit unsupported boundary.
- Dcommit now preserves complete `%B` messages and both mock and working-copy sinks execute the shared typed plan. Production file/svn write-back is connected to exact rev_map/ref/footer and plan-projected tree verification, durable active-journal reconstruction, clean and repository-UUID preflight, and retry-safe in-flight/submitted/fetched/rebase states. Real recovery proves post-fetch failure does not duplicate submission, an in-flight restart refuses automatic resubmission, and `--adopt-revision` resumes only after exact import/plan verification. Broaden remote/auth profiles and frozen write comparison before claiming the Phase 7 behavioral gate.
- `find-rev` no longer recursively aggregates every rev_map. HEAD/commit identity and an optional explicit tree-ish select one validated mapping; the readonly 41/41 and golden 25/25 suites cover branch commit lookup and same-numbered trunk/branch revision scoping.
- Frozen documentation defines noMetadata as one-shot and explicitly excludes log. Real initial import plus readonly 42/42 now prove footer-free import succeeds, later mutating operations fail before state changes, and rev_map-backed find-rev/info remain usable.
- Reset now persists its old/new ref and rev_map target before CAS. Unit recovery covers crashes on either side of the ref update and rejects journal paths outside `.git/svn`; the full workspace passes with resolver-wide pending-reset guards.
- Log now reads full Git author identity plus epoch, reverses configured authors-file identities when unique, emits the frozen local SVN date shape, preserves message boundaries, follows numeric range direction, pads oneline revisions, preserves stat/raw/patch blocks, and feeds author/date/count/message/path into the passing strict Perl golden comparison.
- Import no longer advances final tracking refs directly from fast-import. Commit objects land on internal staging refs, and one durable repository batch publishes every mapping's final ref, exact rev_map suffix, and recoverable per-mapping `unhandled.log` append. Fetch resumes pending mappings, completed mappings are not republished, and resolver-backed readonly/write commands fail closed.
- Placeholder ownership now uses the frozen `unhandled.log` event shape rather than filename inference. URI-decoded ownership is reconstructed from both current and gzipped logs at the selected rev_map tip; repeated GC merges old and new log history instead of overwriting prior state. Editor, copy, reset-boundary, real same-name, incremental round-trip, and post-GC fetch regressions pass.
- Fixed fetch mappings now retain sparse scan progress as a single transactionally published trailing-zero rev_map record. Recovery handles both an untouched marker and a marker already replaced by the first real publication record; the existing journal remains fail-closed rather than claiming whole-prefix torn-write repair.
