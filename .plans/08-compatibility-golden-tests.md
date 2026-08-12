# Phase 8: Exact Compatibility Artifacts and Release Gates

## Objective

Prove the declared `git-svn-rs` profiles against the frozen Git `v2.54.0` Perl implementation using deterministic fixtures, exact semantic artifacts, explicit skip policy, and non-skippable release CI.

## Current State

State: `release-pass` for the declared v1 profiles at
`c0dfb2067f75806935b2b36462d5819923652634`.

The exact harness now captures config, ref and graph identity, full rev_maps,
HEAD/index/worktree state, modes, properties, readonly output, reset/rebase/gc,
clone behavior, successful writes, recovery, and no-write failures. Strict mode
uses frozen Git `v2.54.0` and makes absent dependencies or skipped comparisons a
failure. Linked libsvn passes both default-parallel and serial gates.

Every scenario summary records execution status, frozen tag/commit, Git,
git-svn, SVN/libsvn and Rust versions, OS/architecture/object format,
timezone/locale, backend, build-feature identity, and artifact profile. The
compatibility workflow is reusable by the release/tag workflow, audits all eight
required summaries, runs the linked dcommit suite, and publishes a release summary
bound to the tested commit SHA. Protected hosted
[release gate run #31561696796](https://github.com/OathMoon/rgs/actions/runs/31561696796)
passed every strict, linked, static, upload, and independent same-SHA verification
step for commit `c0dfb2067f75806935b2b36462d5819923652634`. Artifact
`frozen-compatibility-artifacts` contains the normalized Perl/Rust captures, all
eight required executed/passed scenario summaries, and a passed release summary
bound to that exact SHA.

## Normative References

All fixtures use the frozen sources listed in `.plans/git-svn-rs-plan.md`, especially:

- [`git-svn.perl`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/git-svn.perl)
- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`Ra.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Ra.pm)
- [`Fetcher.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Fetcher.pm)
- [`Editor.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Editor.pm)
- [`Log.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Log.pm)

Each capture records the exact upstream commit and tool versions used.

## Gate Tiers

### Developer gate

- Runs quickly on a normal developer machine.
- Missing Perl/SVN/libsvn may be an explicit skip.
- Passing means local units/regressions are healthy, not compatible.

### Backend integration gate

- Required SVN CLI/libsvn tools are installed.
- Target adapters/profiles actually run; filtered/skipped tests fail the job.
- Default-parallel linked tests are mandatory; serial is an additional diagnostic run.

### Release compatibility gate

- Perl `git-svn`, SVN tools, linked libsvn, and every required profile are present.
- No compatibility scenario may skip.
- Rust and Perl artifacts are compared with only the allowed normalization list.
- A release cannot be cut from a developer-gate-only result.

## Artifact Schema

### Source manifest

- frozen Git tag and commit;
- URLs/paths of upstream files consulted;
- `git --version`, `git svn --version`, SVN/libsvn version, Rust version;
- OS, object format, timezone, locale, backend/profile;
- executed/skipped scenario list and skip reason.

### Repository config and metadata

- all relevant `[svn-remote]` values with cardinality/order;
- canonical URL/root/UUID and mappings;
- metadata directories/files and unhandled/placeholder state;
- rev_map raw bytes, logical records, UUID, revision, exact object ID, zero records, and byte length;
- recovery/migration journal state where applicable.

### Git graph identity

For every imported commit/ref:

- ref name and exact tip OID;
- commit OID, parents, and tree OID;
- author/committer names, emails, timestamps, and offsets;
- full message bytes/normalized line-ending decision and footer;
- path, blob OID/content, and mode;
- mapping/revision correlation.

A complete canonical graph fingerprint may be compared when fixture-specific absolute URLs prevent direct OID equality, but the fingerprint contains every identity field. It must not replace OIDs with a generic token.

### Clone and working state

- stdout, stderr, and exit status;
- local branch list and current branch;
- resolved HEAD and upstream/tracking relationship;
- index entries and worktree paths/modes/content;
- difference between default clone and `--no-checkout`;
- repository cleanliness.

### Command output

- `find-rev` exact commit/revision and tree-ish scope;
- `info` fields;
- all declared log modes including authors/dates/IDs where present;
- reset/rebase/gc/dcommit output, stderr, and exit status;
- diagnostics for unsupported/ambiguous/auth/failure cases.

