# T186 Git 服务 II-b：LFS 实测、快照的性能与回合前后的配置指纹

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主安全）。T061 的执行者接着做
- 分支：`feat/T186-git-lfs-perf-fingerprint`
- 依赖：T061（合并中）。T099 依赖本任务

## 背景

T061 开工前按估计拆出了本任务（计划 v1.3.51）。T061 评审时又发现一处设计要求没有任务认领，也并了进来（v1.3.53）：回合开始和结束时随快照计算仓库可执行配置指纹，前后不同就提示。

## 范围

**做**（计划 T186 一行；设计 5.2.10 的快照档、LFS、仓库可执行配置指纹，5.9.6）：
1. **`cargo xtask git-lfs`**：
   - 按锁定版本从 git-lfs 的官方 GitHub release 下载，核对哈希后解压；
   - 请求头用项目占位 UA，不带个人信息；
   - CI 的 `rust` 作业先取它，再跑测试。
2. **IT-git-lfs-snapshot**：
   - 用真实的 git-lfs 跑快照档：`tool(…, lfs)` 指向 xtask 下载的那个；
   - LFS 指针文件进快照，用户配置里的 git-lfs 标准取值换成宿主的绝对路径后照样工作（Q32 的 LFS 部分）；
   - 结果写进汇报，由编排者更新 Q32。
3. **PERF-snapshot**：
   - 快照的性能基准，例如大仓库的起始与结束快照；
   - `perf` 作业还没实现（`run: None`），先写成每晚运行的用例；
   - 基准数据记在 `notes.md`，只写机型泛称，不写机器名和路径。
4. **回合前后的配置指纹**（IT-git-turn-fingerprint）：
   - 回合开始和结束时随快照，用 T097 的纯函数计算仓库可执行配置指纹；
   - 前后不同时，在回合里写 `notice{code: repo_config_changed, params: {paths}}`；
   - 指纹的内容照设计 5.2.10：有效配置的全部来源文件、`hooks/` 与 `core.hooksPath`、`info/attributes`、全部 `.gitattributes`。

**不做**：审查面板的提示与用户档的确认文案（M1b）。

## 验收标准

- [ ] IT-git-lfs-snapshot、IT-git-turn-fingerprint 通过；PERF-snapshot 在每晚跑，基准数据写进 notes。
- [ ] `cargo xtask git-lfs` 有单元测试，覆盖哈希不符时失败。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T186 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T186-git-lfs-perf-fingerprint/ci/` 通过，摘要随任务提交。

## 交接事项

- T061：快照档的 `Place`、`snapshot::take`、白名单配置里 LFS 取值的生成；回合快照的接入点。
- T097、T098：指纹的纯函数；仓库档调用前后的比对。
- T034：git 与 git-lfs 从 `Discovery` 取。

## 注意

- 测试仓库全在临时目录里，不读本机用户的 git 配置。
- 只下载公开物料；本机没有网络时，LFS 用例要明确跳过并说明原因，不能假装通过。

## 规模

- 停止线：320；硬停止线：380

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

T061 合并之后，在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T186-git-lfs-perf-fingerprint main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
