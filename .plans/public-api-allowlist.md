# git-svn-rs-core v0.1 Public API Allowlist

Last audited: 2026-08-28

This document distinguishes the API required by the shipped CLI from reusable
core facades retained for v0.1. A `pub` item not listed here is compatibility
surface only when another listed facade requires its type in a public signature;
its implementation module is not thereby promised stable.

## CLI-consumed surface

The `git-svn-rs` binary directly consumes only:

- `cli::{Cli, Command}` and command argument types reachable from them;
- `commands` command entry points;
- `diagnostics` report construction;
- `error::GitSvnError` at the process boundary.

The compatibility shim only locates and executes the CLI binary; it imports no
Rust symbols from core.

## Retained v0.1 reusable facades

The following module facades remain public because they define configured input,
typed repository state, transport/editor contracts, or independently tested
library workflows:

- `authors`, `config`, `filters`, `mapping`, `glob_spec`, and `path_url`;
- `git`, `git_svn_id`, `metadata`, `migration`, and `rev_map`;
- `svn` transport/auth/editor/RA contracts and supported backend facades;
- `import::{ImportOptions, ImportSummary, import_mock_revisions,
  import_mock_revisions_for_ref, import_ra_revisions,
  import_ra_revisions_for_ref}`;
- `fast_import`, `fetch_editor`, `import_transaction`, and `log_formatter`;
- `dcommit` plan/editor/fingerprint types re-exported from the module facade;
- `dcommit::{coordinator, journal, journal_registry}` durable recovery types.

## Intentionally private implementation

- `filesystem` and `tracking_state`;
- `svn::libsvn::{ffi, runtime, auth, ra, native_delta, tests}`;
- `import::{discovery, replay, publication, tests}`;
- `commands::dcommit::{target, preflight, planning, working_copy, post_submit}`;
- `commands::fetch::{runtime, preflight, mirror_identity}`;
- dcommit implementation modules `attributes`, `commit_editor`, `diff_planner`,
  `fingerprint`, `plan_builder`, `prepared_builder`, `property_mapper`,
  `tree_projection`, and `journal_persistence`.

The dcommit facade re-exports its retained v0.1 types and functions. Callers must
not depend on the private implementation module paths.

## Change rule

New public items require a named consumer and an update to this allowlist.
Internal tests should use module tests or the retained facade instead of making an
implementation module public. Public visibility is not a substitute for test
access.
