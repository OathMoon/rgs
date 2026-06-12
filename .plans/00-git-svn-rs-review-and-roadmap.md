# git-svn-rs Review and Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Use Git's Perl `git-svn` implementation as the compatibility map for staged Rust implementation.

**Architecture:** Treat this file as the architecture bus for all other `.plans` files. Implement compatibility units first (`GlobSpec`, path/url utilities, rev map, RA session, fetch editor, commit editor, log formatter, auth prompt, migration, golden tests), then wire user-facing commands to those units. A task named "optimization" in a split plan is mandatory when it provides one of these compatibility units.

**Tech Stack:** Rust 1.95, Cargo workspace, Git CLI plumbing, Subversion CLI fixtures, optional `subversion`/`subversion-sys` behind `svn-libsvn`, Perl `git svn` golden fixtures when available.

---

## Perl Module Responsibility Map

- `git-svn.perl` controls CLI command names, option spelling, dispatch, and legacy command compatibility.
- `perl/Git.pm` controls the shared Perl Git command wrapper model: repository discovery, working-copy context, config lookup semantics, command output capture, pipe close/error propagation, prompt fallback, path quoting helpers, temp lock helpers, and object IO helpers. Rust `GitCli` must use this as the compatibility reference for Git plumbing wrappers, not only the `git-svn` submodules.
- `Git::SVN.pm` controls repository metadata, `git-svn-id`, fetch range calculation, rev map format, commit creation, `noMetadata`, `useSvmProps`, `useSvnsyncProps`, `rewriteRoot`, and `rewriteUUID`.
- `Git::SVN::GlobSpec` maps to Rust `glob_spec.rs`: refspec glob parsing, wildcard constraints, `{a,b}` pattern handling, depth, regex, and `full_path`.
- `Git::SVN::Utils` maps to Rust `path_url.rs`: path canonicalization, URL canonicalization, path joining, and adding SVN paths to URLs.
- `Git::SVN::Ra` maps to Rust `ra/session.rs` and `ra/fetch_loop.rs`: RA sessions, auth config setup, `check_path`, `get_dir`, `get_log`, windowed fetch, glob matching, URL minimization, unknown revision skipping, `do_update`, and `do_switch`.
- `Git::SVN::Fetcher` maps to Rust `fetch_editor.rs`: SVN delta editor import, path filtering, `.git` rejection, path stripping, pathname encoding, `svn:executable`, `svn:special`, empty-directory placeholders, absent path recording, and unhandled property logging.
- `Git::SVN::Editor` maps to Rust `commit_editor.rs`: linear `dcommit` write-back, `git diff-tree -z -r -C` parsing, `check_diff_paths`, `ensure_path`, `open_or_add_dir`, `D/C/R/A/M/T` operation ordering, autoprops, manual props, `svn:executable`, `svn:special`, and optional explicit mergeinfo.
- `Git::SVN::Log` maps to Rust `log_formatter.rs`: `log`, `find-rev`, revision ranges, `--oneline`, `--incremental`, `--show-commit`, verbose changed paths, date formatting, and metadata extraction.
- `Git::SVN::Migration` maps to Rust `migration.rs`: legacy metadata discovery, `.rev_db` to `.rev_map` migration policy, empty `[svn-remote]` detection, and one-way migration warnings.
- `Git::SVN::Prompt` maps to Rust `auth/prompt.rs`: username/password prompts, `--username`, `--no-auth-cache`, SSL server trust, and client certificate prompts.
- `Git::SVN::Memoize::YAML` maps to Rust `cache/yaml.rs`: stable on-disk cache shape for reusable compatibility data where a human-readable cache is useful.

## Roadmap Principles

