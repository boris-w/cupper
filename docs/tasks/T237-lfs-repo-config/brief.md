# T237 仓库档里 git-lfs 执行的程序与联网

- 派发日期：2026-11-04
- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`fix/T237-lfs-repo-config`
- 依赖：T235（发现者，已合并）

## 背景

T235 开工时用锁定的 git-lfs 3.8.0 实测：宿主的 git-lfs 在仓库档运行时，会执行仓库 git 配置里 `lfs.extension.<名>.clean/smudge` 指定的程序。
- 写在 `.git/config` 里会执行，写在 `.lfsconfig` 里不会。
- 仓库档的可执行类键清单（`rules.rs` 的 `names_program`）没有 `lfs.extension.*`。
- 用 `-c lfs.extension.x.clean=` 置空也不行，git-lfs 会直接失败，不会跳过。

这和 Q32 是同一类问题：宿主自动发起的调用不应执行仓库配置指定的任何程序（5.2.10 第 4 条、5.8.5）。会触发它的有：
- revert 时的 smudge；
- `git/status` 的 `update-index --refresh`（读代码的判断，未实测）；
- 其他会让 git-lfs clean 或 smudge 的仓库档命令。

T235 同时记下两件相关的事：
- smudge 在本地找不到对象时会去 LFS 服务器下载，宿主就会联网。T235 只补上了 cupper 自己写入的指针的对象，其他缺失的对象照旧会联网。
- revert 时 smudge 失败被报成 `conflict/stale_diff`，掩盖了真实原因。

详情在 T235 卷宗 `notes.md` 的"For T237"一节。

## 范围

**做**（计划 T237 一行）：
1. **审查 git-lfs 会执行程序或联网的配置**：
   - `lfs.extension.*`、`lfs.customtransfer.*.path`、`lfs.standalonetransferagent`，以及 git-lfs 文档和源码里其他会启动程序的键；
   - 会联网的路径（smudge 下载、`lfs.url`、batch 请求）。
   逐项实测，结果列表写进 notes。
2. **定下处理方式并实现**，报给 `main` 定了再写代码：
   - 方案一：出现这些键时拒绝（`unsafe_repo_config`）；
   - 方案二：按非标准 LFS 处理（不运行 git-lfs，指针原样显示）；
   - 联网：仓库档里禁止 git-lfs 下载（例如经环境变量或配置让 smudge 不联网），本地没有对象时以明确的语义码失败。
3. **smudge 失败的错误映射**：报出真实的原因，不再是 `stale_diff`。
4. **用例**：
   - **IT-git-lfs-repo-config**：恶意夹具里配 `lfs.extension.*` 等键，证明没有程序被执行、没有联网；用 T229 的写法，先用用户自己的 git 做对照，证明夹具确实会执行。
   - **UNIT-git-lfs-store**：补上 T235 `git/lfs.rs` 里存储位置（`lfs.storage` 的相对、绝对路径）和 `core.sharedRepository` 各取值（`all`、八进制、`umask` 等）的单元用例。
5. 快照档用白名单配置，一并确认不受影响，写进汇报。
6. 写进 5.2.10 的部分，在汇报的"设计偏差/补充"一节列出，由编排者在合并时改。

**不做**：与 LFS 无关的 Git 服务改动。

## 验收标准

- [ ] IT-git-lfs-repo-config、UNIT-git-lfs-store 通过；IT 在 runner 上连跑 10 次（由 `main` 触发 test-repeat，它已经会下载 git-lfs）。
- [ ] 结论写进汇报：哪些键会执行程序、哪些会联网，各自怎么处理。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：300；硬停止线：360（审查之后的估算报给 `main` 时可以调整）

## 参考资料

- 设计：5.2.10（调用约定第 2、4 条，LFS 的白名单取值）、5.8.5、13 的 Q32。
- 卷宗：T235（notes 的"For T237"一节）、T229（Q32 写操作的用例与对照写法）、T098（仓库档规则）、T186（锁定的 git-lfs）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条）。
- 测试只在临时目录的仓库上操作；不连任何 LFS 服务器，联网的验证用不可达的地址或本地的假端点。
- 恶意夹具里的程序不能真的被执行，用例要能证明这一点。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
