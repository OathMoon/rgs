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

The golden compatibility harness compares the deterministic trunk fixture subset against Perl `git svn` when the external tools are installed. It normalizes and compares supported config entries, remote refs, `git-svn-id` footers, and committed `.rev_map` records. Branch/tag copy imports, Perl-only config metadata, author identity formatting, timestamps, working tree checkout details, and no-op `.rev_map` placeholders are excluded until the Rust implementation supports those fields.

## `svn-libsvn` feature status

The `svn-libsvn` Cargo feature currently compiles a feature-gated backend shell without linking to libsvn. Default builds do not require libsvn. In this Windows environment, Subversion is available through TortoiseSVN `svn.exe` 1.14.0 and runtime DLLs, but the libsvn headers/import libraries or `pkg-config` metadata needed for reliable Rust FFI linking are not present.

When built with `--features svn-libsvn`, `git-svn-rs diagnose` reports the feature as enabled and the link status as `not-linked`; `LibSvnBackend` methods return a clear not-linked error instead of silently falling back.
