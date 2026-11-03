# T111 汇报

- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`feat/T111-codex-runtime`（头提交 `cf1c557`）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/41
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41581455721 ，8 个检查全部通过（rust、ui、e2e、compat-framework、protocol、ts、license、testdata-scan）

## 做了什么

1. **运行时按 agent 分派**（单独的搬动提交）：新增 `driver/runtime/agent.rs`。`Agent`（Claude、Codex）集中了各 agent 不同的部分：CLI 布局、变量名、设置里的 CLI 路径、已验证的版本范围、适配器版本、错误分类器、能力和配置项映射。`Extractor` 把 `ClaudeSession` 和 `CodexSession` 包成同一个接口。`Launch` 带上 `agent`；`launch`、`status`、能力缓存、`adapters` 行和 `agents/*` 都不再写死 Claude。宿主为每种 agent 起一个 `AgentRuntime`，各有自己的 Finder、日志（`adapter-<agent>.log`）、探测目录和 `rss_samples`。`Wire::Opened`、`Wire::Configured`、`Wire::Prompted` 改用该 agent 自己的分类器。
2. **`CODEX_PATH` 解析**：新增 `resolve::CODEX`，覆盖 Homebrew cask，以及 npm 安装的 JS 启动器（K15）。启动器背后的原生二进制可能在三处：嵌套的平台包、提升到旁边的平台包，或包自身的 `vendor/`（0.159 及以前）。这三处是照 0.163.0 的 `bin/codex.js` 源码核对的：只读了 jsdelivr 上的公开文件，没有安装，也没有运行。`CODEX_HOME` 沿用已有的校验，变量名按 agent 取。
3. **codex-acp 从包内复制**：`plans()` 加了 codex-acp 的计划，`Resources/runtime/codex-acp-<版本>/` 按内嵌清单核对哈希后复制；`Manifest` 加了 `codex_acp`。`runtime::adapter(dir, plans, agent)` 取代了 `claude_adapter`。每个 agent 的运行时状态只算它自己需要的组件（`Component::of`），索引也按 agent 分别记录。设置变更时两个 agent 都会重新发现。
4. **未信任的项目**：Codex 收到 `ProjectConfigPolicy::Ignore` 的 attach 时直接拒绝，不拉起适配器。为此新增 `AgentErrorClass::ProjectUntrusted` 和 `FailureCode::ProjectUntrusted`（`project_untrusted`，附加式改动，schema-compat 判为 additive，已经 `pnpm gen`）。`session/create` 原有的 `unavailable/project_untrusted` 判断，在 Codex 注册之后开始生效。
5. **`RunState` 接入自发回合**：Codex 的 `threadStatus` 经 `CodexSession` 转成 `RunState` 发给会话，由 `AutonomousJudge` 消费。运行时也按会话记下线程是否在工作，回收时要用。
6. **回收**（5.6.8、K20）：每 30 s 采样时检查 Codex 的适配器能不能回收。
   - 先要空闲：没有在途的 prompt 或 `session/new`、`session/resume`，没有待应答的交互请求，没有线程在工作，没有会话正在打开，代理最近 10 s 没有输出。
   - 再满足三条之一：超过 2 GiB；30 分钟没有工作（新增时长 `codex_reclaim_idle`）；做过探测且已没有会话（ADR-09"探测后回收"）。
   - 回收走宿主关闭的顺序。会话收到 `AdapterExited { reclaimed: true }`：回合机不写 `agent_restarted` 通知（除非真有回合被切断），下一条消息 resume。
   - 回收期间来的 attach 等新实例；来的 prompt 以 "Session not found" 失败，由会话 resume 后重发（K6）。
7. **`agents/restart`**：接上路由和命令账本。`AgentRuntime::restart()` 取代 `clear_crashed()`：清掉失败记录，并按关闭顺序停掉正在运行的适配器。这次停止不算失败，所以不触发退避。
8. **T218 的规则对 Codex 生效**：IT-adapter-restart 的"切断的 prompt"用例改为两种 agent 都跑。
9. **版本探测重试**（T066 交接）：`--version` 超时后，用 30 s 再试一次（新增时长 `version_probe_retry`），两种 agent 都适用。

## 关键取舍

- **回收用 `AdapterExited` 加 `reclaimed` 字段，不另起事件**：回收和崩溃对会话的影响相同，都是变为 `Detached`、下次 resume。唯一的区别是空闲时不该写"agent 已重启"。用同一个事件，回收恰好和 prompt 撞上时也能正确结束回合。
- **回收条件在运行时内部判断**：在途请求、交互请求、线程状态都只有运行时看得到。代价是判断最多晚 30 s。"没有进行中的回合"用三点近似：没有在途 prompt、线程没在工作、10 s 内没有输出。已知局限：K3 取消后仍在运行、但 10 s 没有输出的命令，在超过 2 GiB 时会被回收掉。
- **Codex 的 `Ignore` 由运行时拒绝**：项目在会话运行中变为 stale 时，会话 actor 仍会以 `Ignore` 重新 attach，而 codex-acp 的 `session/new` 会拉起项目里的 MCP。所以必须在运行时这一层拦住，不能只靠 `session/create` 的判断。
- **进程内启动宿主的测试**（`supervisor` 的 actor 种类用例、`gold_b`）用的是用户自己的 `HOME`。Codex 的 Finder 如果走生产发现，就会找到本机的 codex 并运行 `--version`。所以前者给 Codex 也注入了假 agent；后者的 `in_process` 改为对所有 agent 生效。

