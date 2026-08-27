# Changelog

All notable user-visible changes to `git-svn-rs` are recorded in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses Semantic Versioning for published crate versions.

## [0.1.0] - Unreleased

### Added

- The `git-svn-rs` command for the declared core `git svn` workflows: `init`,
  `clone`, `fetch`, `rebase`, `dcommit`, `log`, `info`, `find-rev`, `gc`,
  `reset`, and `diagnose`.
- An opt-in `git-svn` compatibility shim, published separately as
  `git-svn-rs-shim` so the primary package does not replace Perl `git svn`.
- Compatible Git configuration, `git-svn-id`, `.git/svn/**/.rev_map.*`, ref,
  commit graph, working-tree, and recovery behavior for the declared v1
  profiles.
- Safe linear dcommit through the SVN CLI working-copy sink, including durable
  submitted-state recovery that avoids duplicate SVN revisions.
- Optional linked-libsvn read/import support through the `svn-libsvn` feature.
- Strict frozen compatibility artifacts against Git/Perl `git svn` 2.54.0.

### Compatibility scope

- Required release profiles are local `file://` and authenticated local
  `svn://` clone/fetch and linear dcommit through the SVN CLI.
- Linked libsvn is a read/import backend. Dcommit continues to write through the
  SVN CLI and uses linked libsvn only for post-submit read/import when enabled.
- Authenticated loopback HTTP/HTTPS DAV and configured or loopback
  `svn+ssh://` are fixture-validated profiles, not general remote-service
  compatibility claims.
- Native libsvn write-back, automatic legacy metadata migration, complete
  `Log.pm` behavior, branch/tag write-back, `set-tree`, `commit-diff`, and
  property-editing commands remain deferred.

### Toolchain baselines

- Rust 1.95 is the minimum supported Rust toolchain.
- Subversion/libsvn 1.14 is the minimum linked-backend runtime and development
  baseline; default builds do not require libsvn.
- Git/Perl `git svn` 2.54.0 at commit
  `0b13e48a3a30cdfa94e8ef842e24d6045ab3d015` is the frozen comparison oracle,
  not a runtime dependency of `git-svn-rs`.

[0.1.0]: https://github.com/OathMoon/rgs/releases/tag/v0.1.0
