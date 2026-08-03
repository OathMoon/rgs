# git-svn-rs Implementation Progress Record
Last audited: 2026-08-01
Branch: `codex-execute-git-svn-rs-plans`
Committed HEAD at audit: `0303507 Validate remote dcommit profiles`
Product requirements and ordering live in `.plans/git-svn-rs-plan.md` and
`.plans/00-git-svn-rs-review-and-roadmap.md`.

## Current Overall State

The repository provides an initially complete core workflow for covered `file://`,
local `svn://`, configured and real loopback `svn+ssh`, mock, and authenticated
loopback HTTP/HTTPS DAV reads and writes. Hosted CI remains available only by
explicit manual dispatch.

| Phase | State | Current evidence | Main gap |
|---|---|---|---|
| 1 workspace/CLI | `structural-pass` | CLI, core, opt-in shim, diagnostics, explicit unsupported/global output options | remaining option/layout edge semantics |
| 2 config/mapping | `structural-pass` | layouts, globs, authors, filters, ref sanitization, svnsync identity config | remaining encoding/platform edge semantics |
| 3 metadata/rev_map | `behavior-pass` for covered local profiles | SHA-1/SHA-256 maps, locks/fsync, canonical paths, identity resolution, recovery, exact v0-v5 rejection | automatic migration/typed errors |
| 4 SVN adapters | `behavior-pass` for covered file/svn/SSH/loopback-HTTP(S) profiles | shared replay, per-file RA-serf delta batons, audited FFI, byte-safe revprops, cache-first TTY auth, real OpenSSH and authenticated DAV E2E | broader remote/platform validation |
| 5 import/clone/fetch | `behavior-pass` for covered local profiles | stdlayout/direct URL replay, copies/follow-parent, bounded fetch, collisions, linked CLI parity | remaining obscure Fetcher semantics |
| 6 readonly | `behavior-pass` for covered profiles | scoped queries/log/reset/gc, option-complete rebase with streamed progress, and PTY pager | broader platform terminal fidelity |
| 7 dcommit | `behavior-pass` for covered profiles | typed plans, v4 recovery, stale-target preflight, file/svn/HTTP(S)/configured and real-SSH exact writes | broader remote faults |
| 8 golden/release | `behavior-pass` | strict frozen Perl 2.54.0 suite passes 41/41 locally; manual-only Linux workflow defined | hosted validation explicitly deferred |

## Validated Capabilities

### Foundation and metadata

- Rust workspace with `git-svn-rs`, reusable core library, and opt-in `git-svn`
  compatibility shim.
- Typed command surface, explicit v1 exclusions, config serialization, mapping
  globs, authors, filters, URL helpers, and metadata option conflict checks.
- `diagnose` reports package version, frozen Git 2.54.0 commit, platform, and
  compiled/linked libsvn state without reading secret-bearing environment values.
- Scalar `svn-remote.*` keys reject multiple values instead of silently selecting
  a different repository identity; mapping keys retain intentional multiplicity.
- Persisted remote booleans use Git's full boolean syntax and reject invalid or
  duplicate values. Relative authors files are persisted as invocation-cwd
  absolute paths, so later fetches resolve the same file with path-rich errors.
- Global `-q`/`--quiet` and `-v`/`--verbose` fail explicitly instead of being
  parsed and silently ignored.
- SHA-1/SHA-256 rev_maps support zero records, non-creating reads, append ordering,
  OS locks, fsync, reset, gitfiles, and commondir. All read/write entry points
  reject non-monotonic revisions and non-trailing zero OIDs before mutation.
- Legacy v0-v5, rev_db, missing/partial config, mixed layouts, and multi-UUID
  ambiguity receive exact actionable rejection without mutation.
- New metadata uses `.git/svn/<full-ref>`; an existing flattened layout remains
  readable, while mixed canonical/legacy identity is rejected.
- Fetch, normal info, and dcommit validate ref tip, nonzero rev_map record, and
  `git-svn-id` URL/revision/UUID agreement before their safety boundaries.
  Structurally verified importer `branch@revision` auxiliary refs remain valid.
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
- CLI `get_dir` returns immediate files/directories plus node properties; an empty
  nested branch inside an ancestor copy is discovered with its auxiliary parent.
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
- Copy-parent lookup and dependency ordering select the most specific overlapping
  mapping while retaining an empty root mapping as the final fallback.
