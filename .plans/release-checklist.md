# Release Checklist

This checklist governs a `git-svn-rs` crate or tag release. Completing it does
not itself authorize publishing crates, creating a tag, or publishing a GitHub
release.

## 1. Candidate identity and scope

- [ ] Select one committed candidate SHA and record it in
  `.plans/implementation-progress-record.md` separately from the last proven
  release SHA.
- [ ] Confirm the capability inventory and README still describe only the
  declared profiles; deferred commands, protocols, native write-back, and
  migration behavior remain deferred.
- [ ] Confirm `git diff --check` is clean and the candidate checkout contains no
  tracked generated fixtures or credentials.

## 2. Version and dependency consistency

- [ ] `git-svn-rs-core`, `git-svn-rs`, and `git-svn-rs-shim` use the same
  version in their manifests.
- [ ] The CLI dependency on `git-svn-rs-core` names that same version as well as
  its workspace path.
- [ ] `Cargo.lock` is regenerated with the release toolchain and contains the
  candidate package versions.
- [ ] `CHANGELOG.md` has a dated entry for the version and its compatibility
  scope; README and capability inventory agree with it.
- [ ] The declared minimums remain distinct: Rust 1.95 is the compiler baseline,
  SVN/libsvn 1.14 is the linked-backend baseline, and frozen Git/Perl 2.54.0 is
  the comparison oracle rather than a runtime dependency.

## 3. Package audit and publish order

Run from a clean checkout:

```powershell
./scripts/verify-package-readiness.ps1
```

- [ ] The package lists contain `README.md`, `LICENSE-MIT`, and
  `LICENSE-APACHE`.
- [ ] No package contains `golden-stdlayout-*`, `svn-fixture-*`, `.svn/`,
  `.git/`, `.plans/`, `.github/`, `.codex/`, `.zcode/`, or repository-private
  documentation.
- [ ] Each extracted package builds independently.
- [ ] The temporary clean registry proves the fixed publish order:
  `git-svn-rs-core` → `git-svn-rs` → `git-svn-rs-shim`.
- [ ] The extracted CLI package resolves `git-svn-rs-core` at the same version
  from the temporary registry after Cargo removes the workspace path.
- [ ] Package sizes and SHA-256 checksums are retained with the release notes.

Never publish CLI before the matching core version is available from the target
registry. The shim is last because it exposes the compatibility command name.

## 4. Local verification

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo test -p git-svn-rs-core --features svn-libsvn
cargo test -p git-svn-rs-core --features svn-libsvn -- --test-threads=1
cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn
cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

- [ ] Default workspace and all static gates pass.
- [ ] Linked core parallel and serial gates pass with the feature actually
  linked.
- [ ] Linked CLI read/import and dcommit post-submit workflows pass.

## 5. Strict and hosted evidence

```powershell
./scripts/verify.ps1 -StrictCompat
```

- [ ] Frozen comparison executes 41/41 scenarios with zero skips.
- [ ] All eight required scenario summaries are `executed` and `passed` with
  the expected frozen commit, backend, and feature identifiers.
- [ ] Push the exact candidate SHA, then run the protected release workflow.
- [ ] Download `frozen-compatibility-artifacts` and verify
  `release-summary.json` reports the exact candidate SHA, status `passed`, eight
  required summaries, and all linked profiles.
- [ ] Update the last proven release SHA only after the hosted same-SHA verifier
  passes.

## 6. Publish, tag, and rollback

- [ ] Obtain explicit authorization before publishing crates or creating a tag.
- [ ] Publish and verify registry availability in this order: core, CLI, shim.
- [ ] Create the version tag only after all three intended packages resolve.
- [ ] Publish release notes from the matching CHANGELOG entry and attach the
  retained checksums and release summary.
- [ ] If a package fails before any dependent package is published, stop and fix
  forward with a new version; never overwrite an immutable registry artifact.
- [ ] If core is published but CLI cannot be published, do not publish the shim
  or create the tag. Record the partial release and release a corrected version.
- [ ] If hosted evidence is missing or bound to another SHA, do not tag or
  publish; restore the last proven release SHA in status documentation.
