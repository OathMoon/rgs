# git-svn-rs 核心闭环替代品计划

## Summary

- 从空目录 `E:\Repositories\gitsvn` 初始化 Rust + Git 项目，开发独立命令 `git-svn-rs`。
- 默认不覆盖系统 `git-svn`；额外提供可选 `git-svn` shim，使用户可通过 `git svn ...` 调到新实现。
- 第一版实现核心闭环：`clone`、`init`、`fetch`、`rebase`、`dcommit`、`log`、`info`、`find-rev`、`gc`、`reset`。
- 参考依据：[git-svn 官方文档](https://git-scm.com/docs/git-svn)、[git-svn.perl](https://github.com/git/git/blob/master/git-svn.perl)、[raw git-svn.perl](https://raw.githubusercontent.com/git/git/master/git-svn.perl)、[perl 目录](https://github.com/git/git/tree/master/perl)、[Git.pm](https://raw.githubusercontent.com/git/git/master/perl/Git.pm)、[Git::SVN.pm](https://raw.githubusercontent.com/git/git/master/perl/Git/SVN.pm)、`subversion`/`subversion-sys` crates.io 元数据；详细参考索引见 `.plans/00-git-svn-rs-review-and-roadmap.md` 的 `Reference Sources`。

## Key Changes

- 使用 Rust workspace：主 crate 输出 `git-svn-rs`，核心逻辑放入 library crate，CLI 用 `clap`，错误用 `thiserror`/`anyhow`。
- SVN 后端绑定 `libsvn`：优先使用 `subversion` crate，不足处通过 `subversion-sys` 封装安全 FFI；所有 APR pool、auth、RA session、delta editor unsafe 代码隔离在 `svn_ffi` 模块。默认构建仍不依赖 libsvn，但 release 兼容门禁必须在安装 SVN/libsvn/Perl `git svn` 的环境中运行。
- Git 后端调用原生 `git` 命令而不是 libgit2：使用 `git init/config/fast-import/rev-list/diff-tree/rebase/update-ref/cat-file`，保证行为贴近当前 Git。
- Git 命令封装参考 `perl/Git.pm`：覆盖 repository/worktree 发现、config 单值/多值读取、stdout/stderr/exit-code 传播、pipe close、路径反引用、prompt fallback 和 temp lock 语义。
- 完全兼容 `git-svn` 元数据：读写 `.git/config` 的 `[svn-remote "<name>"]`、`.git/svn/**/.rev_map.*`、`git-svn-id:` footer、`unhandled.log`。
- 支持标准布局和自定义布局：`--stdlayout`、`--trunk/-T`、`--branches/-b`、`--tags/-t`、`--prefix`、多个 branches/tags mapping。
- 支持核心选项：authors file/program、ignore/include paths、ignore refs、revision range、log-window-size、localtime、no-metadata、rewrite-root、rewrite-uuid、username、config-dir、no-auth-cache。
- 路径过滤 regex 使用 Perl 兼容优先方案，至少覆盖官方文档中的 lookahead 示例；无法兼容的 Perl 特性返回明确错误。
- `fetch/clone` 数据流：`RaSession` 用 `get_log` 发现变更窗口，用 `do_update`/`do_switch` 驱动 `SvnFetchEditor`，按 ref mapping 分流，处理 copy/delete/modify/absent/props，写入 Git 对象，更新 remote refs 和 rev_map。`FastImport` 只是字节安全的对象写入机制，不作为 SVN 行为模型。
- SVN 属性处理：支持 `svn:executable`、`svn:special`/symlink、空目录 placeholder；其他属性记录到 `.git/svn/<ref>/unhandled.log`。
- `dcommit` 第一版只支持线性提交：通过 `GitDiffPlanner` 解析 `git diff-tree -z -r -C`，通过 `SvnCommitEditor`/`PathEnsurer`/`PropertyMapper` 逐个将 Git commit diff 写回 SVN，支持 `--dry-run`、`--commit-url`、`--no-rebase`、显式 `--mergeinfo`；复杂 mergeinfo 自动生成暂不实现。
- 未纳入核心闭环的命令，如 `branch`、`tag`、`set-tree`、`propget`、`propset`、`show-ignore`，CLI 可识别但返回 `unsupported in v1`。

## Planning Rules

- `.plans/00-git-svn-rs-review-and-roadmap.md` 是架构总线；其他计划必须服从它的阶段依赖和门禁。
- 各分计划里的兼容单元不是可选优化。`GlobSpec`、path/url utilities、rev_map lock/fsync、migration、RA session、fetch editor、commit editor、log formatter、auth prompt、golden comparison 都是对应阶段的必做内容。
- 本地默认验证允许缺少 SVN/libsvn/Perl `git svn` 时显式 skip；release 兼容验证不允许 skip，缺依赖视为环境失败。
- Golden tests 不接受“只比较数量/长度”的弱断言。必须比较归一化后的 config、ref 名和对象、`git-svn-id` footer、rev_map 记录、命令输出和关键文件模式。

## Public Interfaces

- CLI:
  - `git-svn-rs <command> [options] [arguments]`
  - 可选 shim：`git svn <command> ...`
- 核心类型:
  - `SvnRemoteConfig`：表示 `[svn-remote]` 配置、URL、ref mapping 和兼容选项。
  - `RefMapping`：SVN path/glob 到 Git ref 的映射。
  - `RevMap`：SVN revision、UUID、Git object id 的双向索引。
  - `SvnBackend` trait：封装 `log`、`replay/fetch revision`、`commit_delta`、`props`、`auth`。
  - `GitBackend` trait：封装 Git plumbing 调用、object format、refs、fast-import、rebase。
- 不暴露稳定 Rust API；library crate 主要服务测试和命令复用。

## Test Plan

- 单元测试：config 解析、refspec/glob/brace expansion、rev_map 读写、`git-svn-id` 解析、authors 映射、路径过滤优先级。
- 集成测试：用 `svnadmin` 创建临时 SVN 仓库，覆盖 `file://` 标准布局、自定义布局、分支、标签、copy、delete、rename、可执行位、symlink、空目录。
- 兼容测试：在带原版 `git-svn` 的 CI 环境生成 golden fixture，对比 refs、commit message、rev_map、`find-rev`、`info`、`log` 输出。
- 写回测试：线性 `dcommit` 写入 SVN 后再 `fetch/rebase`，验证 SVN revision、Git ref、rev_map 和工作区状态一致。
- Windows 优先：首版在当前 Windows 环境构建；文档写明 SVN/libsvn 安装与 `PATH`/库路径要求。

## Assumptions

- 当前目录为空且不是 Git 仓库；实施第一步会执行 `git init`。
- 首版目标是“可用核心闭环”，不是一次性覆盖 `git-svn` 的所有历史命令。
- 不直接复制 Perl 代码；以官方文档、行为测试和脚本结构作为规格参考。
- 默认命令名保持 `git-svn-rs`，只有用户显式安装 shim 时才启用精确 `git svn` 入口。
