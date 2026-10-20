# T177 会话 actor I-f：超长工具输出

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T089、T169、T175 的执行者接着做
- 分支：`feat/T177-orch-long-output`
- 依赖：T175（合并中）。T099 依赖本任务

## 背景

T175 写完空闲退出与订阅时载入后，推算超过停止线，按裁定把超长输出拆成本任务（计划 v1.3.45）。现在工具输出没有长度上限：超长输出不转 blob，也没有尾部更新。你在 T175 的中途汇报里已经给出了做法和估计（实现约 140、用例约 70），本任务照那份来。

## 范围

**做**（计划 T177 一行；协议 03"会话流"一节里 `output_state` 的取值、5.3.11 中"超长输出"一条；设计 5.2.6 里"超长输出写 blob"一条）：
1. **合并器（纯核心）里的状态与节拍**：
   - 累积输出，超过内联上限后，先把缓冲里的 delta 发出去，再转 `truncated`，之后停发 delta；
   - 每秒至多一次用 `item.updated` 更新 `output_tail` 和 `output_bytes`；
   - 节拍只经 `Clock`；纯核心里不做 I/O、不 await。
2. **结束时写 blob**：工具结束时，合并器交出 `Spill`，由 actor await 写 blob，再交回合并器，然后发 `item.completed`（`complete`：blob 引用加尾部预览）。`item.completed` 一定排在 blob 引用之后。同时登记 `blob_refs`。
3. **回合结束时仍在运行的超长工具**：记 `abandoned`，保留最后的尾部，不写 blob。settle 是同步的，等不了写盘；这一点认可。
4. **测试夹具**：让夹具的 `Peer` 能读 `0x02` 分片，用 `blob/get` 读回 blob 的内容。

**不做**：界面上的输出显示（T145 等）；空闲退出与订阅时载入（T175 已做）。

## 验收标准

- [ ] IT-orch-long-output：
  - 超过上限后转 `truncated`、停发 delta，转之前缓冲的 delta 已全部发出；
  - 尾部每秒至多更新一次（手动时钟）；
  - 结束时是 blob 引用加尾部预览，`item.completed` 在 blob 之后；`blob/get` 读回的内容完整；
  - 回合结束时仍在运行的超长工具是 `abandoned`、有尾部、没有 blob。
  - **进程级检查**（T062 的发现）：现在一条约 500 kB 的工具输出会写出超过一帧的 `item.completed`，在线客户端收到 `stream/reset {rebuilt}` 后再也收不到这个会话的事件，新订阅的客户端订阅后连接被关闭。本任务完成后，在真实宿主进程上（`tests/gateway_sync.rs` 或 `tests/common/` 的夹具）跑 `tool_done t1 500000`，断言在线与新订阅的客户端都正常、没有帧超限；并把 T062 迁 SP-4a t1、t7 时暂缺的 blob 断言补回 `gateway_sync.rs`。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T177 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T177-orch-long-output/ci/` 通过，摘要随任务提交。

## 交接事项

- T175：你自己的中途汇报与 `notes.md` 里的超长输出部分。
- T029、T168：blob 的分片、`blob/get` 与 `blob_refs`。
- T014：`apply` 规则；快照里的长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`（`handoff-notes.md` 里 T014 的行）。

## 注意

- 代码约束：
  - 时间只经 `Clock`；
  - 纯核心里不做 I/O、不 await；
  - 每条事件流只有一个写者；
  - 不在 tokio 工作线程上同步调用慢的系统接口。
- 不运行真实的 Claude Code、Codex CLI。

## 规模

S，估约 210 行，加上进程级检查约 40 行，约 250。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 300 行（包括贴线）就先停下汇报**；硬停止线 340。**每写完一个模块就统计一次。**

## 工作方式

T175 合并之后，在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T177-orch-long-output main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
