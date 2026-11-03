# T111 汇报

- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`feat/T111-codex-runtime`（worktree T054），头提交 `cf1c557`
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/41
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41581455721 。**交接时 8 个检查都还是 pending**，结果请编排者在 PR 上核对。我这边的 CI 监视在交接时一并结束了。

## 做了什么

1. **运行时按 agent 分派（搬动，单独一个提交）**：新增 `driver/runtime/agent.rs`。`Agent` 带上每种 agent 不同的部分：CLI 布局、环境变量名、已验证范围、适配器版本、错误分类器、能力、配置项。`Extractor` 包住 `ClaudeSession` 和 `CodexSession`。`launch`、`status`、`cache`、`agents`、宿主装配都改为按 agent 走，宿主给每种 agent 起一个 `AgentRuntime`。
   - `Wire::Opened`、`Wire::Configured`、`Wire::Prompted` 里写死的 `claude::agent_error` 换成了按 agent 的分类器。
   - T218 的判断仍在分类器之后，Codex 同样生效。
2. **Codex 的 CLI 发现**：新增 `resolve::CODEX`，覆盖 Homebrew cask，以及 npm 的 `bin/codex.js` 解析到平台包里的原生二进制（嵌套、提升两种位置，0.159 及以前在包内的 `vendor/`），解决 K15。`CODEX_PATH` 设为原生二进制，`CODEX_HOME` 照 `CLAUDE_CONFIG_DIR` 的规则校验。
3. **codex-acp 的运行时**：
   - 清单加 `codex_acp`，安装器多一个 `CodexAcp` 组件，按内嵌清单逐文件核对哈希后，从包内复制 `index.mjs` 和 LICENSE。
   - 每种 agent 的运行时状态只算它需要的组件。索引按 agent 记运行时状态，设置变更时两种 agent 都重新发现。
   - Claude 专用的 `_meta` 和 Q43 告警只用于 Claude。
4. **未信任的项目**：
   - `session/create` 原本就由 `policy::attach` 拒绝；现在 Codex 注册进了会话管理器，这里返回 `unavailable/project_untrusted`。
   - 另外运行时收到 Codex 的 `Ignore` attach 时直接拒绝，不拉起适配器。codex-acp 一建会话就会拉起项目里的 MCP，所以这一层必须挡住。
   - 新增 `AgentErrorClass::ProjectUntrusted`，以及 `FailureCode` 的 `project_untrusted`。这是附加改动，`pnpm gen` 和 schema-compat 都通过（"0 breaking, 1 additive"）。
5. **RunState**：Codex 的 `threadStatus` 经运行时到达会话，`AutonomousJudge` 原本就会消费它。用例断言了 active、idle 两个状态都能送到会话。
6. **回收（5.6.8、K20）**：
   - 每 30 s 采样时检查 Codex 适配器是否空闲：没有在途的 prompt 或 session/new、resume，没有待答的请求，没有线程在工作，代理 10 s 内没有输出。空闲且满足以下任一条件就回收：超过 2 GiB，30 分钟没有工作，或探测后已经没有会话。
   - 回收沿用宿主关闭的顺序。会话收到 `AdapterExited { reclaimed: true }`：回合机不写 `agent_restarted`（除非有回合被打断），下一条消息 `session/resume`。
   - 回收期间到来的 attach 等新实例；回收期间到来的 prompt 以 "Session not found" 失败，由会话 resume 后重发（K6）。
7. **`agents/restart`**：走路由和账本。运行时 `restart()` 取代了 `clear_crashed()`：清掉失败计数，按关闭顺序停掉正在运行的适配器（不算一次失败），会话重新 attach。
8. **版本探测重试**（T066 的交接）：`--version` 超时后用 30 s 再试一次，两种 agent 都适用。
9. **测试**：
   - UNIT-cli-resolver-codex。
   - IT-codex-untrusted：运行时层一个；进程级一个，还走完了 Codex 会话的创建、回合结束和 `agents/restart` 后继续发消息。
   - IT-K20-reclaim：运行时层两个（超过 2 GiB；30 分钟没有工作），会话层一个（回收不写通知，下一条消息 resume）。
   - IT-adapter-restart 的"被切断的 prompt"现在两种 agent 都跑，守住 T218 对 Codex 的规则；同一组用例里加了 restart 的场景。
   - 编号都已登记，没有新增。

## 关键取舍

