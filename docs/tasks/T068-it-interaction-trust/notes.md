# T068 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；
- `handoff-notes.md` 第一列含 T068 的两行（T109 的 6 个场景、T100 发现的 `rescan` 没有持有 `Hold`）；
- 设计 5.9.8 的"项目信任""配置档位"两条、12-milestones 里 AP-5、SEC-10、CP-4、PS-2 的验收；
- 代码：
  - `orchestrator/session/trust_tests.rs`（IT-orch-trust-gating）、`gating_tests.rs`、`interaction_tests.rs`；
  - 会话 actor 的 `rescan`、`trust`、`unconfirmed`；`orchestrator/tier_guard.rs`；`policy/attach.rs`、`policy/input.rs`；
  - `driver/runtime/mod.rs` 里 `settingSources` 的下发；
  - 假 agent 的 `question`、`form`、`plan_update`、`plan_confirm`、`permission`、`echo_meta`、`raw`；
  - `tests/lifecycle.rs` 的夹具。

行数指 rustfmt 展开后的行，含文档注释和空行；按净增计。

### 夹具

| 项 | 内容 | 估计 |
|---|---|---|
| `tests/common/` | 从 `lifecycle.rs` 挪出两边都用的夹具：拉起宿主、连接、等待、查表、退出码、`session/create`、`session/send`、时钟推进。宿主守卫改成先 SIGTERM 让宿主有序退出、回收它的适配器，再等它结束。原来的 `kill` 会留下适配器和孙进程，见 T008 修复 1 | 净增约 20（挪动约 170 行，两边对冲） |
| 本文件的辅助 | 带假 agent 的宿主（可带假 agent 参数）；`project/add` 加 `project/trust`；读一个会话流里某类事件的 `data`；按请求回答；当前提示的原因 | 80 |

### 用例

| 用例 | 内容 | 估计 |
|---|---|---|
| IT-form | `form` 多问题表单：请求是 `question`，索引里 `needs_action` 且 `pending_kind: input`；按字段回答后 agent 收到答案；审计有 `interaction.responded` | 55 |
| IT-plan-confirm | `plan_update` 加 `plan_confirm`：请求是 `plan_confirm`，`pending_kind: approval`；选 `exit-plan-default` 后 agent 切到 `default`，回合继续 | 50 |
| IT-interaction-expire | 带错摘要的回答被拒（`digest_mismatch`）；适配器在请求待处理时退出，请求转为 `expired`，之后的回答被拒（`expired`） | 60 |
| IT-config-unconfirmed | 回合中假 agent 自己把模式抬到 `bypassPermissions`：宿主改回原值，会话进入 `config_unconfirmed`，下一条消息不发；`session/confirm_config` 用摘要确认后才发出；审计有 `config.unconfirmed`、`config.confirmed`。改回失败的分支用假 agent 的 `set_config_error`：回合被中断 | 70 |
| IT-trust-stale | 受信任的项目，`.claude/settings.json` 是指向工作区外的链接；改写链接目标后，下一条消息之前的重扫让项目变为 `stale`：消息不发，提示 `project_config_changed`，索引和审计有 `stale`；再信任、恢复队列后消息发出 | 70 |
| IT-trust-rescan-before-prompt | 回合进行中改写项目配置，回合结束后排着的消息在发出前被重扫拦下；悬空链接让扫描失败、带原因；同时是 `rescan` 持有 `Hold` 的复现（见就地修） | 70 |
| IT-trust-settingsources | 未信任的项目：附加时 `_meta` 带 `settingSources: ["user"]`，`session.configured.project_config` 为 `ignored`；受信任时不带、为 `loaded` | 45 |
| **用例小计** | | **420** |

### 就地修

| 项 | 内容 | 估计 |
|---|---|---|
| `rescan` 持有 `Hold` | `Parts` 带上 `Quiescence`；另起的重扫 task 持有 `Hold`，交回结果后释放。复现：手动时钟的宿主上发出要重扫的消息之后，`test/clock_advance {ms: 0}` 一返回就断言已经暂停；修之前重扫还没做完，断言会失败 | 8 |

### 合计

约 580 行，在约 850 的上限之内，开工。过程中发现别的缺陷再单列。