- Auth resolves explicit credentials, askpass, then TTY username/no-echo password
  without persistence. Default reads probe SVN cache/public access before prompting;
  interactive writes confirm credentials before the first write, while non-TTY writes
  may use SVN's cache.
- Full-URL and configured `svn+ssh` paths validate exact read/write invocation.
- `useSvnsyncProps` validates byte-safe r0 source identity, atomically caches it,
  keeps mirror rev_maps separate from source footer/author/info identity, and works
  for direct/stdlayout replay; malformed/partial props publish no state.
- `useSvmProps` discovers/caches source identity byte-safely, imports per-revision
  `svm:headrev` into atomic transport/source rev_maps, supports mirror fallback and
  source-aware readonly queries; reset/dcommit reject before mutating dual maps.
- Loopback HTTP and HTTPS DAV Basic-auth fixtures cover denied no-credential clone,
  secret-safe errors, authenticated clone, and incremental fetch through both the
  SVN CLI and linked libsvn backends. HTTPS uses a self-signed CA supplied through
  an explicit SVN config directory. The strict fixtures passed locally on
  2026-08-01 using non-privileged packaged Apache; denied clone fails before
  creating `.git/svn`. RA-serf replay now assigns one native baton per interleaved
  file and consumes the 1.10+ textdelta-stream callback. Strict CI installs Apache
  and OpenSSH server dependencies.
- Authenticated loopback HTTP and HTTPS DAV dcommit completes preflight, one
  linear write, post-fetch verification, ref/rev_map publication, and exact SVN
  content validation. HTTPS reuses the explicit CA trust configuration and Basic
  credentials without persisting the password.
- Ambiguous `fetch REMOTE --fetch-all` and `fetch --parent --fetch-all`
  combinations fail before metadata or recovery side effects.
- Cross-remote fixed/wildcard and wildcard/wildcard destination intersections
  fail before migration, recovery, repository access, or metadata mutation.
- Readonly/write commands and `fetch --parent` resolve named remotes by nearest
  identity; path/rev_map ambiguity fails closed without masking a unique identity.
- Partial/full-URL layouts match frozen mapping selection. CLI/libsvn normalize
  peg-sensitive `@`/`%40` only at the command boundary; real clone/fetch/dcommit
  covers an encoded session URL and an `@` working-copy file path.
- A placeholder filename without empty-directory preservation remains a frozen
  clone-compatible no-op and cannot change the effective import configuration.

### Readonly and maintenance

- `find-rev` selects one validated mapping identity rather than flattening all
  rev_maps, and ignores trailing zero scan markers for before/after searches.
- noMetadata rejects metadata-dependent queries without mutation; hash lookup is empty.
- `info`, supported SVN-style `log` ranges/modes/pathspecs, conservative `gc`,
  recoverable `reset`, and current-parent selective `rebase` are implemented.
- The compatibility parser preserves explicit `log -- <pathspec>` boundaries, so
  a path remains a path even when it has the same name as a Git ref.
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
- Rebase permits untracked-only worktrees while dcommit retains strict cleanliness,
  and validates tracking identity before dry-run, fetch, cleanliness checks, or
  mutation. Info paths report encoded URLs, last-change identity/date, and checksums.
- Rebase command-local verbosity and `--fetch-all/--all` match the frozen order.
  Fetch-all is confined to the initially resolved svn-remote's mappings, ignores
  unrelated remotes, and retains the original upstream identity after fetch.
- CLI rebase inherits Git's stderr so successful progress is streamed without
  changing the core/dcommit captured-output interface.
- Log treats typed `--pager=<value>` as a frozen no-op when stdout is not a TTY
  and starts the explicit pager with inherited output when stdout is a TTY; a
  Linux PTY fixture verifies invocation and complete record output.
- Reset accepts positional revision plus `-r/--revision` fallback (positional
  wins), validates it before recovery, and freezes exact/parent stdout. `--parent`
  selects the nearest earlier nonzero record, including sparse histories.
- Reset uses expected-old CAS plus a durable transaction; resolver-backed commands
  fail closed while recovery is pending, and reset validates tracking before execution.
- GC and migration walkers skip root and nested symlinks, preventing metadata
  inspection, deletion, or compression from escaping `.git/svn`.
- GC compression recovers synced Unix unlink or Windows tombstone crashes without duplicate appends.