### SVN write artifacts

- resulting SVN revision, author, full log message, changed paths, copies, node kinds, properties, and content;
- Git ref/rev_map/HEAD/worktree state after post-fetch and rebase;
- partial-failure journal/resume result;
- proof that wrong-target/dirty/unsupported cases produced no SVN revision.

## Allowed Normalization

Normalization is allowlisted and documented per field. Typical allowed values:

- temporary fixture root paths and equivalent file URL roots;
- platform path separators in diagnostics;
- CRLF/LF only where the command contract treats them equivalently;
- nondeterministic process IDs/temp names;
- localized boilerplate only when a structured semantic artifact is compared separately.

Forbidden normalization includes:

- commit/ref/rev_map OIDs;
- authors or timestamps/timezone offsets;
- parent or tree identity;
- SVN revision/UUID/path;
- file mode/content/property values;
- branch/HEAD/worktree state;
- replacing all clone output with success;
- treating a skipped comparison as a pass.

## Fixture Matrix

### Core read fixtures

- single-path repository URL ending in `/trunk`;
- repository-root single-path import;
- standard layout and custom relative/full-URL layouts;
- multiple branches/tags, same revision on multiple refs, and ref collision diagnostics;
- incremental windows, parent fetch, revision keyword/ranges;
- authors, filters, ignored refs, rewrite metadata, noMetadata limitations;
- executable, symlink, textual/unknown properties, non-ASCII/encoded paths;
- empty/absent directories, copy/delete/replace, branch-to-branch copy, follow-parent/backfill;
- SHA-1 and SHA-256 Git repositories;
- interrupted import and recovery.

### Readonly fixtures

- `find-rev rN [tree-ish]`, before/after, zero/missing/ambiguous;
- log modes, dates, messages, ranges, limits, pathspecs, changed paths;
- info across trunk/branch/tag/local commits;
- reset/rebase clean/dirty/auth/recovery;
- gc of metadata actually produced by fetch.

### Write fixtures

- first-parent wrong-target merge regression;
- dirty/stale/ambiguous/bad commit URL no-write checks;
- A/M/D/C/R/T, nested directories, modes/symlinks, full messages;
- auto-props and direct/container property set/delete;
- explicit mergeinfo;
- authenticated file/svn profiles and any declared HTTP(S) profile;
- injected failure after SVN success and idempotent resume;
- default rebase and `--no-rebase` final state.

## Required Work

1. Replace weak artifact fields and normalizers with the schema above.
2. Make fixture timestamps/timezone deterministic enough for exact graph comparison.
3. Add default/no-checkout branch/HEAD/index/worktree capture.
4. Add single-subdirectory clone as the first P0 strict fixture.
5. Add multi-ref resolver and first-parent wrong-target fixtures.
6. Split developer/backend/release entry points and make release skip impossible.
7. Produce a machine-readable scenario summary so CI proves what actually ran.
8. Keep failure artifacts on mismatch and print the capture directory.
9. Run linked tests default-parallel and serial; default-parallel failure blocks the backend gate.
10. Pin CI/tool installation to documented versions or record deviations.

## Invariants

- Artifact comparison checks semantics before presentation normalization.
- Every normalization rule names the instability it removes and has a unit test.
- Rust-only correlation tests are useful but cannot replace Perl compatibility tests.
- A fixture source manifest is part of the artifact, not a comment in the plan.
- Release jobs fail when a required dependency/profile/scenario is unavailable.

## Gates

### Structural gate

- deterministic fixture/capture/comparison modules compile;
- source manifests and tiered skip policies exist;
- artifact schema includes exact object and clone state fields.

### Behavioral gate

- all core fixtures run against real SVN tools;
- known P0 regressions fail before fixes and pass afterward;
- linked default-parallel and serial jobs are stable;
- mismatch output identifies field/scenario and preserves artifacts.

### Release gate

- strict Perl comparison runs with zero skips;
- all required profile rows execute;
- graph, metadata, worktree, readonly, write, and recovery artifacts match;
- CI publishes the source manifest and scenario summary;
- only after this gate may Phase 8 and the first compatibility release be marked `release-pass`.

## Out of Scope

- normalizing away differences merely because exact comparison currently fails;
- using count/length-only assertions for semantic artifacts;
- declaring `master` compatibility without a separately pinned forward-compat run.
