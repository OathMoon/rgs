# git-svn-rs Implementation Progress Record

Last audited: 2026-07-15
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `b86e4f2cc47338f3290025038f31edf03b27ffab`
Latest implementation commit: `613efa8 feat: bridge native incremental file deltas`

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
| 6 readonly | `in-progress` | common find-rev/info/log/reset/rebase/gc subset | scoped multi-ref resolver, tree-ish, remaining Log/noMetadata behavior |
| 7 dcommit | `in-progress` | first-parent target, full messages, typed plan builder/editor units, journal state/store, local file/svn write-back | production plan/sink convergence, recovery coordinator, remote profiles |
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
- These flows are not yet safe to call multi-ref compatible because global rev_map scanning and target resolution remain incomplete.

### Local write preview

- Mock write-back uses the planned diff/commit editor units.
- Local `file://` and local authenticated `svn://` write-back use an SVN working copy and cover many file/property cases, post-fetch, and rebase.
- Typed raw diffs preserve A/M/D/C/R/T modes, object IDs, similarity, and both paths. The common plan builder materializes final copy/move content, symlink encoding, mode-property set/delete operations, mergeinfo, and stable raw metadata; editor operation failures abort without closing.
- The versioned dcommit journal stores the whole oldest-first queue with `Queued`/`Ready`/`Submitted`/`FetchedVerified` ordering, atomic generation snapshots, corruption fallback, and an exclusive lock. It is not yet connected to a recovery coordinator.
- Production write-back does not yet execute the shared editor plan and common remote schemes remain unsupported.

### Golden infrastructure

- Deterministic SVN fixture and artifact capture/comparison infrastructure exists.
- Current artifacts cover exact ref tips, remote-reachable commit graph identity, rev_map object IDs, default/no-checkout clone HEAD/index/worktree state, configs, modes, properties, tree content, and readonly outputs.
- Find-rev and show-commit artifacts now retain exact commit IDs, and clone output retains status/stdout/stderr. Remaining log/output normalization and missing strict Perl execution still prevent an exact compatibility claim.

## Audited Critical Gaps

### P0

1. Strict Perl comparison currently skips because Perl `git-svn` is unavailable; several presentation normalizers still omit log author/date details.

### P1

- Native `do_update` and `do_switch` are production-backed; MD5 base/result checksums, absent/abort callbacks, persistent unhandled metadata, and immediate native callback errors are implemented.
- Test callback recorders are operation-owned, the global serialization lock is removed, auth prompt panics are contained, and the linked default-parallel suite passes. A full production callback/error audit remains before claiming all FFI callbacks panic-free.
- Production dcommit preserves full messages but still bypasses the common `DcommitPlanBuilder`/`SvnCommitEditor`; the durable journal model exists but has no production recovery coordinator.
- Dcommit target selection now uses the nearest first-parent rev_map identity with footer URL/UUID/revision validation; local merge ranges are rejected before write. Broader stale/dirty/commit-URL preflight remains incomplete.
- `find-rev` can flatten unrelated rev_maps and lacks optional tree-ish scope.
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
- `cargo test --workspace` passes after the Phase 7 foundation slice (about 458 seconds). `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `git diff --check` also pass.

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
- Dcommit now preserves complete `%B` messages in mock and working-copy sinks. Typed raw `T` changes and non-UTF-8 rejection are tested; the plan builder preserves final copy/move bytes and explicit executable/special transitions. Next, move `.gitattributes`/auto-props into the builder, add a working-copy `CommitSink`, then connect the journal coordinator with fault injection before switching production.