### Dcommit

- Mock and local file/svn write-back execute a shared typed plan including
  add/modify/delete/copy/move, symlink, executable, mergeinfo, and SVN properties.
- The durable journal stores the whole oldest-first queue with pre-submit
  in-flight state, stable fingerprints, exact remote-head checks, submitted
  recovery, fetch verification, and final rebase/no-rebase state.
- Dirty worktrees, merge ranges, wrong repository UUIDs, ambiguous submissions,
  multiple active journals, and completed-ledger overlap fail closed.
- `--adopt-revision` resumes unknown-outcome local file/svn submissions,
  including mapped commit URLs, only after exact target/ref/rev_map/footer/tree
  verification; omitting the bound commit URL during recovery fails closed.
- Recovery fingerprints use a versioned v4 encoding and bind explicit commit-URL,
  username, config-dir, and auth-cache intent while excluding passwords. Active
  v2/v3 journals migrate on a compatible retry. Effective post-fetch author,
  filter, rewrite, and empty-directory intent is also bound, including authors-file
  content. A non-advancing sink revision stays ambiguous and cannot be resubmitted.
- Explicit commit URLs must resolve to one tracked mapping before write. The
  journal binds that ref/rev_map, verifies the imported footer/tree/OID, and a
  submitted-state restart re-verifies without creating another SVN revision.
- New file/svn transactions bind the mapping ref and rev_map, rejecting a remotely
  advanced target before journal/checkout; a second workcopy proves zero write.
- Authenticated svnserve preflight failure is verified to leave neither an SVN
  revision nor a dcommit journal.
- A post-submit password rotation resumes with the same username and new secret
  without credential leakage or a duplicate SVN revision; changing username is
  rejected, as is changing bound authors-file content.
- Lost-response and save faults retain the last durable state. Real post-fetch
  save failure proves ref/rev_map publication can precede a Submitted restart that
  only re-fetches/verifies; multi-entry faults neither duplicate nor skip commits.
  A visible Submitted snapshot avoids sink calls, and a lost Complete tombstone
  safely retries rebase without sink access.
- SVN subprocesses remain non-interactive. Dcommit reuses one askpass/TTY secret
  across preflight/write/post-fetch; wrong input leaves zero revision or journal,
  secrets stay out of output/config, and Windows echo restore is reviewed.
- Configured `svn+ssh` dcommit completes preflight/write/post-fetch; a real
  loopback OpenSSH fixture additionally validates Ed25519 key authentication,
  explicit known-host trust with strict checking, clone, write-back, and ref
  publication. Incompatible profiles fail early, and svnsync rejects before
  journal/write/import recovery.
- `dcommit --revision` fails before recovery or lookup rather than silently
  ignoring its unsupported SVN editor-base semantics.
- Mock write-back rejects `--commit-url` before journal, lock, or remote mutation
  because the mock sink cannot honor a URL override.
- Dcommit filters all-empty plans without consuming SVN revisions or creating
  journals; explicit mergeinfo-only commits remain effective and mixed queues
  keep contiguous revisions.
- Dry-run uses the real typed planner, rejects gitlinks, applies completed-ledger
  overlap checks, and refuses active/pending recovery without lock, journal,
  checkout, fetch, rebase, reset, sink calls, or metadata mutation. Real SVN
  targets also receive read-only remote UUID and base-revision validation.
- Dcommit target precedence is CLI `--commit-url`, persisted `commiturl`, `pushurl`
  plus mapping, then read URL; full URLs bind ref/rev_map/footer through recovery.

### Golden and release evidence

- The strict frozen Perl Git 2.54.0 suite passes 41/41 locally.
- Exact artifacts cover ref tips, reachable graph/OIDs, rev_maps, configs,
  HEAD/index/worktree, tree contents/modes/properties, readonly outputs, clone
  output, local file/svn writes, submitted recovery, and dirty no-write behavior.
- Each exact scenario writes a JSON summary and retains normalized Perl/Rust
  artifacts.
- `.github/workflows/compatibility.yml` installs frozen Git/SVN/libsvn and runs
  workspace, linked parallel/serial, linked CLI, formatting, clippy, and strict
  compatibility gates. `scripts/verify.ps1 -StrictCompat` mirrors these local gates.

## Current Verification

Verified on 2026-08-01:

