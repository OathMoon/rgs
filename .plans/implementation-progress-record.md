# git-svn-rs Implementation Progress Record

Last audited: 2026-07-10
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
| 1 workspace/CLI | `structural-pass` | workspace, CLI, shim, diagnose, unsupported commands | inert options and global verbosity contract |
| 2 config/mapping | `structural-pass` | basic config/glob/authors/filter/layout units | URL/session path model, full layout URLs, metadata runtime semantics |
| 3 metadata/rev_map | `structural-pass` | SHA-1/SHA-256 rev_map, locks/fsync/reset, gitfile discovery | read/create split, transaction/recovery, ambiguity, real migration |
| 4 SVN adapters | `in-progress` | SVN CLI, linked RA metadata/log/auth, test-only native delta adapter | public true delta path, FFI isolation/panic safety, parallel stability |
| 5 import/clone/fetch | `in-progress` | broad local stdlayout file/svn replay | single-path clone, real timestamps, checkout, unified editor, full Fetcher semantics |
| 6 readonly | `in-progress` | common find-rev/info/log/reset/rebase/gc subset | scoped multi-ref resolver, tree-ish, remaining Log/noMetadata behavior |
| 7 dcommit | `in-progress` | mock editor plus local file/svn working-copy write-back | first-parent safety, shared production plan, full messages, recovery, remote profiles |
| 8 golden/release | `structural-pass` | broad fixture/capture harness | strict Perl execution and exact graph/clone-state artifacts |

## Validated Capabilities

### Foundation

- Rust workspace with CLI, core library, and opt-in `git-svn` shim.
- Typed core command surface and explicit v1 unsupported commands.
- Basic Git CLI wrapper, config serialization, `GlobSpec`, authors, filters, URL helpers, and metadata option conflict checks.
- Binary rev_map primitives for SHA-1/SHA-256, zero records, append order, locks, fsync, reset, gitfile, and commondir.

### Local read/import preview

- Standard-layout `file://` and local `svn://` clone/fetch fixtures cover trunk, branches, tags, copies, deletes, modes, symlinks, filters, authors, rewrite metadata, revision ranges, empty placeholders, and incremental anchors.
- Default build uses SVN CLI enriched log replay.
- Linked build can read RA metadata/log/file/directory properties and route command import through `RaSession`/`SvnFetchEditor`, but public `do_update` is still log-backed replay.
- A private test-only native `svn_ra_do_update3` adapter covers initial/incremental content, properties, deletes, and nested directory callbacks.

### Readonly preview

- Common flows exist for `find-rev`, `info`, `log` modes/ranges/pathspecs, `gc`, `reset`, and `rebase --dry-run` on supported metadata layouts.
- These flows are not yet safe to call multi-ref compatible because global rev_map scanning and target resolution remain incomplete.

### Local write preview

- Mock write-back uses the planned diff/commit editor units.
- Local `file://` and local authenticated `svn://` write-back use an SVN working copy and cover many file/property cases, post-fetch, and rebase.
- Production write-back does not yet execute the shared editor plan and common remote schemes remain unsupported.

### Golden infrastructure

- Deterministic SVN fixture and artifact capture/comparison infrastructure exists.
- Current artifacts cover many configs, ref names, footers, rev_map shapes, modes, properties, tree content, and readonly outputs.
- Existing normalization is too weak for an exact compatibility claim and is scheduled for Phase 8 replacement.

## Audited Critical Gaps

### P0

1. Default single-subdirectory clone such as `file:///repo/trunk` can request `/trunk/trunk/...` and fail.
2. Import ignores SVN dates and writes loop-index epochs, so Git commit IDs and log dates are wrong.
3. Standard-layout clone can fetch a remote ref while leaving `HEAD` unborn and the worktree empty; `--no-checkout` is inert.
4. Dcommit resolver can select a merged tracking ref with a larger SVN revision instead of the nearest first-parent SVN identity.
5. Golden comparison omits/refactors away commit OIDs, dates, authors, parent graph, rev_map OIDs, clone output, and branch/HEAD/worktree state; strict Perl comparison currently skips.

### P1

- Default CLI and linked paths use different fetch behavior models.
- Native true-delta adapter is test-only.
- Linked default-parallel tests can race on global callback state and abort through an FFI panic.
- Production dcommit bypasses `GitDiffPlanner`/`SvnCommitEditor`, truncates messages, and has no partial-success journal.
- `find-rev` can flatten unrelated rev_maps and lacks optional tree-ish scope.
- `localtime`, `log-window-size`, `fetch --parent`, revision keywords, fetch-time option overlay, and metadata modes are incomplete.
- Fetcher lacks complete checksum/absent/unhandled-property/path-encoding/persistent-placeholder/follow-parent behavior.
- Ref/rev_map publication is not a complete recoverable transaction.
- Migration is inspection only.

## Capability Profile State

| Profile | Audited state |
|---|---|
| `file://` + SVN CLI standard layout read | `behavior-pass` for the covered local fixture only |
| `file://` + SVN CLI single-path read | `in-progress` due duplicated-path failure |
| local `svn://` + SVN CLI read/write | `behavior-pass` for covered explicit-credential fixtures only |
| linked libsvn file/svn metadata and log replay | `behavior-pass` for covered local fixtures only |
| linked libsvn true delta | `structural-pass`; test-only |
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
7. Phase 4/5: make FFI callbacks panic-free, remove global test state, promote true delta to production, then converge the CLI adapter.
8. Phase 7: make all production sinks execute one `DcommitPlan` and add partial-success resume.
9. Phase 6/3: finish scoped readonly resolution and migration policy.

## Handoff Notes

- The v2 plan rewrite is documentation-only; no business code was changed.
- The audit report is intentionally retained as evidence, while the v2 plan files are the current authority.
- An aborted linked test left two untracked `crates/git-svn-rs-core/golden-stdlayout-*` directories during the audit; they are test artifacts, not source. Cleanup was not performed after the filesystem action was denied, so verify workspace status before the next implementation commit.
- Preserve the pre-existing untracked `.codex/` directory.
