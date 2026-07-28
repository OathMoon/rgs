# git-svn-rs Implementation Progress Record

Last audited: 2026-07-28
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `97735da Align Log.pm timezone and revision selection`
Latest implementation commits:

- `f11ecd1 Complete ref safety and linked replay parity`
- `fa5f81f Fail closed on unvalidated SVN protocols`
- `9f4b223 Harden readonly log and rebase compatibility`
- `b7d6855 Verify binary SVN properties end to end`
- `97735da Align Log.pm timezone and revision selection`

This is the concise handoff record. Product requirements live in
`.plans/git-svn-rs-plan.md`; architecture and ordering live in
`.plans/00-git-svn-rs-review-and-roadmap.md`.

## Status Vocabulary

- `not-started`
- `in-progress`
- `structural-pass`
- `behavior-pass`
- `release-pass`

Do not use an unqualified “complete” or “supported”. Skipped external checks cannot
produce `release-pass`.

## Current Overall State

The repository now provides an initially complete local core workflow for the
covered `file://`, local authenticated `svn://`, and mock profiles. It remains a
preview rather than a general `git svn` replacement: HTTP(S) and `svn+ssh` are
fail-closed, some Log.pm/rebase modes remain incomplete, and the required hosted
compatibility workflow has not yet had its first successful run.

| Phase | State | Current evidence | Main gap |
|---|---|---|---|
| 1 workspace/CLI | `structural-pass` | CLI, core, opt-in shim, diagnostics, explicit unsupported commands | inert options and global verbosity exactness |
| 2 config/mapping | `structural-pass` | layouts, globs, authors, filters, reversible ref sanitization | remaining option/layout edge semantics |
| 3 metadata/rev_map | `behavior-pass` for covered local profiles | SHA-1/SHA-256 maps, locks/fsync, canonical metadata paths, legacy fallback, transactional publication/recovery | broader migration and remote ambiguity policy |
| 4 SVN adapters | `behavior-pass` for covered local profiles | common editor contract, CLI and linked delta replay, raw binary properties including invalid UTF-8 E2E | full FFI callback audit and remote transport validation |
| 5 import/clone/fetch | `behavior-pass` for covered local profiles | stdlayout/direct URL replay, copies/follow-parent, bounded fetch, collisions, linked CLI parity | remaining obscure Fetcher semantics |
| 6 readonly | `in-progress` | scoped find-rev/info/log/reset/gc/rebase; tree-ish/revision anchors and merge strategy contract | remaining Log.pm formatting modes |
| 7 dcommit | `behavior-pass` for covered local profiles | typed plans, durable recovery, local file/svn exact write comparisons | remote write-back and broader recovery faults |
| 8 golden/release | `behavior-pass` | strict frozen Perl 2.54.0 suite passes 40/40 locally; Linux workflow defined | first hosted execution |

## Validated Capabilities

### Foundation and metadata

- Rust workspace with `git-svn-rs`, reusable core library, and opt-in `git-svn`
  compatibility shim.
- Typed command surface, explicit v1 exclusions, config serialization, mapping
  globs, authors, filters, URL helpers, and metadata option conflict checks.
- SHA-1/SHA-256 rev_maps support zero records, non-creating reads, append ordering,
  OS locks, fsync, reset, gitfiles, and commondir.
- Legacy rev_db/v0-v2/mixed layouts and multi-UUID ambiguity fail closed without
  mutation.
- New metadata uses `.git/svn/<full-ref>`; an existing flattened layout remains
  readable, while mixed canonical/legacy identity is rejected.
- Git::SVN-compatible ref sanitization is reversible. Candidate and existing
  file/directory ref namespace collisions fail before publication; duplicate fixed
  mappings for one SVN path use deterministic last-wins behavior.

### Import, clone, and fetch

- Local `file://`, authenticated local `svn://`, and mock fixtures cover direct
  `/trunk`, standard layout, branches/tags, copies/deletes, modes, symlinks,
  authors, filters, rewrite metadata, revision ranges, and checkout/no-checkout.
- CLI and linked libsvn use the common `RaSession`/`FetchEditor` coordinator.
  Linked stdlayout copy replay and direct subdirectory sessions pass the same
  34-case CLI suite as the default backend.
- Native update/switch handles initial and incremental text deltas, copy-only
  files, directory/file properties, checksums, absent nodes, deletes, and callback
  error conversion.
- CLI base64 properties and libsvn `svn_string_t.data/len` retain arbitrary bytes.
  Unknown property values are stored byte-exactly and URI-encoded in
  `unhandled.log`; a real `file://` fixture verifies invalid UTF-8 bytes end to
  end through filtered replay.
- Imports preserve SVN timestamps and identities, including authors mappings,
  localtime, metadata rewrite, and noMetadata one-shot behavior.
- Multi-mapping publication stages commits and atomically journals ref, rev_map,
  and unhandled-log publication. Restart resumes only unfinished mappings.
- Empty-directory ownership, bounded log windows, `fetch --parent`, copy-source
  backfill, auxiliary `branch@rev` refs, ancestor directory copies, wildcard
  discovery high-water, ignore-refs scope, and trailing-zero scan markers match
  the covered frozen Perl artifacts.
- HTTP(S) and `svn+ssh` fetch are explicitly deferred and fail before SVN metadata
  creation or import recovery. mock/file/svn remain the accepted profiles.

### Readonly and maintenance

- `find-rev` selects one validated mapping identity rather than flattening all
  rev_maps.
- `info`, supported SVN-style `log` ranges/modes/pathspecs, conservative `gc`,
  recoverable `reset`, and current-parent selective `rebase` are implemented.
- Log preserves author/date/message framing, numeric direction, oneline revision
  width, and record-safe stat/raw/patch passthrough for covered cases.
