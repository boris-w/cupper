# T034 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；
- `handoff-notes.md` 第一列含 T034 的各行（T026、T033、T037、T088、T035、T096、T098、T094）；
- 设计 5.2.9、5.3.5 的索引流、5.5.10；
- 代码：`supervisor/`（组装与 `Part`）、`orchestrator/session/`（`IndexNotes`、`NoIndex`）、`store/streams/`（`StreamWriter`、加载与检查点）、`cupper-protocol` 的 `index.rs` 与 `view/index.rs`、`git/`（仓库档）、`driver/locate`（`CliLocator`）。

行数指 rustfmt 展开后的行，含文档注释和空行。

### 实现（草估）

| 模块 | 内容 | 草估 |
|---|---|---|
| `index/actor.rs` | `IndexActor`：邮箱、索引流唯一的 `StreamWriter`、`projects` 表唯一的写者；扫描、git 等慢操作另起 task，完成后以消息回送（5.2.3 规则 5）；登记进静止屏障 | 150 |
| `index/port.rs` | `IndexPort`（`deliver` 按方法分派、`opened`、`closed`）；`Part`（`stop` 写检查点）；`IndexNotes` 的实现 | 80 |
| 启动第 7 步与重建 | 绑定套接字之前打开索引流；按 `projects`、`sessions` 两张表对账：视图里缺的、和表不一致的补发 `upserted`，表里没有的发 `removed`。这样不必判断"检查点作废、日志又已裁剪"，所有情况都能对上（T026 交接） | 100 |
| `StreamWriter::checkpoint` | 有序退出时立即写一次检查点（现在只有每 1,000 条写一次） | 30 |
| 项目与信任 | `project/add`（realpath；用仓库档取 git common dir 和 `--show-toplevel`；名称、位置；写行；扫描；`Trust::added`）、`project/trust`（重新扫描，`Trust::confirmed`，写信任列）、`project/trust_info`（重新扫描，`rescanned`，必要时转 `stale` 并写行，findings 与差异）；错误映射；`project.upserted` | 220 |
| 帧上限 | 见下文待裁定 1：按字节预算截断 findings 和差异，并告诉客户端省略了多少；协议附加字段 | 60 + 10 |
| `clients`、`host` | `opened`、`closed` 时写 `clients.updated`（`Gateway::clients()`）；启动时写 `host.updated`（版本、弱认证、待升级、防休眠、上次是否异常退出） | 50 |
| 宿主组装 | 建 `IndexActor`，`routes.index(..)`，作为第三个 `Part`；`GitTool` 的来源见待裁定 2 | 60 |
| 状态计算纯核心 | 每个会话的摘要；按 5.2.9 的优先级算状态；`pending_kind`；`unread`、`error`；每个会话 100 ms 合并（经 `Clock` 定时） | 200 |
| `IndexNotes` 扩充与接线 | 现在只有 `pending`。状态计算还需要：会话创建、回合开始和结束（含状态、失败码）、标题、分支、归档、最近活动。会话 actor（T043、T088 的代码）要在相应位置调用 | 100 |
| 已读、在场 | `session/mark_read` 只前移不后退，写 `sessions.last_read_seq`；`client/presence` 只采信 `desktop_ui` | 90 |
| 通知意图 | 进入 `needs_action`、回合结束、出错，且没有前台的 `desktop_ui` 在看时，写 `notification.intent`（`intent_id` 经 `IdGen` 生成） | 50 |
| `agents` 接口 | 给 T009 的 `AgentStatus` 留的入口，T009 之前为空 | 20 |
| **实现小计（草估）** | | **1,220** |
| **按计划 1.1 的经验乘 1.7** | | **约 2,075** |

### 测试

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-index-status | 优先级、`pending_kind`、`unread` 与已读只前移、`error` 被成功回合或查看清掉、100 ms 合并（`ManualClock`） | 150 |
| UNIT-index-intent | 三种原因；前台 `desktop_ui` 在看时不写；CLI 的在场不算 | 120 |
| IT-project-add-trust | 真实宿主进程经网关：临时目录里建 git 仓库并添加项目（工作区在仓库根之下）、扫描、信任、改文件后 `stale`、`trust_info` 的差异、大量 findings 时每帧不超过 64,000 字节 | 250 |
| 重建与检查点 | 视图与表对账；有序退出写检查点 | 80 |
| **测试小计** | | **约 600** |

