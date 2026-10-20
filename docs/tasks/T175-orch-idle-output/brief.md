# T175 会话 actor I-e：空闲退出与超长输出

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T089、T169 的执行者接着做
- 分支：`feat/T175-orch-idle-output`
- 依赖：T089（已合并）、T169（合并中）。T099 依赖本任务

## 背景

T089 开工前按三分拆出了本任务（计划 v1.3.38）。现在有两处没做：
- 会话 actor 不会自己退出：会话一旦载入，就一直留在内存里；
- 工具输出没有长度上限：超长输出不转 blob，也没有尾部更新。

T089 已经做了按需载入，所以 actor 退出后，再来的调用会重新载入。

## 范围

**做**（计划 T175 一行；设计 5.2.3、5.2.6、协议 5.3 的 `output_state`、5.3.x 第 756 行附近"超长输出归会话 actor"）：
1. **actor 空闲 10 分钟退出**：
   - 时长经 `Clock` 计时，可以经 `test_config.durations` 缩短；
   - "空闲"的定义照设计：没有运行中的回合、待处理的交互、排队消息，也没有订阅者在等这个会话的写入。如果设计里没写全，在汇报里列出你采用的定义；
   - 退出时用管理器的 `forget`（只在邮箱匹配时才从表里撤出，T089 的交接），之后再来的调用走按需载入；
   - 登记进静止屏障：定时器与退出过程按 T047 的规则持有 `Participant`、`Hold`。
2. **超长工具输出**（协议的 `output_state`）：
   - 超过内联上限后停发 delta，转为 `truncated`；
   - 宿主每秒至多一次用 `item.updated` 更新 `output_tail` 和 `output_bytes`；
   - 结束时给出 blob 引用加尾部预览（`complete`）；
   - blob 由 actor 直接 await 写入，保证 `item.completed` 排在 blob 引用之后（设计 02-host 第 297 行）；
   - 每秒的节流只经 `Clock`。

**不做**：录制导出里的输出处理（已有）；界面上的输出显示（T145 等）。

## 验收标准

- [ ] IT-orch-actor-idle：
  - 空闲到时退出，表里撤出；
  - 有运行中回合、待处理交互或排队消息时不退出；
  - 退出后再投递，按需载入并照常处理；
  - 退出与新调用同时到达时只有一个 actor（`forget` 的邮箱匹配）。
- [ ] IT-orch-long-output：
  - 超过上限后转 `truncated`、停发 delta；
  - 尾部每秒至多更新一次（手动时钟）；
  - 结束时是 blob 引用加尾部预览，`item.completed` 在 blob 之后；
  - `blob/get` 读回的内容完整。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T175 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T175-orch-idle-output/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T175、T169 的行）

- T089：
  - `session/load.rs`；
  - 管理器先放邮箱再载入；
  - `forget` 只在邮箱匹配时撤出；
  - `Fixture::restart`。
- T047：静止屏障的参与者规则，另起的 task 持有 `Hold`。
- T029、T168：blob 的分片与 `blob/get`；`getBlob` 在界面侧的读法。
- T059：宿主层的空闲退出（与本任务的 actor 空闲退出是两回事，不要混用计时）。

## 注意

- 代码约束：
  - 时间只经 `Clock`，定时只经 `Clock`；
  - 纯核心里不做 I/O、不 await；
  - 每条事件流只有一个写者；
  - 不在 tokio 工作线程上同步调用慢的系统接口。
- 不运行真实的 Claude Code、Codex CLI。

## 规模

S，估约 360 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 400 行（包括贴线）就先停下汇报**；硬停止线 450。

**每写完一个模块（实现或一组用例）就用 `git diff --numstat` 统计一次，写进 `notes.md`**。最近两个任务都是写完才统计，越过了停止线才发现，这次请务必逐模块统计。

## 工作方式

T169 合并之后，在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T175-orch-idle-output main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
