# git-svn-rs

Rust implementation plan and staged replacement for the core `git svn` workflow.

The default command is `git-svn-rs`. A `git-svn` compatibility shim is planned as an explicit opt-in package step.

## Verification

On Windows, run the local verification script from the repository root:

```powershell
./scripts/verify.ps1
```

The script runs `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.

Some fixture and golden compatibility tests use external Subversion tools or Perl `git-svn`. By default they skip when those tools are unavailable. Set `GIT_SVN_RS_STRICT_COMPAT=1` or pass `-StrictCompat` to `scripts/verify.ps1` to make missing compatibility tools fail the run.

The Windows GitHub Actions workflow also exposes a manual `strict_compat` input that runs `scripts/verify.ps1 -StrictCompat` in CI.

The golden compatibility harness compares the deterministic trunk fixture subset against Perl `git svn` when the external tools are installed. It normalizes and compares supported config entries, remote refs, `git-svn-id` footers, and committed `.rev_map` records. Branch/tag copy imports, Perl-only config metadata, author identity formatting, timestamps, working tree checkout details, and no-op `.rev_map` placeholders are excluded until the Rust implementation supports those fields.

## `svn-libsvn` feature status

The `svn-libsvn` Cargo feature currently compiles a feature-gated backend shell and runs a vcpkg-based libsvn link probe at build time. Default builds do not require libsvn. On Windows with vcpkg, install the development libraries with:

```powershell
vcpkg install subversion:x64-windows
$env:VCPKG_ROOT = "E:\vcpkg"
$env:VCPKG_DEFAULT_TRIPLET = "x64-windows"
$env:VCPKGRS_DYNAMIC = "1"
$env:PATH = "$env:VCPKG_ROOT\installed\x64-windows\bin;$env:PATH"
```

`VCPKG_ROOT` should point at the actual vcpkg checkout. `VCPKGRS_DYNAMIC=1` is required for the dynamic `x64-windows` triplet so test binaries can load the vcpkg DLLs at runtime.

When built with `--features svn-libsvn`, `git-svn-rs diagnose` reports the feature as enabled. The link status is `linked` when the vcpkg probe finds Subversion, or `not-linked` when the probe cannot find it. `linked` currently means only that the native development libraries are discoverable; `LibSvnBackend` API methods still return a clear not-implemented error until real libsvn RA bindings are added.