## 验证方式

- 本机单份运行：`cargo test -p cupper-host --features test-harness --tests` 全部通过（单元 461 个，加各集成测试文件）；`cargo test -p cupper-protocol` 通过；`pnpm --dir packages/protocol run test` 通过；`cargo clippy --workspace --all-targets --all-features` 以及 host 的三个特性组合（无特性、dev-auth、test-harness）都没有警告；`cargo fmt --check`、prettier、`trace-ids check`、`schema-compat` 都通过。全程没有运行真实的 Codex 或 Claude CLI，也没有读写 `~/.codex`、`~/.claude`。
- 用例：
  - UNIT-cli-resolver-codex：`unit_cli_resolver_codex_install_layouts`。
  - IT-K20-reclaim：`it_k20_reclaim_over_2_gib_once_no_prompt_is_in_flight`（经注入点设定 3 GiB；在途 prompt 挡住第一次采样，并断言 `RunState` 先 active 后 idle、resume 到新实例）；`it_k20_reclaim_after_30_minutes_without_work`；会话层的 `it_k20_reclaim_an_idle_session_resumes_without_a_notice`。
  - IT-codex-untrusted：运行时层的 `it_codex_untrusted_an_attach_without_the_project_config_is_refused`；进程级的 `it_codex_untrusted_no_session_until_the_project_is_trusted`。后者对 codex 形态的假 agent 走完：未信任时拒绝、信任后创建会话、发送、回合正常结束、不带 Claude 的 `_meta`，然后经网关调 `agents/restart`、等到通知后再发消息、resume 成功。
  - T218 对 Codex：`it_adapter_restart_a_cut_off_prompt_ends_with_the_instance` 两种 agent 都跑。
  - 重启：`it_adapter_restart_backs_off_then_crashed` 加了三点：重启不触发退避；重启期间的 prompt 以 "Session not found" 失败；`crashed` 的 agent 重启后能再次启动。
  - 版本重试：`unit_version_classify_probes_a_fake_cli` 加了"第一次超时、第二次成功"。
- 验收：三个登记的用例都通过；Codex 会话经替身走完了创建、发送、回合结束、回收和 resume；T218 规则有用例守住；PR 上的 8 个检查全部通过。

## 规模

- `cargo xtask size` 的毛计：增 1016、删 278。按 brief 的裁定，搬动提交（14df4ac，+376 −168）按净增 208 计，合计 **848**，在停止线 900 之内。
- 工具本身不认搬动提交，所以会显示越过停止线 116，但仍在硬停止线 1100 之内。
- 各提交的行数在 `notes.md` 里。
- 用例约占 300 行，其中回收 105、CLI 布局 60、未信任 50、重启和 T218 30、版本重试 20。

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

1. **设计修改建议**：
   - 5.3.11 的 `FailureCode` 列表加 `project_untrusted`。
   - 5.6.2 的 `AdapterExited` 加 `reclaimed`，说明回收时不写 `agent_restarted`；`AgentError` 分类加 `project_untrusted`。
   - 5.6.8 写明：回收在采样时判断，最多晚 30 s；"空闲"还包括没有在途的 `session/new`、`session/resume` 和 10 s 内没有输出；"探测后回收"的实现是"做过探测且没有会话时，下一次采样回收"；回收或重启期间，attach 等新实例，prompt 以 "Session not found" 失败并重发；`agents/restart` 不计入失败、不退避。
2. **界面**：`stream` 里没有 `error_project_untrusted` 的文案，现在会显示"其他错误"，由界面任务补 i18n。新建会话页和设置页会看到 Codex 的状态（没装 CLI 时为 `unavailable`），由 T144、T151 处理。
3. **需要真实 Codex 的交接项没做**：T102 的"长时间运行下的进程数和内存，核对回收阈值和 NFR-11"，归 T113 或维护者手动。T102 的 K21（Codex 的工具命令和 MCP 不在适配器的进程组里）已经被通用的关闭顺序覆盖：发信号前先枚举全部后代的进程组，回收也走这条路径。
4. **其他交接项留给后续**：T058 的"空闲时 thread active 是否挡住出队"没做（T110 评估过，现在不需要）；K14 在会话打开时的改判归 T112；`initialize` 里的 notices、compaction 归 T113。
5. **`agents/restart` 的竞争**：会话还没收到 `AdapterExited` 时发出的 prompt，如果在实例结束之后才到运行时，会被丢弃，回合以 `adapter_exited` 结束，与崩溃时相同。这和设计里"界面先提示会影响所有会话"一致。
6. **本机复现时要注意**：lifecycle 里的 `it_cli_commands_*`、`it_elevate_lineage_*` 需要先 `cargo build -p cupper-cli --features test-harness`（CI 的 `cli-e2e` 一步会做），否则在本机会失败，与本任务无关。
7. **worktree 保留着**，等编排者合并后再清理。
