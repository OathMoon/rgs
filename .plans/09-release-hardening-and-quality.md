# Phase 09: Release Hardening and Quality Closure

## Objective

在不扩张 v1 产品范围的前提下，关闭 2026-08-12 仓库审阅发现的发布阻断问题，使当前声明档案具备可重复、可保护、绑定具体提交的发布证据，并降低后续维护 linked-libsvn、import 和 dcommit 路径的成本。

本计划是跨 Phase 3–8 的收口计划，不取代原阶段计划，也不自动把 deferred 能力纳入首发范围。

## Authority and Inputs

- [产品总计划](git-svn-rs-plan.md)
- [能力边界清单](release-capability-inventory.md)
- [当前进度记录](implementation-progress-record.md)
- [Phase 4 SVN adapter 计划](04-svn-fixtures-and-backend.md)
- [Phase 7 dcommit/CI 计划](07-dcommit-shim-ci.md)
- [Phase 8 黄金与发布门槛](08-compatibility-golden-tests.md)
- [2026-08-12 审阅报告](../docs/项目审阅报告-2026-08-12.md)

## Current State

State: `release-pass` for the Phase 09 P0/P1 scope at
`c0dfb2067f75806935b2b36462d5819923652634`.

审阅基线：`1139ae497567d4ae787be849ae31d68b2552aed8`。

2026-08-12 本地收口结果：

1. linked-libsvn incremental add 现在在共同 `FetchEditor` 契约中应用目标
   revision 的完整文件属性；executable、special 和 type transition 回归通过；
2. 新增真实 submitted-state 恢复回归，证明 post-fetch 失败后只恢复同一
   revision，不重复调用 SVN sink；
3. 所有 SVN/golden fixture 统一使用可配置 temp root，仓库根运行不再产生
   新的随机 fixture 目录；
4. strict workflow 和 `verify.ps1 -StrictCompat` 均纳入完整 linked dcommit，
   release/tag 工作流通过 `workflow_call` 强制同一 SHA 证据；
5. libsvn binding ADR、顶层 typed error 分类和首批 resolver/rev_map/dcommit
   高风险路径迁移已完成；
