# Phase 3: Git Plumbing, Metadata State, RevMap, and Migration

## Objective

Provide the durable Git/SVN metadata boundary: tested Git plumbing, `git-svn-id`, rev_map binary compatibility, unambiguous tracking identity, transactional ref/rev_map updates, recovery, and legacy metadata policy.

## Current State

State: `release-pass` for the declared v1 metadata layouts.

Read-only rev_map access is non-creating; SHA-1/SHA-256 records, trailing-zero
markers, locking/fsync, canonical and legacy paths, tracking identity, and
transactional ref/rev_map publication are covered by recovery tests. Resolver,
rev_map and dcommit safety boundaries now surface stable typed categories through
the top-level `GitSvnError` while retaining existing CLI text and source chains.

Legacy v0-v5 metadata is deliberately inspected and rejected without mutation;
automatic conversion remains deferred and is not part of the v1 claim.

## Normative References

- [`Git.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git.pm)
- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`Migration.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Migration.pm)
- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)

## Required Boundaries

### GitRepository

The tested Git wrapper owns repository/worktree discovery, config cardinality, object format, refs, ancestry/first-parent queries, object IO, fast-import, diff-tree, update-ref CAS, rebase, stdout/stderr, and exit-code translation.

Production modules do not use a test-named escape hatch or spawn Git directly.

### MetadataStateStore

This boundary owns:

- metadata directory naming;
- rev_map file discovery by remote/ref/UUID;
- read-only open versus create-new;
- lock acquisition and stale-lock policy;
- record validation and append/reset;
- ref/rev_map consistency checks;
- import/dcommit recovery journals;
- migration discovery, backup, execution, and warnings.

It may be a cohesive module rather than one large trait.

## Required Work

### 1. Complete Git wrapper semantics

- Enforce single-value versus multi-value config behavior from `Git.pm`.
- Expose first-parent/footer queries needed by the shared resolver.
- Expose compare-and-swap `update-ref` with expected old OID.
- Preserve binary stdout for object/path operations and structured stderr/status errors.
- Remove production use of `run_for_test` and centralize all Git process execution.

### 2. Separate rev_map read and creation

- `open_existing` or equivalent never creates a directory/file.
- create is explicit and validates remote/ref/UUID/object format.
- discovery returns zero, one, or ambiguous candidates; ambiguity is an error.
- record parsing rejects truncated files, invalid object widths, non-monotonic revisions, invalid trailing zeros, and object IDs of the wrong format.

### 3. Define import consistency and recovery

For each mapping batch:

1. read and validate current ref/rev_map state;
2. construct objects without mutating the final ref where possible;
3. prepare rev_map records and expected old ref;
4. update persistent state with a documented ordering and CAS;
5. verify final ref tip and rev_map tail;
6. on interruption, provide deterministic resume/repair rather than silently creating new metadata.

The implementation need not provide cross-filesystem atomicity, but every intermediate state must be detectable and recoverable.

### 4. Define dcommit partial-success state

Before each SVN submission, record the Git OID, target remote/UUID/path, and expected base revision. After SVN success, record returned revision before fetch/rebase. Resume must not submit the same Git commit twice.

The journal contains no password or credential material.

### 5. Centralize tracking identity and resolver inputs

- Remote names are not hard-coded to `svn`.
- A tracking identity includes remote, canonical repository root, UUID, mapping path, Git ref, rev_map path, and validated head record.
- First-parent footer and rev_map must agree when both exist.
- Multiple matching identities fail closed.

Phase 6 and Phase 7 consume this boundary; they do not rescan all rev_maps independently.

### 6. Implement a deliberate migration policy

Support one of these documented outcomes for each legacy layout from the frozen `Migration.pm` baseline:

- migrate with preflight, backup, one-way warning, validation, and recovery; or
- reject with an exact actionable diagnostic and leave files untouched.

At minimum cover v0-v5 discovery, `.rev_db.*` to `.rev_map.*`, old URL/config discovery, empty `[svn-remote]`, gitfile/commondir, and mixed-layout ambiguity. “Needs migration” inspection alone does not pass the gate.

### 7. Introduce structured metadata errors

Distinguish missing metadata, ambiguity, corruption, lock contention, unsupported legacy layout, CAS conflict, incomplete transaction, object-format mismatch, and external Git failure.

## Invariants

- Read-only commands never create metadata.
- A rev_map is never selected by directory iteration order.
- Ref advancement without a corresponding detectable rev_map state is never reported as success.
- Every write checks the expected old identity before mutation.
- Recovery is idempotent and does not rewrite already-published Git objects or duplicate SVN commits.

## Gates

### Structural gate

- Git wrapper and metadata state boundaries compile;
- existing SHA-1/SHA-256/lock/fsync/zero-record tests remain green;
- duplicate rev_map/config readers are removed from commands.

### Behavioral gate

- fault-injection tests cover failure before objects, before ref update, before/after rev_map update, and during recovery;
- ambiguous UUID/ref/mapping cases fail without mutation;
- read-only missing metadata does not create files;
- migration fixtures either migrate and validate or reject without mutation;
- first-parent identity agrees with footer and rev_map in multi-ref histories.

### Release gate

- rev_map bytes, paths, UUIDs, object IDs, reset/rebuild behavior, and migration outcomes match the frozen baseline for declared layouts;
- import and dcommit interruption scenarios have documented, tested recovery;
- strict artifacts prove ref tip and rev_map object identity, not just record count/length.

## Out of Scope

- a general-purpose transactional filesystem;
- automatic repair of arbitrary hand-edited/corrupt metadata without an explicit command and backup;
- migrating layouts outside the frozen baseline without a separate compatibility decision.
