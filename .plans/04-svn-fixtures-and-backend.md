# Phase 4: SVN Adapters, RA/Delta, Authentication, and Native Safety

## Objective

Provide repeatable SVN fixtures and transport adapters that expose one revision/editor contract to the compatibility domain, with a production libsvn delta path and safe authentication/error boundaries.

## Current State

State: `in-progress`.

Existing capabilities include local SVN fixtures, mock RA/editor behavior, an SVN CLI reader, linked libsvn metadata/log/file/property access, simple auth baton support, log-backed `do_update`/`do_switch`, and extensive test-only `svn_ra_do_update3` callback scaffolding.

This phase has not passed because:

- public libsvn `do_update`/`do_switch` still synthesize editor calls from logs;
- the true delta-to-`FetchEditor` adapter is inside `#[cfg(test)]`;
- default-parallel linked tests can race on global callback recorders and abort through an `extern "C"` panic;
- auth covers only a subset of simple credentials and not the full declared remote profiles;
- unsafe FFI, APR, auth, RA, replay, adapter, and large tests share one file.

## Normative References

- [`Ra.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Ra.pm)
- [`Fetcher.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Fetcher.pm)
- [`Prompt.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git/SVN/Prompt.pm)
- [`Git.pm`](https://github.com/git/git/blob/0b13e48a3a30cdfa94e8ef842e24d6045ab3d015/perl/Git.pm) prompt fallback
- [`subversion` 0.1.10](https://docs.rs/subversion/0.1.10/subversion/)
- [`subversion-sys`](https://docs.rs/subversion-sys/latest/subversion_sys/)

## Adapter Contract

Transport adapters provide:

- canonical repository root, session URL/path, UUID, and latest revision;
- bounded `get_log` discovery with changed paths and revision properties;
- `check_path` and `get_dir` where branch/copy planning requires them;
- `do_update`/`do_switch` that drive the common editor contract;
- explicit capability/auth/error information.

Adapters do not write Git objects, refs, rev_maps, or implement fetch filtering/placeholder semantics.

## Required Work

### 1. Record a libsvn binding ADR before expanding FFI

Implement the same small spike with the current handwritten layer and `subversion` 0.1.10 where possible:

- open a session and read UUID/latest revision;
- bounded log request;
- initial and incremental `do_update` into a delta consumer;
- simple auth and error chain.

Record coverage gaps, Windows/vcpkg behavior, lifetimes, error quality, maintenance cost, and the selected safe/hybrid/manual approach. Do not rewrite solely for architectural aesthetics.

### 2. Isolate native responsibilities

If handwritten FFI remains, split or clearly isolate:

- raw ABI declarations/layout checks;
- APR runtime/pool ownership;
- SVN error conversion;
- auth/config providers;
- RA session methods;
- delta reporter/editor adapter;
- integration fixtures/tests.

Domain code must not manipulate raw pools, pointers, or callback tables.

### 3. Make callbacks panic-free and operation-owned

- No `unwrap`, assertion, mutex poisoning, or Rust unwind can cross C ABI.
- Every callback validates baton/pointers and stores failure in operation state or returns an SVN error.
- Recorder/adapter state belongs to the baton; process-global mutable vectors/counters are removed from behavior tests.
- Pool and buffer lifetimes are documented and tested for initial, incremental, abort, and error paths.

### 4. Promote the native delta adapter to production

Move the verified adapter out of tests and cover:

- target revision and root lifecycle;
- add/open/close directory and file;
- copy-from path/revision;
- delete entry;
- file/directory property add/change/delete;
- `apply_textdelta` against the correct base content;
- base and result checksum validation;
- absent file/directory;
- close and abort edit;
- repository subpath sessions and switch URLs.

Only then replace public log-backed `do_update`/`do_switch`. Retain log replay, if needed, as an explicitly named fallback diagnostic—not as a second compatibility model.

### 5. Converge the SVN CLI adapter

The CLI adapter may obtain snapshots through `svn log/cat/propget`, but it must drive the same normalized editor/event contract consumed by Phase 5. It cannot call a direct log-to-`FileChange` import path.

Adapter parity tests compare normalized event streams and final Git artifacts, not internal call counts.

### 6. Complete auth/provider behavior by profile

Model and test:

- explicit username/password without persisting passwords;
- config-dir and no-auth-cache;
- cached credentials where allowed;
- askpass then terminal fallback for production, with non-interactive behavior explicit;
- SSL server trust and client certificate for any claimed HTTPS profile;
- svn+ssh external transport behavior if that profile is enabled.

Auth errors preserve SVN child error context and never echo credentials.

### 7. Make fixtures capability-oriented

Keep reusable fixtures for:

- `file://` repository;
- anonymous and authenticated `svnserve`;
- standard and single-subdirectory layouts;
- copy/delete/property/empty/absent/incremental delta histories;
- an opt-in remote HTTP(S) fixture if that profile becomes a first-release requirement.

Fixture dependency absence is a developer skip only. Backend/release jobs fail when their required fixture cannot run.

## Invariants

- The adapter reports repository-relative paths consistently with Phase 2.
- `get_log` is discovery; editor callbacks are content behavior.
- A linked feature that failed to link is “unavailable”, not a silent success/fallback in a linked gate.
- FFI errors are structured and retain the native chain.
- Default-parallel test execution is a required safety signal, not an optional optimization.

## Gates

### Structural gate

- adapter interfaces and fixture helpers compile in default/unlinked/linked builds;
- ADR is recorded;
- unsafe/native responsibilities are isolated;
- no C callback contains a path that can panic.

### Behavioral gate

- public libsvn update/switch use true reporter/delta callbacks;
- default and linked adapters produce equivalent normalized editor events and exact Git artifacts for the shared fixture;
- initial, incremental, copy, delete, property, checksum, absent, abort, auth, and subpath cases pass;
- linked suite passes repeatedly with the default parallel harness and once serially for diagnostics.

### Release gate

- every claimed profile executes with its real transport/auth path;
- no required backend test is filtered or skipped;
- backend errors and prompt behavior match the frozen baseline where observable;
- capability diagnostics and documentation exactly match the built binary.

## Out of Scope

- native commit-editor write-back, which belongs to Phase 7;
- supporting every SVN auth provider before its protocol profile is declared;
- keeping both log replay and true delta as equal production semantics.
