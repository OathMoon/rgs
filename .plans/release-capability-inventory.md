# Release Capability Inventory

Baseline: Git `v2.54.0` (`0b13e48a3a30cdfa94e8ef842e24d6045ab3d015`)

This inventory is the Phase 1/2 release boundary. Every parsed command and option
is either implemented for a named gate or rejected before repository mutation.

Evidence state (2026-08-12): the Phase 9 P0/P1 closure at
`c0dfb2067f75806935b2b36462d5819923652634` passes the full local default and
linked matrix and protected hosted
[release gate run #31561696796](https://github.com/OathMoon/rgs/actions/runs/31561696796).
The retained `frozen-compatibility-artifacts` release summary is passed and bound
to that exact commit SHA.

## Command surface

| Command | Release state | Behavioral gate |
|---|---|---|
| `init` | implemented | layout/config serialization and pre-mutation validation |
| `clone` | implemented | exact single-path/stdlayout/full-URL graph and clone-state golden comparisons |
| `fetch` | implemented | real CLI/libsvn replay, option overlay, parent/all and recovery tests |
| `rebase` | implemented | readonly CLI and frozen output/argument comparisons |
| `dcommit` | implemented for declared linear profiles | real file/svn/HTTP(S)/configured and loopback-SSH write/recovery gates |
| `log`, `info`, `find-rev` | implemented for the documented subset | readonly CLI and strict golden comparisons |
| `gc`, `reset` | implemented for supported metadata layouts | readonly/recovery and strict golden comparisons |
| `diagnose` | implemented | default/linked feature and frozen-baseline tests |
| `branch`, `tag`, `set-tree` | explicitly unsupported in v1 | CLI failure/no-mutation gate |
| `propget`, `propset`, `proplist` | explicitly unsupported in v1 | CLI failure/no-mutation gate |
| `show-ignore`, `show-externals` | explicitly unsupported in v1 | CLI failure/no-mutation gate |
| any unknown subcommand | explicitly unsupported in v1 | CLI failure/no-mutation gate |

## Option surface

| Option group | Release state and consumer |
|---|---|
| global `-q/--quiet`, `-v/--verbose` | explicitly rejected; they cannot be silently inert |
| `-s/-T/-b/-t/--prefix` | consumed by the shared layout builder and URL normalization path |
| authors file/program | persisted by init, overlaid by fetch/log/dcommit, executed by import/display/write recovery |
| ignore/include paths, ignore refs | combined by fetch overlay and consumed by the shared importer/filter model |
| revision forms | consumed by clone/fetch/log/reset; rejected by init and dcommit where semantics are unsupported |
| log window/localtime | persisted/overlaid and consumed by bounded replay and commit identity creation |
| metadata modes and rewrite options | validated before mutation, persisted, immutable after import, consumed by identity/replay |
| username/password/config-dir/no-auth-cache | runtime auth overlay; passwords are never persisted or captured |
| preserve-empty-dirs/placeholder filename | consumed by import placeholder ownership/reconciliation |
| clone `--no-checkout` | consumed by clone materialization and compared in exact clone-state artifacts |
| fetch/rebase `--fetch-all`, fetch `--parent` | consumed by named-remote resolver and fetch selection |
| rebase dry-run/local/merge/rebase-merges/strategy/verbosity | consumed by rebase orchestration and Git argument construction |
| dcommit dry-run/adopt/commit-url/mergeinfo/no-rebase | consumed or rejected at the pre-write capability boundary |
| log range/authors/non-recursive/color/pager/limit/modes/Git args | consumed by readonly selection, formatter, pager and passthrough layers |
| info URL/path, find-rev tree-ish/before/after, reset revision/parent | consumed by validated readonly/reset resolvers |

## Declared protocol profiles

| Profile | Read | Write | Release claim |
|---|---|---|---|
| SVN CLI `file://` | single-path, subpath and standard layout | safe linear dcommit | required release profile |
| SVN CLI local authenticated `svn://` | clone/fetch | safe linear dcommit | required release profile |
| linked libsvn `file://`, local `svn://` | RA/delta read/import, including dcommit post-submit verification/recovery | CLI working-copy sink only | linked read/import profile; native write deferred |
| loopback authenticated HTTP/HTTPS DAV | CLI and linked libsvn read/import | CLI working-copy sink | validated loopback profile, not arbitrary remote infrastructure |
| configured and real loopback `svn+ssh://` | clone/fetch | CLI working-copy sink | validated configured/loopback profile |
| `mock://` | test infrastructure | test infrastructure | never a user-facing release profile |
| SVM metadata mode | read/import/query subset | rejected for reset/dcommit | explicitly read-only |

The release does not claim arbitrary proxy, enterprise CA, SSH-agent, remote
server, or platform behavior beyond a profile that has its own recorded gate.