6. default workspace、linked core parallel/serial、linked CLI read/write 本地
   矩阵通过；protected hosted
   [release gate run #31561696796](https://github.com/OathMoon/rgs/actions/runs/31561696796)
   也完成相同 strict/linked/static 矩阵、artifact 上传和独立 same-SHA 校验。

最小根因：`svn_ra_do_update3` 在该 incremental add 路径没有可靠投递
`change_file_prop`，而 native adapter 的目标 revision 预读已经取得权威文件
属性却没有带入 operation baton。修复把这些属性绑定到本次 update，并在
共同 editor 的 `add_file` 后通过 `change_file_prop_bytes` 应用；没有绕过
`FetchEditor` 或直接修补 Git tree。

## Scope

### In scope

- 修复 linked libsvn incremental/add-file 属性 replay；
- 验证 dcommit 写后失败的安全恢复；
- 建立跨平台、可配置、默认不污染源码树的 fixture 根目录；
- 补齐 linked dcommit CI 和严格发布门槛；
- 更新进度、阶段状态和 capability claim；
- 记录 libsvn binding ADR；
- 逐步建立结构化错误边界；
- 对高风险大模块做有测试保护的最小拆分；
- 收口 filters 性能、公共 API 和首发仓库卫生。

### Out of scope

- 原生 libsvn commit editor/sink；
- 自动转换 legacy v0–v5 metadata；
- 新增 branch/tag、`set-tree`、`commit-diff` 或属性编辑命令；
- 宣称任意企业 HTTP(S)、代理、CA、SSH agent 或远端服务兼容；
- 一次性实现完整 Log.pm；
- 为了重构美观而重写已经通过的默认 CLI 路径。

## Principles and Invariants

- P0 正确性优先于模块重构和新功能。
- 不通过放宽 tree/mode/property 校验来隐藏 linked 差异。
- 已成功产生 SVN revision 的恢复测试必须证明不会重复提交。
- fixture 的默认位置不得依赖源码工作区的文件系统语义。
- developer、backend 和 release 三类门槛有不同含义，不能用 developer pass 代替 release pass。
- linked 默认并行测试是安全门槛；串行运行只作为额外诊断。
- 每个公开支持声明必须绑定 profile、backend、平台范围、提交和验证证据。
- 每次变更保持最小范围，并为发现的问题增加先失败后通过的聚焦测试。

## Work Order

### P0: Release blockers

预计用时：3–5 个工程日。P0 全部完成前不增加新协议、命令或 native write 能力。

#### P0-1. Reproduce and fix linked add-file property replay

目标：linked 构建中，新增 executable 文件和 symlink/special 节点经 dcommit、post-fetch 后产生与 `DcommitPlan` 一致的 Git tree。

实施步骤：

1. 保留以下两个现有失败测试作为最低回归：
   - `dcommit_writes_executable_property_to_file_svn_when_tools_exist`；
   - `dcommit_writes_special_property_to_file_svn_when_tools_exist`。
2. 在 `libsvn_backend.rs` 增加更小的 native delta 集成 fixture：
   - incremental revision 新增普通 executable 文件；
   - incremental revision 新增 `svn:special` symlink；
   - 同一 revision 同时包含文本 delta 和属性；
   - 属性添加、删除和 type transition。
3. 确认缺陷位于 reporter/editor callback delivery、property normalization 还是 `SvnFetchEditor` 状态应用，记录最小根因。
4. 修复 native adapter，使新增文件的完整属性进入共同 `FetchEditor` 契约。不得绕过 editor 直接修改 Git tree。
5. 保持默认 SVN CLI 行为不变；增加恢复回归后当前套件为 73/73。

验收：

```text
cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear \
  dcommit_writes_executable_property_to_file_svn_when_tools_exist -- --exact

cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear \
  dcommit_writes_special_property_to_file_svn_when_tools_exist -- --exact
```

两个测试必须通过，且验证仍比较 exact mode/content/property，不能删除或弱化 post-fetch tree projection。

#### P0-2. Prove post-submit recovery after the linked failure class

目标：即使 post-submit import 或验证失败，下一次执行也只回读/验证已提交 revision，不重复调用 commit sink。

实施步骤：

1. 注入“SVN commit 成功、linked post-fetch 在 publication 前失败”；
2. 检查 journal 已持久记录 submitted revision 和绑定 target；
3. 第二次运行恢复同一 revision；
4. 检查 SVN HEAD 只增加一次，ref/rev_map/footer/tree 最终一致；
5. 覆盖 `--no-rebase`，并至少覆盖一个 executable 或 special 计划。

如果 P0-1 不能在当前迭代修复，临时安全措施必须在首次 SVN 写入前拒绝 linked dcommit 组合，或显式强制该组合的 post-fetch 使用已验证 CLI backend。不得继续保留“写入后才发现不支持”的行为。

#### P0-3. Make fixture roots portable and deterministic

目标：从仓库根目录运行标准验证时，不再依赖 `/mnt/e`、NTFS/DrvFS 或当前目录的 rename/mode 语义，也不在源码树留下随机 fixture。

实施步骤：

1. 增加单一 `test_temp_root()`/fixture helper，解析顺序建议为：
   - `GIT_SVN_RS_TEST_TMPDIR`；
   - `CARGO_TARGET_TMPDIR`；
   - 系统 `temp_dir()`。
2. 把 `StandardSvnFixture`、golden compatibility、真实 SVN CLI/libsvn fixture 迁移到该 helper。
3. fixture 明确设置需要的 Unix mode/SVN property，不依赖宿主目录继承或 auto-props 偶然行为。
4. 成功时由 `TempDir` 清理；失败 artifact 统一复制/保留到显式的 compat artifact 目录并打印路径。
5. 不以向 `.gitignore` 添加宽泛 `svn-fixture-*` 作为根本修复；仅在存在固定 artifact 根时忽略该根。

验收：

- 在 `/mnt/e/Repositories/gitsvn` 直接运行 `cargo test --workspace`；
- 在原生 `/tmp` 根运行真实 SVN/黄金用例；
- 两处结果一致；
- `git status --short` 不新增随机 fixture 目录。

#### P0-4. Close linked and strict CI coverage

目标：任何 linked dcommit property 回归都能在发布前阻断，而不是由用户在写入 SVN 后发现。

实施步骤：

1. 在 `.github/workflows/compatibility.yml` 和 `scripts/verify.ps1 -StrictCompat` 增加 linked dcommit gate；
2. 最低覆盖 executable、special、type transition 和 submitted recovery；资源允许时运行完整 `dcommit_linear`；
3. 确认 linked golden 中按 cfg 跳过的 write scenarios 不被 summary 误计为执行；
4. required scenario summary 增加 backend/build-feature 标识；
5. 当前 HEAD 上重新运行 hosted strict workflow 并保留 artifact。

#### P0-5. Reconcile status documents

目标：计划、进度记录、README 和能力清单对同一提交、profile 和例外给出一致声明。

实施步骤：

1. 更新 `implementation-progress-record.md` 的日期、HEAD、测试计数和已知缺陷；
2. 更新 Phase 3–7 的 Current State，删除已经完成的旧阻断描述；
3. Phase 4/5/7 在 linked 属性修复前不得声称该组合已通过；
4. Phase 8 的 release evidence 绑定新的 hosted run 和提交；
5. README 明确默认 CLI、linked read/import、CLI write sink 和组合验证边界。

### P1: Governance and error boundaries

预计用时：约 1 周。P1 可以在 P0 通过后并行拆分为独立小提交。

#### P1-1. Record the libsvn binding ADR

新增 `.plans/adr/` 下的 libsvn binding ADR，至少记录：

- 继续使用 handwritten FFI、采用 `subversion` crate 或 hybrid 的比较；
- 支持的 libsvn 最低版本和 ABI/layout 检查；
- APR runtime/pool 所有权和 callback 生命周期；
- auth provider、error chain 和 secret handling；
- Linux pkg-config 与 Windows vcpkg 行为；
- unsafe 代码边界和测试策略；
- native commit sink 明确延后或进入未来里程碑的理由。

ADR 通过前不得扩张新的 raw FFI surface。

#### P1-2. Establish structured top-level errors

目标：满足总计划要求的错误分类，同时避免一次性改写全部函数签名。

顺序：

1. 定义稳定的顶层类别：unsupported、auth、ambiguity、metadata corruption、partial write、external command、invalid invocation；
2. 先在命令边界和 `main.rs` 使用 typed error；
3. 保留内部模块已有 `JournalError`、`CoordinatorError` 等类型，并通过 `From`/source chain 上卷；
4. 逐条迁移高价值路径：resolver、rev_map、dcommit post-submit、libsvn native error；
5. 稳定 CLI 文本和退出码，不要求一次性把所有 `Result<_, String>` 消除。

验收：测试能够按 error category/variant 断言关键安全失败，必要的用户可见文本仍与冻结基线一致。

#### P1-3. Create an enforceable release gate

目标：保留昂贵 strict workflow 的可控触发，同时保证发布/tag 不能绕过它。

建议层级：

| Gate | Trigger | Required evidence |
|---|---|---|
| Developer | PR/push | fmt、default workspace、all-target/all-feature clippy |
| Backend | manual/nightly 或受保护 workflow_call | 真实 SVN CLI、linked parallel、linked serial diagnostic、linked CLI read/write |
| Release | tag/release workflow | frozen Perl 41/41、零跳过、required summaries、当前 SHA artifact |

若出于成本继续保留 strict manual dispatch，则 release/tag 工作流必须校验同一 SHA 的成功 strict run 和 artifact，不能仅依赖人工说明。

### P2: Maintainability and release hygiene

预计用时：1–2 周，可分批交付；每次拆分必须保持行为和测试不变。

#### P2-1. Split only the proven large boundaries

建议拆分顺序：

1. `svn/libsvn.rs`：`ffi`、`runtime`、`auth`、`ra`，保留 `native_delta`；
2. `import.rs`：discovery/planning、revision replay、publication；
3. `commands/dcommit.rs`：target/preflight、planning、working-copy runtime、post-submit verify；
4. `commands/fetch.rs`：runtime config/backend factory 与 command orchestration。

拆分约束：

- 不引入只有一个实现的 speculative trait；
- 不同时改变行为和模块布局；
- 每个提交只拆一个边界并运行相关 focused tests 加 workspace gate。

#### P2-2. Hoist immutable import state out of revision loops

- `PathFilters` 在 mapping/import loop 外构造一次；
- authors mapping、placeholder 配置和不变 URL/path prefix 同样只解析一次；
- 对长 revision history 增加简单 benchmark 或计数回归，证明 regex 编译次数不随 revision 数增长。

#### P2-3. Narrow the core public API

- 记录 CLI crate 实际消费的 core symbols；
- 将 import internals、journal internals、fetch editor implementation 和内部 builders 收紧为 `pub(crate)`；
- 为真正稳定的库入口增加 crate/module 文档；
- 不在 v0.1 承诺未设计的稳定公共 Rust API。

#### P2-4. Complete release repository hygiene

- 添加 MIT 和 Apache-2.0 LICENSE 文本；
- 添加 CHANGELOG 和 release checklist；
- 验证 core、CLI、shim 的 `cargo package`/publish 顺序；
- 记录 minimum Rust/libsvn/SVN/Git/Perl versions；
- 校验 README、crate metadata 和 repository URL 一致。

## Verification Matrix

P0/P1 完成后的最低验证矩阵：

```text
cargo fmt --all -- --check
cargo test --workspace
cargo test -p git-svn-rs-core --features svn-libsvn
cargo test -p git-svn-rs-core --features svn-libsvn -- --test-threads=1
cargo test -p git-svn-rs --features svn-libsvn --test clone_fetch_real_svn
cargo test -p git-svn-rs --features svn-libsvn --test dcommit_linear
cargo clippy --all-targets --all-features -- -D warnings
```

严格环境额外要求：

- `GIT_SVN_RS_STRICT_COMPAT=1`；
- frozen Git/Perl `git svn` 2.54.0；
- 41/41 黄金 scenario 执行且零跳过；
- 8 个 required release summary 全部为 executed/passed；
- linked backend 默认并行和串行诊断均执行；
- linked CLI clone/fetch/dcommit 实际执行；
- hosted artifact 标识当前 commit SHA、工具版本、backend 和 feature profile。

## Deliverables

P0：

- linked executable/special 修复及 focused regressions；
- submitted recovery regression；
- portable fixture root helper 和迁移；
- linked dcommit strict CI；
- 同步后的进度/阶段/README；
- 当前 HEAD hosted compatibility artifact。

P1：

- libsvn binding ADR；
- 顶层错误分类和首批高风险路径迁移；
- 可执行的 developer/backend/release 门槛。

P2：

- 分批模块拆分；
- filters/import 不变状态性能修复；
- 收紧的 core API；
- LICENSE、CHANGELOG、release checklist 和 package 验证。

### Delivery status (2026-08-12)

| Item | Status | Evidence |
|---|---|---|
| P0 linked property replay | complete locally | linked dcommit 73/73; native backend 36/36 |
| P0 submitted recovery | complete locally | executable post-fetch failure resumes with one SVN revision |
| P0 portable fixtures | complete locally | centralized temp root; root workspace run leaves no new fixture directory |
| P0 linked/strict gates | implemented and locally exercised | linked parallel/serial plus CLI 48/48 and 73/73 |
| P0 hosted current-SHA artifact | complete | run #31561696796 retained a passed release summary for `c0dfb2067f75806935b2b36462d5819923652634` |
| P0 status reconciliation | complete | Phase 3–9, README, inventory, and progress record aligned |
| P1 libsvn ADR | complete | `adr/0001-libsvn-binding-strategy.md` |
| P1 structured errors | complete for planned first boundary | stable categories at CLI/resolver/rev_map/dcommit boundaries |
| P1 enforceable release gate | complete in repository | developer, reusable backend, and release/tag workflows |

All P0/P1 completion criteria below are satisfied for the recorded evidence
commit. P2 remains explicit post-release maintenance scope.

## Completion Definition

只有同时满足以下条件，Phase 09 才可标为 `release-pass`：

1. P0 两个 linked property 回归和恢复回归通过；
2. 标准测试从仓库根目录运行，不依赖人工切换到 `/tmp`；
3. default、linked parallel、linked serial、linked CLI read/write 全部通过；
4. strict frozen comparison 41/41、零跳过，required summary 完整；
5. 当前 commit 的 hosted release artifact 可获取；
6. Phase 4 ADR 已记录；
7. release/tag 流程不能绕过 strict evidence；
8. 进度记录、Phase 3–8、README 和 capability inventory 状态一致；
9. 范围外能力仍明确拒绝或标记 deferred，没有被本计划暗中扩张。

P2 的大模块拆分和 API 收紧可以在首发后继续，但不得以此为理由延后 P0 正确性修复；若 P2 未全部完成，应在进度记录中作为维护性债务明确保留。
