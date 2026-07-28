# git-svn-rs Implementation Progress Record

Last audited: 2026-07-28
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `0f5ac1e Harden svn+ssh tunnel fixture boundaries`
Latest implementation commits:

- `d42efb3 Harden dcommit recovery intent`
- `ea77681 Support local-only rebase mode`
- `1f87814 Reject inert global output options`
- `dbf6dc8 Fix sparse reset parent selection`
- `29d2545 Reject ambiguous fetch scope early`
- `a820e13 Reject mock commit URL overrides`
- `cdc463e Preserve merge topology during rebase`
- `caeb41d Support frozen log color mode`
- `3bded4a Match frozen rebase dry-run identity`
- `fc2c420 Complete scoped rebase orchestration options`
- `f16c366 Handle frozen non-interactive log pager mode`
- `9cdd6ec Advance readonly phase to covered behavior pass`
- `d270051 Validate svn+ssh clone and fetch transport`
- `0f5ac1e Harden svn+ssh tunnel fixture boundaries`

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

The repository now provides an initially complete core workflow for the covered
`file://`, local authenticated `svn://`, configured external `svn+ssh` tunnel,
and mock profiles. It remains a preview rather than a general `git svn`
replacement: HTTP(S), real OpenSSH authentication/trust, and remote dcommit are
not validated; interactive TTY paging/output streaming remains incomplete; and
the required hosted compatibility workflow has not yet had its first successful
run.

| Phase | State | Current evidence | Main gap |
|---|---|---|---|
| 1 workspace/CLI | `structural-pass` | CLI, core, opt-in shim, diagnostics, explicit unsupported/global output options | remaining option/layout edge semantics |
| 2 config/mapping | `structural-pass` | layouts, globs, authors, filters, reversible ref sanitization | remaining option/layout edge semantics |
| 3 metadata/rev_map | `behavior-pass` for covered local profiles | SHA-1/SHA-256 maps, locks/fsync, canonical metadata paths, legacy fallback, transactional publication/recovery | broader migration and remote ambiguity policy |
| 4 SVN adapters | `behavior-pass` for covered file/svn/configured-tunnel profiles | common editor contract, audited fail-closed FFI callbacks, CLI/linked delta replay, invalid UTF-8 properties and external svn+ssh tunnel E2E | HTTP(S) and real OpenSSH validation |
| 5 import/clone/fetch | `behavior-pass` for covered local profiles | stdlayout/direct URL replay, copies/follow-parent, bounded fetch, collisions, linked CLI parity | remaining obscure Fetcher semantics |
| 6 readonly | `behavior-pass` for covered non-interactive profiles | scoped queries/log/reset/gc plus option-complete rebase | TTY pager and successful stderr stream fidelity |
| 7 dcommit | `behavior-pass` for covered local profiles | typed plans, durable recovery, local file/svn exact write comparisons | remote write-back and broader recovery faults |
| 8 golden/release | `behavior-pass` | strict frozen Perl 2.54.0 suite passes 40/40 locally; Linux workflow defined | first hosted execution |

## Validated Capabilities

### Foundation and metadata

- Rust workspace with `git-svn-rs`, reusable core library, and opt-in `git-svn`
  compatibility shim.
- Typed command surface, explicit v1 exclusions, config serialization, mapping
  globs, authors, filters, URL helpers, and metadata option conflict checks.
- Global `-q`/`--quiet` and `-v`/`--verbose` fail explicitly instead of being
  parsed and silently ignored.
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

- Local `file://`, authenticated local `svn://`, configured `svn+ssh` tunnel, and
  mock fixtures cover direct `/trunk`, standard layout, branches/tags,
  copies/deletes, modes, symlinks, authors, filters, rewrite metadata, revision
  ranges, and checkout/no-checkout.
- CLI and linked libsvn use the common `RaSession`/`FetchEditor` coordinator.
  Linked stdlayout copy replay and direct subdirectory sessions pass the same
  34-case CLI suite as the default backend.
- Native update/switch handles initial and incremental text deltas, copy-only
  files, directory/file properties, checksums, absent nodes, deletes, and callback
  error conversion.
- Production libsvn callbacks validate required baton/output/pool and
  string-data/length pairs, return owned errors on invalid lifecycle state, catch
  log receiver panics, and avoid panic-based error construction. Non-null dangling
  pointers remain an unavoidable libsvn ABI responsibility.
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
- `svn+ssh` read paths accept case-insensitive schemes. A persisted temporary SVN
  tunnel config drives real `svnserve -t`, validates its exact invocation, and
  covers direct clone plus incremental fetch in default and linked modes.
- HTTP(S) fetch remains deferred and fails before SVN metadata creation or import
  recovery. `svn+ssh` dcommit remains rejected before write preparation.
- Ambiguous `fetch REMOTE --fetch-all` and `fetch --parent --fetch-all`
  combinations fail before metadata or recovery side effects.

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
- Log accepts the frozen `-A`/`--authors-file` display mapping and gives the
  command-line file precedence over the persisted remote configuration.
- Verbose Log.pm paths remain repository-relative, scored rename/copy records
  follow the frozen omission behavior, and golden normalization no longer hides
  an erroneous SVN path prefix. Repeated trailing blank message lines collapse
  with frozen line-count framing.
- Log supports frozen `--non-recursive`; default invocation carries Git's `-r`
  recursive-diff flag and the option removes it explicitly.
- Log supports typed `--color` and frozen `color.diff` automatic selection,
  preserving ANSI diff/stat/raw output without corrupting SVN record parsing.
- Rebase accepts both frozen `-m`/`-M` merge forms and passes merge/strategy
  arguments to Git in the frozen order.
- Rebase supports frozen `-l`/`--local`, skipping remote fetch while retaining
  resolver, clean-worktree, and tracking-branch checks.