- Log resolves an explicit pre-pathspec tree-ish to its own tracking identity,
  anchors exact revisions through that identity's rev_map, counts only showable
  SVN records for `--limit`, suppresses adjacent duplicate revisions, and emits
  the frozen separator for an empty normal result.
- Log uses Git's configured abbreviated object name, preserves the author's
  recorded timezone, and excludes forged footer revisions absent from the
  selected rev_map for exact and range queries.
- Rebase accepts both frozen `-m`/`-M` merge forms and passes merge/strategy
  arguments to Git in the frozen order.
- Reset uses expected-old CAS plus a durable transaction; resolver-backed commands
  fail closed while reset/import recovery is pending.

### Dcommit

- Mock and local file/svn write-back execute a shared typed plan including
  add/modify/delete/copy/move, symlink, executable, mergeinfo, and SVN properties.
- The durable journal stores the whole oldest-first queue with pre-submit
  in-flight state, stable fingerprints, exact remote-head checks, submitted
  recovery, fetch verification, and final rebase/no-rebase state.
- Dirty worktrees, merge ranges, wrong repository UUIDs, ambiguous submissions,
  multiple active journals, and completed-ledger overlap fail closed.
- `--adopt-revision` resumes an unknown-outcome local file/svn submission only
  after exact imported-tree verification.
- SVN subprocesses are non-interactive and apply persisted/CLI auth and config
  options consistently.
- Non-dry-run HTTP(S), `svn+ssh`, unsupported, or incompatible write profiles fail
  before journal discovery/lock or write preparation. Dry-run remains descriptive.

### Golden and release evidence

- The strict frozen Perl Git 2.54.0 suite passes 40/40 locally.
- Exact artifacts cover ref tips, reachable graph/OIDs, rev_maps, configs,
  HEAD/index/worktree, tree contents/modes/properties, readonly outputs, clone
  output, local file/svn writes, submitted recovery, and dirty no-write behavior.
- Each exact scenario writes a JSON summary and retains normalized Perl/Rust
  artifacts.
- `.github/workflows/compatibility.yml` installs frozen Git/SVN/libsvn and runs
  workspace, linked parallel/serial, linked CLI, formatting, clippy, and strict
  compatibility gates. `scripts/verify.ps1 -StrictCompat` mirrors these local gates.

## Current Verification

Verified on 2026-07-28:

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo test -p git-svn-rs --test readonly_commands -- --test-threads=1` (56/56)
- `GIT_SVN_RS_STRICT_LIBSVN=1 cargo test -p git-svn-rs-core --features svn-libsvn`
- `GIT_SVN_RS_STRICT_LIBSVN=1 cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn -- --nocapture --test-threads=1` (35/35)
- `GIT_SVN_RS_STRICT_COMPAT=1 GIT_SVN_RS_COMPAT_ARTIFACT_DIR=/tmp/git-svn-rs-current-artifacts cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (40/40)
- `cargo clippy --all-targets --all-features -- -D warnings`
- `git diff --check`

One full linked-core run saw a transient authenticated-svnserve connection refusal;
the immediate focused rerun passed, and the subsequent full linked-core run passed
134/134 unit tests and continued successfully through the integration suites.

## Remaining Work

### P0

1. Run the required hosted compatibility workflow and retain its artifacts. The
   current environment has no GitHub credentials, so this is externally blocked
   until the branch is pushed and the workflow can be dispatched.

### P1

- Complete the remaining frozen Log.pm modes and rebase merge/strategy semantics.
- Audit every production libsvn callback for panic/error ownership before a broad
  native safety claim.
- Validate HTTP(S) DAV/SSL and `svn+ssh` with dedicated fixtures before enabling
  either read profile.
- Extend dcommit recovery fault injection and commit-URL/auth intent coverage.
- Review remaining inert CLI options and either implement or reject them explicitly.

## Important Commit Anchors

- `7f531ee`, `ab52ef1`: CLI, workspace, config/mapping foundations.
- `edb9161`: SHA-256 rev_map import/fetch.
- `9d354f6`, `e3b8cf9`, `4bf9205`: shared replay and durable dcommit coordinator.
- `7e27bcf`, `19ad4ef`: in-flight dcommit and recoverable reset.
- `8405e79`: recoverable staging-ref import publication foundation.
- `f11ecd1`: ref/path collision safety, canonical metadata, linked copy parity,
  binary properties, release gates, and README refresh.
- `fa5f81f`: remote URL profiles, early fail-closed fetch/dcommit, and uniform
  non-interactive SVN writes.
- `9f4b223`: tree-ish/rev_map-scoped Log.pm selection and framing plus the frozen
  rebase merge/strategy command contract.
- `b7d6855`: real invalid-UTF-8 SVN property replay and filtered-editor byte
  callback forwarding.
- `97735da`: rev_map-bounded Log.pm ranges, author timezone preservation, and
  configured object abbreviation.

## Next Steps

Continue in this order unless new verification changes priority:

1. Phase 6: close the highest-value missing Log.pm/rebase compatibility slice.
2. Phase 4: finish the production callback audit.
3. Phase 7: broaden recovery fault injection and commit-URL intent validation.
4. Phase 8: run hosted CI when credentials/external execution are available.

## Handoff Notes

- Preserve pre-existing untracked `.codex/`, `.zcode/`, `CLAUDE.md`, `docs/`, and
  generated `golden-stdlayout-*`/`svn-fixture-*` directories.
- Do not infer general remote support from the SVN CLI's underlying scheme support;
  the public command path intentionally gates unvalidated HTTP(S)/svn+ssh profiles.
- The linked backend is a read/import backend. Dcommit still uses the SVN CLI
  working-copy sink for the covered local write profiles.
- Migration remains inspection/rejection rather than automatic conversion.