- `cargo fmt --all -- --check`; clippy with all targets/features; `git diff --check`
- `cargo test --workspace`; core lib 171/171; readonly follow-up 82/82
- dcommit linear 72/72; config mapping 17/17; real SVN default 48/48
- linked core unit 203/203 and backend 34/34 in parallel and serial runs; real SVN
  linked CLI 48/48
- `GIT_SVN_RS_STRICT_COMPAT=1 GIT_SVN_RS_COMPAT_ARTIFACT_DIR=/tmp/git-svn-rs-current-artifacts cargo test -p git-svn-rs-core --test compat_golden -- --nocapture` (41/41)

## Remaining Work

There is no active hosted-run task. Automatic `push` and `pull_request` triggers
are intentionally disabled; the workflow can only be started manually with
`workflow_dispatch` if hosted validation is requested again.

## Important Commit Anchors
- `7f531ee`, `ab52ef1`, `edb9161`: workspace/config and SHA-256 foundations.
- `9d354f6`, `e3b8cf9`, `4bf9205`, `8405e79`: shared replay and durable
  dcommit/import coordinators.
- `f11ecd1`, `08eef18`: canonical metadata/collision safety and audited libsvn FFI.
- `fa5f81f`, `5dd0ede`, `838a618`: protocol gating, DAV fixture, and askpass reads.
- `23e5c2a`: strict HTTP/HTTPS DAV reads, fresh-clone/fetch preflight, and
  interleaved RA-serf per-file/textdelta-stream replay.
- `0303507`: authenticated HTTP/HTTPS DAV writes, real OpenSSH key/host-trust
  clone and dcommit, and strict CI OpenSSH dependency coverage.
- `d42efb3`, `b7d1a9e`, `53707fe`, `87e1446`: mapped commit-URL recovery,
  versioned fingerprints, adoption, and password-safe resume.
- `23bf393`, `793fb5d`, `afe3fb4`, `7968d5e`, `3573d2f`: durable dcommit fault
  and multi-entry restart boundaries.
- `9f4b223`, `9ad34d8`, `78b31da`: Log/rebase contracts, PTY pager, full-URL golden.
- `22c71e9`, `6e8b595`, `53d08a8`, `91042ca`: stale-target, peg-sensitive,
  named-remote, and configured-tunnel E2E.
- `98b87f1`, `cf7910b`, `d3792bf`: copy-parent ordering and replay diagnostics.
- `5616b2f`, `d8d5130`, `31a74c6`: rebase cleanliness, historical info, and
  scan-marker-safe find-rev.
- `89558ac`, `7636ee8`, `962d36b`: no-op filtering and zero-mutation dry-run
  planning/journal boundaries.
- `c399274`, `debfc31`: stable authors-file paths and Git-compatible booleans.
- `e5eb148`, `5e0ea43`: semantic tracking validation at command safety boundaries.
- `4143f8f`, `7b3f335`: cross-remote ref preflight and reset compatibility.
- `2bfac94`, `5265ad9`, `62aa49c`, `4799765`, `f69dcdd`: safety/readonly batch.
- `c8f2eb0`…`5c21e68`: GC/targets/info/auth; `504a3f1`, `9544578`, `0efb075`:
  svnsync CLI/config, revprops, and source identity.
- `10ea719`, `a548b20`, `7f493af`: multi-map transactions, noMetadata limits,
  and byte-safe SVM CLI/config/identity discovery foundation.
- `daf5508`, `bdce0b3`, `754c57b`: SVM dual maps, username prompts, v0-v5 policy.

## Next Steps

1. Continue local compatibility work; use the manual hosted workflow only after
   an explicit decision to resume hosted validation.

## Handoff Notes

- Preserve pre-existing untracked `.codex/`, `.zcode/`, `CLAUDE.md`, `docs/`, and
  generated `golden-stdlayout-*`/`svn-fixture-*` directories.
- Configured `svn+ssh` tunnel and real loopback OpenSSH reads/writes passed locally;
  the latter uses generated Ed25519 keys and strict explicit known-host trust.
  Authenticated loopback HTTP/HTTPS DAV reads and writes passed locally on
  2026-08-01; linked libsvn covers reads while dcommit uses the CLI sink.
- The linked backend is a read/import backend. Dcommit still uses the SVN CLI
  working-copy sink for the covered local write profiles.
- Migration remains inspection/rejection rather than automatic conversion.
