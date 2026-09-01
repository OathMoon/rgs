# git-svn-rs

Staged Rust implementation and compatibility path for core `git svn` workflows.

The primary command is `git-svn-rs`. The workspace also contains a `git-svn`
compatibility shim, but installing or packaging that command name is an explicit
opt-in so it does not replace Perl `git svn` by default.

The declared v1 profiles, Phase 9 P0/P1 hardening, and Phase 10 maintainability
and package readiness have protected same-SHA release evidence at `ffb2e22`.
This is not a general replacement for every `git svn` workflow.
The exact command, option, and protocol boundary is recorded in the
[release capability inventory](.plans/release-capability-inventory.md).

Rust 1.95 is the minimum supported compiler. Subversion/libsvn 1.14 is the
minimum for the optional linked backend, while frozen Git/Perl `git svn` 2.54.0
is the compatibility oracle used by the strict tests rather than a runtime
dependency. Release changes are recorded in [CHANGELOG.md](CHANGELOG.md), and
package candidates follow the [release checklist](.plans/release-checklist.md).

## Verification

On Windows, run the local verification script from the repository root:

```powershell
./scripts/verify.ps1
```

The default verification runs formatting, the workspace test suite, and all-target,
all-feature clippy with warnings denied.

For local Windows development, use the
[Windows native + WSL strict verification procedure](docs/windows-wsl-verification.md).
Keep Windows native results separate from the WSL frozen comparison. Four required
dcommit golden scenarios are Unix-only, so the complete eight-summary strict gate
cannot currently pass on native Windows, even with Perl `git svn` installed.

Some fixture and golden tests require external Subversion tools or Perl `git svn`.
They may skip in the developer gate when those tools are unavailable. On a supported
Unix environment with all strict dependencies, set `GIT_SVN_RS_STRICT_COMPAT=1`.
If PowerShell is available there, the complete script gate can also be run with:

```powershell
./scripts/verify.ps1 -StrictCompat
```

Strict mode makes missing compatibility dependencies or skipped scenarios fail. In
addition to the default gates, the script runs the linked `git-svn-rs-core` suite
with both the default parallel harness and `--test-threads=1`, then runs the linked
CLI `clone_fetch_real_svn` and `dcommit_linear` workflows. Tests create SVN and
golden fixtures under `GIT_SVN_RS_TEST_TMPDIR`, then `CARGO_TARGET_TMPDIR`, then
the system temporary directory; they no longer use the source tree by default.

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
- deterministic local `file://`, authenticated local `svn://`, authenticated
  loopback HTTP/HTTPS DAV, and configured `svn+ssh://` dcommit;
- submitted-write recovery without a duplicate revision; and
- dirty-index rejection with no SVN write or Rust recovery journal.

The Phase 10 release baseline executes all 41 covered golden scenarios and the linked
matrices in parallel and serial modes, including 48/48 real clone/fetch and 73/73
dcommit cases. Protected hosted
[release gate run #33155854101](https://github.com/OathMoon/rgs/actions/runs/33155854101)
passed for commit `ffb2e227f182be7b3afadbd0d2fcdf3744842e0e`. Its
`frozen-compatibility-artifacts` download contains eight executed/passed required
scenario summaries and a `release-summary.json` bound to that exact SHA. The
release/tag workflow calls the strict compatibility workflow and independently
downloads and verifies the same-SHA artifact.

Windows portability follow-up `4b49af6` passed
[Windows #23](https://github.com/OathMoon/rgs/actions/runs/33158740460) and
[Developer gate #11](https://github.com/OathMoon/rgs/actions/runs/33158740456).
The Windows run's optional strict compatibility step was skipped. These are
maintenance CI results, not a new protected release artifact. See the
[progress record](.plans/implementation-progress-record.md) for the evidence
boundaries. Version `0.1.0` remains unreleased; package readiness does not imply
a registry publish or tag.

The 2026-08-31 working-tree repairs fix Windows SVN property argument expansion
and revision-property byte handling. Both local matrices pass all nine gates:
Windows native/linked and WSL strict/linked, including 41 frozen golden tests
and eight required WSL summaries. Native Windows Perl comparisons still skip.
The [dated review](docs/项目审阅报告-2026-08-31.md) binds these results to the
verified source fingerprint; they are not a new protected release artifact.

Authenticated loopback HTTP and HTTPS DAV cover clone, incremental fetch, and
SVN CLI working-copy dcommit. Reads pass through both the SVN CLI and linked
libsvn backends; HTTPS uses an explicit CA trust configuration. A loopback
OpenSSH fixture also covers Ed25519 key authentication, strict known-host trust,
clone, and dcommit. These local fixtures are not a blanket claim for arbitrary
remote server, proxy, SSH-agent, or enterprise certificate configurations.
SVM repositories are read-only: `dcommit` and `reset` fail before mutation until
dual-map write/reset semantics are implemented. Missing usernames and passwords
can be supplied through askpass or an enabled terminal prompt without persistence.

## `svn-libsvn` feature status

The `svn-libsvn` Cargo feature enables the native Subversion backend and probes for
the platform's libsvn development libraries at build time. Default builds do not
require libsvn. Linked libsvn read/update behavior has `file://`, local `svn://`,
configured `svn+ssh://` tunnel, real loopback OpenSSH, and authenticated loopback
HTTP/HTTPS DAV fixture coverage. Dcommit continues to use the SVN CLI
working-copy sink; its post-submit read/import and recovery path uses the linked
backend when built with the feature. Incremental executable/special additions and
special-to-regular transitions have linked native-delta regression coverage.

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
