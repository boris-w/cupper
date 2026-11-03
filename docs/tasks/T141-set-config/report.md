# T141 汇报

- 执行者：子 agent（全栈：宿主编排与界面；拆分后本任务只含宿主）
- 分支：`feat/T141-set-config`（PR https://github.com/boris-w/cupper/pull/57 ，头提交 0fd0cbc）
- 完成日期：2026-11-03

宿主的 `session/set_config` 做完了，IT-set-config-tier 本机通过。交汇报时 PR 的 CI 还在跑，结果还没有：https://github.com/boris-w/cupper/actions/runs/41621995878 。

做完宿主时规模是 +470，比估算高 38%。照这个偏差推算，整个任务约 1,010，超过 1,000，按事先的规则停下来报告了你。你裁定拆分：界面（权限按钮、"{模型} {强度} ⌄"、完全访问提示、组件测试、审阅截图）和 E2E-full-access-native 移到 T231。

## 做了什么

1. **`AgentPort::set_config`（带回复的请求）**
   - 原来的 `DriverCommand::SetConfig` 没有回复，agent 拒绝时什么也不回来。现在它只留给 TierGuard 改回档位时用。
   - runtime 拿到 `session/set_config_option` 的结果后，先给会话发 `ConfigChanged`，再把同一份选项交给等着的回复。agent 拒绝时回它的错误；适配器不在时回"the adapter went away"。
2. **会话 actor 处理 `session/set_config`**
   - **核对**：
     - 选项必须在能力里，否则返回 `unsupported`，`data.capability` 为选项名；
     - 取值必须是这个选项的取值之一，否则返回 INVALID_PARAMS；
     - 权限模式按取值的 `risk`，从当前档位出发，用 `policy::tier_change` 判定（5.8.2 的表）；
     - 非档位的选项只需要方法本身的 `write`；SEC-9 已由网关的 `policy::set_config` 检查。
   - **已连接**：经驱动请求 agent，等它的回复。回复到达后：
     1. 按返回的选项写 `session.configured`；
     2. 让 TierGuard 核对新模式；放宽之前已经告诉过它设定者是谁（`Set {by, tier}`），此时记下 `mode_set_by`，进入完全访问时同时记下 `full_access_confirmed_at`；
     3. 在同一次提交里完成账本记录并回复 `{config}`。
     - 所以客户端拿到回复时，流和行里已经是新配置。
     - agent 拒绝时返回 `agent_error`，带 `agent_code` 和脱敏后的 `agent_message`。
   - **未连接**（例如宿主重启后会话处于空闲）：只记进会话配置，写 `session.configured`，立即回复；下次 attach 时应用。
   - **持久**：成功修改的值也记进会话配置，之后 resume 应用的是用户选的值，不会退回创建时的值；`config_json` 随 `session.configured` 更新。
   - **审计**：切到完全访问时写 `privileged.executed`，内容取自 `policy/describe` 的描述。
3. **路由**：`Routes::sessions` 加上 `SessionSetConfig`；dispatch 测试的"未实现"表去掉这一项，网关测试里重复的路由删掉。

## 关键取舍

- **回复晚于流**：会话自己用回复里的选项写 `session.configured`，随后到达的那条 `ConfigChanged` 内容相同，不再写任何东西。这样就不会出现"回复已到、流里还是旧配置、`mode_set_by` 还没写"的竞态。
- **放宽档位走 TierGuard 现有的 `chosen` 机制**：agent 报告的模式不超过所选档位时，才把设定者改成发命令的一方。降档同样会解除 `config_unconfirmed`，与设计 5.2.6 的第二种解除方式一致。
- **未连接时不主动 attach**：只记下，由下次 attach 应用，开销最小。你已同意。

## 验证方式

- **IT-set-config-tier**（`orchestrator/session/config_tests.rs`，经网关和会话夹具）：
  - `it_set_config_tier_each_cell_of_the_tier_table`：
    - 从 normal 放宽到 elevated：UI control、UI window、cli+write+approve、test_harness 允许；cli+write 被拒（`forbidden scope`，缺 approve）；remote_device 被拒（`forbidden role`，SEC-9）。
    - 放宽到 full_access：只有 UI control 和 test_harness 允许，其余是 `forbidden scope`（缺 privileged），remote_device 是 `forbidden role`。
    - 每个允许的格子都核对了 `mode_set_by`，以及 `full_access_confirmed_at` 是否写入。
    - 收紧对各列都开放；`cli` 只读被拒。
  - `it_set_config_tier_other_options_refusals_and_a_detached_session`：
    - cli+write 改模型成功，回复带完整配置；`cli` 只读被拒；
    - 未知选项返回 `unsupported`，未知取值返回 INVALID_PARAMS；
    - agent 拒绝时返回 `agent_error`，带 `-32602` 和 agent 的原话；
    - 宿主重启后，未连接时的修改不惊动 agent，下一条消息触发的 attach 带上新值。
  - 本机连跑 5 次（每次 0.03 秒）都通过。
- **其他本机检查**（各跑一次）：
  - `cargo test -p cupper-host --features test-harness --no-fail-fast` 全部通过；
  - `cargo clippy -p cupper-host --all-targets --all-features -- -D warnings`、`cargo fmt --all --check` 都通过。
- **协议**：没有改，`schema-compat` 预计 0 breaking。
- **规模**：`cargo xtask size` 为 +470 −30。

## 提交列表

```
0fd0cbc docs(task): T141 notes
de99d6b feat(host): session/set_config by the tier rules
```

## 遗留问题 / 范围外发现

1. **转给 T231**：界面的权限按钮和"{模型} {强度} ⌄"、完全访问提示、组件测试、审阅截图、E2E-full-access-native，以及顺带接上 T143 的 `change_model` 入口。宿主这边已经就绪：
   - 普通档和 elevated 由窗口直接发 `session/set_config`；
   - 完全访问走 `requestPrivileged({kind: "set_config", …})`，主进程会经 `policy/describe` 和原生确认框后在 control 连接上发出。
2. **attach 期间的修改会延后**：attach 已经发出、还没返回时，会话算未连接，这期间的修改要等下一次 attach 才应用，当前这次 attach 用的还是旧值。概率低，可以作为后续小修。
3. **设计要补的几处**（你说合并时改）：
   - `AgentPort::set_config` 带回复；
   - 回复晚于 `session.configured`；
   - 未连接时只记下，下次 attach 时应用，并更新 `config_json`；
   - 成功修改的值进入会话配置，resume 时重新应用。

notes.md（估算、实际规模与拆分、取舍）已在分支上提交，位置 `docs/tasks/T141-set-config/notes.md`。
