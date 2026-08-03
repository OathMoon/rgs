# Phase 1: Workspace and CLI Contract

## Objective

Maintain the three-crate Rust workspace and provide a CLI whose parsed commands, options, exit behavior, and diagnostics match the declared `git svn` compatibility surface.

## Current State

State: `behavior-pass`.

Verified release surface:

- `git-svn-rs` CLI crate, `git-svn-rs-core`, and opt-in shim;
- typed subcommands for the v1 read/write workflow;
- explicit errors for known out-of-scope commands;
- help, diagnose, forwarding, stdout/stderr, exit-code, and no-mutation tests;
- a command/option/protocol inventory at
  `.plans/release-capability-inventory.md` with every parsed entry classified;
- explicit rejection of global quiet/verbose flags rather than inert parsing.

The declared v1 CLI surface has no known inert option. Deferred commands remain
recognized but fail before repository mutation. Final release status is tied to
the Phase 8 hosted compatibility gate rather than to additional Phase 1 work.

## Normative References

- [`git-svn.perl` at the frozen baseline](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/git-svn.perl)
- [`Documentation/git-svn.adoc`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/Documentation/git-svn.adoc)
- [`Git.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git.pm) for process/error conventions

## Scope

- workspace and binary packaging;
- command/option spelling and argument cardinality;
- global verbosity/quiet handling;
- unsupported/deferred capability diagnostics;
- exit codes and stdout/stderr ownership;
- version/diagnostic output that identifies the compatibility baseline and backend availability;
- optional shim forwarding without shadowing the default binary.

## Required Work

### 1. Build a CLI capability inventory

For every parsed command and option, record one of:

- implemented and linked to a behavioral phase gate;
- explicitly unsupported in this release;
- deferred behind a named profile/feature.

Remove or explicitly reject any parse-only option. The inventory must include global `--verbose/--quiet`, `clone --no-checkout`, `fetch --parent`, revision forms, auth/config arguments, metadata options, and layout options.

### 2. Centralize dispatch outcomes

- Commands return structured success/error results to the binary boundary.
- The binary decides stdout, stderr, and exit status once.
- Unsupported capability, invalid usage, backend unavailable, auth failure, and runtime failure remain distinguishable.
- Secrets are never rendered in debug or error output.

### 3. Implement observable verbosity semantics

- `--quiet` suppresses progress only, not requested command output or errors.
- increasing `--verbose` levels expose diagnostics without changing behavior;
- contradictory flags follow the frozen baseline or fail explicitly;
- tests assert stdout and stderr separately.

### 4. Keep the shim opt-in and transparent

- The shim forwards arguments and exit status exactly.
- Diagnostics can distinguish the shim from Perl `git-svn` so golden detection cannot compare Rust against itself.
- Installation does not overwrite a system `git-svn` unless the user explicitly chooses it.

### 5. Report the frozen baseline

`--version` or `diagnose` must expose:

- `git-svn-rs` version;
- frozen Git baseline `v2.54.0` / `0b13e48...`;
- default versus linked libsvn backend availability;
- enough environment information to reproduce a compatibility run, without secrets.

## Invariants

- Parsing is not implementation evidence.
- No command module prints ad hoc progress directly if it bypasses global quiet/verbose policy.
- Known unsupported commands fail before mutating Git or SVN state.
- CLI additions require a target phase and an acceptance test; they are not added speculatively.

## Gates

### Structural gate

- workspace builds on the declared Rust version;
- help lists supported and recognized-unsupported commands;
- shim forwarding and backend diagnostics compile in default and feature builds.

### Behavioral gate

- the capability inventory has no inert entries;
- stdout/stderr/exit-code tests cover valid, invalid, unsupported, unavailable-backend, and auth-error paths;
- quiet/verbose behavior is observable and stable;
- unsupported commands perform no repository mutation.

### Release gate

- CLI spelling and applicable default behavior match the frozen `git-svn.perl`/documentation fixtures;
- every user-visible support claim links to a release-pass profile;
- the shim cannot be mistaken for the Perl comparison backend.

## Out of Scope

- implementing branch/tag write-back, `set-tree`, `commit-diff`, or property-editing commands;
- adding a logging framework unless required to satisfy the observable verbosity contract;
- stabilizing a public Rust API.
