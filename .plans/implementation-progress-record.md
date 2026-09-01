# git-svn-rs Implementation Progress Record
Last audited: 2026-09-01
Branch: `codex-execute-git-svn-rs-plans`
Audited maintenance HEAD: `4b49af688ceaf64350bfce8ad98549581cca6785`
Release candidate SHA: `8496b12b12aecec537e7310c1a4fed7c5ddda522`
Phase 9 P0/P1 implementation evidence: `c0dfb2067f75806935b2b36462d5819923652634`
Last proven release-candidate commit: `8496b12b12aecec537e7310c1a4fed7c5ddda522`
Prior protected hosted evidence: [release gate run #33155854101](https://github.com/OathMoon/rgs/actions/runs/33155854101)
Current protected candidate evidence: [release gate run #33467284345](https://github.com/OathMoon/rgs/actions/runs/33467284345)
Product requirements and ordering live in `.plans/git-svn-rs-plan.md` and
`.plans/00-git-svn-rs-review-and-roadmap.md`.

## Current Overall State

The repository provides the covered `file://`, local `svn://`, configured and real
loopback `svn+ssh`, mock, and authenticated loopback HTTP/HTTPS DAV workflows. The
2026-08-12 Phase 9 P0/P1 local closure fixes linked post-submit properties,
portable fixtures, typed top-level errors, and protected release gates. The
protected hosted gate passed the same strict/linked/static matrix and independently
verified the retained artifact against the release commit SHA. Phase 10 completed
the maintainability and package-readiness scope at `ffb2e22`; deferred capabilities
remain outside the release claim. Follow-up Windows portability fixes at
`4b49af6` pass the Windows and Developer gates; they do not replace the protected
release baseline. The Windows strict compatibility step was skipped, not passed.
The 2026-08-31 local WSL revalidation additionally passed the complete strict and
linked matrix on the maintenance source; it is not a new hosted release artifact.
The initial local Windows matrix exposed native TortoiseSVN argument expansion
and revision-property encoding/newline failures. Working-tree repairs now pass
all nine Windows gates and all nine WSL strict/linked/static gates. Both bind the
same verified source fingerprint; this local evidence does not upgrade the
protected release baseline.

| Phase | State | Current evidence | Main gap |
|---|---|---|---|
| 1 workspace/CLI | `release-pass` for declared v1 profiles | complete capability inventory, CLI/core/opt-in shim, diagnostics, output/exit boundaries, explicit unsupported no-mutation tests, hosted gate | future commands remain explicitly out of scope |
| 2 config/mapping | `release-pass` for declared profiles | centralized config, relative/full layouts, globs, authors, filters, ref sanitization, metadata modes/auth precedence, exact hosted artifacts | broader future platform/remote profiles |
| 3 metadata/rev_map | `release-pass` for declared layouts | SHA-1/SHA-256 maps, locks/fsync, canonical paths, identity resolution, recovery, typed resolver/rev_map/dcommit boundaries, exact v0-v5 rejection | automatic migration and broader internal typed-error migration |
| 4 SVN adapters | `release-pass` for declared CLI/linked read profiles | shared replay, complete incremental add properties, per-file RA-serf batons, audited FFI, ADR, Phase 10 module split, portable fixtures, hosted parallel/serial linked gates | native write-back, broader remote/platform validation |
| 5 import/clone/fetch | `release-pass` for declared profiles | stdlayout/direct URL replay, copies/follow-parent, bounded fetch, collisions, hosted linked CLI 48/48 parity | remaining obscure Fetcher semantics |
| 6 readonly | `release-pass` for covered profiles | scoped queries/log/reset/gc, option-complete rebase with streamed progress, PTY pager, exact hosted artifacts | broader platform terminal fidelity |
| 7 dcommit | `release-pass` for declared profiles | typed plans, v4 recovery, linked property/readback recovery, CLI sink profiles, hosted linked dcommit 73/73 gate | native write-back and broader remote faults |
| 8 golden/release | `release-pass` at `6f22803` | hosted 41 scenarios, 8 required summaries, backend/build-feature identity, same-SHA release summary and verifier | forward-compatibility runs remain separately scoped |
| 9 hardening | `release-pass` for P0/P1 at `6f22803` | linked property/recovery fixes, temp-root migration, ADR, typed errors, protected hosted release evidence | maintenance scope transferred to Phase 10 |
| 10 maintainability/package | `release-pass` at `ffb2e22` | package registry proof; libsvn/import/dcommit/fetch splits; API narrowing; local and hosted strict/linked/static gates; exact same-SHA artifact | future maintenance remains separately scoped |

## Validated Capabilities

### Foundation and metadata

- Rust workspace with `git-svn-rs`, reusable core library, and opt-in `git-svn`
  compatibility shim.
- Typed command surface, explicit v1 exclusions, config serialization, mapping
  globs, authors, filters, URL helpers, and metadata option conflict checks.
- `.plans/release-capability-inventory.md` classifies every command, option, and
  protocol claim; accepted entries have a consumer and deferred commands fail
  with stable exit/output behavior before mutation.
- `diagnose` reports package version, frozen Git 2.54.0 commit, platform, and
  compiled/linked libsvn state without reading secret-bearing environment values.
- Scalar `svn-remote.*` keys reject multiple values instead of silently selecting
  a different repository identity; mapping keys retain intentional multiplicity.
- Persisted remote booleans use Git's full boolean syntax and reject invalid or
  duplicate values. Relative authors files are persisted as invocation-cwd
  absolute paths, so later fetches resolve the same file with path-rich errors.
- Global `-q`/`--quiet` and `-v`/`--verbose` fail explicitly instead of being
  parsed and silently ignored.
- The CLI boundary uses `GitSvnError` categories for unsupported, authentication,
  ambiguity, metadata corruption, partial write, external command, and invalid
  invocation failures while preserving frozen text and nested error sources.
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
  48-case CLI suite as the default backend.
- CLI `get_dir` returns immediate files/directories plus node properties; an empty
  nested branch inside an ancestor copy is discovered with its auxiliary parent.
- Native update/switch handles initial and incremental text deltas, copy-only
  files, directory/file properties, checksums, absent nodes, deletes, and callback
  error conversion.
- Incremental linked add replay supplies authoritative file properties before
  close, preserving executable and special modes; removal and special-to-regular
  transitions are covered by real native-backend fixtures.
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
  without persistence. Unix askpass retries only the transient `ETXTBSY` spawn
  failure exposed by the hosted runner; other start failures remain immediate.
  Default reads probe SVN cache/public access before prompting; interactive writes
  confirm credentials before the first write, while non-TTY writes may use SVN's
  cache.
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
- A real linked post-fetch failure after an executable-file SVN commit leaves the
  submitted revision durable; retry performs fetch/verification only, creates no
  second SVN revision, and publishes matching tree/ref/rev_map/footer state.

### Golden and release evidence

- The strict frozen Perl Git 2.54.0 suite passes 41/41 locally.
- Exact artifacts cover ref tips, reachable graph/OIDs, rev_maps, configs,
  HEAD/index/worktree, tree contents/modes/properties, readonly outputs, clone
  output, local file/svn writes, submitted recovery, and dirty no-write behavior.
- Each exact scenario writes a JSON summary with execution status, frozen source,
  toolchain/platform, backend, build-feature, and artifact-profile identity, and retains
  normalized Perl/Rust artifacts.
- `.github/workflows/compatibility.yml` installs frozen Git/SVN/libsvn and runs
  workspace, linked parallel/serial, linked CLI read/write, formatting, clippy,
  and strict compatibility gates. The release/tag workflow calls that reusable
  gate and rejects artifacts not bound to the current SHA.
- Protected hosted release gate
  [#31562384493](https://github.com/OathMoon/rgs/actions/runs/31562384493)
  passed for `6f22803c8fdacd9a7217cbb0dda339fb03bcfe47`; the downloaded schema-2
  release summary records `status: passed`, eight required scenarios, the default
  SVN-CLI-vs-frozen-Perl profile, and all four linked backend profiles.
- Phase 10 protected release gate
  [#33155854101](https://github.com/OathMoon/rgs/actions/runs/33155854101)
  passed for `ffb2e227f182be7b3afadbd0d2fcdf3744842e0e`. Its independent verifier
  passed, and the downloaded artifact digest
  `b160bce332f2072cf2ebf80f23b94a3fec21f296d6f017c97990952ddb2523db`
  matches GitHub's recorded digest.

## Current Verification

### Windows property repairs (2026-08-31, verified working tree)

- Fixed file-based dcommit property values with explicit UTF-8 for `svn:*`,
  revprop fixture writes, per-property XML reads, and XML newline/reference
  decoding. Plans/journals, dependencies, tool versions and formal gates are
  unchanged. Details: `docs/项目审阅报告-2026-08-31.md`, section 8.
- Windows: all nine gates pass. Default workspace includes read 42/42, dcommit
  63/63, readonly 79/79, core 175/175 and SVN fixture 5/5. Linked parallel and
  serial each pass core 207/207, backend 36/36 and fixture 5/5; linked CLI read
  42/42 and write 63/63, linked diagnose, fmt, clippy and diff check pass.
- WSL: all nine gates pass. Default strict golden 41/41 plus eight executed/passed
  frozen summaries; linked parallel/serial core 212/212 and backend 36/36;
  linked CLI read 48/48 and write 73/73; linked diagnose/static checks pass.
- Evidence: `target/windows-audit-20260831-185524/` and
  `target/wsl-audit-20260831-Vm2KR3/`, each with `verification-summary.json`,
  `source-manifest.json`, `verified-source.patch` and `independent-audit.json`.
  Source fingerprint: `f1bc127e4e5206eaa415f245fc70d42bfa61ed5a125ff355d82e979f8ad22288`;
  base HEAD `4b49af6` plus uncommitted repairs. Both start/end checks agree.
- The four prior Windows failures are closed without relaxing assertions.
  Regressions cover UTF-8 mergeinfo/log, LF/CRLF/CR, empty/binary values, and
  XML references. Windows Perl comparisons still skip; only the default WSL
  strict run supplies frozen comparison evidence. This is not a release artifact.

### Earlier split-verification findings (2026-08-31, before repair)

- Windows `target/windows-audit-20260831-111849/` recorded five passing/four
  failing gates with four distinct failed tests. Wildcard argument expansion and
  fixture/property encoding failures are superseded by the repaired matrix above.
  Earlier WSL proof remains in `target/wsl-audit-20260831-KC6jbX/`.
- Keep the Windows Cargo target separate and TEMP outside any Git repository.
  The local sandbox causes SVN E720005 on the user temp path; approved normal
  path access resolves it without system changes. Environment failures and
  interrupted/invalid-collector attempts are not final compatibility evidence.

### WSL strict and linked revalidation (2026-08-31)

- Verified source at `4b49af688ceaf64350bfce8ad98549581cca6785` with only the
  pending status-document edits. The audit retains that tracked patch and its
  SHA-256; production code, tests, manifests, lockfile and workflows match HEAD.
- WSL2 Ubuntu Linux x86_64, Rust/Cargo 1.97.1, frozen Git/Perl 2.54.0,
  SVN/libsvn 1.14.3; `LC_ALL=C`, `TZ=UTC`, Linux `/tmp` fixture root.
- `GIT_SVN_RS_STRICT_COMPAT=1 cargo test --workspace --locked` passed, including
  core 178/178, readonly 82/82, real clone/fetch 48/48, dcommit 73/73 and golden
  41/41. All eight required summaries were independently checked as executed and
  passed with the frozen commit, default backend and build-feature identifiers.
- Linked core passed both parallel and `--test-threads=1`: core 210/210 and
  native backend 36/36 in each run. Linked CLI clone/fetch passed 48/48 and
  dcommit passed 73/73. Linked-feature golden functions are not counted as a
  second executed frozen comparison; the default strict run supplies that proof.
- `diagnose` reported `platform: linux/x86_64`, `libsvn feature: enabled`, and
  `libsvn link: linked`. Formatting, all-target/all-feature clippy with warnings
  denied, and `git diff --check` passed.
- Local logs, per-gate commands, default compatibility artifacts and
  `verification-summary.json` are retained under
  `target/wsl-audit-20260831-BkgAae/`. All nine gates passed. This is local WSL
  evidence with pending documentation edits, not protected hosted release
  evidence; the last proven release SHA remains `ffb2e22`.

### Release candidate strict audit (2026-09-01)

- Candidate `8496b12b12aecec537e7310c1a4fed7c5ddda522` was checked in a clean
  WSL-native detached clone. The WSL login environment resolved
  `/home/oathmoon/.local/bin/git`: Git/git-svn 2.54.0, SVN/libsvn 1.14.3,
  Rust/Cargo 1.97.1, with `LC_ALL=C` and `TZ=UTC`.
- `GIT_SVN_RS_STRICT_COMPAT=1 cargo test --workspace --locked` passed. The
  frozen Perl comparison executed all 41 golden tests; all eight required
  scenario summaries were `status=passed`, `execution=executed`, and bound to
  frozen Git commit `0b13e48a3a30cdfa94e8ef842e24d6045ab3d015` with the expected
  `svn-cli-vs-frozen-perl` and `default-svn-cli` identifiers.
- Linked core passed both parallel and serial runs. Linked CLI clone/fetch
  passed 48/48 and dcommit passed 73/73. `diagnose` reported Linux/x86_64,
  enabled libsvn and `libsvn link: linked`; fmt, all-target/all-feature clippy
  with warnings denied, and `git diff --check` passed.
- This is candidate-bound local WSL evidence only; it does not replace hosted
  evidence. The protected same-SHA gate and artifact verifier are recorded in
  the following section.

### Protected candidate release gate (2026-09-01)

- Protected release gate run [#33467284345](https://github.com/OathMoon/rgs/actions/runs/33467284345)
  was manually dispatched from `release-candidate-8496b12` and completed
  successfully in 7m 3s. Both `strict-compatibility / frozen-git-svn` and
  `verify-current-sha-evidence` passed; the run page identifies the commit as
  `8496b12b12aecec537e7310c1a4fed7c5ddda522`.
- The uploaded `frozen-compatibility-artifacts` artifact is 123 KB and has
  GitHub digest
  `sha256:1e21a0dc52daa90f6257daac7963409fc3c65c6e3a8b2a94e6e05e6bb5d1cbda`.
  The downloaded ZIP has the identical SHA-256.
- Extracted `release-summary.json` reports schema 2, `status=passed`, the exact
  candidate `commit_sha`, required scenario count 8, the expected
  `svn-cli-vs-frozen-perl`/`default-svn-cli` identifiers, and all four linked
  backend profiles. Each required scenario summary is independently present,
  `passed`, `executed`, and bound to frozen Git commit
  `0b13e48a3a30cdfa94e8ef842e24d6045ab3d015`.
- This establishes protected same-SHA release-gate evidence for the candidate.
  No registry publish, version tag, or public release has been performed;
  those remain separate release actions.

### Maintenance follow-up audit (2026-08-31)

- Reconciled the task `推进可维护性与打包计划`
  (`01a04285-f30b-7763-81a6-cd1b0ef7324e`) with repository changes and GitHub's
  public run/jobs API. The updated review is in
  `docs/项目审阅报告-2026-08-31.md`.
- `daf5fc8` and `4b49af6` fix Windows CRLF, working-tree checksum, and local-time
  test assertions; production sources are unchanged from `ffb2e22`.
- [Windows #23](https://github.com/OathMoon/rgs/actions/runs/33158740460) and
  [Developer gate #11](https://github.com/OathMoon/rgs/actions/runs/33158740456)
  both succeeded on `4b49af688ceaf64350bfce8ad98549581cca6785`. The Windows run
  was a `push`: `Verify` succeeded, but `Verify strict compatibility` was
  `skipped`. This corrects the linked task's final description of that step.
- The linked task records local Windows readonly 79/79, formatting, and
  all-feature clippy passing. This audit additionally passed CLI smoke 5/5,
  clone/fetch smoke 19/19, and Windows real-SVN clone/fetch 42/42 after selecting
  an explicit long-path temporary root. The full workspace run was stopped in
  the dcommit suite and is not counted as a complete pass.
- This audit passed formatting, all-target/all-feature clippy, `git diff --check`,
  and all three package-list checks (README, both licenses, no generated fixture
  paths). The local native link probe lacked `VCPKGRS_DYNAMIC`; no linked or
  strict Windows result is claimed. PDB-cache and SVN short-temp-path failures
  were environment issues, not evidence reopening the completed Phase 10 scope.
- The API also reconfirmed protected run #33155854101 and its independent
  verifier succeeded at `ffb2e22`. Artifact content/digest evidence below comes
  from the original task's download audit; this audit did not redownload it.

### Phase 10 release baseline (2026-08-28)

Verified locally on 2026-08-28 in WSL from the repository root:

- `cargo test --workspace`: core 178/178, readonly 82/82, dcommit 73/73,
  real SVN CLI 48/48, and all 41 available golden scenarios passed.
- `cargo test -p git-svn-rs-core --features svn-libsvn` and the same command with
  `--test-threads=1`: linked core 210/210 and native backend 36/36 passed in both
  parallel and serial runs. Feature-gated golden test functions also pass, but
  their intentional linked skips are not counted as executed release scenarios.
- Linked CLI real workflows passed clone/fetch 48/48 and dcommit 73/73, including
  executable/special projection, type transition, and no-resubmit recovery.
- The Phase 10 import-runtime regression passed 1/1 and proves that 100 repeated
  change applications compile the configured include/ignore regexes only twice;
  final-tree import mock passed 25/25 and linked CLI clone/fetch passed 48/48.
- All three package lists and archives contain README plus both licenses and
  exclude generated fixture and `.svn` paths. The package-readiness script mirrors
  the locked dependency set into a temporary `file:` registry, uses an isolated
  workspace and clean Cargo home, and verifies core → CLI → shim order. The CLI
  package downloads the registered `git-svn-rs-core 0.1.0`, then both Cargo's
  package verification and an extracted-package check pass. No registry publish
  or tag is performed.
- A separate strict default-backend run with frozen Git/Perl 2.54.0 and
  SVN/libsvn 1.14.3 executed all required comparisons: golden 41/41, with all
  eight required summaries marked executed/passed and carrying the expected
  frozen commit, backend, and build-feature identifiers.
- Final Phase 10 linked revalidation passed core 210/210 and native backend 36/36
  in both parallel and serial modes, then linked CLI clone/fetch 48/48 and
  dcommit 73/73. The final static gate passed formatting, all-target/all-feature
  clippy with warnings denied, and `git diff --check`.
- Final package-readiness revalidation passed core → CLI → shim in an isolated
  temporary registry. Archive SHA-256 values were `a1c17054…81ac`,
  `592e3602…c5cd`, and `09fe17da…f3d`, respectively.
- Fixture creation used the centralized system temp root; the repository-root run
  introduced no new random `golden-stdlayout-*` or `svn-fixture-*` directories.
- Formatting, all-target/all-feature clippy with warnings denied, and
  `git diff --check` passed for this working tree.
- Hosted release gate
  [#33155854101](https://github.com/OathMoon/rgs/actions/runs/33155854101)
  completed successfully: strict comparison, eight-summary audit, linked core
  parallel/serial, linked CLI clone/fetch/dcommit, Rust 1.98 static gates,
  artifact upload, and the separate same-SHA verifier all passed. The downloaded
  schema-2 `release-summary.json` is bound to
  `ffb2e227f182be7b3afadbd0d2fcdf3744842e0e`, records the expected backend,
  features and four linked profiles, and came from a ZIP whose SHA-256 matches
  GitHub's artifact digest.

## Remaining Work

Phase 9 P0/P1 and Phase 10 are complete with protected hosted evidence. The current
release candidate is `8496b12`; its WSL/Unix strict gate, protected workflow, and
same-SHA artifact verification all passed on 2026-09-01. The candidate is release
gate verified, but no registry publish, version tag, or public release has been
performed.
Any future release-candidate commit must rerun the protected workflow so its artifact
remains bound to that candidate SHA; older artifacts are never inherited. `4b49af6`
has successful maintenance CI, but no replacement protected artifact is established
by this audit. Publishing crates and creating a tag remain separate release
actions; `0.1.0` is still recorded as unreleased. Broader
remote/platform profiles, native libsvn write-back, automatic migration, full
Log.pm, and other deferred capabilities stay outside this release claim.

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
- `9f48097`: automatic frozen-compatibility workflow triggers removed; manual
  dispatch retained.
- `3aab5f1`: Phase 1/2 capability closure and Phase 8 machine-readable release
  summary gate.
- `e2c90e8`: dual-stack hosted DAV readiness and actionable probe diagnostics;
  validated by Frozen compatibility run #5.
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
- `80b74fb`: Phase 9 P0/P1 property/recovery, portable-fixture, governance,
  typed-error, and protected-gate closure.
- `991eef5`, `c0dfb20`: hosted compatibility environment correction and transient
  Unix askpass execution hardening; `c0dfb20` is validated by release gate
  #31561696796.
- `6f22803`: Phase 9 hosted evidence reconciliation; validated by final release gate
  #31562384493 and retained current-SHA artifact.
- `8aa0ac6`: Phase 10 governance, package isolation/licenses, Actions runtime,
  and immutable import runtime baseline.
- `42d0a6f`: release documents and isolated core → CLI → shim registry proof.
- `7395e90`–`40352a3`: incremental libsvn tests/native-delta/FFI/runtime/auth/RA
  split with linked boundary verification.
- `6e03aa3`: import discovery/replay/publication split.
- `2f6cc8f`, `cc3e690`: dcommit and fetch command-boundary splits.
- `3c1eff1`: v0.1 public API allowlist and implementation visibility narrowing.
- `34fc415`: Phase 10 structural completion recorded before final release gates.
- `39bf24f`, `ffb2e22`: Rust 1.98 static-gate compatibility; `ffb2e22` is
  validated by protected release gate #33155854101 and its retained same-SHA artifact.
- `0cb5cd6`: Phase 10 hosted-evidence documentation; `daf5fc8`, `4b49af6`:
  Windows test portability closure, validated by Windows #23 and Developer #11.

## Next Steps

1. Treat `8496b12` and protected run #33467284345 as the current release-gate
   verified candidate. Keep registry publishing, version tagging, and public
   release as explicit separate actions.
2. Keep `4b49af6`'s successful Windows/developer evidence distinct from both the
   release baseline and candidate evidence. Do not inherit artifacts across SHAs or
   count a skipped Windows strict step.
3. Keep deferred profiles outside maintenance-only follow-up work.
4. Preserve the verified working-tree property repairs and their source-bound
   local evidence. Any later release candidate still needs its own protected
   same-SHA gate; do not substitute WSL evidence for native Windows coverage.

## Handoff Notes

- Preserve pre-existing untracked `.codex/`, `.zcode/`, `CLAUDE.md`, `docs/`, and
  generated `golden-stdlayout-*`/`svn-fixture-*` directories.
- Configured `svn+ssh` tunnel and real loopback OpenSSH reads/writes passed locally;
  the latter uses generated Ed25519 keys and strict explicit known-host trust.
  Authenticated loopback HTTP/HTTPS DAV reads and writes passed locally on
  2026-08-01; linked libsvn covers reads while dcommit uses the CLI sink.
- The linked backend is a read/import backend. Dcommit still uses the SVN CLI
  working-copy sink for the covered local write profiles, while linked builds use
  libsvn for post-submit read/import verification and recovery.
- Test fixtures default to the system temp directory; use
  `GIT_SVN_RS_TEST_TMPDIR` for an explicit root in CI or diagnostics.
- Migration remains inspection/rejection rather than automatic conversion.
