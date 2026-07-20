# git-svn-rs Implementation Progress Record

Last audited: 2026-07-20
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `8876d0f0 fix: preserve svn log passthrough output`
Latest implementation commit: `8876d0f0 fix: preserve svn log passthrough output`

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
| 3 metadata/rev_map | `structural-pass` | SHA-1/SHA-256 rev_map, locks/fsync/reset, gitfile discovery | read/create split, transaction/recovery, ambiguity, real migration |
| 4 SVN adapters | `in-progress` | CLI/libsvn share the RA editor contract; native update/switch/checksums/errors | auth profiles, binary properties, broader remote validation |
| 5 import/clone/fetch | `in-progress` | local replay, unhandled metadata, timestamps, checkout, strict revision forms/runtime overlay | windowing/parent fetch, atomic publication, remaining Fetcher semantics |
| 6 readonly | `in-progress` | identity-scoped find-rev, explicit noMetadata limits, recoverable reset, and SVN-style Log identity/date/range/passthrough output | broader rebase behavior and external exactness |
| 7 dcommit | `in-progress` | production working-copy sink runs through the durable coordinator with clean preflight, in-flight markers, and plan-projected tree checks | explicit manual reconciliation for ambiguous submissions and remote profiles |
| 8 golden/release | `structural-pass` | exact refs/graph/rev_map and clone-state artifacts | strict Perl execution and remaining command-output parity |

## Validated Capabilities

### Foundation

- Rust workspace with CLI, core library, and opt-in `git-svn` shim.
- Typed core command surface and explicit v1 unsupported commands.
- Basic Git CLI wrapper, config serialization, `GlobSpec`, authors, filters, URL helpers, and metadata option conflict checks.
- Binary rev_map primitives for SHA-1/SHA-256, zero records, append order, locks, fsync, reset, gitfile, and commondir.

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

### Readonly preview

- Common flows exist for `find-rev`, `info`, `log` modes/ranges/pathspecs, `gc`, `reset`, and `rebase --dry-run` on supported metadata layouts.
- `find-rev` resolves revision queries through the current or explicit tree-ish identity and commit queries through the commit identity, reading only that rev_map. Same-numbered revisions on other refs are not flattened into the result; explicit invalid or ambiguous tree-ish scopes fail.
- `noMetadata` remains a one-shot import mode: initial import and rev_map-backed `find-rev`/`info` work, while later fetch/rebase, log, and dcommit fail before ref/rev_map or SVN mutation.
- Reset serializes with dcommit, rejects active write journals, persists the expected old/new ref identity and rev_map target, moves the ref with expected-old CAS, and recovers crashes before or after the ref update. Other resolver-backed commands fail closed while a reset journal is pending.
- Log derives the frozen SVN author from an unambiguous authors-file reverse mapping or email login fallback, formats author epochs in the local SVN date shape, preserves message whitespace, and skips leading blank lines in oneline mode.
- Log follows requested numeric range direction, emits revision-first `--oneline --show-commit`, and pads lower-width oneline revisions to the first displayed revision width.
- Log record framing preserves per-commit `--stat`, `--raw`, and patch passthrough blocks without contaminating the next commit identity.
- The remaining readonly flows are not yet fully compatible because broader rebase behavior and strict external comparison remain incomplete.

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

### Golden infrastructure

- Deterministic SVN fixture and artifact capture/comparison infrastructure exists.
- Current artifacts cover exact ref tips, remote-reachable commit graph identity, rev_map object IDs, default/no-checkout clone HEAD/index/worktree state, configs, modes, properties, tree content, and readonly outputs.
- Find-rev and show-commit artifacts retain exact commit IDs, clone output retains status/stdout/stderr, and structured log artifacts retain author, local date, line count, message, and changed paths. Missing strict Perl execution and remaining output modes still prevent an exact compatibility claim.

## Audited Critical Gaps

### P0

1. Strict Perl comparison currently skips because Perl `git-svn` is unavailable.

### P1

- Native `do_update` and `do_switch` are production-backed; MD5 base/result checksums, absent/abort callbacks, persistent unhandled metadata, and immediate native callback errors are implemented.
- Test callback recorders are operation-owned, the global serialization lock is removed, auth prompt panics are contained, and the linked default-parallel suite passes. A full production callback/error audit remains before claiming all FFI callbacks panic-free.
- Working-copy production dcommit preserves full messages and executes `DcommitPlanBuilder`/`SvnCommitEditor` through the recovery coordinator. Exact plan-projected tree verification accounts for SVN properties/keywords and `.gitattributes` mappings that intentionally differ from the original local commit.
- Dcommit target selection uses the nearest first-parent rev_map identity with footer URL/UUID/revision validation; local merge ranges and dirty pre-submit worktrees are rejected. Broader commit-URL intent/auth profile validation remains incomplete.
- The `Ready -> submit -> Submitted` persistence gap is now fail-closed through a durable pre-submit in-flight marker. Automatic resubmission is prohibited when the outcome is unknown; an explicit, evidence-based manual reconciliation/adoption path remains to be defined.
- Broader rebase behavior remains incomplete after Log range/passthrough formatting, scoped `find-rev`, explicit noMetadata limitations, and reset transaction recovery were implemented.
- Fetch-time authors/filters/localtime/metadata/empty-dir options overlay persisted config; identity-changing overrides are rejected after import. `log-window-size` and `fetch --parent` now fail explicitly until implemented.
- Strict fetch revision forms (`N`, `N:M`, `HEAD`, `BASE:N`, `N:HEAD`, `BASE:HEAD`) are implemented; BASE uses the slowest configured mapping for the selected remote/UUID.
- Fetcher still lacks binary-property transport, complete persistent-placeholder/follow-parent behavior, and recoverable multi-artifact publication.
- Ref/rev_map publication is not a complete recoverable transaction.
- Migration is inspection only.