- Implement Perl-compatible units before command glue. A command task may call a compatibility unit, but must not invent a parallel shortcut that bypasses the unit.
- Do not accept a phase gate while a required compatibility unit is still parked in a later "optimization" section. Move the unit into the phase's main path or mark the phase as incomplete.
- `FastImport` can remain a Git object writing mechanism, but it is not the SVN behavior model. The behavior model for `fetch` is `SvnFetchEditor`, derived from `Fetcher.pm`.
- `dcommit` must use an editor-driven path. The command may still enumerate local commits, but write-back behavior belongs in `GitDiffPlanner`, `SvnCommitEditor`, `PathEnsurer`, and `PropertyMapper`.
- Keep v1 scope conservative: core read/write workflow only. Do not implement branch/tag write-back, automatic mergeinfo generation, `set-tree`, or `commit-diff`.
- Golden compatibility tests are first-class acceptance criteria. If Perl `git svn` is unavailable, skip those tests with an explicit dependency message; do not silently weaken unit tests.
- Default builds must not require libsvn. The `svn-libsvn` feature enables the real RA/editor backend; mock backends support most unit and command tests.

## Perl Directory Review Impact

- The reference root is the full `perl` directory, not only `perl/Git/SVN`, because `git-svn.perl` relies on shared `Git.pm` behavior for Git command execution, config lookup, prompt fallback, path quoting, repository discovery, and lock/temp helpers.
- `GitCli` must expose a small but explicit wrapper layer instead of scattered `std::process::Command` calls. Command modules may use convenience helpers, but process execution, config reads, object-format detection, and error normalization belong behind that wrapper.
- Auth prompt behavior must cross-check both `Git::SVN::Prompt` and `Git.pm::prompt`, including askpass environment fallback and terminal prompt fallback.
- Golden tests must record both `git-svn` module sources and the top-level Perl/Git wrapper source used for comparison so regressions in command/config/prompt behavior are traceable.

## Replanned Dependency Order

1. Foundation: workspace, CLI parsing, unsupported-command diagnostics, and feature diagnostics.
2. Pure compatibility units: `GlobSpec`, path/url canonicalization, ref mapping, authors, filters, metadata options, and migration inspection.
3. Git metadata: `GitCli`, `GitSvnId`, `.git/svn` paths, `.rev_map` binary IO, lock/fsync behavior, object format detection, and metadata conflict validation.
4. SVN access: fixtures, auth prompt abstraction, `RaSession`, `FetchEditor`/`CommitEditor` traits, `do_update`, `do_switch`, mock RA/editor sessions, and feature-gated libsvn shell.
5. Import: byte-safe Git object writer, `SvnFetchEditor`, branch/tag discovery, `init`, `fetch`, and `clone`.
6. Read-only commands: `GitSvnLogFormatter`, bidirectional `find-rev`, `info`, `log`, `gc`, `reset`, and `rebase`.
7. Write-back: `GitDiffPlanner`, `SvnCommitEditor`, `PathEnsurer`, `PropertyMapper`, linear `dcommit`, and optional shim.
8. Compatibility gate: Perl `git svn` golden fixture capture and strict normalized comparison.

## Parallel Development Lanes

- Lane A can implement Phase 1 CLI and diagnostics immediately.
- Lane B can implement `GlobSpec`, path/url helpers, authors, filters, and config serialization after CLI argument names are stable.
- Lane C can implement Git metadata and rev_map independently from Lane B once workspace scaffolding exists.
- Lane D can implement SVN fixtures, auth prompt, RA/editor traits, and mock sessions in parallel with Lane B/C.
- Lane E can implement byte-safe Git object writing in parallel, but real `fetch` must wait for Lane B/C/D compatibility interfaces.
- Lane F can implement log formatting and bidirectional `find-rev` after rev_map and `git-svn-id` are stable.
- Lane G can implement dcommit diff planning and property mapping while fetch is being finalized; production `dcommit` waits for fetch and commit editor wiring.
- Lane H can build the golden harness early, but strict comparisons become required only after clone/fetch and dcommit are functional.

## Verification Modes

