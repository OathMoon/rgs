# Phase 7: Safe Linear Dcommit, Commit Sinks, Shim, and CI

## Objective

Commit eligible local Git commits to the correct SVN branch through one validated `DcommitPlan`, synchronize the resulting revisions back into Git/rev_map, and rebase safely.

## Current State

State: `in-progress`.

Current local `file://` and local `svn://` working-copy paths cover many adds, deletes, modes, symlinks, renames/copies, attributes, auth options, commit URL, mergeinfo, post-fetch, and rebase scenarios. Mock tests exercise `GitDiffPlanner` and `SvnCommitEditor`.

The phase has not passed because:

- target resolution can select the ancestor tracking ref with the highest SVN revision rather than the nearest first-parent SVN identity;
- production working-copy write-back bypasses the planned `GitDiffPlanner`/`SvnCommitEditor` behavior model;
- full Git commit messages are truncated to the subject;
- clean/stale/merge topology checks are incomplete;
- partial SVN success has no durable resume state;
- property clearing can leave stale SVN properties;
- common remote write profiles and production prompts are unsupported/unvalidated.

## Normative References

- [`Editor.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Editor.pm)
- [`Ra.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Ra.pm)
- [`Prompt.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Prompt.pm)
- [`git-svn.perl`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/git-svn.perl)
- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)

## Required Flow

```text
resolve first-parent SVN identity
        ↓
validate clean/stale/topology/target/auth
        ↓
enumerate eligible commits in order
        ↓
build DcommitPlan for one Git commit
        ↓
persist pending-operation journal
        ↓
execute CommitSink
        ↓
persist returned SVN revision
        ↓
fetch through Phase 5 and verify identity
        ↓
clear journal and rebase/reset unless --no-rebase
```

No SVN write occurs before all preflight checks for the first commit pass.

## Work Order

### P0-1. Replace target selection with first-parent identity

- Walk HEAD first-parent history to the nearest valid `git-svn-id`/rev_map identity as the frozen baseline does.
- Validate remote name, canonical repository root, UUID, mapping path, tracking ref, and rev_map record.
- Respect explicit branch/commit arguments and `--svn-remote` if included in the CLI contract.
- A merge where another SVN tracking ref has a larger revision must not change the target.
- Multiple/conflicting candidates fail closed before creating a working copy or auth prompt.

Required regression: first parent belongs to branch A/r100, merged branch B is r200; dcommit must target A or reject according to baseline, never choose B by numeric revision.

### P0-2. Complete write preflight

- Require clean index/worktree and no unresolved operation journal.
- Verify local tracking state is current enough for the target and expected base revision.
- Accept only the declared linear/first-parent commit topology; reject unsupported merges before write.
- Validate commit URL/push URL points at the same repository UUID and intended branch.
- Resolve all credentials/capabilities before the first write where possible.

### P0-3. Preserve commit identity and message semantics

- Enumerate commits deterministically in oldest-first submission order.
- Preserve full commit message, not only subject.
- Implement baseline author/trailer behavior included in the v1 CLI/config contract.
- Retain binary-safe path/object data and reject unsupported path encoding explicitly.

### P1-1. Make `DcommitPlan` the sole write behavior model

`GitDiffPlanner` consumes raw `git diff-tree -z -r -C` metadata including old/new modes, OIDs, status, similarity, and both paths. It emits a plan with:

- target identity and base revision;
- ordered D/C/R/A/M/T operations;
- parent-directory requirements;
- file content/OIDs and mode transitions;
- property set/delete operations;
- explicit mergeinfo if supplied;
- full log message and author metadata.

Mock, working-copy CLI, and future native commit editor execute this same plan. Production command glue does not independently interpret name-status output.

### P1-2. Complete commit sink semantics

- Preflight SVN node kinds for affected paths.
- Ensure/open/add parent directories deterministically.
- Apply operations in baseline-compatible order and abort on no-op unless explicit mergeinfo requires a commit.
- Preserve executable/special/symlink transitions.
- Apply auto-props and supported `.gitattributes` mappings through one property model.
- Emit `propdel` when a previously present textual/needs-lock/direct property is cleared.
- Parse committed revision from structured/native result where possible, not fragile localized output.

The working-copy sink may remain a supported adapter if it executes the common plan. Native commit-editor support is a separate adapter, not a second planner.

### P1-3. Add durable partial-success recovery

Use Phase 3 journal state for each Git commit:

- pending before submission;
- submitted with returned SVN revision;
- fetched/verified;
- rebased/complete.

On rerun, an already-submitted commit is fetched/verified rather than resubmitted. Recovery detects repository changes and refuses unsafe guessing.

### P1-4. Keep auth/config consistent through the whole flow

- checkout/commit/native sink, post-commit fetch, and final rebase use the same resolved runtime config and credentials where applicable.
- Passwords are not persisted or logged.
- `--no-auth-cache`, config-dir, prompt/cache behavior, commiturl, and pushurl follow the declared profile.
- A post-submit auth/fetch failure leaves an actionable journal state.

### P1-5. Expand profiles only after safety gates

Order of profile work:

1. strict `file://` CLI sink;
2. strict authenticated local `svn://` CLI sink;
3. native libsvn commit sink if selected by ADR;
4. HTTP(S) commiturl/pushurl with repeatable auth fixture;
5. svn+ssh only after an explicit release decision.

Accepted-but-unvalidated schemes must fail explicitly for non-dry-run dcommit.

### P2. Maintain shim and CI

- Shim forwards dcommit arguments, stdout/stderr, and exit code unchanged.
- CI separates developer, backend, and strict release gates.
- Required linked/native tests run default-parallel; a serial job is diagnostic, not a substitute.
- Strict compatibility is a required protected release job, not manual-only evidence.

## Invariants

- Target ambiguity, dirty state, stale state, unsupported topology, or incompatible commit URL stops before the first SVN write.
- One Git commit produces at most one SVN revision unless the baseline explicitly says otherwise.
- A returned SVN revision is durably recorded before a failure-prone fetch/rebase step.
- Every production sink executes the same plan and property operations.
- `--dry-run` uses the real resolver and planner but performs no Git/SVN metadata mutation.

## Gates

### Structural gate

- shared first-parent resolver and `DcommitPlan` are used by mock and production paths;
- full messages and property delete operations exist in the plan;
- recovery journal state is defined;
- command code no longer carries an independent production diff interpreter.

### Behavioral gate

- wrong-branch merge, ambiguity, dirty tree, stale tracking state, unsupported merge, bad commit URL, and auth failure all stop before write;
- A/M/D/C/R/T, nested parents, executable/symlink/type changes, auto-props, direct/container property set/delete, full messages, and mergeinfo pass on real SVN;
- injected failure after SVN success resumes without duplicate revision;
- post-fetch identity/ref/rev_map and rebase/no-rebase state are correct;
- file and svn profiles run with real tools and no relevant skips.

### Release gate

- declared write profiles match frozen Perl behavior and resulting SVN/Git artifacts;
- first-parent target safety regression is mandatory;
- strict CI proves full messages, properties, revisions, ref/rev_map state, worktree state, and recovery;
- public documentation names unsupported schemes/topologies/options.

## Out of Scope

- branch/tag creation, `set-tree`, and `commit-diff`;
- automatic mergeinfo generation;
- silently flattening arbitrary merge histories;
- adding remote protocols before target and recovery safety pass.