## Capability Profile State

| Profile | Audited state |
|---|---|
| `file://` + SVN CLI standard layout read | `behavior-pass` for the covered local fixture only |
| `file://` + SVN CLI single-path read | `behavior-pass` for the covered local fixture only |
| local `svn://` + SVN CLI read/write | `behavior-pass` for covered explicit-credential fixtures only |
| linked libsvn file/svn metadata and log replay | `behavior-pass` for covered local fixtures only |
| linked libsvn true delta update/switch | `behavior-pass` for covered file/svn fixtures, including unhandled metadata and callback failure |
| HTTP(S) read | accepted/unvalidated; no support claim |
| HTTP(S) write | not implemented |
| svn+ssh | accepted/unvalidated or deferred; no support claim |
| strict frozen-baseline compatibility | not passed |

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

### Golden harness

- `1461caf`, `2241af4`, `7927615`, `7424b18`, `b4765c7`: artifact/rev_map comparison series.
- `4e597f6`, `a5f99cc`, `917309c`, `0ec6461`, `df65538`, `980c98e`: config, UUID, properties, tree, and log-range extensions.

### Linked libsvn

- `16e024f`, `fdd1a43`, `e1416d9`, `a65b2c4`: link/version/session/log foundation.
- `c1240484`, `9f1aa8b7`, `19328574`, `2fb35a8`, `5e20caed`: RA surface and command integration through log-backed editor replay.
- `b5e9659` through `48bd2119`: native delta/reporter/txdelta scaffolding.
- `8bf5258` through `613efa8`: private test-only delta-to-`FetchEditor` adapter.

## Corrected Next Steps

Do not continue protocol breadth or additional callback coverage before these steps:

1. Phase 2/5: implement canonical URL/session/mapping paths and add the failing single-subdirectory fixture.
2. Phase 5/8: import real SVN timestamps/timezones and compare exact graph identity.
3. Phase 5/8: implement default clone branch/HEAD/worktree and `--no-checkout` behavior.
4. Phase 1/2/5: complete revision forms and make every inert option implemented or explicitly rejected.
5. Phase 3/7: replace dcommit target resolution with first-parent validated identity before any further write support.
6. Phase 8: replace weak object/clone normalizers and provision non-skippable Perl compatibility CI.
7. Phase 7: make all production sinks execute one `DcommitPlan` and add partial-success resume.
8. Phase 6/3: finish scoped readonly resolution and migration policy.
9. Phase 4/5: define binary-property transport, finish persistent-placeholder/follow-parent behavior, and validate HTTP(S)/svn+ssh profiles.

The first four corrected P0 items were completed on 2026-07-12; preserve their regression tests while continuing with exact golden identity and dcommit preflight.

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
- Dcommit now preserves complete `%B` messages and both mock and working-copy sinks execute the shared typed plan. Production file/svn write-back is connected to `CommitSink`/`PostSubmit`, exact rev_map/ref/footer and plan-projected tree verification, durable active-journal reconstruction, clean and repository-UUID preflight, and retry-safe in-flight/submitted/fetched/rebase states. Real file-SVN recovery proves post-fetch failure does not duplicate submission, an in-flight restart refuses ambiguous resubmission, and a two-repository regression proves a wrong `--commit-url` cannot write. Next, define explicit manual reconciliation for ambiguous outcomes and broaden remote/auth profile validation before claiming the Phase 7 behavioral gate.
- `find-rev` no longer recursively aggregates every rev_map. HEAD/commit identity and an optional explicit tree-ish select one validated mapping; the readonly 41/41 and golden 25/25 suites cover branch commit lookup and same-numbered trunk/branch revision scoping.
- Frozen documentation defines noMetadata as one-shot and explicitly excludes log. Real initial import plus readonly 42/42 now prove footer-free import succeeds, later mutating operations fail before state changes, and rev_map-backed find-rev/info remain usable.
- Reset now persists its old/new ref and rev_map target before CAS. Unit recovery covers crashes on either side of the ref update and rejects journal paths outside `.git/svn`; the full workspace passes with resolver-wide pending-reset guards.
- Log now reads full Git author identity plus epoch, reverses configured authors-file identities when unique, emits the frozen local SVN date shape, preserves message boundaries, follows numeric range direction, pads oneline revisions, preserves stat/raw/patch blocks, and feeds author/date/count/message/path into golden comparison. Strict Perl execution is still unavailable, so external exactness is unproven.
