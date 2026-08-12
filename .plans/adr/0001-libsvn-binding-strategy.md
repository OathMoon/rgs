# ADR 0001: Keep a constrained handwritten libsvn boundary for v0.1

- Status: Accepted
- Date: 2026-08-12
- Scope: `git-svn-rs-core` linked read/import backend

## Context

The linked backend already implements the release profiles through a small handwritten
FFI surface in `svn/libsvn.rs` plus the production reporter/delta adapter in
`svn/libsvn/native_delta.rs`. It has exact local `file://`, `svn://`, configured and
loopback SSH, and authenticated loopback DAV coverage. Replacing that boundary during
release hardening would combine an ABI migration with behavior changes in the highest
risk import path.

The alternatives considered were:

1. keep the current handwritten declarations and callbacks;
2. replace them with the safe `subversion` crate (evaluated at 0.1.10);
3. use `subversion`/`subversion-sys` for selected operations while retaining the current
   native delta adapter.

`subversion` 0.1.10 exposes RA, delta/editor, auth, APR-backed lifetime, and native error
wrappers, so it is a credible future replacement. It is still pre-1.0 and adopting it
would change pool ownership, callback wrappers, linking/build dependencies, and error
types together. A hybrid inside one RA operation would create two ownership models for
the same APR/libsvn objects and is therefore the least attractive option.

## Decision

For v0.1, retain the existing handwritten FFI boundary and freeze its surface. No new
raw declaration, callback table field, or auth provider may be added without either:

- a focused ABI/layout check and linked integration test for that surface; or
- a follow-up ADR that migrates the whole owning operation to `subversion`.

The preferred future direction is a coherent safe-wrapper migration, operation by
operation (runtime/session/auth/error/delta together), rather than passing APR or SVN
pointers between handwritten and crate-owned wrappers.

Native commit-editor/write-back remains deferred. Dcommit continues to use the SVN CLI
working-copy sink; the linked backend is used for post-submit read/import verification.

## ABI and platform contract

- Minimum linked version: Subversion/libsvn 1.14, enforced by `pkg-config` probes on
  non-Windows targets.
- Linux and other Unix-like builds discover `libsvn_ra`, `libsvn_delta`, `libsvn_subr`,
  APR, and APR-util through `pkg-config`.
- Windows discovers the `subversion` package through vcpkg. Enabling the Cargo feature
  without a successful probe produces an explicit unlinked diagnostic; a linked gate
  must reject that state.
- Runtime diagnostics read `svn_subr_version`; the linked test matrix covers the raw
  layouts actually dereferenced by supported operations. The repository does not claim
  ABI compatibility outside the probed 1.14+ profile.
- Handwritten `repr(C)` layouts must match the public headers for the minimum version.
  A field/layout expansion requires a header-derived check or migration to generated
  `subversion-sys` bindings before use.

## APR and callback ownership

- APR is initialized once by the linked runtime; each operation owns a pool whose
  lifetime encloses its RA session calls and synchronous reporter drive.
- Reporter/editor batons remain valid until `finish_report` returns. File batons are
  operation-owned, tracked while active, released by `close_file`, and reclaimed by the
  operation baton on abort/error.
- Counted SVN strings are copied by `data/len`; they are never treated as NUL-terminated
  property data.
- No panic may cross an `extern "C"` boundary. Callbacks validate required pointers,
  catch editor panics, retain the first operation error, and return an owned SVN error.

## Authentication, errors, and secrets

- Auth providers are operation/session-owned. Explicit credentials, cache policy,
  config directory, askpass, and terminal fallback follow the declared profile order.
- Passwords are never placed in diagnostics, journals, config, or error messages.
- Native SVN child errors are copied before `svn_error_clear` and retained at the
  command error boundary as `external command` or `authentication` failures.
- New SSL/client-certificate providers require a declared protocol profile and focused
  secret-redaction tests.

## Verification strategy

The required linked evidence is:

- default-parallel and serial core suites;
- real linked CLI clone/fetch and linked post-submit dcommit suites;
- initial/incremental text delta, copy, delete, property add/delete/type transition,
  checksum, absent-node, abort/error, subpath, auth, and byte-preservation fixtures;
- runtime diagnostics proving the feature is both enabled and linked.

The serial run is diagnostic only; the parallel run is the safety gate. Any migration to
`subversion` must pass the same normalized editor-event and exact Git artifact suite
before the handwritten owning operation is removed.

## Consequences

This decision avoids a release-blocking rewrite and keeps the already audited unsafe
surface bounded. It also leaves maintenance cost in the handwritten declarations and
requires discipline when libsvn APIs change. The safe crate remains the preferred
candidate for a later, separately reviewed migration after its RA/delta/auth behavior is
proven against this repository's frozen compatibility fixtures.