- **回收的判断放在运行时自己的采样里**，没有另起 actor 消费 `resources()`：在途请求、待答请求和线程状态都只有运行时知道。代价是回收最多晚 30 s。
- **回收复用关闭流程**，会话事件用 `AdapterExited` 加一个 `reclaimed` 标志，没有另立事件：有回合和回收撞上时，仍按适配器退出处理，不会让回合挂住。
- **"没有进行中的回合"由运行时近似判断**（在途请求、线程状态、10 s 内的输出）。取消后仍在运行、又超过 10 s 不出声的 Codex 命令（K3），遇到超过 2 GiB 时会被回收掉，这是已知的局限。
- **为 `project_untrusted` 加了 `FailureCode`**，没有用 `other`：设计里这一路就是 `unavailable(project_untrusted)`，回合的 `error_code` 应当说清原因。
- **测试进程不跑本机的 codex**：进程内启动宿主的两处（`supervisor` 的 actor 种类用例、`gold_b`）给 Codex 也注入了程序；`in_process` 改为对所有 agent 生效。否则 Codex 的发现会在用户自己的 `HOME` 下找到 `/usr/local/bin/codex` 并运行 `--version`。

## 验证方式

- 本机单份运行：`cargo test -p cupper-host --features test-harness --tests` 全部通过（unit 461；integration 的 crash_concurrency、driver_k、gateway_sync、gold_b、interaction_trust、lifecycle、migrations、startup 全过）。
  - lifecycle 里两个 CLI 用例需要先 `cargo build -p cupper-cli --features test-harness`，与 CI 的 cli-e2e 一步一致。
- 其他检查都通过：`cargo clippy --workspace --all-targets --all-features -D warnings`，cupper-host 的无特性、dev-auth、test-harness 三套 clippy；`cargo fmt --check`、prettier；`cargo test -p cupper-protocol`、`pnpm --dir packages/protocol run test`、schema-compat、`trace-ids check`。
- 全程没有运行真实的 Codex CLI 或 Claude Code，没有读写 `~/.codex`、`~/.claude`。只用 curl 默认 UA 读过 jsdelivr 上 `@openai/codex@0.163.0/bin/codex.js` 的公开源码，用来确认二进制的位置。
- 验收标准：
  - 三个用例都通过；Codex 会话能经假 agent 走完创建、发送、回合结束和回收：✓
  - T218 的规则对 Codex 生效，有用例：✓
  - PR 上 8 个检查：交接时都还在 pending，待确认。

## 规模

- `cargo xtask size` 毛计：+1016 −278。
- 搬动提交（`14df4ac`，+376 −168）按净增计 208，合计 1016 − 376 + 208 = **848**，在停止线 900 之内。
- `xtask size` 不认搬动提交，所以它会显示越过停止线 116，但仍在硬停止线 1100 之内。
- 各提交的明细在 `docs/tasks/T111-codex-runtime/notes.md`。

## 提交列表

```
cf1c557 docs(task): update the T111 notes
acc0ccd fix(host): resend a prompt that meets a restart
5c6dadf fix(host): hold the reclaim while the agent works on its own
ddbf68f docs(task): add the T111 notes
8127397 fix(host): hold the reclaim while a session opens
585645d test(host): expect codex-acp in the self-test's runtime report
c8f1adf feat(host): try a timed-out version probe once more
b6555c6 feat(host): serve agents/restart
dfb85e8 feat(host): reclaim Codex's idle adapter
ae438b5 feat(host): refuse Codex in projects that are not trusted
767a4fa feat(host): run Codex's adapter beside Claude's
a3ec2e6 feat(host): resolve Codex's CLI to its native binary
14df4ac refactor(host): run one agent runtime per agent
```

## 遗留问题 / 范围外发现

**设计修改建议**

- 5.3.11 / 5.6.2：`FailureCode`、`AgentError` 的分类加 `project_untrusted`。
- 5.6.2：`AdapterExited` 加 `reclaimed`。
- 5.6.8：写明以下实现细节：
  - 回收在采样时判断，所以最多晚 30 s；
  - "探测后回收"实现为"探测过、且没有会话时回收"；
  - `agents/restart` 不计入失败次数；
  - 回收或重启期间的 prompt 以 "Session not found" 失败后重发；
  - 代理 10 s 内有输出时不回收。
- 5.2.12：`--self-test` 的 `runtime` 字段多了 `codex-acp`。

**界面**：`stream` 命名空间没有 `error_project_untrusted` 文案，目前退回到"其他错误"，留给界面任务（T144 或 T151）。

**留给其他任务的事项**

- T102 交接 444（长时间运行下 MCP 进程数和内存，核对回收阈值和 NFR-11 的估算）要用真实 Codex，留给 T113 或维护者。
- T102 交接 445（K21 的 Codex 部分）：回收和关闭沿用"发信号前先枚举全部后代的进程组"，没有另写代码。
- T058 / T110 交接 207（空闲时 thread active 要不要挡住出队）：没有改，T110 已评估为暂不需要。
- 会话打开时 K14 的改判，在 T112。

**行为说明**：`agents/restart` 返回时适配器可能还在关闭。会话在收到 `AdapterExited` 之前、实例已经结束之后发出的 prompt，回合会以 `adapter_exited` 结束，与崩溃时相同。

**本机环境**：lifecycle 的两个 CLI 用例要先构建带 `test-harness` 的 `cupper` CLI，否则会失败，与本任务无关（见 notes）。

**worktree 已保留**，CI 有问题时可以接着修。