- Developer mode: `cargo test --workspace` must pass; SVN/libsvn/Perl-dependent tests may skip only with explicit messages.
- Compatibility mode: run on an environment with Git, SVN CLI tools, libsvn development libraries, and Perl `git svn`; no golden, fetch, or dcommit compatibility test may skip.
- Release mode: compatibility mode plus `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, default build, and `svn-libsvn` feature build.

## Split Plan Files

- `.plans/01-foundation-cli-workspace.md`: Create Git/Rust workspace, CLI shape, unsupported command behavior, logging, and diagnostics.
- `.plans/02-config-mapping-authors-filters.md`: Implement `GlobSpec`, path/url utilities, config parsing, layout mapping, authors resolution, and filter precedence.
- `.plans/03-git-metadata-revmap.md`: Implement Git plumbing, `git-svn-id`, `.git/svn` metadata, `.rev_map`, migration, and metadata option compatibility.
- `.plans/04-svn-fixtures-and-backend.md`: Implement fixtures, `RaSession`, RA cache, auth prompt mocks, editor traits, mock backend, and feature-gated libsvn backend.
- `.plans/05-import-clone-fetch.md`: Implement `init`, `clone`, `fetch`, `SvnFetchEditor`, branch/tag discovery, properties, empty directories, absent paths, and Git object writing.
- `.plans/06-readonly-commands-rebase-reset.md`: Implement `GitSvnLogFormatter`, `find-rev`, `info`, `log`, `gc`, `reset`, and `rebase`.
- `.plans/07-dcommit-shim-ci.md`: Implement linear `dcommit` through `GitDiffPlanner` and `SvnCommitEditor`, optional `git-svn` shim, and Windows-first verification.
- `.plans/08-compatibility-golden-tests.md`: Generate Perl `git svn` golden fixtures and compare Rust output for metadata, refs, rev maps, logs, import edge cases, and linear `dcommit`.

## Delivery Gates

- Phase 1 gate: `cargo test --workspace` passes and `git-svn-rs --help` lists core and known unsupported commands.
- Phase 2 gate: `GlobSpec` golden tests match `GlobSpec.pm`, path/url canonicalization matches `Utils.pm`, config serialization preserves all core options, and filter precedence rejects `.git` paths before regex checks.
- Phase 3 gate: `.rev_map` tests cover SHA-1/SHA-256 sizes, all-zero records, `rev_map_max(want_commit)`, lock/fsync behavior, metadata migration inspection, object-format detection, and metadata option conflicts.
- Phase 4 gate: `RaSession` mock tests cover `check_path`, `get_dir`, `get_log`, `do_update`, `do_switch`, auth prompts, fixture creation, and feature-gated libsvn diagnostics.
- Phase 5 gate: `SvnFetchEditor` imports fixture revisions with symlinks, executable bits, copy-from, empty dirs, placeholders, absent paths, byte-safe file contents, branch/tag discovery, and `git-svn-id` footers.
- Phase 6 gate: read-only command output matches golden fixtures for `log`, bidirectional `find-rev`, `info`, `reset`, and `rebase --dry-run`.
- Phase 7 gate: linear `dcommit` writes `A/M/D/C/R/T` changes through `SvnCommitEditor`, fetches the new SVN revision, updates rev maps, and optionally rebases.
- Phase 8 gate: Perl `git svn` and Rust `git-svn-rs` produce matching normalized golden artifacts for all v1 compatibility scenarios; count-only or length-only assertions are not sufficient.

## dcommit Implementation Path

- Keep the v1 command scope linear: only commits in `<configured-svn-tracking-ref>..HEAD` on one upstream are eligible.
- `LinearDcommitCommand` resolves the configured SVN-tracking ref from `[svn-remote]` metadata, enumerates commits, checks cleanliness, handles `--dry-run`, calls `GitDiffPlanner`, calls `SvnCommitEditor`, fetches the returned revision, and rebases unless `--no-rebase` is set.
- `GitDiffPlanner` runs `git diff-tree -z -r -C` and parses `A`, `M`, `D`, `C`, `R`, and `T` records with old/new modes, object IDs, and paths.
- `SvnCommitEditor` preloads SVN path types with `check_diff_paths`, opens the SVN commit editor, applies operations in `D/C/R/A/M/T` order, and aborts if no changes and no explicit mergeinfo exist.
- `PathEnsurer` implements `ensure_path` and `open_or_add_dir` so nested additions and copies create missing parent directories correctly.
- `PropertyMapper` handles `svn:executable`, `svn:special`, symlink toggles, autoprops, and `.gitattributes svn-properties`.
- Explicit `--mergeinfo` may be accepted later; automatic mergeinfo generation is not part of v1.

## Execution Rules

- Implement phases in order and commit after each task group.
- Keep `svn-libsvn` feature optional until CI has libsvn installed.
- Prefer Git CLI plumbing over `libgit2`; every Git command wrapper must be tested with a temp repository.
- When a plan mentions Perl behavior, add a unit or golden test that captures the behavior before implementation.
- Do not add branch/tag write-back, automatic mergeinfo generation, `commit-diff`, `set-tree`, or property editing commands to v1.

## Reference Sources

### Git documentation and command entry points

- [git-svn official documentation](https://git-scm.com/docs/git-svn)
- [git-svn.perl on GitHub](https://github.com/git/git/blob/master/git-svn.perl)
- [raw git-svn.perl](https://raw.githubusercontent.com/git/git/master/git-svn.perl)
- [Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)

### Perl implementation modules

- [perl directory](https://github.com/git/git/tree/master/perl)
- [Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)
- [Editor.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Editor.pm)
- [Fetcher.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Fetcher.pm)
- [GlobSpec.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/GlobSpec.pm)
- [Log.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Log.pm)
- [Migration.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Migration.pm)
- [Prompt.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Prompt.pm)
- [Ra.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Ra.pm)
- [Utils.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Utils.pm)
- [Memoize/YAML.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN/Memoize/YAML.pm)

### Rust and SVN dependency references

- [subversion crate](https://crates.io/crates/subversion)
- [subversion docs](https://docs.rs/subversion)
- [subversion-sys crate](https://crates.io/crates/subversion-sys)
- [subversion-sys docs](https://docs.rs/subversion-sys)
- [svn crate](https://crates.io/crates/svn)
- [svn docs](https://docs.rs/svn)
- [subversion-rs repository](https://github.com/jelmer/subversion-rs)
- [svn-rs repository](https://github.com/lvillis/svn-rs)

## Reference Usage By Phase

- Phase 1 uses the git-svn documentation and `git-svn.perl` for command names, option spelling, and unsupported-command compatibility.
- Phase 2 uses `GlobSpec.pm`, `Utils.pm`, and the git-svn documentation for refspec expansion, path normalization, URL canonicalization, authors, and filters.
- Phase 3 uses `Git.pm`, `Git::SVN.pm`, `Migration.pm`, and `git-svn.perl` for Git command wrapper behavior, config reads, repository discovery, `git-svn-id`, `.git/svn` paths, `.rev_map` records, and legacy metadata migration.
- Phase 4 uses `Git.pm`, `Ra.pm`, `Prompt.pm`, `subversion`, and `subversion-sys` for RA sessions, authentication, prompt fallback, SVN config directories, and libsvn feature gating.
- Phase 5 uses `Fetcher.pm`, `Ra.pm`, and `Git::SVN.pm` for SVN delta import, branch/tag glob discovery, empty-directory placeholder handling, and commit creation.
- Phase 6 uses `Log.pm`, `Git::SVN.pm`, and the git-svn documentation for `log`, `find-rev`, `info`, `reset`, and `rebase` behavior.
- Phase 7 uses `Editor.pm`, `Ra.pm`, and `Prompt.pm` for linear `dcommit`, SVN commit editor behavior, path creation, properties, and authentication.
- Phase 8 uses `perl/Git.pm`, all relevant `Git::SVN` Perl modules, and `git-svn.perl` as golden fixture authorities and records every fixture source in the test output.

## Self Review

- Spec coverage: the roadmap now maps every referenced Perl module to Rust implementation units and phase gates.
- Placeholder scan: no phase relies on an unnamed compatibility layer; `fetch`, `dcommit`, and golden tests have explicit implementation paths.
- Type consistency: shared names are fixed across files: `SvnRemoteConfig`, `GlobSpec`, `RevMap`, `RaSession`, `SvnFetchEditor`, `GitSvnLogFormatter`, `GitDiffPlanner`, `SvnCommitEditor`, and `GitSvnId`.
