# T009 工作笔记

## 开工前的核对

- `AgentRuntime`（`driver/runtime/mod.rs`）：
  - attach 的回复里命令列表为空，是因为 `available_commands_update` 要在 `session/new` 回复之后 2–5 ms 才到；
  - `FromAgent::AuthStatus` 被忽略；
  - `DriverCommand::Probe` 被丢弃。这个命令在 `AgentPort::send` 上没有回复通道，接上要给运行时加一个带回复的方法，并删掉 `api.rs` 里的这个变体（没有别处用它）。
- 提取层（T053）每次 `available_commands_update` 都给出 `CommandsChanged`；会话核心现在不处理它。会话 actor 只在 attach 回来时比较并写 `session.capabilities`。
- `agent_capabilities` 表已有（`agent`、`adapter_version`、`cli_version`、`captured_at`、`json`）；`session/create` 已经从这张表读缓存（T043），但还没有人写。
- `agents/capabilities`、`agents/refresh` 在网关的 unsupported 清单里标着 `Until("T055")`；协议类型已有：`AgentsCapabilitiesResult.cached`，`AgentsRefreshResult.status`（`AgentStatus`）。
- `AgentStatus` 要的字段里，`Launch` 现在没有：CLI 的安装方式（`CliInstall`，定位结果里有）和配置目录的状态（`production` 算过，但没留下）。
- 发现链 `supervisor::discover` 和 `claude_launch` 是 `supervisor` 的私有函数，`agents/refresh` 要复用。

## 规模估计（开工前）

上限约 750 行（含测试）。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 运行时：命令列表 | 按会话记住上次发出的列表，内容没变就不发；会话打开期间先存着，attach 的回复带上；agent 级的最近列表给之后的 attach 和探测 | 40 | — |
| 能力缓存 `runtime/cache.rs` | 写表（attach 成功、命令变化、探测）、读表（版本不符视为没有）、启动时作废旧版本的行 | 60 | 50 |
| 登录状态 | `_auth/status_update` → kind、plan；prompt 以 `not_logged_in` 失败 → `logged_out`；变化时交给索引 | 25 | — |
| 探测 | 带回复的 `probe()`；`<数据目录>/probe/claude/` 删掉重建（0700）；在里面 `session/new`，等命令列表最多 200 ms（经 `Clock`），`session/close` | 70 | 40 |
| `Launch` 补字段 | CLI 的安装方式、配置目录的状态 | 15 | — |
| 会话 actor | 收到 `CommandsChanged` 时更新 `session.capabilities`，内容变化才写 | 20 | 30 |
| `agents/*` 端口与 `AgentStatus` | `agents/capabilities`（读缓存）、`agents/refresh`（重跑发现与版本检查，换掉运行时的 `Launch`，缓存为空时探测）；状态汇总与语义码映射；交给索引的接口和空实现 | 130 | 70 |
| 宿主组装 | 登记两个方法、探测目录、空的索引接口；unsupported 清单删两行 | 25 | — |
| UNIT-commands-dedup | 假 agent 两个回合，各推一次同样的命令列表，会话只收到一次 | — | 50 |
| 合计 | | 385 | 240 |

草估约 625 行。按前几个任务的偏差（实现多 15–30%），推算约 **700–750 行**，贴着 750 的上限。按 brief，贴线也要开工前停下来汇报。

### 拆分建议

| 部分 | 内容 | 估计 |
|---|---|---|
| A（T063 的 IT-K14、IT-auth-none 依赖） | 命令列表进 attach 与按会话去重（UNIT-commands-dedup）、会话 actor 只在变化时写 `session.capabilities`、登录状态、能力缓存的写和读 | 约 400 |
| B | 探测与探测目录、`agents/capabilities`、`agents/refresh`、`AgentStatus` 与语义码映射、交给索引的接口 | 约 350 |

不拆的话，按 A、B 的顺序提交。