- Rebase supports frozen `-p`/`--rebase-merges`; a real local merge graph verifies
  that rebasing onto the selected tracking ref retains one merge commit.
- Rebase dry-run reports the selected remote branch and full SVN URL exactly like
  frozen Perl; strict golden comparison now retains both identity lines.
- Rebase command-local verbosity and `--fetch-all/--all` match the frozen order.
  Fetch-all is confined to the initially resolved svn-remote's mappings, ignores
  unrelated remotes, and retains the original upstream identity after fetch.
- Log accepts typed `--pager=<value>` as a frozen no-op when stdout is not a TTY,
  rather than passing it incorrectly to `git log`; an explicit TTY pager request
  fails clearly until interactive streaming is implemented.
- Reset `--parent` selects the nearest earlier nonzero rev_map record, including
  sparse histories, while exact reset remains exact.
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
- Recovery fingerprints include explicit commit-URL override intent under a
  versioned v2 encoding. A non-advancing sink revision remains durably in-flight
  and ambiguous, so retry cannot resubmit it.
- Authenticated svnserve preflight failure is verified to leave neither an SVN
  revision nor a dcommit journal.
- SVN subprocesses are non-interactive and apply persisted/CLI auth and config
  options consistently.
- Non-dry-run HTTP(S), `svn+ssh`, unsupported, or incompatible write profiles fail
  before journal discovery/lock or write preparation. Dry-run remains descriptive.
- Mock write-back rejects `--commit-url` before journal, lock, or remote mutation
  because the mock sink cannot honor a URL override.

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
- `cargo test -p git-svn-rs --test readonly_commands -- --test-threads=1` (63/63)
- `cargo test -p git-svn-rs --test dcommit_linear -- --test-threads=1` (46/46)
- `GIT_SVN_RS_STRICT_LIBSVN=1 cargo test -p git-svn-rs-core --features svn-libsvn`
- `cargo test -p git-svn-rs --test clone_fetch_real_svn -- --nocapture --test-threads=1` (36/36)
- `GIT_SVN_RS_STRICT_LIBSVN=1 cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn -- --nocapture --test-threads=1` (36/36)
- `GIT_SVN_RS_STRICT_COMPAT=1 GIT_SVN_RS_COMPAT_ARTIFACT_DIR=/tmp/git-svn-rs-current-artifacts cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (40/40)
- `cargo clippy --all-targets --all-features -- -D warnings`
- `git diff --check`

One earlier linked-core run saw a transient authenticated-svnserve connection
refusal; the immediate focused rerun passed. The callback audit's subsequent
strict linked-core run passed 140/140 unit tests and all integration suites.

## Remaining Work

### P0

1. Run the required hosted compatibility workflow and retain its artifacts. The
   current environment has no GitHub credentials, so this is externally blocked
   until the branch is pushed and the workflow can be dispatched.

### P1

- Add real TTY pager execution and preserve successful Git rebase stderr/progress
  streaming for release-level output fidelity.
- Validate HTTP(S) DAV/SSL and real OpenSSH key/host-trust behavior with dedicated
  fixtures; the configured external-tunnel protocol path is already covered.
- Extend dcommit recovery fault injection and commit-URL/auth intent coverage.
- Decide whether an explicit `--placeholder-filename` without
  `--preserve-empty-dirs` should fail rather than remain a low-risk no-op.

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
- `2a68582`: runtime Log.pm authors-file override with CLI and output coverage.
- `57eb773`: frozen repository-relative verbose paths and unmasked golden checks.
- `08eef18`: fail-closed libsvn callback inputs, lifecycle, allocation, property,
  panic, and owned-error boundaries.
- `32dd27a`: frozen trailing-blank collapse and message line counts.
- `cf90c68`: frozen recursive/non-recursive Git log argument contract.
- `d42efb3`: commit-URL recovery intent, versioned fingerprints, non-advancing
  submission ambiguity, and authenticated no-write preflight evidence.
- `ea77681`, `dbf6dc8`: local-only rebase and sparse rev_map parent reset.
- `1f87814`, `29d2545`, `a820e13`: early rejection of inert global output
  options, ambiguous fetch scope, and unsupported mock commit-URL overrides.
- `cdc463e`, `caeb41d`: frozen merge-topology rebase and Log.pm color behavior.
- `3bded4a`: exact frozen rebase dry-run tracking identity and stronger golden
  evidence.
- `fc2c420`: scoped fetch-all, command-local verbose, and fixed upstream identity
  across rebase orchestration.
- `f16c366`: frozen non-TTY pager no-op with an explicit interactive boundary.
- `d270051`, `0f5ac1e`: case-insensitive `svn+ssh` read routing and hardened
  external-tunnel clone/fetch evidence without widening dcommit.

## Next Steps

Continue in this order unless new verification changes priority:

1. Phase 4: add HTTP DAV and real OpenSSH authentication/trust fixtures.
2. Phase 7: broaden recovery fault injection and commit-URL intent validation.
3. Phase 6 release gap: add PTY pager and successful stderr stream evidence.
4. Phase 8: run hosted CI when credentials/external execution are available.

## Handoff Notes

- Preserve pre-existing untracked `.codex/`, `.zcode/`, `CLAUDE.md`, `docs/`, and
  generated `golden-stdlayout-*`/`svn-fixture-*` directories.
- Configured `svn+ssh` external-tunnel reads are covered, but this does not imply
  validated OpenSSH authentication/host trust or `svn+ssh` dcommit. HTTP(S) remains
  gated.
- The linked backend is a read/import backend. Dcommit still uses the SVN CLI
  working-copy sink for the covered local write profiles.
- Migration remains inspection/rejection rather than automatic conversion.
