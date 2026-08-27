# Phase 10: Maintainability and Package Readiness

## Objective

在不扩大已声明兼容范围、不改变 Phase 9 已验证行为的前提下，分批降低
linked-libsvn、import、dcommit 和 fetch 的维护风险，收紧尚未承诺稳定的
Rust API，并把三个 crate 推进到内容可审计、许可完整、发布顺序明确的状态。

本计划承接原 Phase 9 P2。Phase 9 保持 `release-pass`，后续重构工作不得回写
或重新解释其完成条件。

## Authority and Baseline

- [产品总计划](git-svn-rs-plan.md)
- [路线图](00-git-svn-rs-review-and-roadmap.md)
- [当前进度记录](implementation-progress-record.md)
- [Phase 9 发布收口](09-release-hardening-and-quality.md)
- [能力边界清单](release-capability-inventory.md)
- [libsvn binding ADR](adr/0001-libsvn-binding-strategy.md)
- [2026-08-12 审阅报告](../docs/项目审阅报告-2026-08-12.md)

最后一个完整 hosted release 基线是
`6f22803c8fdacd9a7217cbb0dda339fb03bcfe47`，对应 protected
[release gate run #31562384493](https://github.com/OathMoon/rgs/actions/runs/31562384493)。
Phase 10 工作提交必须与该基线分开记录；只有重新完成同一 SHA 的 protected
workflow 后，新的 HEAD 才能替代该发布基线。

## Current State

State: `in-progress`.

package/runtime 基线与发布文档批次已完成本地实现和验证：

1. 三个 crate 使用显式 package 内容白名单，随机 golden/SVN fixture 不再进入
   `cargo package` 清单；
2. 仓库和每个发布 crate 均携带 MIT 与 Apache-2.0 正文；
3. import 使用一次顶层 `ImportRuntime` 惰性保存 authors mapping 和编译后的
   `PathFilters`，同一次 windowed/multi-mapping import 不再按 revision 重建；
4. GitHub-hosted workflow 的 checkout/artifact actions 切换到 Node 24 运行时的
   稳定 major；
5. CHANGELOG、release checklist、版本基线和回滚规则已经记录；
6. 临时 clean registry 按 core → CLI → shim 顺序验证三个归档，CLI 的
   normalized manifest 从 registry 解析同版本 core 后完成 Cargo 自验证和独立 check；
7. Phase 9 P2 的其余结构/API 工作由本计划单独跟踪。

以上变更不增加命令、协议、native write-back 或兼容性声明。
Phase 10 本地提交 `8aa0ac6` 和 `42d0a6f` 均未取得与其绑定的 hosted
current-SHA artifact；`6f22803` 仍是最后一个已证明发布基线。

| Initial item | Local status | Evidence boundary |
|---|---|---|
| 10-0 governance | `complete-local` | Phase 9 冻结，Phase 10 独立计划/进度记录已建立 |
| 10-A1 package isolation/licenses | `complete-local` | 三个包清单及归档内容已审计，core 已从隔离包离线构建 |
| Actions runtime maintenance | `complete-local` | workflow YAML 已更新；未宣称 hosted 运行证据 |
| 10-B immutable import runtime | `complete-local` | 计数回归、import mock 和 linked CLI clone/fetch 已通过 |
| 10-A2/A3 release docs/order | `complete-local` | `42d0a6f`；本地 registry 证明 core → CLI → shim 归档、自验证和独立构建 |
| 10-C–10-G | `pending` | 按下方独立批次推进 |

## Scope

### In scope

- 发布包内容、许可证、CHANGELOG、发布清单和版本基线；
- 已证明过大的模块边界拆分；
- import 不变状态与可测的重复初始化成本；
- v0.1 稳定 API allowlist 和内部实现可见性收紧；
- 每批维护提交的默认、linked、静态和 package 验证；
- 最终候选提交的 same-SHA hosted release artifact。

### Out of scope

- native libsvn commit editor/sink；
- 新协议、远端基础设施或平台支持声明；
- 自动迁移 legacy metadata；
- 新命令或完整 Log.pm 模式；
- 与模块拆分同时发生的行为重写；
- 在 crates.io 发布或创建 tag，除非另有明确授权。

## Invariants

- Phase 9 的最后已证明 SHA 与 Phase 10 当前工作 SHA 分开记录。
- 每个提交只移动一个边界或完成一项独立卫生工作；移动代码和改变行为不得混合。
- 不引入只有一个实现且没有安全边界价值的 trait。
- 保持现有 CLI 文本、退出码、refs、rev_maps、commit graph、journal 和 artifact
  schema 不变。
- 新模块默认私有；现有公开符号在 API allowlist 建立前不批量降级可见性。
- 不删除或修改用户保留的未跟踪 fixture；package 隔离通过 manifest 完成。
- 每个里程碑通过本地门禁后再决定是否运行昂贵的 hosted gate；最终发布候选必须
  重新运行完整 protected gate。

## Work Breakdown

### 10-0. Governance and clean baseline

目标：让 Phase 9 维持冻结状态，并为维护提交建立独立状态和验收口径。

- 新建本计划并从 Phase 9 P2 链接到这里；
- 在进度记录中同时保留 last proven release SHA 和 Phase 10 working state；
- 为每一批记录 focused tests、workspace gate 和回滚边界；
- 不把未运行的 hosted evidence 标记为当前 HEAD 证据。

验收：计划、进度记录和 Phase 9 对责任归属给出一致描述。

### 10-A. Package content and release documents

按以下小提交推进：

1. **Package isolation and licenses**
   - core、CLI、shim 使用显式 `include` 白名单；
   - 每个 `.crate` 包含 `LICENSE-MIT`、`LICENSE-APACHE` 和 README；
   - generated fixture、工作副本、SVN repository 和任意用户未跟踪文件均不进入包。
2. **Release documentation**
   - 新增 CHANGELOG，采用版本化、面向用户的兼容范围描述；
   - 新增 release checklist，覆盖版本号、锁文件、package 清单、默认/linked/strict
     gate、current-SHA artifact、tag 和回滚检查；
   - 记录最低 Rust 1.95、linked libsvn/SVN 1.14，以及 frozen Git/Perl 2.54.0
     的不同角色，避免把测试基线误写为运行时依赖。
3. **Package/publish ordering**
   - 验证顺序固定为 core → CLI → shim；
   - 发布前从干净 checkout 检查 `.crate` 内容、大小和构建；
   - path dependency 重写后的 CLI 包只在 core 相同版本可解析时标记 publish-ready。

最低验收：

```text
cargo package -p git-svn-rs-core --allow-dirty --list
cargo package -p git-svn-rs --allow-dirty --list
cargo package -p git-svn-rs-shim --allow-dirty --list
```

三个清单必须包含双许可证，且不得出现 `golden-stdlayout-`、`svn-fixture-`、
`.svn/` 或工作区私有目录。

当前本地结果：

- 三个 `cargo package --list` 和 `--no-verify --offline` 归档成功；每个
  `.crate` 均包含 README 与双许可证，且无 fixture/`.svn` 污染；
- `git-svn-rs-core` 已通过 `cargo package --allow-dirty --offline` 的隔离
  验证构建；
- `scripts/verify-package-readiness.ps1` 把锁定依赖镜像到临时 `file:`
  registry，在隔离工作区和全新 Cargo home 中依次打包 core、CLI 和 shim；
- core 加入临时 registry 后，CLI 的 normalized manifest 从该 registry
  下载 `git-svn-rs-core 0.1.0`，Cargo 自验证和解包后的独立 check 均通过；
- 三个归档的本地 registry SHA-256 已输出，shim 只在 CLI 验证完成后加入
  registry。该证据不执行真实 crates.io publish，也不授权发布或创建 tag。

### 10-B. Immutable import runtime

目标：与一次顶层 import 绑定的不变配置只初始化一次，同时保持原错误顺序。

- 惰性缓存编译后的 `PathFilters` 和解析后的 authors file；
- windowed import 和多个 concrete mapping 复用同一 runtime；
- authors-prog 只缓存程序配置，仍按 author 执行，不缓存动态输出；
- placeholder filename、localtime 和 mapping prefix 保持轻量借用/按 mapping 派生，
  不建立没有量化收益的缓存；
- 计数回归比较重复 revision 处理，regex 编译次数不得随 revision 数增长。

验收：import mock、RA filter、windowed import 和 golden/CLI clone-fetch 行为不变。

### 10-C. Split `svn/libsvn.rs`

这是最高风险结构工作，严格按移动优先顺序完成：

1. 将 inline tests 移到 `svn/libsvn/tests.rs`，只移动不改断言；
2. 将 `native_delta.rs` 的 `use super::*` 改为显式依赖，先暴露真实边界；
3. 提取 `ffi.rs`：raw C structs、function pointers 和 extern declarations；
4. 提取 `runtime.rs`：APR init/pool、C string 和 `svn_error_t` ownership；
5. 提取 `auth.rs`：credential providers、batons 和 callbacks；
6. 提取 `ra.rs`：session/open/log/list/get-file/properties；
7. 保留 `libsvn.rs` 为安全 facade 和 backend 组装入口。

每一步都必须运行 linked parallel 和 serial gate；FFI layout、callback lifetime、
panic/secret safety 不得因移动而放宽。

### 10-D. Split import orchestration

在 10-B 稳定后按数据流拆分：

- `discovery.rs`：wildcards、copy dependencies、auxiliary mappings、backfill/order；
- `replay.rs`：revision identity、authors/time/filter、editor replay；
- `publication.rs`：staging refs、rev_maps、scan markers、placeholder/unhandled log；
- `import.rs`/`mod.rs`：公开入口、window orchestration 和 batch coordination。

先移动私有函数和测试，再调整模块可见性；不得改变 commit 顺序或 publication
事务边界。

### 10-E. Split command orchestration

#### Dcommit command

- `target.rs`：target/commit-URL/ref/rev_map binding；
- `preflight.rs`：cleanliness、topology、remote head 和 pending recovery checks；
- `planning.rs`：已有 dcommit domain model 的命令层组装；
- `working_copy.rs`：options、temporary checkout、SVN subprocess 和 sink；
- `post_submit.rs`：fetch/readback/verification/rebase completion；
- `dcommit.rs`：短入口和顺序协调。

已有 `dcommit/*` domain 模块不重复拆分。

#### Fetch command

- `runtime.rs`：effective config、range、backend/auth factory；
- `preflight.rs`：mapping overlap 和 tracking state；
- `mirror_identity.rs`：SVM/svnsync hydration/high-water；
- `fetch.rs`：短入口和 import 调度。

每个命令边界单独提交，并运行对应 CLI real-SVN suites。

### 10-F. Narrow the public Rust API

该里程碑最后执行，避免与模块移动叠加：

1. 记录 CLI 实际消费的 core symbols 和 v0.1 public allowlist；
2. 新拆模块从私有可见性开始，仅通过稳定 facade 暴露必要入口；
3. 将依赖内部符号的 integration tests 移入模块测试或改走稳定入口；
4. 优先隐藏 dcommit builders/attributes/fingerprint/tree projection 等实现模块；
5. journal/coordinator、import transaction、fetch editor 等高耦合符号只有在测试和
   CLI 消费者迁移后再收紧；
6. 为保留的稳定入口补充 crate/module 文档。

不得为了完成指标一次性把全部 `pub` 改为 `pub(crate)`。

### 10-G. Final package and release revalidation

- 从干净 checkout 重跑 package 清单和包构建；
- 运行默认、linked parallel/serial、linked CLI read/write、fmt 和 clippy；
- 运行 strict frozen comparison，要求 41/41、8 required summaries、零跳过；
- 提交并推送候选 SHA 后运行 protected release workflow；
- 下载并核对 `release-summary.json` 的 exact SHA、backend、features 和 scenario 数；
- 只有该 artifact 成功后才更新 last proven release SHA。

## Verification Matrix

每个行为相关或结构相关批次至少执行：

```text
cargo fmt --all -- --check
cargo test --workspace
cargo test -p git-svn-rs-core --features svn-libsvn
cargo test -p git-svn-rs-core --features svn-libsvn -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

按领域追加：

- import：`import_mock`、`compat_golden`、CLI `clone_fetch_real_svn`；
- libsvn：`libsvn_backend`、linked CLI clone/fetch；
- dcommit：dcommit unit/restart/coordinator 和 CLI `dcommit_linear`；
- fetch：CLI `clone_fetch_real_svn`；
- package：三个 `cargo package --list`，并检查污染前缀和许可证。

## Delivery Slices

建议按以下独立批次交付，而不是一次性大重构：

| Slice | Contents | Expected commits |
|---|---|---:|
| 10-0/10-A1/10-B | plan、package isolation、licenses、actions、import runtime | 2–4 |
| 10-A2/A3 | changelog、release checklist、version/publish validation | 2–3 |
| 10-C | libsvn tests/boundaries/FFI/runtime/auth/RA | 4–6 |
| 10-D | import discovery/replay/publication | 3–4 |
| 10-E | dcommit and fetch command boundaries | 5–7 |
| 10-F | API allowlist, test migration, visibility/docs | 3–5 |
| 10-G | clean package and hosted release evidence | 1–2 |

完整 Phase 10 预计 3–5 周、约 15–25 个小提交。估算用于控制批次大小，不是跳过
验收或扩大范围的依据。

## Completion Definition

Phase 10 只有同时满足以下条件才可标为 `release-pass`：

1. 三个 package 内容可审计、无生成 fixture、双许可证完整；
2. CHANGELOG、release checklist、最低版本和 publish 顺序已记录并验证；
3. import 不变状态计数回归证明初始化成本不随 revision 数增长；
4. 四个已证明的大模块边界完成拆分，且所有行为/artifact contract 不变；
5. v0.1 public API allowlist 建立，内部实现不再被无意公开；
6. 默认与 linked local matrix、strict frozen 41/41、8 summaries 全部通过；
7. protected hosted artifact 精确绑定 Phase 10 最终候选 SHA；
8. deferred 能力没有被暗中纳入支持声明。
