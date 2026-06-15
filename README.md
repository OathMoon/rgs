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

The golden compatibility harness currently provides deterministic fixture manifests, tool detection, and artifact capture skeletons. Rust-vs-Perl output comparison is intentionally skip-aware until production fetch artifacts exist.
