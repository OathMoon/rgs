# Phase 5: Import, Fetch, Clone, and Git Object Identity

## Objective

Turn SVN revisions into baseline-compatible Git commits, refs, rev_maps, local branches, and working trees through the single `SvnFetchEditor` behavior model.

## Current State

State: `release-pass` for the declared v1 import/clone/fetch profiles.

SVN CLI and linked libsvn replay share the `RaSession`/`FetchEditor` pipeline and
cover direct URLs, standard layouts, timestamps/identities, checkout modes,
bounded and parent fetches, copies/follow-parent, properties, persistent empty
directories, and recoverable multi-mapping publication. The current linked CLI
matrix passes all 48 real clone/fetch cases, including the corrected incremental
add-file property projection.

Obscure Fetcher semantics and unvalidated remote/platform combinations remain
outside the declared profile rather than implicit release claims.

## Normative References

- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`Ra.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Ra.pm)
- [`Fetcher.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Fetcher.pm)
- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)

## Required Pipeline

```text
config + tracking state
        ↓
bounded get_log discovery
        ↓
mapping/revision selection
        ↓
do_update or do_switch
        ↓
SvnFetchEditor (sole behavior model)
        ↓
complete commit plan
        ↓
Git object writer
        ↓
MetadataStateStore publication and verification
        ↓
clone branch/HEAD/worktree update when requested
```

`FastImportStream` remains a byte-safe Git writer. It does not decide SVN copy, property, filter, placeholder, or parent behavior.

## Work Order

### P0-1. Fix repository/session/mapping path handling

Consume the Phase 2 URL model and remove URL splitting from import logic.

Required fixtures:

- clone repository root without layout options;
- clone `.../trunk` as a single path;
- clone stdlayout repository root;
- init at repository root then fetch mapping subpaths;
- full URL layout options;
- repository subpath sessions for CLI and libsvn.

No adapter may construct `trunk/trunk/...` or strip a local file-system portion as an SVN path.

### P0-2. Preserve SVN date and complete Git identity

- Parse SVN revision date without loss of ordering or timezone semantics.
- Set both Git author and committer time according to the frozen baseline.
- Implement UTC default and `--localtime` offset behavior.
- Preserve mapped author/committer name/email, full message bytes where representable, footer, parent(s), tree, and modes.
- Reject invalid/missing date according to a documented baseline decision; never substitute loop indices or wall-clock now.

Golden fixtures compare complete graph fingerprints and exact OIDs when deterministic.

### P0-3. Complete clone checkout semantics

Default clone must:

- create the expected local branch from the selected SVN tracking ref;
- make `HEAD` resolve to a commit;
- populate index and working tree;
- preserve local modifications only in scenarios where the baseline permits an existing worktree.

`--no-checkout` creates refs/metadata/branch state as the baseline does but does not populate the working tree. Tests assert the difference explicitly.

### P0-4. Complete revision and fetch-option semantics

- Support numeric revision, numeric range, `NUMBER:HEAD`, `BASE:NUMBER`, `BASE:HEAD`, and `HEAD` where the frozen fetch parser supports them.
- Implement `fetch --parent` from current HEAD tracking identity.
- Use configured/runtime `log-window-size` with a bounded loop and progress monotonicity.
- Apply CLI/config authors, filters, ignore refs, metadata, localtime, auth, and placeholder options through the centralized overlay.
- Unsupported forms fail before import mutation.

### P1-1. Unify all production import through `SvnFetchEditor`

- Phase 4 CLI and libsvn adapters drive the same callback/event contract.
- Remove the direct `RevisionEvent` to `FileChange` production path.
- Mock import may remain only as a contract test adapter.
- Shared adapter fixtures must yield the same commit plan and exact graph.

### P1-2. Complete Fetcher delta semantics

Implement and test:

- add/open/delete file and directory;
- copy-from path/revision and switch behavior;
- base-tree loading and base/result checksum validation;
- fulltext reconstruction from delta windows;
- executable and special/symlink transitions;
- unknown file/directory properties written to `unhandled.log`;
- absent file/directory recording;
- path encoding and `.git` rejection;
- close/abort behavior that cannot publish a partial commit.

Textual properties used only for write-back/golden inspection must not accidentally alter Git blob content unless the baseline does so.

### P1-3. Make empty-directory behavior tree-based and persistent

- Determine emptiness from base tree plus the completed delta, not only changed paths in the revision.
- Track placeholder ownership in config/metadata so a real child removes only generated placeholders.
- Handle newly empty directories, property-only directory changes, copies, deletes, filters, incremental fetch, checkout, and reset.
- Record absent/empty directory information needed by `mkdirs`/gc-compatible metadata even if those commands remain deferred.

### P1-4. Complete branch/tag discovery and follow-parent

- Maintain discovery high-water state without losing branches that did not change in the current log window.
- Consider copy-from source mappings even when the source branch has no changed path in the window.
- Reproduce baseline parent/backfill behavior, including auxiliary `branch@rev` refs when required.
- Detect mapping/ref collisions and ignored refs deterministically.
- Add long/convoluted history fixtures separately from fast default tests.

### P1-5. Publish Git/ref/rev_map state recoverably

Use Phase 3 state operations:

- validate old ref/rev_map identity;
- write objects and prepare records;
- publish with expected-old ref CAS and documented rev_map ordering;
- verify the tail and tip;
- recover or report an actionable incomplete transaction after injected failure.

No successful fetch may leave an undetectable ref/rev_map mismatch.

## Invariants

- The same SVN revision/mapping/config produces the same Git identity across default and linked adapters.
- No commit is published before its editor closes successfully and checksums pass.
- A skipped/filtered SVN revision is represented according to baseline rev_map behavior, not silently confused with an imported commit.
- Parent selection is based on validated tracking history, not maximum SVN revision number.
- Clone success includes branch/HEAD/worktree assertions unless `--no-checkout` is explicit.

## Gates

### Structural gate

- one import coordinator consumes `SvnFetchEditor` output;
- timestamp/date type and checkout step exist;
- direct production log-to-Git path is removed or unreachable;
- Phase 2 URL and Phase 3 state boundaries are used.

### Behavioral gate

- single-path and stdlayout file/svn fixtures pass for initial and incremental fetch;
- timestamps, timezones, authors, full messages, parents, trees, modes, refs, rev_maps, branch/HEAD, index, and working tree are asserted;
- default and linked adapters produce equivalent exact graphs;
- windowing, parent fetch, revision forms, filters, copy/follow-parent, empty/absent/property/checksum/abort, and recovery scenarios pass;
- no required external scenario is skipped in the backend job.

### Release gate

- single-path and stdlayout clone/fetch artifacts match the frozen Perl baseline without object-ID or clone-state normalization;
- all declared read profiles reach their target state in the capability matrix;
- repeated fetch is idempotent, interrupted fetch is recoverable, and no profile produces divergent Git history.

## Out of Scope

- optimizing large-repository throughput before bounded correctness is proven;
- implementing branch/tag write-back;
- keeping separate compatibility semantics for SVN CLI and libsvn.
