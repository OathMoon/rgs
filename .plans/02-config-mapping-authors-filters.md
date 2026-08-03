# Phase 2: URL, Configuration, Mapping, Authors, and Filters

## Objective

Provide one compatibility model for SVN repository identity, session URLs, repository-relative paths, ref mappings, option precedence, author mapping, path filters, and metadata modes.

## Current State

State: `release-pass` for the declared layout and protocol profiles.

Verified behavior covers centralized config loading, single-path and standard,
custom, partial, and full-URL layouts, ordered fixed/wildcard mappings, authors
files/programs, Perl-style negative lookahead filters, and ignore-over-include
precedence across CLI and linked replay.

Subdirectory URLs no longer duplicate path components; full layout URLs,
branches-only/tags-only construction, prefixes, ref collisions, encoded paths,
metadata immutability, auth/config precedence, and every accepted fetch option
are exercised or explicitly rejected. Broader platform/remote combinations are
future profiles, not gaps in the declared v1 profiles.

The exact config/refspec/URL artifacts and required layout scenarios passed in
Phase 8 hosted run
[#5](https://github.com/OathMoon/rgs/actions/runs/30790332534).

## Normative References

- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)
- [`Git::SVN.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN.pm)
- [`GlobSpec.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/GlobSpec.pm)
- [`Utils.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Utils.pm)
- [`Git.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git.pm) for config cardinality

## Required Domain Model

The implementation must represent, without string guessing:

- canonical repository root URL;
- configured/session URL;
- session path within the repository;
- repository-relative changed path;
- mapping-relative Git path;
- optional push/commit URL;
- remote name and ref namespace.

Names may differ, but conversions must be typed/tested and all SVN CLI/libsvn/dcommit code must use the same model.

## Required Work

### 1. Correct URL and path semantics

- Fix `file:///repo/trunk` and equivalent remote subdirectory sessions without producing `trunk/trunk/...`.
- Canonicalize URL components through `url` and/or SVN canonicalization; preserve scheme/host and percent-encode as the baseline does.
- Support relative and full URL values for `-T/--trunk`, `-b/--branches`, and `-t/--tags`.
- Reject layout URLs outside the repository identity or split them into a valid common root and relative mapping as upstream does.
- Cover spaces, non-ASCII names, trailing slashes, peg-sensitive `@`, Windows file URLs, and repository subpaths.

### 2. Correct layout construction

- `--stdlayout` supplies trunk/branches/tags defaults; explicit options override only the supplied components.
- Supplying only branches or tags must not invent a trunk mapping.
- Multiple branches/tags values retain order and detect ref collisions.
- Prefix requirements, including the trailing slash when branches are used, follow the baseline.
- Single-path tracking produces `refs/remotes/git-svn`; layout tracking uses the baseline namespace unless the user supplies a prefix.

### 3. Centralize configuration loading and overlay

Create one loader used by fetch, resolver, readonly, rebase, and dcommit. It must:

- distinguish single-value and multi-value keys;
- parse leading force `+` only where refspec syntax allows it;
- preserve all remote names rather than hard-coding `svn`;
- apply documented CLI/remote/global precedence;
- combine command-line and persisted ignore/include expressions where the baseline combines them;
- never persist passwords;
- return structured errors for duplicate/invalid values instead of selecting an arbitrary value.

### 4. Complete option semantics

The config model must carry only options with an implemented consumer. Required coverage includes:

- authors file/program;
- ignore/include paths and ignore refs;
- `log-window-size`, `localtime`, and revision forms;
- username, config-dir, no-auth-cache, and runtime password overlay;
- `noMetadata`, `useSvmProps`, `useSvnsyncProps`, rewriteRoot, and rewriteUUID;
- preserve-empty-dirs and placeholder filename;
- follow-parent and no-minimize-url where included in the release surface.

Metadata-affecting options are immutable after the first import. Mutually exclusive combinations fail before repository mutation. `noMetadata` is treated as a one-shot mode with documented fetch/log/rebuild limits.

### 5. Preserve author and filter compatibility

- Authors files reject malformed mappings with file/line context.
- Authors programs receive the same author input and their failures/invalid identities are explicit.
- `.git` paths are always rejected before user regex evaluation.
- Ignore takes precedence over include; command-line/config combination follows the baseline.
- Filter decisions use repository/mapping-relative paths consistently across CLI and libsvn adapters.

## Invariants

- No production module derives an SVN relative path by splitting the configured URL on `://`.
- A canonical URL is never used as a filesystem path and vice versa.
- One config loader owns key names, defaults, cardinality, and precedence.
- Unsupported options fail explicitly at the CLI/config boundary.
- Mapping expansion cannot silently map two SVN sources to the same Git ref.

## Gates

### Structural gate

- typed URL/path conversions and centralized config loading compile;
- `GlobSpec`, authors, filter, and metadata conflict unit tests pass;
- production command modules no longer carry duplicate config parsers.

### Behavioral gate

- real SVN fixtures cover root URL, single-subdirectory URL, stdlayout, custom relative layout, and full layout URLs;
- branches-only/tags-only/prefix/multiple mapping behavior matches the frozen baseline;
- command-line fetch options demonstrably affect behavior or fail explicitly;
- metadata option immutability and one-shot `noMetadata` behavior are tested;
- CLI and libsvn adapters resolve the same canonical repository-relative paths.

### Release gate

- config/refspec/URL artifacts match the frozen Perl baseline for every declared layout profile;
- single-path clone no longer duplicates path components;
- remote/profile support claims include auth/config precedence fixtures without secret leakage.

## Out of Scope

- arbitrary SVN URL rewriting beyond the frozen metadata options;
- automatic resolution of mapping collisions without user configuration;
- adding new metadata modes not present in the baseline.