### 合计

| 口径 | 实现 | 测试 | 合计 |
|---|---|---|---|
| 草估 | 1,220 | 600 | **约 1,820** |
| 实现乘 1.7 | 2,075 | 600 | **约 2,675** |
| 按我前三个任务的实测比（实现约为草估的 1.0–1.1 倍） | 约 1,340 | 600 | **约 1,940** |

三种算法都超过约 1,500 的上限，所以开工前停下，见下一节。

## 汇报与拆分建议

**方案 A：按任务书的例子拆成两个（建议）。**

- **T034（关键路径，T047 依赖它）：索引 actor 骨架、项目与信任、`clients`、`host`、宿主组装。**
  - 实现：草估约 760，包括 actor、端口、`Part` 与检查点、启动第 7 步与对账、项目与信任、帧上限、`clients`、`host`、组装。
  - 测试：约 330，包括 IT-project-add-trust、对账与检查点。
  - 合计：按实测比约 1,170；按乘 1.7 约 1,620。
  - `IndexNotes` 的实现先只接 `pending`，记下待处理的个数和类别，供第二部分使用。
  - T047 要的静止屏障登记、索引流和 `project/add` 都在这一部分里。
- **T034b：状态计算与通知。**
  - 内容：状态计算纯核心与 100 ms 合并、`IndexNotes` 扩充与会话 actor 的接线、已读与在场、通知意图、`agents` 接口。
  - 测试：UNIT-index-status、UNIT-index-intent。
  - 合计：草估约 460 加测试 270；按实测比约 780，按乘 1.7 约 1,050。
  - 依赖 T034。

**方案 B：拆成三个。** 把方案 A 的第一部分再拆开：
- 骨架、`clients`、`host`、组装：约 950；
- 项目与信任：约 790。

每份都远低于上限，但关键路径上多一个节点。只有在你仍按乘 1.7 计、认为方案 A 的第一部分（约 1,620）超线时，才建议这样拆。

## 待裁定（开工前请一并定）

1. **帧上限的处理。**
   - 问题：`project/add`、`project/trust_info` 的结果里，findings 可以有上千条，`detail` 又不截断；`diff_from_trusted` 最多 2,000 条，每条约 60 字节，也可能超过 64,000 字节。
   - 建议：参照 `history/page` 的 48,000 字节预算，结果里的 findings 和差异合计不超过这个预算。
     - 单条 `detail` 先截到 2,000 字节，按字符边界切，末尾加 `…`；
     - 然后按顺序装入，装不下就停；
     - 协议加两个可选字段（附加式改动，跑 `pnpm gen`）：`TrustScan.findings_omitted?: u32` 和 `ProjectTrustInfoResult.diff_omitted?: u32`；
     - 确认框据此写"另有 N 项未列出"；摘要仍然覆盖全部内容。
   - 另一个做法是 `project/trust_info` 加分页参数。代码和协议改动都更多，不建议。
2. **`project/add` 要用仓库档，需要一个 `GitTool`。**
   - 现状：`GitTool::new` 要 CLI 发现给出的 git 路径和可信 PATH（`CliLocator::tool`、`trusted_path`），而 `CliLocator` 又要登录 shell 探测（T094 的 `driver/probe`）。宿主组装里现在还没有这条链，原计划由 T055 接。
   - 建议：由本任务在 `Host::start` 里接上"登录环境 → `CliLocator` → `GitTool`"。test-harness 构建用 `test_config.login_env`。T055 接会话管理器时复用同一份结果。另一个做法是等 T055，但本任务在关键路径上，不宜等。
   - 还有一点：git 不可用时，`project/add` 照常添加，`is_git = false`，扫描范围只是项目目录本身。这一点请确认。
3. **启动时按表对账，而不是只在"检查点作废、日志又已裁剪"时重发。** 每次启动都把视图和 `projects`、`sessions` 两张表比一遍，有差异就补发事件。这样不用判断日志是否完整。代价是启动时多一次查询，会话数在千级以内，可以接受。
4. **`IndexNotes` 的扩充要改会话 actor**（T043、T088 的代码）。按方案 A，这部分归 T034b；我理解这是在范围之内的接线，请确认。
5. **`host.updated.last_exit_abnormal`**：由启动时 epoch 判定的 `LastExit` 得出，需要 `Host::start` 把它传给 `IndexActor`。改动在 T037 的组装代码里，只有几行。
