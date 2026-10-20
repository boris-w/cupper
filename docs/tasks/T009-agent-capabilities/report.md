# T009 汇报

- 执行者：子 agent（工程师）
- 分支：`feat/T009-agent-capabilities`，在原 T052 的 worktree 里从 main 开出
- 完成日期：2026-10-20

## 做了什么

T009 已完成，A、B 两部分按先 A 后 B 的顺序提交，`cargo xtask ci pr --record` 在干净的 worktree 里通过，摘要已提交。代码共 +883 −62 行（不含 notes 和 CI 摘要），比你放宽后的约 850 行多约 4%，在 900 的硬线以内；超出的原因见"关键取舍"末条。

**A：命令列表、`session.capabilities`、能力缓存（+262 −16）**
- **命令列表去重**：`AgentRuntime` 按会话记住上一份命令列表的 sha-256 摘要，内容没变就不发 `CommandsChanged`（设计 5.6.2）。会话打开期间收到的列表只记下来、不发事件。
- **attach 的回复带上命令列表**：用 agent 级的最近一份，不等本会话自己的列表。第一个会话时它可能还是空的，之后的列表经 `CommandsChanged` 补上。
- **`session.capabilities` 只在变化时写**：会话 actor 收到 `CommandsChanged` 时，命令列表和已写的不同才写一条新的。
- **能力缓存**：新增 `driver/runtime/cache.rs`。attach 成功、命令列表变化、探测完成时写 `agent_capabilities`；宿主启动时删掉适配器或 CLI 版本与当前不符的行。

**B：探测、状态、`agents/*`（+621 −46）**
- **探测 `probe()`**：
  - 带回复，按裁定删掉了没有回复通道的 `DriverCommand::Probe`；
  - 每次先把 `<数据目录>/probe/claude/` 删掉重建（0700），按已信任在里面打开会话；
  - 等命令列表最多 200 ms，时长登记为 `probe_commands`，计时经 `Clock`；
  - 写进缓存并等写入提交后才回复，随后发 `session/close`。
- **登录状态**：从 `_auth/status_update` 只取 kind 和套餐名（K16）；prompt 以 `not_logged_in` 失败时改为 `logged_out`。
- **`AgentStatus` 与语义码映射**（`driver/runtime/status.rs`）：
  - 没找到 CLI：`state: unavailable`，不带 `cli`；
  - 运行时核对失败：`adapter.runtime: failed`；运行时目录缺文件：`missing`；
  - 版本分类放在 `cli.compat`；
  - CLI 的安装方式、配置目录的状态现在保存在 `Launch` 里。
- **交给索引**：实例开始、结束、登录状态变化、重新查找时，经 `AgentNotes` 交给索引；T034 合并前用空实现 `NoAgentNotes`。
- **两个方法的端口**（`driver/runtime/agents.rs`）：
  - `agents/capabilities` 读缓存；
  - `agents/refresh` 重跑登录 shell、定位和版本检查，作废旧版本的缓存，换掉下次拉起用的 `Launch`，缓存为空且 CLI 可用时探测一次，最后返回状态；
  - Codex 在 M1a 没有运行时，两个方法都回 `unavailable`。
- **宿主组装**：登记这两个方法；分派测试的夹具照宿主的做法登记它们，unsupported 清单删了两行。

## 关键取舍

- **attach 不等命令列表**（按你认可的第 2 条）：命令列表经去重后的 `CommandsChanged` 在 attach 之后补上；只有探测按 5.6.7 等 200 ms。
- **发现链抽成可复用的 `Finder`**：`discover` 原来拿着一个 `rusqlite::Connection` 跨 `await`，这样的 future 不是 `Send`，网关的回复放不进去。我把它改成先读项目根目录再发现，并把 `claude_launch` 改成可复用的 `Finder`，供启动和 `agents/refresh` 共用（+43 −26）。B 比估计多出的行数主要在这里。
- **压缩过测试**：为了收在硬线内，压缩了状态映射的单测和探测的等待代码。

## 验证方式

- 在干净的 worktree 里于 `dd1cfff` 跑 `cargo xtask ci pr --record docs/tasks/T009-agent-capabilities/ci/`，全部通过。摘要是 `pr-dd1cfff255c1.json`，提交为 `cfe36d0`。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,580 通过 / 0 失败 |
  | protocol | 185 / 0 |
  | ts | 353 / 0 |
  | license | 1 / 0 |
  | testdata-scan | 3 / 0，`sensitive-words` 跳过 |
  | ui、e2e | 还没实现 |

  第一次跑 CI 时撞上本机磁盘满，失败了；我删掉本 worktree 的增量编译缓存后重跑，通过。这次失败与代码无关。
- 对照验收标准：
  - **UNIT-commands-dedup**：pass。有两个用例：
    - 运行时一级，用假 agent：同样的列表在每个回合开始重复推送，只到达会话一次；换成另一份列表时发出；原来的列表回来时也发出；缓存跟着更新，启动时删掉了旧版本的行。
    - 会话一级：`session.capabilities` 只在列表变化时写。
  - **能力缓存、`agents/capabilities`、`agents/refresh`**：一个用例覆盖，用假 agent。
    - 缓存为空时，`capabilities` 返回空；`refresh` 之后，探测目录被清空且权限为 0700；
    - 缓存里有 5 条命令，`steering.supported` 为真；
    - 状态为 `running`、`logged_in`、运行时 `ready`；
    - Codex 回 `unavailable`。
  - **状态映射**：另有单测，覆盖没找到 CLI、运行时核对失败、两种登录状态。
- 宿主的 clippy 在四种特性组合下都通过（全部、无、`dev-auth`、`test-harness`）。
- 测试一律用假 agent，没有运行真实 CLI。

## 提交列表

```
cfe36d0 docs(task): record the T009 pr CI summary
dd1cfff docs(task): record the T009 line counts and choices
cb98129 feat(host): probe agents, report their status, serve agents/*
8483ca5 feat(host): send command lists once per change and cache capabilities
302a515 docs(task): record the T009 checks, size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **给 T034、T100**：`AgentNotes::agent(AgentStatus)` 是索引接 `agents` 的入口。现在宿主组装时传的是 `NoAgentNotes`，换成索引的实现即可。
2. **给 T008**：
   - `AgentStatus` 的 `rss_bytes` 留空，`crashed` 状态也还没有；
   - 与 T008 在 `runtime/mod.rs` 上的冲突，按你说的由你合并时处理。
3. **给 T044**：`ConfigChanged` 写进 `session.capabilities` 的部分没有动，仍在 `SetConfig` 那一块。
4. **磁盘**：磁盘满时我删了本 worktree 的 `target/debug/incremental`（约 16 GB 的增量编译缓存），之后重新编译生成，不影响结果。worktree 本身已保留。