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

## 统计（`git diff --numstat`，只算 `crates/`）

| 项 | 增 | 删 |
|---|---|---|
| `tests/common/mod.rs`（从 `lifecycle.rs` 挪来，加守卫的改动） | 202 | 0 |
| `tests/lifecycle.rs` | 6 | 176 |
| `tests/interaction_trust.rs`（七个用例加辅助） | 471 | 0 |
| `Cargo.toml`（新测试目标要求 `test-harness`） | 5 | 0 |
| 就地修 1：`rescan` 持有 `Hold` | 7 | 1 |
| 就地修 2：适配器死在 prompt 中途时请求转为 `expired` | 6 | 0 |
| **合计** | **697** | **177** |

净增 520 行，估计约 580。

## 过程记录

- **夹具**：挪进 `tests/common/`，`lifecycle.rs` 和本文件都用。
  - 宿主守卫 `Running` 改成先 SIGTERM 再等结束：原来的 SIGKILL 会让宿主来不及停下适配器，留下适配器和它的孙进程，这正是之前看到的残留。
  - `common` 带一个写了理由的 `allow(dead_code)`：每个测试 crate 只用到其中一部分。
- **IT-form**：`form color=red|blue` 变成一个 `question` 请求，只有一个 `single_choice` 字段（自定义答案并在 `allow_custom` 里）；状态 `needs_action`、`pending_kind: input`；回答后 agent 收到 `{"question_0":"blue"}`，请求 `resolved`，审计有 `interaction.responded`。
- **IT-plan-confirm**：选 `exit-plan-auto`（需要 privileged，harness 有）。agent 把模式切到 `auto` 并继续说完，`session.configured` 最后是 `auto`，没有进入 `config_unconfirmed`。
- **IT-interaction-expire**：摘要不对时回 `conflict`、`digest_mismatch`。按 `adapters` 表里的 pid 杀掉适配器之后，请求转为 `expired`（`adapter_exited`），之后的回答回 `expired`，回合以 `adapter_exited` 失败，索引里状态为 `error`。
  - 这里发现了就地修 2：修之前请求是 `cancelled`（`turn_ended`），回合的失败码是 `other`。
- **IT-config-unconfirmed**：
  - 假 agent 用 `raw` 推一条 `current_mode_update` 到 `bypassPermissions`。宿主改回 `default`，进入 `config_unconfirmed`（`full_access_unconfirmed`）；下一条消息排着、队列暂停；`session/confirm_config` 之后发出；审计依次有 `config.unconfirmed`、`config.confirmed`。
  - 改回失败的分支用假 agent 的 `set_config_error`：回合被中断（`cancelled`），同样进入 `config_unconfirmed`。
- **IT-trust-stale**：`.claude/settings.json` 是指向工作区外的链接，另放 12 MB 的 agent 文件，让扫描要花一阵。
  - 回合中改写链接目标；回合结束后在手动时钟上 `test/clock_advance {ms: 0}`，一返回就查到 `project_config_changed` 的提示；下一条消息排着、队列暂停。
  - 索引和审计里都有 `stale`；重新信任、恢复队列后消息发出。
  - **复现**：修之前连跑 10 次，10 次都在"一返回就查提示"这一步失败（提示还没写）；修之后 10 次全过。
  - 起初的写法是"消息发出后再推进时钟"，测不出这个缺陷：`session/send` 的回复要等发送前的检查（`Purpose::Check`）做完才返回。没有 `Hold` 的，是回合结束、恢复、附加这几种重扫。
- **IT-trust-rescan-before-prompt**：项目是 git 仓库的子目录（登录环境给 `PATH`，让宿主找到 git）。
  - 回合中改写仓库根的 `.claude/settings.json`，回合中排着的消息在发出前被拦下。
  - 重新信任、恢复后消息发出；再放一个悬空的 `.mcp.json`，下一条消息被拦下，提示带原因 `dangling_link`，审计有 `project.trust.scan_failed`。
  - 第二个回合结束后先推进一次时钟，确认那次回合结束的重扫已经做完，再放悬空链接，免得那次重扫也撞上它、多记一条。
- **IT-trust-settingsources**：
  - 未信任的项目：附加时 `_meta` 是 `{"claudeCode":{"options":{"settingSources":["user"]}}}`，`project_config` 为 `ignored`；
  - 信任之后新开的会话：`_meta` 为 `null`，`project_config` 为 `loaded`。
- **测试目标**：要求 `test-harness` 特性，和 `lifecycle` 一样，否则不带特性的 clippy 编不过。

## 就地修

1. **`rescan` 没有持有静止屏障的 `Hold`**（T100 合并时发现）：`Parts` 带上 `Quiescence`，重扫 task 持有 `Hold`，交回结果后释放。增 7、删 1。
2. **适配器死在 prompt 中途时，请求被记成 `cancelled`、回合失败码是 `other`**：SDK 在适配器输出结束时让在途的 prompt 以 `incoming_transport_closed` 失败，这个失败常常比运行时的 `AdapterExited` 先到会话。现在把这个错误归为新的 `AgentErrorClass::AdapterExited`（失败码 `adapter_exited`），回合以 `adapter_exited` 失败，请求按 SEC-10 转为 `expired`，两者谁先到结果都一样。增 6。
