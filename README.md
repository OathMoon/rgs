# git-svn-rs

Staged Rust implementation and compatibility path for core `git svn` workflows.

The primary command is `git-svn-rs`. The workspace also contains a `git-svn`
compatibility shim, but installing or packaging that command name is an explicit
opt-in so it does not replace Perl `git svn` by default.

The current implementation is a substantial preview, not yet a general replacement
for `git svn`. Its strongest validated profiles are local `file://` and authenticated
local `svn://` repositories through the SVN CLI backend.

## Verification

On Windows, run the local verification script from the repository root:

```powershell
./scripts/verify.ps1
```

The default verification runs formatting, the workspace test suite, and all-target,
all-feature clippy with warnings denied.

Some fixture and golden tests require external Subversion tools or Perl `git svn`.
They may skip in the developer gate when those tools are unavailable. Set
`GIT_SVN_RS_STRICT_COMPAT=1`, or run:

```powershell
./scripts/verify.ps1 -StrictCompat
```

Strict mode makes missing compatibility dependencies or skipped scenarios fail. In
addition to the default gates, the script runs the linked `git-svn-rs-core` suite
with both the default parallel harness and `--test-threads=1`, then runs the linked
CLI `clone_fetch_real_svn` workflows.

## Current compatibility evidence

The strict golden suite passes 41/41 covered scenarios against the frozen Perl
`git svn` 2.54.0 baseline. The exact comparisons currently cover:

- trunk, standard-layout, and direct `/trunk` URL clone/import behavior;
- branch, tag, directory-copy, and follow-parent history;
- default and `--no-checkout` HEAD, index, and working-tree state;
- author identities, commit timestamps, messages, refs, commit graphs, and
  `git-svn-id` metadata;
- `.rev_map` object IDs and transactional trailing-zero scan markers;
- direct `useSvmProps` clone/fetch with source-zero skips, sparse source/transport
  rev maps, mirror fallback revisions, and source-aware readonly queries;
- covered `find-rev`, `info`, `log`, `gc`, `reset`, and rebase behavior;
- deterministic local `file://`, authenticated local `svn://`, and configured
  non-interactive `svn+ssh://` tunnel dcommit;
- submitted-write recovery without a duplicate revision; and
- dirty-index rejection with no SVN write or Rust recovery journal.

This is a `behavior-pass` for those scenarios, not a blanket compatibility claim.
Plain HTTP reads have a loopback Apache DAV fixture and are enabled separately
from HTTPS. HTTPS trust/authentication, real OpenSSH key/host-trust behavior, and
HTTP(S) write-back are not claimed. Configured `svn+ssh://` external-tunnel
reads/writes are covered without implying real OpenSSH key or host-trust support.
SVM repositories are read-only: `dcommit` and `reset` fail before mutation until
dual-map write/reset semantics are implemented. Missing usernames and passwords
can be supplied through askpass or an enabled terminal prompt without persistence.

## `svn-libsvn` feature status

The `svn-libsvn` Cargo feature enables the native Subversion backend and probes for
the platform's libsvn development libraries at build time. Default builds do not
require libsvn. Linked libsvn read/update behavior has `file://`, local `svn://`,
configured `svn+ssh://` tunnel, and loopback HTTP DAV fixture coverage; this does
not imply HTTPS, general OpenSSH authentication, or native write-back support.

On Ubuntu or Debian, install the system development packages with:

```bash
sudo apt update
sudo apt install libsvn-dev pkg-config subversion
cargo test -p git-svn-rs-core --features svn-libsvn
```

The Linux probe requires Subversion 1.14 or newer and checks `libsvn_ra`, `libsvn_delta`, and `libsvn_subr` through `pkg-config`. Use `PKG_CONFIG_PATH` when the `.pc` files are installed outside the system search path.

On Windows with vcpkg, install the development libraries with:

```powershell
vcpkg install subversion:x64-windows
$env:VCPKG_ROOT = "E:\vcpkg"
$env:VCPKG_DEFAULT_TRIPLET = "x64-windows"
$env:VCPKGRS_DYNAMIC = "1"
$env:PATH = "$env:VCPKG_ROOT\installed\x64-windows\bin;$env:PATH"
```

`VCPKG_ROOT` should point at the actual vcpkg checkout. `VCPKGRS_DYNAMIC=1` is required for the dynamic `x64-windows` triplet so test binaries can load the vcpkg DLLs at runtime.

When built with `--features svn-libsvn`, `git-svn-rs diagnose` reports the feature as enabled. The link status is `linked` when the platform probe finds Subversion, or `not-linked` when the probe cannot find it.
