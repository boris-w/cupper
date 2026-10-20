# T089 会话 actor I-c：恢复、空闲退出与超长输出

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T054、T059、T129、T080、T136、T166 的执行者接手（T059 做过宿主的监护与空闲退出）
- 分支：`feat/T089-orch-recovery`
- 依赖：T043（已合并）；T044、T088、T100、T109 也已合并，它们留了交接。T062、T069（宿主集成测试）依赖本任务，二者又挡着 T099

## 背景

T043 拆分时（计划 v1.3.11），恢复、空闲退出和超长输出分给了本任务。现在的状态：
- 不在内存里的会话，`deliver` 和重启后的 `get` 都回 `not_found`；
- 宿主重启后会话不能继续；
- 会话 actor 不会自己退出；
- 工具输出没有长度上限。

几个后续任务（T088、T044、T109、T100）各留了恢复时要注意的点，见下方"交接事项"。

## 范围

**做**（计划 T089 一行；设计 5.2.6 的"崩溃与恢复"、5.2.8、5.5.5、5.4.6）：
1. **按需载入**：接在 `SessionManager::deliver` 里。不在内存里的会话从库载入后再投递；重启后的 `session/get` 也能拿到。
2. **从库恢复**：
   - `TurnMachine`、`QueueCore` 从库恢复，含 `reply_to`；排队消息与命令的对照（`command_id`）从 `queue_entries` 恢复；
   - 已结束的请求经 `Input::Ended` 交给交互代理，用 `interactions.request_json` 和状态；
   - `interaction/get` 对已淘汰出视图的请求改从表里读。
3. **补写启动清理留下的事件**：T037 启动清理留下的 `turn.completed{interrupted}` 和交互的 `expired`，在载入时补写，并决定是否暂停队列。
4. **检查点作废时重发**：`session.configured`、`session.capabilities`、`queue.updated`。
5. **准备阶段崩溃**：崩溃发生在准备阶段时，重开第一个回合（第一条消息不丢）。
6. **空闲退出**：actor 空闲 10 分钟退出，时长经 `Clock` 和 `test_config.durations` 可缩短。退出之后再来的消息走第 1 条的按需载入。要登记进 T047 的静止屏障，照 `Participant::idle` 和 `Hold` 的规则。
7. **超长工具输出**：超过上限转 blob、标 `truncated`，尾部每秒更新一次（设计 5.4.6）。

**不做**：崩溃与晚到输出的后续（T044 已做）；进程级集成测试（T062、T069）。

## 验收标准

- [ ] IT-orch-recovery：覆盖上面 1–7 各条，至少包括：
  - 重启后对旧会话 `send` 能继续，不会回 `not_found`；
  - 队列与 `reply_to` 恢复；
  - 已结束的请求回 `interaction/get`；
  - 补写的 `interrupted` 与 `expired`；
  - 检查点作废后的三条重发；
  - 准备阶段崩溃后第一条消息不丢；
  - 空闲 10 分钟退出后再投递；
  - 超长输出转 blob、尾部节流。
- [ ] 恢复后 `session/get` 与索引算出的状态一致（T100 的提醒：两份实现）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T089 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T089-orch-recovery/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T089 的行；开工前逐行读一遍）

- T043、T037：按需载入的接入点；`command_id` 从 `queue_entries` 恢复；补写的事件与是否暂停队列。
- T088：`interactions.request_json`、`Input::Ended`；`interaction/get` 从表里读。
- T044：设定者的完整 `Principal` 只在内存里，库里只有 `mode_set_by` 的 ID。恢复时按 ID 找回 principal，或者一律视为由创建者设定。选一种，并在汇报里说明理由。
- T109：
  - 信任闸门的原因和"想加载项目配置"的标志只在内存里，从 `session.configured.project_config` 和恢复后的第一次重扫重新得出；
  - `ignore_for_good` 要持久化，否则重启后会话可能重新加载项目配置。这是安全相关的，必须做；
  - `rescan` 里的 `std::fs::canonicalize` 是同步调用，加文件操作时一起挪进 `spawn_blocking`。
- T100：
  - 恢复的会话照常进入索引的状态核心；
  - `StreamWriter::append` 遇到空的 `bodies` 直接返回，`with` 里的行写入不执行。要补文档，并在 `bodies` 为空、`with` 非空时报错或照常写行；
  - `session/get` 与索引各算一份状态，恢复时留意两边一致。
- T047、T138：静止屏障的参与者规则。T138 的教训：不要在 tokio 工作线程上同步调用慢的系统接口。

## 注意

- 代码约束：
  - 每条事件流只有一个写者；
  - 时间只经 `Clock`；
  - 有状态的决策逻辑写成纯核心，核心里不做 I/O、不 await；
  - 子进程只经 `cupper-sys::spawn`。
- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。

## 规模

M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 850 行（包括贴线）就先停下汇报，并给出拆分建议**；硬停止线 950。每完成一个模块就用 `git diff --numstat` 统计一次（守则第 8 条）。

## 工作方式

在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T089-orch-recovery main`。`feat/T166-ui-stream-rows` 已经合并，不要再往上提交。这个 worktree 的 `target/` 刚清理过，第一次构建会慢一些。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,320 行（实现约 900、测试约 420），在写代码之前停下，给了三种拆法。

**裁定：做法二，三分（计划 v1.3.38）。**
- **T089 收窄为载入与恢复**：
  - 按需载入，含并发载入同一会话时只起一个 actor；
  - 各核心的恢复入口与设定者、信任闸门；
  - `ignore_for_good` 持久化（迁移 `0002`）与 `canonicalize` 挪进 `spawn_blocking`；
  - `Input::Ended` 与 `interaction/get` 读表；
  - 补写中断与过期、是否暂停队列；
  - 两项零碎（`append` 空 `bodies`；`session/get` 与索引一致）。
  - 用例 IT-orch-recovery 只覆盖这些。上限约 900 行，硬停止线 980。
- **新增 T169（崩溃后的重建）**：检查点作废时重发三类事件、准备阶段崩溃后重开第一个回合；新增 IT-orch-rebuild。
- **新增 T175（空闲退出与超长输出）**：actor 空闲退出与退出后再投递、超长输出转 blob 与尾部节流；新增 IT-orch-actor-idle、IT-orch-long-output。
- T169、T175 都依赖 T089，彼此独立，T089 合并后再派；T069 改为也依赖 T169，T099 改为也依赖 T175。

**两点确认**：
1. **设定者的恢复**：按 `mode_set_by` 的 ID 找回，认可。找不到时取保守的一侧：如果"由创建者设定"和"来源不明"两种假设对 `config_unconfirmed` 的结论不同，取"需要确认"。两种假设的差别写进 `notes.md`。
2. **`ignore_for_good` 留在 T089**，认可。迁移要有用例证明：重启后会话不会重新加载项目配置。
