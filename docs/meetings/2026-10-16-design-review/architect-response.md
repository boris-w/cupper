# 设计评审第二轮：架构师的回应

- 日期：2026-10-16
- 回应人：架构师（代表 D01 原作者）
- 范围：
  - 第一轮全部"阻断"意见（4 条）和"重要"意见（45 条）；
  - 建议级意见怎样处理；
  - 评审者之间分歧和重叠的统一方案；
  - 新增的用户决定事项；
  - 修订工作量，以及契约冻结检查。

## 总体回应

**1. 结论概览**

- 4 条阻断全部接受。
- 45 条重要意见：
  - 接受 41 条；
  - 部分接受 4 条：桌面 #6、安全 #6、测试 #13、测试 #21；
  - 不同意 0 条。

**2. 不推翻任何 ADR**

要细化的 ADR 有 8 份：ADR-03、04、05、06、07、08、10、11。另外 ADR-02 的约束清单要加一条："时间只能通过 Clock 获取"。修订清单见最后一节。

**3. 影响最大的三件事**

- 宿主核心的"时钟抽象 + 纯核心 + 传输接口"必须在 M1a 的第一个宿主任务里定下来（测试 #1）。
- 新增两道信任边界，M1a 就要有：
  - 宿主自动调用 git 时用"安全档位"（安全 #1）；
  - 项目信任（安全 #2）。
- 有一批协议和 ShellAPI 的改动会改变现有契约的形状，必须在 M1a 冻结契约之前落地，清单在最后一节。照这个顺序做，就不违反"M1b 只加方法和事件"。

M1a 的范围因此大约多出 4 块工作：
- 核心骨架与测试基础设施；
- 项目信任；
- 主进程转发的测量闸门；
- 最简界面要补的元素。

**4. 设计评审时要定的 4 个待定问题**

| 问题 | 结论 |
|---|---|
| Q1 连接模型 | 定为"control 连接 + 每个窗口一条数据连接"。补上三条：<br>- 生命周期（桌面 #1）；<br>- `initialize` 只接受一次、角色不可变（安全 #4）；<br>- 主进程转发的测量提前到 M1a（见 e）。 |
| Q4 codex-acp 随包 | 随包。附带以下条件：<br>- CI 按 npm 的 SRI 校验上游文件；<br>- 清单记录 sha-256；<br>- 列出它内联的 22 个依赖的许可证声明；<br>- 每晚跑一次 `runtime-pack --verify`；<br>- test-harness 构建里可以注入 registry 地址。 |
| Q11 `threadStatus` | 有 prompt 在途时，以 prompt 的响应为准。补三条：<br>- 界面只看 `SessionStatus`；"需要处理"只由 cupper 自己的待处理请求决定（桌面）；<br>- `threadStatus` 变成 idle，不会让挂着的交互请求失效（安全）；<br>- 测试工程师列的 4 个用例照收。 |
| Q14 晚到的工具输出 | 见 (b)：不改原回合，在当前回合里写一个独立的 `late_tool_output` 条目。 |

**5. 需要转回提出方再确认一次的点**

- **安全**：
  - (a) 审查文档里不启用 `require-trusted-types-for`；
  - 安全 #6 的"写入位置"只能显示到 agent 透出的程度；
  - 窗口通过计划确认切到 auto，仍然走原生确认（比模式选择器更严，不改）。
- **桌面**：
  - (b) 原条目不改写，改由视图的反向索引在原条目旁显示链接；
  - 桌面 #8 由宿主判定"最终回答"。
- **测试**：
  - (c) 键盘仍然用 `before-input-event` 识别，e2e 改用主进程的 `sendInputEvent` 模拟按键；
  - 测试 #21 的金样例仍在 macOS 上跑。

**6. 本轮核实**

做法：从 npm 下载 tarball，在临时目录里只读阅读，不执行，结束后已删除。User-Agent 用 `cupper-research`。

- **`@agentclientprotocol/claude-agent-acp` 0.91.0，`dist/acp-agent.js`**：
  - `createSession` 先写 `settingSources: ["user","project","local"]`，再展开 `_meta.claudeCode.options`。所以客户端可以在 `session/new` 和 `session/resume` 里传入 `settingSources`。
  - `settingSources` 在 `OPTION_REBUILDS_SESSION` 里。resume 时如果这个值不同，会重建底层会话。
- **`@agentclientprotocol/codex-acp` 2.2.2，`dist/index.js`**：
  - `createSessionConfig` 在用户配置之后，对会话的 cwd 和附加目录一律写入 `projects[<路径>].trust_level = "trusted"`。这是会话级覆盖，不写盘（推断）。
  - `_codex/hooks/trust` 会通过 `configBatchWrite` 写 Codex 配置。
- **两个适配器读取的环境变量**：`CODEX_CONFIG`、`DEFAULT_AUTH_REQUEST`、`MODEL_PROVIDER`、`INITIAL_AGENT_MODE`、`DISABLE_MCP_CONFIG_FILTERING`、`APP_SERVER_LOGS`、`CLAUDE_AGENT_LOGS`、`CLAUDE_AGENT_ACP_EXPERIMENTAL_V2`、`CLAUDE_MODEL_CONFIG`、`IS_SANDBOX`、`CLAUDE_CONFIG_DIR`、`ANTHROPIC_BASE_URL` 等。用于安全 #9。
- **`@pierre/diffs` 1.5.2**：
  - `FileDiff.js`、`File.js` 等处有 15 处以上把字符串赋给 shadow root 的 `innerHTML` 或 `insertAdjacentHTML`；
  - `hostTheme.js` 等动态创建 `<style>`；
  - Worker 由调用方传入的 `workerFactory()` 创建；
  - 默认 `preferredHighlighter = "shiki-js"`，不需要 wasm；
  - 导出了产出 HAST 的 `DiffHunksRenderer`。
- **`electron` 44.8.0，`electron.d.ts`**：
  - `input-event` 的参数类型只有 `type` 和 `modifiers`；
  - `before-input-event` 的 `Input` 带 `key`、`isComposing`、`isAutoRepeat`；
  - macOS 上 `acceptFirstMouse` 默认为 false；
  - `powerMonitor` 有 darwin 平台的 `shutdown` 事件；
  - 有 `will-frame-navigate`；
  - preload 只在打开 `nodeIntegrationInSubFrames` 时才进入子框架。

## 逐条回应（阻断）

### 测试 #1：没有时间源抽象，并发模型用不了虚拟时间

**结论：接受。**

**理由**：宿主里有十几类时长规则。tokio 的 `time::pause` 只能用于 current_thread runtime，管不到 std 线程和 `spawn_blocking`（评审者已核实）。如果拆任务后才补，orchestrator、driver、store、supervisor 的接口都要返工。

**修改落点**

1. **`02-host.md` 新增 5.2.16"时间与可测性"**：
   - **`Clock` 接口**：
     - 提供 `now_mono()`、`now_wall_ms()`、`timer_at(deadline)`。
     - 生产用 `SystemClock`，测试用 `ManualClock`。
     - 所有 actor、normalizer（40 ms 合并）、IndexActor（100 ms 合并）、账本过期、保留清理、supervisor（空闲、退避、采样、回收）都只通过它取时间、定时。
     - clippy 的 `disallowed-methods` 禁止在 `clock` 模块之外调用 `Instant::now`、`SystemTime::now`、`tokio::time::{sleep, sleep_until, interval, timeout}`。
     - 写线程的 2 ms 攒批只影响吞吐，不受 Clock 控制；测试中关掉攒批。
   - **纯核心**：
     - `TurnMachine`、`QueueCore`、`InteractionBroker`、`AutonomousJudge`、`Coalescer` 都写成 `step(&mut self, input, now) -> Vec<Action>`，不做 I/O，也不 await。
     - `SessionActor` 只负责执行 Action，并把执行结果作为新的 input 回送。
     - 性质测试直接驱动纯核心。
   - **驱动的传输接口 `AcpTransport`**：
     - 有子进程 stdio 和内存双工两种实现；
     - 金样例的 B 层在进程内运行宿主核心，用录制里的 `t` 推进虚拟时钟。
   - **test-harness 构建**：
     - `--test-config` 里可以覆盖所有时长，可以选 `clock: manual`；
     - 提供仅测试可用的 `test/clock_advance {ms}` 和 `test/run_maintenance {task}`；
     - 这些只在 `test-harness` 特性下编译，scope 为 `test`。
2. **`02-host.md` 5.2.3**：规则里加第 6 条"时间只经 Clock"。
3. **`01-repo-build-ci.md` 5.1.7**：加上同一条约束。
4. **`09-testing.md` 5.9.1、5.9.8**：性质测试一律用 `ManualClock`。"中断后 10 s 内一定结束"在纯核心上用虚拟时间断言。
5. **`12-milestones.md` 5.12.2**：M1a 的第一个宿主任务是"核心骨架"，包括 Clock、纯核心框架、传输接口、故障点、测试配置通道。orchestrator、driver、store 的任务都依赖它。

**标注**：
- ADR：ADR-02 的约束清单加一条，写进 CLAUDE.md；
- PRD：无；
- 用户：无。

### 安全 #1：宿主自动发起的 git 会执行仓库配置指定的程序

**结论：接受。**

**理由**：宿主运行在所有 agent 沙箱之外。而 `core.fsmonitor`、hooks、filter、diff 驱动这些配置，都在 agent 可写的位置（githooks 文档写明，`worktree add` 之后会运行 post-checkout）。这条路径让 agent 不经审批，就能借宿主执行代码。

**修改落点**

1. **`02-host.md` 5.2.10 的"调用约定"拆成两种档位**：
   - **自动档**：宿主自己发起的命令，包括 status、快照的 `add -A` 和 `write-tree`、`update-index --refresh`、`worktree add/lock/unlock/remove`、`ls-files`、`diff-tree`、`diff`、`apply`。
     - 固定加 `-c core.fsmonitor=false`，以及 `-c core.hooksPath=<数据目录>/git/empty-hooks`（一个空目录，权限 0500）。
     - 加 `--no-pager` 和 `GIT_PAGER=cat`；diff 一族统一加 `--no-ext-diff --no-textconv`。
     - 会话 Attach 时、每个回合开始时，读一次 `git config -z --list --show-origin --includes`。对"可执行类键"做白名单比对：`filter.*.{clean,smudge,process}`、`diff.*.{command,textconv}`、`merge.*.driver`、`core.{sshCommand,pager,editor,askPass}`、`gpg.*program`、`credential.*`、`uploadpack.*`、`remote.*.{receivepack,uploadpack}` 等。
     - 白名单只放 git-lfs 的标准取值。其他值都用 `-c` 覆盖成空值。
     - 覆盖是否真的生效，用下文的"恶意配置仓库"样例逐键验证（Q32）。验证不了的键，所在仓库直接跳过快照，写 `snapshot.recorded{status: skipped, reason: unsafe_repo_config}`。
   - **用户档**：只用于用户在界面上点"提交"或"推送"。保留 hooks、签名和凭据助手，与在终端里执行一致。按钮旁注明"将运行仓库的 hooks"。
2. **回合开始和结束时，随快照计算"仓库可执行配置指纹"**：
   - 包括 `.git/config` 及其 include、`.git/hooks` 和 `core.hooksPath` 指向目录下的文件、工作区和 `.git/info/attributes` 里的 attributes 文件。
   - 指纹变化时，在回合里写 `notice{code: repo_config_changed, params: {paths}}`。审查面板顶部醒目提示；提交按钮的确认文案也带上这一条。
3. **`08-security.md`**：
   - 5.8.5 的 git 行按两档改写；
   - 5.8.7 写明：用户点"提交"时运行 hooks 的风险，与在终端里提交相同。
4. **`09-testing.md` 5.9.8**：加"恶意配置仓库"样例。每类可执行键都指向一个会写标记文件的脚本，跑完全部自动档命令后，断言标记文件不存在。
5. **生效时间**：`GitCmd` 从宿主第一次调用 git 起就分这两档。M1a 如果已经拍回合快照，这条随快照进 M1a。

**标注**：
- ADR-10 修订：调用约定加两档和指纹；
- PRD：无；
- 用户：无。

### 安全 #2：缺少项目信任环节

**结论：接受，M1a 就落地。产品步骤需要用户确认（U-R1）。**

**理由**：本轮已核实两点：
- claude-agent-acp 0.91.0 默认加载 project 和 local 两级配置，SDK 会话不弹信任框；
- codex-acp 2.2.2 在每个会话的配置里，强制把 cwd 设为 trusted。

cupper 取代了 CLI 的交互界面，也就要接手 CLI 的信任对话框。否则 clone 下来的恶意仓库，在第一个 prompt 之前，它的 hooks 就已经在沙箱外运行了。

**修改落点**

1. **`04-data-model.md` 5.4.2**：`projects` 表加 `trust_state`（untrusted、trusted、stale）、`trusted_at`、`trusted_by`、`trust_digest`。
2. **`03-protocol-v1.md`**：
   - 5.3.4 新增 `project/trust {command_id, project_id, findings_digest}`。scope 为 privileged，只能经原生确认。
   - `project/add` 的结果加 `trust: {state, findings}`。
   - `project.upserted` 加 `trust_state`。
   - `policy/describe` 能描述这个操作。
   - 新增 `unavailable` 的 reason：`project_untrusted`。
3. **`02-host.md` 5.2.6 新增"项目信任"小节**：
   - **扫描对象**是会话实际的工作区，内容包括：
     - `.claude/settings.json` 和 `.claude/settings.local.json` 里的 hooks、env、apiKeyHelper、statusLine；
     - `.mcp.json`；
     - `.claude/` 下的 skills、agents、commands；
     - `.codex/` 下的配置和 hooks。
     - 扫描结果作为 findings，并算出摘要。
   - **未信任时**：
     - Claude：在 `session/new` 和 `session/resume` 里通过 `_meta.claudeCode.options.settingSources: ["user"]` 启动，`session.configured` 标 `project_config: ignored`；
     - Codex：拒绝启动（`project_untrusted`），因为 codex-acp 没法关掉项目配置；
     - 只继承用户级配置，不影响 AG-6。
   - **信任之后**：每次 Attach 前、每个回合结束时重新扫描。摘要变了就转为 `stale`：暂停队列，下一个 prompt 之前要求重新确认，原生确认框列出差异。
   - **能力探测目录** `<数据目录>/probe/<agent>/` 由宿主创建，始终为空，视为已信任。
4. **`07-desktop.md`**：
   - 添加项目的整个流程（选目录、`project/add`、信任确认）由主进程在一次 `requestPrivileged({kind: 'add_project'})` 里完成；
   - 会话页显示"未信任：项目配置未加载"。
5. **`08-security.md`**：
   - 5.8.3 的权限矩阵加 `project/trust` 一行：只有 UI control 经原生确认可以执行，另外 test_harness 也可以；
   - 5.8.7 写明残余风险：会话进行中项目配置被改写时，CLI 可能在回合内就加载它，cupper 只能在回合结束时发现（CLI 是否热加载，未核实，记为 Q28）。
6. **`05-flows.md` 5.5.2**：加"添加项目与信任"分支。
7. **兼容剧本**：加一条断言："`settingSources` 为 user 时，项目 hooks 不运行"。

**标注**：
- ADR-05 修订：在 SEC-8 之外新增"项目信任"边界；
- ADR-08 细化：驱动层负责传入 `settingSources`；
- PRD：PS-2 增加信任步骤；
- 用户：**需要决定（U-R1）**。

### 安全 #3：`elevated` 档位没有纳入审批授权

**结论：接受。其中手机的部分交给用户决定（U-R2，M3 生效）。**

**理由**：auto 和 Auto review 都是让分类器代替用户审批。一个只拿到 write 的 CLI 能切到这两种模式，就等于拿到了 approve。另外，同样是切到 auto，走计划确认要 privileged，走模式选择器却只算普通值，前后不一致。

**修改落点**

1. **`06-agent-driver.md` 5.6.5**：逐值写明风险档位。

   | agent | normal | elevated | full_access |
   |---|---|---|---|
   | Claude | `default`、`plan` | `acceptEdits`、`auto` | `bypassPermissions` |
   | Codex | `read-only`、`workspace-write`（取值以兼容矩阵为准，Q25） | `agent`（Auto review） | `agent-full-access` |

   表外的取值一律按 full_access 处理。档位的顺序是 normal < elevated < full_access。
2. **`08-security.md` 5.8.3 的权限矩阵**：`session/set_config` 和 `session/create.config` 按"目标档位是否比当前更宽松"拆成三行。

   | 目标档位 | UI window | CLI | remote_device |
   |---|---|---|---|
   | 收紧或同档 | 需要 write | 需要 write | 需要 write |
   | 放宽到 elevated | ✓（U6），界面加提示 | 需要 write + approve | 按 U-R2，建议 ✗ |
   | 放宽到 full_access | 只能经原生确认 | ✗ | ✗ |

   `interaction/respond` 选 `mode_switch` 类选项时，按 `mode_after` 的档位走同样的规则。`allow_always` 类选项在 UI 上仍然走原生确认，ADR-03 不改。
3. **`02-host.md` 5.2.6 配置项**：
   - Attach、resume 之后，以及收到 `ConfigChanged` 时，都核对实际档位。
   - 出现以下两种情况时，暂停发送（队列暂停原因为 `config_unconfirmed`），写 `session.configured{confirmation_required: true}` 并记审计：
     - 实际是 full_access，但会话没有"经原生确认"的记录（`sessions.full_access_confirmed_at`）；
     - 实际是 elevated，但最近一次设置它的 principal 没有相应权限。
   - 用户经原生确认后继续，或者选择降档。
   - 新建会话时，如果 agent 的默认模式（来自用户自己的 CLI 配置）就是 full_access，第一个 prompt 之前同样要原生确认。
4. **`03-protocol-v1.md`**：
   - `session.configured` 加 `confirmation_required?`；
   - 新增错误 `conflict/config_unconfirmed`；
   - ShellAPI 的 `PrivilegedAction` 加 `confirm_config`。

**标注**：
- ADR-05 修订：风险档位纳入 scope 规则；
- PRD：SEC-9 对 elevated 的解释；
- 用户：**需要决定（U-R2）**。

## 逐条回应（重要）

### 桌面客户端工程师（#1–#15）

| 编号 | 结论 | 理由 | 修改落点 | ADR/PRD/用户 |
|---|---|---|---|---|
| 1 HostPort 生命周期 | 接受 | 渲染进程重载或崩溃后，旧订阅没人消费，主进程的缓存会无限增长；渲染进程也拿不到握手结果 | `07` 5.7.1 新增"数据连接的生命周期"：<br>- 连接绑定到文档实例。主框架发生非同文档的 `did-start-navigation`，或者 `render-process-gone`、`closed` 时关闭连接；<br>- `open()` 改为异步，返回握手结果；<br>- 重连时先连 control，再连 window；<br>- 关掉最后一个窗口时保留 control 连接，Dock 重新打开时新建窗口连接；<br>- 收到 `powerMonitor` 的 `shutdown` 时跳过 NFR-8 的确认，由宿主自己处理 SIGTERM；<br>- 重发规则见 (c)。 | 无 / 无 / 无 |
| 2 流控依赖动画帧 | 接受 | 窗口被遮挡后 rAF 停止，会频繁重置流 | `07` 5.7.1、5.7.6：<br>- 消息一到就应用到 store，应用后立即确认；<br>- 只有组件的重新渲染合并到 rAF；<br>- `backgroundThrottling` 保持默认值，并写明原因。<br>`09` SP-6：加"窗口被遮挡、流式输出 10 分钟，切回后没有 `stream/reset`"。 | 无 |
| 3 订阅策略 | 接受 | 订阅策略决定内存、流控预算和切换会话的延迟 | `07` 5.7.6：<br>- 索引流常驻；<br>- 当前会话加最近 3 个会话（LRU）保持订阅；<br>- 其余会话退订，并丢弃视图。<br>`09` 5.9.6：加预算"切换会话到首屏可见 p95 ≤ 150 ms，命中缓存时 ≤ 1 帧"。 | 无 |
| 4 ShellAPI 不完整 | 接受（6 点全部） | `ui-core` 和 `apps/desktop` 由不同任务实现，接口不全两边都会返工 | `07` 5.7.3 重写：<br>- `init(): Promise<ShellInfo>` 一次取回静态信息，变化的值走订阅，不用 `sendSync`；<br>- `host.open()` 返回 `{port, handshake}`；<br>- `HostPort.sendBlob(hash, offset, last, bytes)`，主进程只放行 0x02 帧，并按连接限额；<br>- 新增 `onCommand`、`onNavigate`；<br>- `requestPrivileged` 改为返回 `{status, result?}`；<br>- 权限处理器放行 `clipboard-sanitized-write`，拒绝 `clipboard-read`；<br>- M1b 加 `pathForFile`。 | 无 |
| 5 Esc 被拦 | 接受 | AP-1 要求按 Esc 拒绝 | 合并到 (c)：Esc 加入白名单，并且只能用于拒绝类选项。 | 无 |
| 6 CSP 与组件不兼容 | 部分接受 | 问题属实（本轮也核实了 pierre 的写法）。但不采用 `default` 策略 | 统一方案见 (a)：<br>- 主文档禁止 `default` 策略；<br>- 审查的 diff 视图放进单独源的文档；<br>- streamdown 的配置写成明确要求；<br>- M1a 加一条验收："发布 CSP 下 `securitypolicyviolation` 为 0"。 | ADR-03 细化 |
| 7 快照里的事件没有时间 | 接受 | 快照由视图生成，时间放在信封上会丢 | `03` 5.3.5：<br>- `turn.started` 加 `started_at`；<br>- `item.started` 加 `started_at`；<br>- `item.completed` 加 `ended_at`；<br>- 视图生成快照的 op 时带上这些字段。 | ADR-04 细化 |
| 8 回合折叠缺语义 | 接受 | 判定由宿主做，才符合瘦客户端原则 | `03` 5.3.5：<br>- `turn.completed` 加 `final_item_ids`。宿主在回合结束时判定：回合中最后一个非消息条目之后的 `agent_message` 算最终回答；Codex 如果能拿到 phase，以 phase 为准（是否透出，未核实，记为 Q31）；<br>- `snapshot.recorded{phase: end}` 加 `stat {files, added, removed}`。<br>`07` 5.7.4 写明折叠规则：<br>- 运行中展开；<br>- 结束后收起过程；<br>- 失败或取消时不收起。 | ADR-04 细化 |
| 9 工具摘要字段不全 | 接受 | 退回英文的 `title` 会失去本地化；子代理的工具调用平铺会被金样例固化 | `03` 5.3.5：<br>- `ToolCall` 加 `query?`、`url?`、`parent_item_id?`；<br>- 加"按 `tool_kind` 列出必填字段"表（提取层和金样例都以它为准）；<br>- 分组规则：同一回合内连续的 `tool_call` 为一组，`thought` 不打断、在组内显示，其他条目都打断。 | 无 |
| 10 命令输出的流式语义 | 接受 | 每次都发完整输出，流量是 O(n²) | `03` 5.3.5：<br>- `item.delta` 加 `field: text\|output`，按 (item_id, field) 合并；<br>- 输出超过 16,000 字符后停发 delta，改为每秒至多一次 `item.updated{output_tail, output_bytes, output_state: truncated}`；<br>- 结束时发 blob 引用和尾部预览；<br>- Claude 的输出在结束时一次给出。 | ADR-04、ADR-06 细化 |
| 11 大回合取不回来 | 接受 | PS-8 是 P0 | `03` 5.3.4：<br>- `history/page` 改为 `{session_id, cursor?, max_bytes?}`，返回 `{ops, next?: {turn_no, op_index?}}`。回合之间向前翻，回合之内按 op 顺序向后取；<br>- 视图生成的快照和回合快照都按 delta 上限切分长文本，保证单个 op 不超过帧预算；<br>- 进行中的回合在 `stream/snapshot` 里照常分页。<br>`07`：没取完的回合显示占位。 | ADR-04 细化 |
| 12 按回合虚拟化不够用 | 接受 | 长回合会突破滚动预算 | `07` 5.7.5、5.7.6：<br>- 改为按行虚拟化（回合标题、活动组摘要、消息、卡片拍平成一个行列表）；<br>- 向前插入历史时视口不跳动；<br>- 流式输出时贴底，用户上滚后停止贴底并显示 ↓。<br>M1a 的消息流就按这个结构写。 | 无 |
| 13 M1a 最简界面 | 接受 | NFR-1 的用例要断言这些界面元素 | 见 (e)，`12` 5.12.2 列出 M1a 要加的元素。 | PRD 口径不变 |
| 14 SP-6 缺口 | 接受 | 主进程的方案要在 M1a 定，不能等到 M1b | 见 (e)：主进程转发的测量前移到 M1a 的 windowBridge 任务。<br>`09` 5.9.7：<br>- 夹具按 SP-1 的分布生成，另加一个 2,000 个条目的单回合；<br>- delta 里带时间戳，测"输出到屏幕"的延迟；<br>- 补 4 个场景：切换会话、窗口被遮挡、向上翻页时的滚动稳定、5,000 行 diff；<br>- 以作者的机器为准，CI 只看趋势（与测试 #20 合并）。 | ADR-03 细化（测量时机） |
| 15 安装空窗 | 接受 | 源码核实过这段空窗存在 | `10` 5.10.5 和 `05` 5.5.7：<br>- 空窗期间 `hostLauncher` 不自动拉起宿主，界面显示"正在安装更新"；<br>- Squirrel 报错时重新拉起宿主并提示；<br>- 走 `before-quit-for-update` 时跳过 NFR-8 的检查；<br>- M2 的 LaunchAgent 要先停掉 KeepAlive；<br>- 首次启动时用 `isInApplicationsFolder` 检查安装位置，不在"应用程序"里就提示移动。 | 无 |

### 安全工程师（#4–#13）

| 编号 | 结论 | 理由 | 修改落点 | ADR/PRD/用户 |
|---|---|---|---|---|
| 4 重复 `initialize` 与角色提升 | 接受 | 被注入的渲染进程可以把 window 升成 control | `02` 5.2.4、`03` 5.3.2：<br>- 每条连接只接受一次 `initialize`，再收到就报错并断开；<br>- 角色确定后不可变；<br>- 只有 desktop_ui 能声明角色。<br>`07` 5.7.1、5.7.8：<br>- window 连接的 `initialize` 由主进程发，然后才把端口交给渲染进程；<br>- 出方向禁止 `initialize`、拒绝批量数组；<br>- 转发重新序列化后的对象；<br>- 只放行 0x01、0x02 帧。<br>`08` 5.8.3：矩阵改为"仅由主进程代发"。 | ADR-04、ADR-05 细化 |
| 5 未知的选项 kind 被改写成拒绝 | 接受 | 用户可能点"拒绝"，实际却选中了持久放行 | `03` 5.3.6 兜底表加一行：未知 kind 的 `effect.scope: unknown`、`persistent: true`、`requires: privileged`，`actionable_by` 只有 desktop_ui。<br>`06` 5.6.10：改写只为了让 SDK 能解析，下发给客户端的永远是原值。<br>`07`：<br>- 显示原文并标"效果未知"；<br>- 不套用拒绝的样式；<br>- 不响应 Enter 和 Esc。<br>另加对应的金样例。 | ADR-07 细化 |
| 6 截断与不可见字符 | 部分接受 | 前两项接受。"确切规则和写入位置"只能显示 agent 透出的部分：Claude 写入哪个 settings 文件由 CLI 决定，适配器没有透出 | `03` 5.3.1：<br>- 权限请求里的命令和路径不截断，超过内联上限就改用 blob 加预览；<br>- 卡片标"已截断"，展开后批准按钮才可用；<br>- 摘要覆盖完整内容。<br>`07`：渲染进程和原生确认框都把双向控制符、零宽字符、控制字符显示为转义（两处共用一份工具）。<br>持久选项的确认框显示 agent 给的原文和宿主判定的效果；前缀是解释器类（如 bash、sh、python、node、env、sudo）时，宿主附加"等于允许任意命令"的警示。 | 无 |
| 7 窗口激活与 `requestPrivileged` | 接受 | 合并到 (c) | 见 (c)。<br>`02` 5.2.9：通知被点击后，宿主经 control 连接发 `ui/navigate`，并按 bundle id 激活 Cupper，不走 `cupper://` 这个任何应用都能注册的通用路由。 | ADR-03 细化 |
| 8 提权的限频与话术 | 接受 | 换一个可执行文件就能无限重发申请 | `08` 5.8.4、`03` 5.3.9：<br>- 非 UI 的申请，全局同一时间只允许 1 个未决；<br>- 10 分钟内被拒 2 次，静默 10 分钟（期间只计数）；<br>- 确认框先显示宿主核实过的信息，申请方自填的字段放在"申请方自述，未经核实"下面，限长并转义；<br>- 父进程链断在 launchd、或者进程没有终端时，标"无法确认来源"；<br>- approve 默认不勾选；<br>- CLI 的回答写审计。<br>血缘改为：适配器用 `POSIX_SPAWN_SETSID` 拉起、自成一个会话，只认适配器的会话、进程组和祖先链。这样 git、登录 shell、M2 的终端都不会被误判。<br>SP-4b 遗留第 7 条排进 M1a 驱动任务前的验证。 | ADR-05 修订 |
| 9 API key 变量名单与环境变量 | 接受，并扩充 | 名单如果只是 write 级，就能借它控制适配器 | `02` 5.2.11：设置表补上 `agents.env_passthrough`，scope 为 privileged。<br>`06` 5.6.6：硬性剔除以下变量：<br>- `NODE_*`、`DYLD_*`、`ELECTRON_*`、`CUPPER_*`；<br>- `CLAUDE_CODE_EXECUTABLE`、`CODEX_PATH`、`CODEX_HOME`、`CLAUDE_CONFIG_DIR`；<br>- 本轮核实到的适配器控制变量：`CODEX_CONFIG`、`DEFAULT_AUTH_REQUEST`、`MODEL_PROVIDER`、`INITIAL_AGENT_MODE`、`DISABLE_MCP_CONFIG_FILTERING`、`APP_SERVER_LOGS`、`CLAUDE_AGENT_LOGS`、`CLAUDE_AGENT_ACP_*`、`CLAUDE_MODEL_CONFIG`、`IS_SANDBOX`。<br>`runtime-pack` 从锁定的适配器产物里提取 `process.env` 读取点，出现未分类的新变量就失败。<br>登录 shell 取到的环境只放在内存里。 | ADR-05 细化 |
| 10 自托管 runner 与发布密钥 | 接受 | 见 (d1) | 见 (d1)。 | 无 |
| 11 许可证门禁不可靠 | 接受 | 打包之后包名就不存在了；扫描最终产物时，锁文件一定会命中 | `01` 5.1.6、`10` 5.10.7 分两层：<br>- 对每个 JS 入口，按 pnpm 锁文件和打包器的 metafile 求出生产依赖闭包；<br>- 对最终产物按内容指纹扫描，锁文件按哈希放进允许清单。<br>另加一个负面测试。 | ADR-11 修订 |
| 12 codesign 分支没有覆盖 | 接受 | 见 (d2) | 见 (d2)。 | 无 |
| 13 TCC 归属 | 接受 | agent 拿到辅助功能权限，就能替用户点原生确认框 | `13` Q21：SP-10 的范围加上"给适配器设置 disclaim"。<br>`10`：<br>- cupper 永远不申请辅助功能、自动化、屏幕录制权限；<br>- 两个 Info.plist 都不写这几项的用途说明，CI 断言（"没有用途说明时自动拒绝"，未核实）。<br>`08` 5.8.7 加两条：<br>- 授予 Cupper 的系统权限，agent 会继承；<br>- 合成的输入能操作确认框。 | ADR-05 细化 |

### 测试工程师（#2–#21）

| 编号 | 结论 | 理由 | 修改落点 | ADR/PRD/用户 |
|---|---|---|---|---|
| 2 e2e 的接线方式 | 接受 | 见 (d3) | 见 (d3)。 | 无 |
| 3 假 agent 的能力 | 接受 | 测不到选项映射和 K1、K6 等分支 | `09` 5.9.2，假 agent 分三部分：<br>- **启动配置**：persona（claude 或 codex 的选项 ID、能力、configOptions）、登录状态、故障注入；<br>- **剧本命令**：补上 `hang`（不响应取消）、`write_file`、`steer_hang`、`upstream_retry`、`echo_env`（回显 cwd 和 clientInfo）、`output_then_crash`；<br>- **原始行通道**：`raw <json>`，回放模式整体走它。<br>另加全局不变式：同一会话收到第二个在途 prompt 时立即报错退出。 | 无 |
| 4 规避清单的"回归"列 | 接受 | 这一列拆不出测试任务 | `06` 5.6.9：每条写成"测试层 + 用例编号"。<br>- K20 加资源采样的注入点；<br>- K26 在假 agent 侧断言 `clientInfo.name`；<br>- K23 写明"不测"和理由。 | 无 |
| 5 金样例两层与录制格式 | 接受 | 5.1.6 和 5.9.2 的说法矛盾 | `09` 5.9.3：<br>- A 层是提取层，全部录制都用；<br>- B 层是会话层，只用宿主驱动产生的录制，配合虚拟时钟；<br>- 录制格式写成规格（头行、版本、`recv_nonjson`、note、多实例分段），SP-1 的录制一次性转换过来；<br>- CI 检查每份录制都有对应的金样例。<br>M1a 早期由维护者用 Claude 产出第一批 B 层录制。<br>`01` 5.1.6 的错误写法随之改正。 | 无 |
| 6 规范化规则 | 接受 | 不改金样例会随机失败 | `09` 5.9.3：<br>- 连续的 delta 先折叠再比较；<br>- seq 重新编号；<br>- schema 里加 `x-id`、`x-time`、`x-path` 注解，规范化按注解进行；<br>- 摘要按出现顺序换成占位；<br>- 每行一个事件，键排序；<br>- CI 里设了 `UPDATE_GOLDEN` 就判失败。 | 无 |
| 7 适配器升级后的评审 | 接受 | 重新录制后金样例几乎全变，评审会流于形式 | `09` 5.9.5：<br>- 录制按版本目录保留；<br>- 新增 `tools/rec-shape-diff`，生成形态差异报告；<br>- CI 检查录制里出现的 `option_id` 和模式取值都在对照表里；<br>- 兼容剧本只断言结构，模型不配合时允许重试，结果可以标"不确定"。 | 无 |
| 8 compat runner | 接受 | 见 (d1) | 见 (d1)。 | 用户：U-R4 |
| 9 崩溃注入点 | 接受 | "状态一致"的承诺没有注入点验证 | `09` 5.9.8：test-harness 构建里加编译期门控的故障点，覆盖评审者列出的 7 处，外加"回合快照写入中"。<br>每个故障点都断言：<br>- `integrity_check` 通过；<br>- seq 连续；<br>- 崩溃前客户端看到的历史是崩溃后历史的前缀；<br>- 用同一个 `command_id` 重试，结果符合账本语义。 | 无 |
| 10 第一条消息可能丢失 | 接受 | 设计缺陷 | `02` 5.2.6、`04`：<br>- 第一条消息随受理事务写进 `sessions.initial_prompt_json`，第一个回合开始时在同一事务里清空；<br>- 启动恢复时，对停在 preparing 的会话重新准备工作区（已登记的 worktree 直接复用）；失败就写 `workspace.failed`，并把消息转成一条暂停中的排队消息。<br>另加对应的故障点用例。 | 无 |
| 11 迁移中途崩溃 | 接受 | 会重跑已经执行过的脚本 | `04` 5.4.6：每个脚本在自己的事务里同时更新 `schema_version`。<br>`02` 5.2.13：退出码加 6，表示迁移失败。<br>另加"两个脚本之间崩溃"的用例。 | 无 |
| 12 持久性与 `fullfsync` | 接受，并补充 | 按 SQLite 和 macOS 文档就能决定，不需要做 VM 断电实验 | `02` 5.2.8、`04` 5.4.2：<br>- 用 `synchronous=NORMAL` 加 `checkpoint_fullfsync=ON`；<br>- 写 `pending_external` 的那一批，单独用 `synchronous=FULL` 加 `fullfsync` 提交，然后才执行 git；<br>- CI 用"高负载写入中反复 kill -9、再做完整性检查"来测崩溃一致性。<br>**补充**：断电可能丢掉最近的事件，导致 seq 被重用。宿主记录 `kern.bootsessionuuid`，"异常退出后又重启过系统"时，把 `data_epoch` 加 1：<br>- `initialize` 的结果带 `data_epoch`；<br>- `stream/subscribe` 带上 epoch；<br>- epoch 不一致，或者 `after_seq` 大于宿主的 head，一律走快照。<br>Q15 关闭。 | ADR-06 修订 |
| 13 Playwright 键盘不触发 `before-input-event` | 部分接受 | 发现属实。但 Electron 44 的 `input-event` 拿不到是哪个键（见类型定义），而规则必须区分按键 | 见 (c)：<br>- 生产代码仍用 `before-input-event` 识别键盘；<br>- e2e 用主进程的 `sendInputEvent` 发键；<br>- 负面用例排除前一次输入的影响；<br>- 在 Electron 44 上复测（Q26），并用实体键盘人工确认一次。 | ADR-03 细化 |
| 14 测试代码会不会进发布构建 | 接受 | 见 (d2) | 见 (d2)。 | 无 |
| 15 发布构建独有的路径 | 接受 | 见 (d2)、(d3) | 见 (d2)，以及 dev_key 的双向证明（d3）。 | 无 |
| 16 NFR-1 用例不稳 | 接受 | 原写法会时好时坏 | `09` 5.9.4：<br>- ① 两端都到达同一个 `head_seq` 再比较，只比窗口内的内容，排除 `notification.intent`，视图的序列化格式纳入 schema；<br>- ② 至少一个用例走"cli + elevate"，原生确认框用替换函数批准；<br>- ③ 卡片加"可以操作"的 DOM 标记，断言恰好一个成功；<br>- ④ 加一个用例：经主进程的测试钩子只断开窗口的数据连接；host-client 暴露缺口、重置、重复的计数；<br>- ⑤ 全部用事件做屏障，不用 sleep。 | 无 |
| 17 退出标准 | 接受 | 跑一次全绿不说明稳定 | `12`：<br>- M1a：NFR-1 套件在 CI 的 macOS runner 上连续 20 次全绿，不允许自动重试；<br>- M1b：S1–S4 × 两种 agent 写成逐步检查表并存档；NFR-2、NFR-9 列出用例编号。 | PRD 口径细化，文字不改 |
| 18 追溯矩阵的"验证"列 | 接受 | 拆不出测试任务 | `09` 新增 5.9.10：<br>- 测试编号规则（UNIT-、PROP-、IT-、GOLD-A/B-、E2E-、MAN-），并注明在哪个套件里跑；<br>- 人工验证清单。<br>`12` 5.12.4：引用编号，补上 NFR-4、NFR-7。<br>SEC-6 和 NFR-4 的外发检查先评估 Seatbelt 方案（e2e 进程放在拒绝出站网络的配置里运行），不行就用 nettop 采样（Q27）。 | 无 |
| 19 晚到输出的标记位置 | 接受 | 合并到 (b) | 见 (b)。 | ADR-06、ADR-07 修订 |
| 20 性能测量 | 接受 | 原写法每晚必然时好时坏 | `09` 5.9.6、5.9.7：<br>- 绝对预算只在参考硬件上判定；CI 按历史中位数的 1.5 倍看趋势；<br>- 冷启动的终点是 `performance.mark('cupper:ready')`；<br>- 数据集为 200 个会话，当前会话 1,000 条消息；<br>- 用 rAF 或 trace 测帧时间；<br>- 用嵌入的时间戳测输出到屏幕的延迟；<br>- 事件循环延迟用 `resolution: 1`；<br>- 每项至少 30 个样本；<br>- 浸泡测试写明内存增长上限。 | 无 |
| 21 CI 代价 | 部分接受 | 金样例要编译 `cupper-host`，它依赖只能在 macOS 上编译的 `cupper-sys`，不能挪到 ubuntu | `01` 5.1.6：<br>- 许可证门禁、Vitest、schema 比对、fixtures 往返、TS 与 Rust 视图对比改到 ubuntu；<br>- `rust` 和 `golden` 合并成一个 macOS 作业；<br>- e2e 冒烟按改动路径过滤；<br>- 固定 runner 标签为 `macos-15`；<br>- 截图基线只由专用工作流生成。 | 无 |

## 分歧与重叠的统一方案

### (a) Trusted Types

**结论**：两边都不完全采用，改为第三种方案。

- 主文档严格执行，禁止 `default` 策略，采纳安全 #17(2)。
- 审查面板里的 diff 视图放进一个单独源的文档。
- 桌面 #6 的问题由这个隔离文档解决。

**主文档**（`app://cupper/`）

- **CSP**：
  ```
  default-src 'none'; script-src 'self';
  style-src 'self' 'unsafe-inline'（即 style-src-elem 加 style-src-attr）;
  img-src 'self' blob: data:; font-src 'self'; worker-src 'self';
  connect-src 'none'; frame-src app://cupper-review;
  object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none';
  require-trusted-types-for 'script'; trusted-types cupper-worker
  ```
  - 不写 `'allow-duplicates'`，不出现 `default`。
- **策略**：
  - 只有一个具名策略 `cupper-worker`。它的 `createScriptURL` 只放行 `app://cupper/assets/` 下的同源脚本。
  - ESLint 规则：只允许在 `ui-core/src/security/policies.ts` 里调用 `trustedTypes.createPolicy`，并禁止名为 `default` 的策略。
- **Radix 的样式注入**：react-style-singleton 会动态插入 `<style>`。这里放行 `style-src-elem 'unsafe-inline'`，理由：
  - 样式不能执行脚本；
  - `img-src`、`font-src`、`connect-src` 已经堵住了经 CSS 外发数据的通道；
  - 不需要按次生成 nonce，桌面建议 #20(d) 的静态响应头也就没有冲突。
  - **转回安全确认。**
- **Markdown**：
  - streamdown 关闭 rehype-raw；
  - harden 只放行 http、https 链接，不放行任何图片前缀（远程图片显示为链接），不允许 data 图片；
  - 写进 5.7.8，并纳入金样例。
- **Mermaid（P1）**：不用 `srcdoc`（它本身也是 TrustedHTML 的写入点），改用下面同样的独立文档方式。

**审查文档**（`app://cupper-review/diff.html`，与主文档不同源）

- **为什么放得下 pierre**：
  - @pierre/diffs 有 15 处以上把字符串直接写进 shadow root（本轮已核实）；
  - 默认策略区分不了调用方是谁。
  - 放进独立文档后，可以不加任何修改地使用 pierre 的完整组件（这正是 Codex 用的组件），保住复刻度。
- **隔离方式**：
  - iframe 加 `sandbox="allow-scripts allow-same-origin"`。因为与父文档不同源，`allow-same-origin` 只保留它自己的源，Worker 和模块脚本可以正常加载；
  - 不给 popups、top-navigation、forms、modals 权限；
  - preload 不进入子框架（`nodeIntegrationInSubFrames` 默认关闭，已核实），所以这个文档里没有 ShellAPI，也没有 HostPort；
  - 主进程用 `will-frame-navigate` 阻止它在首次加载之后的任何导航。
- **CSP**：
  ```
  default-src 'none'; script-src 'self'; worker-src 'self';
  style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src app://cupper;
  connect-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors app://cupper
  ```
  - **不加** `require-trusted-types-for`。
  - 防线是：
    - 严格的 `script-src`，注入的脚本和内联事件处理器都不会执行；
    - 跨源隔离；
    - 一条窄的 MessageChannel 协议，主文档持有 `diff_token`，逐条校验 frame 发来的 `hunk_ids` 和评论。
  - **转回安全确认。**
- **Worker**：用 pierre 的 `workerFactory` 选项，在审查文档里自己构造。高亮器固定为 `preferredHighlighter: "shiki-js"`（JS 正则引擎，不需要 wasm）。
- **范围**：只有 `<DiffView>`（含行内评论的锚点）放进去；文件列表、暂存栏、提交框留在主文档。

**后备方案**：如果 M1b 开头的验证（Q29：constructable stylesheet 与 CSP、焦点与快捷键、5,000 行 diff 的性能）不通过，改为只用 pierre 导出的 `DiffHunksRenderer` 产出 HAST，由我们自己用 React 渲染。这样不需要任何 HTML 写入点，代价是要自己实现虚拟滚动和交互。

**验收**：
- M1a：主文档在发布 CSP 下渲染全部组件，`securitypolicyviolation` 为 0；
- M1b：审查文档同样为 0，并纳入 SP-6 的 5,000 行 diff 场景。

**落点**：`07` 5.7.5、5.7.8；`01` 5.1.3（ui-core 依赖说明）；`09` 5.9.7。

**标注**：ADR-03 细化："启用 Trusted Types" → "主文档禁止 default 策略，审查与 Mermaid 用独立源的文档"。

### (b) Q14：晚到的工具输出放在哪里

**原则**：回合快照写入之后不可变。任何事后信息都写成当前流里的新事件，不修改旧回合的条目。

**做法**

1. **取消的那一刻**：
   - 能力描述加 `turn.interrupt_stops_commands`，Codex 为 false。
   - 对这种 agent，回合结束时还在运行的 `execute` 类工具，写 `item.completed{status: abandoned, may_still_run: true}`。它属于原回合，在回合快照写入之前完成。
   - 界面立刻显示"命令可能仍在运行"，满足安全意见里"不要等输出出现才提示"。
2. **晚到的输出出现时**：
   - 在当前回合里新建一个条目 `kind: late_tool_output`，字段为：
     - `ref: {item_id, turn_id, title, command?}`；
     - `output: Content`，与普通工具输出规则相同：内联或 blob，按 (b) 的 `field: output` 流式到达，不丢弃；
     - `exit_code?`、`signal?`。
   - 每个原工具只建一个这样的条目，之后的输出都更新它；收到 `completed` 时结束。
   - 默认折叠，标"取消后仍在运行"，点击可以跳到原条目。
   - 没有进行中的回合时，按 K9 开一个 `autonomous` 回合承载它。命令确实还在运行，显示"运行中"也是准确的。
3. **原条目的显示**：
   - 原条目的事件不改。
   - TS 和 Rust 的视图维护一个反向索引 `late_by_ref`。原条目可见时，在旁边显示"最终退出码 N"和跳转链接；原回合不在内存时不显示，`late_tool_output` 条目本身信息完整。
   - 这样同时满足桌面意见（退出码、尾部输出）和安全意见（附在原条目附近、默认折叠），又不改旧回合。
4. **回合快照**：从来不重写。`late_tool_output` 条目随它所在回合的快照写入。
5. **`history/page`**：两个回合各自完整，不需要特殊处理。
6. **金样例**：
   - 用 codex s07 录制，在 A 层和 B 层分别断言：t1 里有 `abandoned + may_still_run`，t2 里有一个 `late_tool_output`，并且 `ref` 指向 t1 的条目；
   - 用例覆盖：新回合进行中到达、会话空闲时到达、同一工具多次晚到只有一个条目、不属于被取消回合的回合外更新按 K9 处理。

**落点**：
- `03` 5.3.5：新增条目类型；`notice` 的 code 去掉 `late_output_discarded`；能力描述加字段。
- `06` 5.6.9：重写 K3。
- `05` 5.5.5：改图。
- `02` 5.2.8：写明快照不可变。

**标注**：
- ADR-07 修订："丢弃晚到的工具输出" → "不进入新回合的工具流程，单独记录"；
- ADR-06 修订：写明回合快照不可变。

### (c) 真实输入规则（合并桌面 #1⑤、#5，测试 #13，安全 #7）

**1. 识别用哪些事件**

- **键盘**：`before-input-event`，只认 `type: keyDown`。它带 `key`、`isComposing`、`isAutoRepeat`、modifiers。
  - 不用 `input-event` 识别键盘：在 Electron 44 的类型定义里，它只有 `type` 和 `modifiers`，拿不到是哪个键（运行时是否带键码未核实，记为 Q26）。
- **鼠标**：`input-event` 里同一按钮的 `mouseDown` 接 `mouseUp`，间隔不超过 1 s，记在 `mouseUp` 的时刻。
- 主进程为每个窗口记录 `lastInput {kind, key?, at, consumed}`。

**2. 哪些按键算真实输入**

- 只算 Enter（含小键盘 Enter）、Space、Escape、⌘Enter（提问表单的提交）。
- 要求 `isComposing` 为 false、`isAutoRepeat` 为 false；除 ⌘Enter 外，不能带 ⌘、⌃、⌥。
- 其他按键（打字）不记录，并且清掉已记录但还没用掉的输入，防止"打字途中被借用"。

**3. Esc 和拒绝类选项**

- 所有 `interaction/respond` 都要求真实输入，规则统一，便于测试。Esc 在白名单里。
- 主进程转发时，把证据写进新参数 `input: {kind: click|key|native_confirm, key?}`，并覆盖渲染进程自己填的值。
- 宿主检查：
  - `key = escape` 只能用于 `dismissed`，或者 `effect.scope` 为 `reject`、`plan_revise` 的选项；
  - `effect.scope = unknown` 的选项本来就是 privileged，只能走原生确认，键盘无效。
- 这个参数只在 desktop_ui 的连接上有意义，其他 principal 一律忽略。

**4. 输入法上屏**

- `isComposing` 为真的按键一律不记录，包括用 Enter 选词。
- 渲染进程的输入框用同样的判断（`isComposing` 或 keyCode 229）决定不发送（测试 #26）。

**5. 窗口刚被激活后的冷却**

- 窗口 `focus` 之后 1,000 ms 内的键盘输入不记录。
- `focus` 和 `blur` 时清空记录。
- `acceptFirstMouse` 保持 macOS 默认的 false（已核实），所以激活窗口的那次点击不会送到页面。
- 深链接、`open-url`、`second-instance`、通知点击都只做导航，不把焦点移到交互卡片上。
- 卡片可见并获得焦点满 500 ms 后才响应（渲染进程负责）。宿主的 400 ms 兜底不变，只对 window 角色生效。

**6. `requestPrivileged` 要不要求真实输入**

- 要。窗口和白名单与上面相同，但 Esc 不能触发特权操作。
- 同一时间只弹一个原生确认框，其他请求返回 `busy`。
- 用户取消后，2 s 内对同一类操作的再次请求直接拒绝。
- NFR-8 的退出确认由主进程自己发起，不受这些限制。

**7. 用同一个 `command_id` 重发**

- 主进程按窗口记录"已放行的 `command_id` → 参数哈希"。
- `command_id` 相同、参数哈希也相同的重发，不再要求新的输入。
- 收到最终响应或满 10 分钟后，删除这条记录。
- 窗口的文档实例变化时（见桌面 #1）清空：重载之后，渲染进程里未完成的命令已经丢了，由用户重新操作。

**8. Playwright 怎样测**

- **点击**：用 `page.click()`，它会产生 `input-event`。
- **按键**：用 e2e 辅助函数，经 `electronApp.evaluate` 在主进程里调用 `webContents.sendInputEvent({type: 'keyDown', keyCode: 'Return'})`。评审者在 Electron 34 上实测过，它会触发 `before-input-event`。生产代码里没有任何测试分支。
- **负面用例**：页面脚本直接经 HostPort 发送，应被拒绝（`user_activation_required`）。发送之前，先等上一次输入被用掉，或者等超过 1 s。
- **冷却**：主进程执行 `win.blur(); win.focus()` 后立即发 Enter，应被拒绝；1.2 s 后再发，应成功。
- **inputGuard 的时间**来自可注入的时钟，时序规则用 Vitest 的假时钟做单元测试。
- **输入法**：给 inputGuard 喂 `isComposing: true` 的 Input 对象（Vitest），再加组件测试；真实输入法留在人工检查表里。
- **M1a 的 windowBridge 任务**在 Electron 44 上复测 Q26 的三件事：
  - Playwright 的键盘输入是否仍不触发 `before-input-event`；
  - `sendInputEvent` 是否触发它；
  - `input-event` 的键盘事件是否带键码。
  
  另外用实体键盘人工确认一次。

**落点**：`07` 5.7.2（inputGuard）、5.7.7、5.7.8；`03` 5.3.4（`interaction/respond.input`）；`02` 5.2.5；`09` 5.9.4。

**标注**：ADR-03 细化（键的白名单、冷却、`requestPrivileged`）。

### (d) 重叠意见合并成一个方案

#### d1 自托管 runner（测试 #8、安全 #10）

- **去掉 self-hosted runner。** `cargo xtask compat` 只由维护者在本机手动运行。产出一起以 PR 提交：录制、金样例、清单中的"已验证范围"、机器可读的报告。
- **报告内容**：
  - 版本、剧本、结果（通过、失败、不确定）、耗时、重试次数；
  - 配置文件哈希的前后对比；
  - 需要清理的 CLI 会话。
  
  清单里的"已验证范围"由报告生成，不手填。
- **Q8 的通过条件**写进 5.9.5：两个会话同时各发 3 个 prompt，全部在 5 分钟内返回，输出没有串到另一个会话，重复 3 轮。
- **运行环境（U-R4）**：
  - 首选隔离的 `CLAUDE_CONFIG_DIR` 和 `CODEX_HOME`，只复制登录凭据。这样录制里不会混进用户的 MCP 和 hooks，也不会改写用户配置。
  - Claude 在 macOS 上的凭据可能存在钥匙串里，隔离后能否保持登录未核实（Q30）。
- **GitHub Actions 加固**：
  - release job 放进需要人工批准的 protected environment，只由受保护的 `release/*` tag 触发；
  - 第三方 Action 按提交 SHA 固定；
  - 默认 `permissions: contents: read`；
  - 不用 `pull_request_target`；
  - `pnpm install --frozen-lockfile`，保留 pnpm 默认不运行依赖脚本的行为，只放行白名单；
  - Rust 一律 `--locked`。
- **落点**：`01` 5.1.6；`09` 5.9.5；`10` 5.10.7。

#### d2 发布产物的特性检查与签名后冒烟（测试 #14、#15，安全 #12）

分四层：

1. **每个 PR**：
   - 不带任何特性跑一次 `cargo test -p cupper-host`，覆盖权限矩阵的"发布列"：harness 认证和 dev_key 认证都被拒；`diag/recording_export` 和 `test/*` 方法不存在。
   - 宿主和 CLI 提供 `--build-info`，输出启用了哪些特性。
2. **发布构建**：
   - 每个二进制单独执行 `cargo build -p <crate> --release --locked`，避免 cargo 合并特性；
   - 断言 `--build-info` 里没有 `dev-auth` 和 `test-harness`；
   - 发布版 CLI 没有 `--harness-key-fd`；
   - TS 侧的测试钩子用 Vite `define` 的 `__CUPPER_FLAVOR__` 门控，并在产物里扫描钩子的标记串。
3. **每晚一次"发布配置、不签名"的构建**，检查：
   - fuses；
   - Info.plist 里有 `ElectronAsarIntegrity`；
   - `asarUnpack` 只有 `cupper_native.node`；
   - 带调试参数或未知命令行开关启动时立即退出（建议级安全 #17(1)）；
   - asar 的文件清单；
   - 产物扫描；
   - 许可证门禁的两层。
4. **发布流水线签名之后的冒烟测试**（同样用于 `release/vX.Y.Z-rc.N` 标签），断言：
   - 认证方式是 codesign，`weak_auth` 为 false；
   - CLI 默认只读；
   - 杀掉主进程后，新 UI 能接上旧宿主（补上 Q22 留下的 NFR-8 覆盖）；
   - 握手矩阵：UI 主进程被接受；4 个 Electron helper、CLI、node、e2e 版 UI、ad-hoc 重签的 UI 副本都被拒绝；
   - UI 和 CLI 都拒绝假宿主；
   - 发布版 UI 遇到 `weak_auth: true` 时拒绝连接。
   
   任何一项失败，草稿就不发布。

"用 Apple Development 证书做一个 CI 签名要求变体"这一项暂不做：它要把开发证书放进 CI。等 Q5 定了、有了 Developer ID 之后，在 M1b 的发布任务里再评估。

**落点**：`01` 5.1.5、5.1.6；`08` 5.8.1；`09` 5.9.4；`10` 5.10.7。

#### d3 e2e 的接线方式与 ad-hoc 签名（测试 #2、桌面 #17）

- **拉起**：e2e 里只有 UI 拉起宿主，测试和 CLI 都不拉起。
- **`09` 新增 5.9.9"测试替身与注入点"表**，覆盖：
  - agent 命令（跳过运行时安装和 CLI 发现）；
  - CLI 路径与登录 shell 环境；
  - npm registry 地址；
  - 通知器：记录到测试接收端，不调用 UserNotifications，避免授权弹窗抢走焦点；
  - 时钟、故障点、时长覆盖；
  - 原生对话框：Playwright 在主进程里替换 `dialog.showMessageBox`；
  - 渲染进程导出视图的钩子；
  - 主进程"只断开某个窗口的数据连接"的钩子。
  
  每项写明所在的构建或特性、默认值，以及在发布构建中不存在的检查方式（见 d2）。覆盖 agent 命令只能在编译期门控。SP-4a 的 `agents.json` 不保留。
- **传递方式**：
  - 宿主从 stdin 读一行 JSON `{dev_key, harness_key?, test_config?}`，读完立即关闭（格式写进 5.8.1）。
  - e2e 版主进程只在 e2e 构建里读取环境变量 `CUPPER_E2E_DIR`。这个临时根目录里放 harness 密钥文件（0600），以及数据目录、userData、worktree 根目录。
  - 主进程把这份内容留在内存里。`kill -9` 之后重新拉起宿主时，原样再传一次。
  - 测试里的 CLI 用 `--harness-key-fd`。
  - test-harness 构建的 `host/status` 返回宿主的 pid。
- **e2e 构建的签名**：
  - ad-hoc 签名，并关闭 hardened runtime（ad-hoc 签名加 hardened runtime 需要 `disable-library-validation`）；
  - 关闭 `enableCookieEncryption`；
  - 依赖 hardened runtime、codesign、动态状态字的检查，只在 d2 的签名后冒烟里跑；
  - 5.9.4 的每条安全 e2e 都标明在哪种构建上跑。
- **e2e 里 UI 怎样校验宿主**：dev_key 改为双向证明，不再明文出示密钥。
  - `initialize.auth = dev_key{client_nonce, proof: HMAC(key, host_nonce)}`；
  - 结果里带 `host_proof = HMAC(key, client_nonce)`；
  - 假宿主拿不到密钥，过不了 UI 的校验，"假宿主占用套接字"的用例在 e2e 上也就有了意义。harness 密钥同理。
- **落点**：`03` 5.3.2；`07` 5.7.2、5.7.9；`08` 5.8.1；`09` 5.9.4、5.9.9。

**标注**：ADR-05 细化（退化模式的双向证明）；ADR-03 细化（e2e 构建不开 hardened runtime）。

### (e) 影响里程碑排期的意见

| 意见 | 结论 | 对 5.12 的影响 |
|---|---|---|
| 时钟抽象与纯核心（测试 #1） | 接受 | M1a 的第一个宿主任务是"核心骨架与测试基础设施"：Clock、纯核心框架、`AcpTransport`、故障点、`--test-config`、stdin 的 JSON 格式、`--build-info`。orchestrator、driver、store 的任务都排在它之后 |
| 主进程转发的测量提前到 M1a（桌面 #14） | 接受 | 作为 windowBridge 任务的验收：<br>- 合成负载：6 条流各每秒 200 个片段、5 MB 的 blob、5,000 个事件的重新同步；<br>- 用 `monitorEventLoopDelay({resolution: 1})` 测；<br>- 在参考机器上 p99 ≥ 5 ms 时，在 M1a 的界面任务之前改用"utilityProcess 宿主桥 + 委托票据"，同步修订 5.8.1 的签名要求表和 ADR-03（委托票据的草案记入 Q33，只在触发时启用）。<br>测量放在 M1a，就能保证这个后备方案引入的协议改动也落在冻结之前。SP-6（M1b 开头）只管渲染 |
| Q8 在写驱动层之前测完（测试） | 接受 | M1a 第一个驱动任务之前，先做一次技术验证：<br>- Q8：用 SP-1 的 TS 客户端补跑 Claude 的并发剧本；<br>- Q9；<br>- SP-4b 遗留第 7 条。<br>Codex 的 Q8 放在 M1b 的 Codex 驱动任务之前。都要用真实 CLI，**需要用户许可**（U-R5）。另外，6 个会话共用一个假适配器、同时流式输出的集成测试进入 5.9.1 |
| 项目信任 M1a 就要有（安全 #2） | 接受 | M1a 加入：<br>- 宿主：扫描、`project/trust`、`settingSources`、stale 检测；<br>- 界面：添加项目流程里的原生确认，"未信任"标记 |
| M1a 最简界面要增加的元素（桌面 #13） | 接受 | 5.12.2 的桌面端增加：<br>- 带状态的回合标题（已取消、已中断、失败），不做折叠；<br>- 排队条；<br>- 卡片的"已失效""已在 cupper-cli 处理"；<br>- Toaster；<br>- 提权横幅与原生确认框；<br>- 信任确认；<br>- 弱认证标记；<br>- 重连状态。<br>架构按最终形态实现：store、订阅策略、行虚拟化、streamdown 配置、HostPort 生命周期、ShellAPI。从第一天起所有文案都走 i18n 的键，先只做一种语言。发布 CSP 下的违规数为 0 |

**5.12 的范围变化汇总**

- **M1a 增加**：
  - 核心骨架与测试基础设施；
  - 技术验证（Q8 Claude 部分、Q9、Q26、Q27、SP-4b #7）；
  - 项目信任；
  - git 的安全档位（随第一个 git 调用一起做）；
  - windowBridge 的测量闸门；
  - 上表中的界面元素；
  - 假 agent 的 persona 和原始行通道；
  - 金样例 A 层，加第一批 B 层录制；
  - CI 的无特性测试和 macOS 14 夜间测试。
- **M1a 退出标准**：NFR-1 套件连续 20 次全绿。
- **M1b 变化**：
  - SP-6 只测渲染，并补 4 个场景；
  - 审查面板用独立文档（先做 Q29 的验证）；
  - Codex 的 Q8 先做；
  - Codex 在未信任项目上拒绝启动；
  - 发布流水线加签名后冒烟、许可证门禁第二层、RC 标签；
  - 更新的安装空窗处理；
  - S1–S4 的逐步检查表；
  - NFR-2、NFR-9 按用例编号验收。

## 新增的用户决定事项

原有的 Q5、Q6、Q12、Q13、Q22 不变。两处补充：
- Q22 加上安全提出的两个条件：退化模式只由编译特性决定，不允许运行时回退；自行构建的版本使用开发构建的身份和独立的数据目录。
- Q5 补一条安全意见：前缀要选项目自己控制的域名。

新增以下几项：

| 编号 | 事项 | 来源 | 建议 | 影响 |
|---|---|---|---|---|
| U-R1 | 是否增加"项目信任"步骤：<br>- 添加项目时弹原生确认，列出扫描到的 hooks、env、helper、MCP、skills、`.codex/` 配置；<br>- 未信任的项目，Claude 不加载项目配置，Codex 不能用；<br>- 项目配置变了要再确认一次。 | 安全 #2 | **采用。** 这是 CLI 信任对话框在 cupper 里的对应物，AG-6 继承的用户级配置不受影响 | PRD 的 PS-2 增加一步；M1a 范围；ADR-05 |
| U-R2 | M3 的手机能不能把会话切到 `elevated` 档位（Claude 的 auto、acceptEdits，Codex 的 Auto review） | 安全 #3 | **不能。** SEC-9 的"跳过审批"包含 elevated。手机只能切到不比当前更宽松的档位；已经处在 elevated 的会话，手机照常使用。现在就定，是为了 M1a 的策略规则一次写对 | SEC-9 的解释口径（M3）；U6 的低打扰模式只能在桌面上开启 |
| U-R3 | Q3：Node 是否直接采用后备方案 | 安全对 Q3 的意见 | **采用。** 重签的 `node` 作为普通 Mach-O 放进 `CupperHost.app/Contents/Helpers/`，随 App 一起公证，安装时复制并核对 cdhash。<br>- 好处：公证覆盖到 Node；不用写 tar.xz 解包（少一个依赖）；不再留一个要到 M1b 发布时才能验证的架构分叉。<br>- 代价：下载包大约增加十几 MB，安装后多占约 100 MB（估计，未核实） | ADR-08 修订（"Node xz 压缩后约 24 MB"一句要改）；5.10.1、5.2.12 |
| U-R4 | 兼容套件的运行环境 | 测试 #8、安全 #10 | 首选**隔离的配置目录**：把登录凭据复制到临时目录，运行后删除。如果 Q30 验证登录态保持不了，退回到在真实配置上运行，并做前后哈希对比 | 是否同意"复制凭据到临时目录"，或者"在真实配置上运行"（SP-1、SP-4b 都出现过配置被改写） |
| U-R5 | 许可：在 M1a 写驱动层之前，用真实 CLI 跑 Q8 的并发剧本（Claude；Codex 在 M1b 前），会消耗少量额度 | 测试对 Q8 的意见 | 同意，做法与 SP-1 相同 | M1a 的排期 |

## 建议级意见的处理

**直接采纳**

| 编号 | 理由（一句） |
|---|---|
| 桌面 #16 | CLI 改放 `Contents/Helpers/cupper`，符合 Apple 的 bundle 布局；设置页提供"安装命令行工具"，在用户选择的目录里建符号链接 |
| 桌面 #18 | 宿主由 `cupper_native` 调用 `cupper-sys::spawn` 拉起（SETSID、CLOEXEC_DEFAULT、环境变量白名单、stdin 传密钥），与 ADR-01 一致，行为可测 |
| 桌面 #19 | preload 打成单个 CJS 文件；运行时依赖全部由 Vite 打进 bundle；发布 CI 断言 asar 的文件清单 |
| 桌面 #20(a)(b)(c) | 数据连接与窗口创建并行；`show: false` 加 `ready-to-show`；`codeCache`。几乎没有成本，有利于 NFR-6 |
| 桌面 #21 | 视觉常量做成 token，注明来自截图笔记，偏离时写理由，直接关系复刻度 |
| 桌面 #22 | 右侧做成标签容器；侧边栏保留标题行；新建会话页按 Codex 的布局；标题栏加"打开"按钮；弱认证标记按 ADR-05 放在标题栏 |
| 桌面 #23 | 三种构建用不同的 URL scheme，避免通知打开错误的 App |
| 桌面 #24 | 视图把条目和交互请求按 seq 放在同一个有序序列里，快照保持原始的相对顺序 |
| 桌面 #25 | 入方向按字节块转发，由渲染进程切帧，主进程负担最小；转发时用独立的 ArrayBuffer |
| 安全 #14 | 改正更新代理端口的描述：从下载完成一直开到 `quitAndInstall`；升级演练加两项负面测试 |
| 安全 #15 | 原始帧只做结构化脱敏（ADR-06 修订）；M1b 设置页提供"清除快照对象"；诊断包里的 minidump 和原始帧默认不勾选；日志目录和 worktree 根目录设为 0700 |
| 安全 #16 | 判断"此刻有人在看"时只采信 desktop_ui 上报的在场。CLI 的 `mark_read` 按 U4 仍然推进已读位置，但不影响"需要处理"的通知 |
| 安全 #17(1)(3)(4) | 命令行开关改用白名单；`app://` 的路径规范化后检查前缀；CI 断言 asar 完整性。(2) 已在 (a) 中处理 |
| 安全 #18 | 套接字路径过长时，回退到 `DARWIN_USER_TEMP_DIR`，防止 `/tmp` 被抢占 |
| 安全 #19 | 所有子进程都经 `cupper-sys::spawn` 拉起；宿主自己的 stdio 指向 /dev/null；兼容套件用 lsof 断言；`fs`、`terminal` 能力永远声明为不支持 |
| 安全 #20 | 清单覆盖所有会被执行的文件（含 `bundle-claude-acp.mjs`）；重申不调用 npm、不运行生命周期脚本 |
| 安全 #21（前两项） | 规范 JSON 现在就定为 RFC 8785（JCS）；摘要改为完整的 256 位；预先写好 `attestation` 的签名对象。这些都是契约，必须在冻结前定 |
| 测试 #22 | 补 4 条性质测试；`StreamState` 用 shuttle 穷举线程调度；每晚做一次 TS 与 Rust 的差分测试 |
| 测试 #23 | 补两个用例：同一连接上连发两条命令的执行顺序；cdhash 不一致时转为"待升级" |
| 测试 #24 | 从第二个发布版起，改动 schema 或运行时的版本都做升级演练；迁移测试用上一版宿主生成的真实数据库；加"上一版 CLI 连新宿主"的用例 |
| 测试 #25 | 每个 PR 对 `testdata/` 做通用模式的个人信息扫描 |
| 测试 #26 | 输入法的判断逻辑用组件测试覆盖，人工检查表只保留真实输入法的体验 |

**不采纳，或者推迟**

| 编号 | 理由 |
|---|---|
| 桌面 #20(d) `protocol.registerSource` | 实验性 API。现在不用，SP-6 测得冷启动不达标时再评估（(a) 选了不需要按次生成 nonce 的方案，届时没有障碍） |
| 桌面 #17 | 已并入 (d3) 一起处理，不再单列 |
| 安全 #21 第三项（M3 宿主身份密钥放进数据保护钥匙串） | 属于 ADR-12 的范围，M3 之前再定；现在只在 5.13 里记下"届时 entitlements 白名单要跟着改" |

## 修订工作量与契约冻结检查

### 按文件列出要改的小节

| 文件 | 要改的小节 | 规模（估计） |
|---|---|---|
| `02-design.md` | 第 5 节主要结论；版本升到 v0.7 | 小 |
| `01-repo-build-ci.md` | 5.1.2（录制按版本存放）、5.1.3（`--build-info`、ui-core 说明）、5.1.5（无特性测试、按 crate 分别构建）、5.1.6（CI 表重写：ubuntu 作业、去掉 self-hosted、Actions 加固、夜间发布配置构建、macOS 14）、5.1.7（加 Clock、Trusted Types、spawn 三条约束） | 约 60 行 |
| `02-host.md` | 5.2.3（规则 6）、5.2.4（`initialize` 只接受一次）、5.2.5（档位规则、输入证据、`pending_external` 强制同步）、5.2.6（第一条消息、preparing 恢复、配置确认、新增"项目信任"）、5.2.7（未知 kind）、5.2.8（持久性、快照不可变、epoch）、5.2.9（在场、`ui/navigate`）、5.2.10（两档与指纹）、5.2.11（环境变量放行名单）、5.2.13（退出码 6、回退目录、迁移）、5.2.14（CLI 位置）、**新增 5.2.16** | 约 180 行 |
| `03-protocol-v1.md` | 5.3.1（不截断、JCS）、5.3.2（只接受一次、dev_key 双向证明、`data_epoch`）、5.3.3（`cursor_ahead` 与 epoch、历史游标）、5.3.4（`project/trust`、`history/page`、`test/*`、`interaction/respond.input`、`set_config` 档位行、`ui/navigate`）、5.3.5（时间字段、`final_item_ids`、`stat`、`item.delta.field`、ToolCall 字段表与分组规则、`late_tool_output`、`trust_state`、`confirmation_required`、notice code）、5.3.6（unknown 行、256 位摘要）、5.3.7（档位顺序、`interrupt_stops_commands`）、5.3.8（新的 reason）、5.3.9（提权限额） | 约 150 行 |
| `04-data-model.md` | 5.4.2（projects 的信任列，sessions 的 `initial_prompt_json` 和 `full_access_confirmed_at`，meta 的 epoch 与 boot uuid，原始帧的 dir 取值）、5.4.4（摘要格式）、5.4.6（迁移事务、退出码） | 约 30 行 |
| `05-flows.md` | 5.5.1（经原生扩展拉起、并行）、5.5.2（信任分支、第一条消息落盘）、5.5.4（输入证据）、5.5.5（晚到输出）、5.5.6（epoch 与快照）、5.5.7（安装空窗） | 约 60 行（改 4 张图） |
| `06-agent-driver.md` | 5.6.1（SETSID；永不声明 fs、terminal）、5.6.4（未知 kind）、5.6.5（风险表）、5.6.6（环境变量剔除与 CI 提取）、5.6.9（K3 重写、每条的回归列）、5.6.10（改写只为解析）；新增"未信任项目的启动参数" | 约 70 行 |
| `07-desktop.md` | 5.7.1（生命周期、流控、字节块转发、测量闸门）、5.7.2（hostLauncher、notifications 后备模块、inputGuard）、5.7.3（ShellAPI 重写）、5.7.4（布局修正、折叠规则、token）、5.7.5（行虚拟化、审查文档）、5.7.6（订阅策略、到达即应用）、5.7.8（CSP 与 Trusted Types、真实输入、开关白名单、路径穿越、通知）、5.7.9（URL scheme、e2e 签名） | 约 200 行 |
| `08-security.md` | 5.8.1（stdin 格式、双向证明、发布版拒绝 `weak_auth`）、5.8.2（档位）、5.8.3（矩阵新增或拆分约 6 行）、5.8.4（限频、血缘、确认框布局）、5.8.5（git、spawn）、5.8.7（TCC、hooks、信任的残余风险） | 约 90 行 |
| `09-testing.md` | 5.9.1、5.9.2（假 agent 三部分）、5.9.3（A/B 两层、规范化、格式规格）、5.9.4（NFR-1 用例重写、标明构建种类）、5.9.5（运行环境与报告）、5.9.6（测量方法）、5.9.7（SP-6）、5.9.8（故障点、性质测试）、**新增 5.9.9** 注入点、**新增 5.9.10** 测试编号与人工验证清单 | 约 250 行 |
| `10-release.md` | 5.10.1（CLI 和 Node 的位置，按 U-R3）、5.10.2、5.10.3（清单覆盖范围）、5.10.5（安装空窗、"应用程序"检查、端口描述）、5.10.7（冒烟、门禁、protected environment、RC 标签） | 约 60 行 |
| `11-diagnostics.md` | 5.11.2（原始帧只做结构化脱敏）、5.11.3（默认不勾选）、日志目录 0700 | 约 15 行 |
| `12-milestones.md` | 5.12.2、5.12.3（见 e）、5.12.4（验证列改为引用编号，补 NFR-4、NFR-7） | 约 120 行 |
| `13-open-issues.md` | 关闭 Q1、Q4、Q11、Q14、Q15；更新 Q3、Q8、Q10、Q16、Q20、Q21、Q24；新增 Q26–Q33；改写 5.13.2、5.13.3 | 约 80 行 |

- **合计**：约 1,400 行改动，大约是 D01 篇幅的一半。
- **建议**：在一个任务分支上按主题拆成 8–10 个提交：
  1. 时间与可测性；
  2. git 安全档位与项目信任；
  3. 档位与权限矩阵；
  4. 协议契约补齐；
  5. 桌面合同（ShellAPI、生命周期、Trusted Types、真实输入）；
  6. 测试体系；
  7. CI 与发布；
  8. 里程碑与待定问题。
- **新增待定问题**：
  - Q26 Electron 44 的输入事件；
  - Q27 外发检查方法；
  - Q28 CLI 是否热加载项目配置；
  - Q29 审查文档的验证；
  - Q30 隔离配置目录能否保持登录；
  - Q31 codex-acp 是否透出 phase；
  - Q32 `-c` 覆盖是否生效；
  - Q33 委托票据的后备方案。
- **ADR 修订**（由编排者提交）：

  | ADR | 修订内容 |
  |---|---|
  | ADR-02 | 约束清单加"时间只经 Clock" |
  | ADR-03 | 连接模型、Trusted Types 落法、真实输入细则、测量时机、e2e 签名 |
  | ADR-04 | 方法清单的增补；`item.delta.field`；历史游标；JCS 与 256 位摘要；epoch |
  | ADR-05 | 项目信任；风险档位；提权限频与血缘；环境变量；dev_key 双向证明；TCC |
  | ADR-06 | 回合快照不可变；持久性配置定案；原始帧只做结构化脱敏 |
  | ADR-07 | 晚到输出 |
  | ADR-08 | 驱动传入 `settingSources`；Node 的位置（按 U-R3） |
  | ADR-10 | 两档调用约定与指纹 |
  | ADR-11 | 门禁分两层 |

### 契约冻结检查

**检查方法**：逐项判断本次改动属于"新增方法、事件、可选字段、开放枚举值"，还是"改变已有契约的形状或语义"。v1 的类型在 M1a 就全部定义，包括 M1b 才用的方法。所以只要以下改动在 M1a 的协议 crate 任务合并之前写进设计并实现，就**不违反**"M1b 只加方法和事件"。如果拖到 M1b，其中第 1 组会直接违反这条规则。

**第 1 组：改变已有契约，必须在冻结前完成**

1. `initialize`：
   - dev_key 认证的形状改为 `{client_nonce, proof}`；
   - 结果加 `host_proof` 和 `data_epoch`；
   - 只接受一次、角色不可变的语义。
2. `stream/subscribe` 加 `epoch`；`cursor_ahead` 时走快照。
3. `history/page` 的参数和结果改为新的游标结构。
4. `item.delta` 加 `field`，合并键随之改变。字段虽然是可选的，旧客户端却会把命令输出拼进正文，所以必须从第一版起就有。
5. 摘要改为完整的 256 位，规范 JSON 改为 JCS（影响 `request_digest` 和 `params_hash`）。
6. 权限请求里的命令和路径不再截断（改变 5.3.1 的通则）。
7. 策略语义：
   - `set_config` 和 `mode_switch` 按档位判定；
   - 项目信任对会话启动的门控；
   - `interaction/respond.input` 与 Esc 的限制；
   - 未知 kind 一律 privileged。
   
   `session/set_config` 虽然是 M1b 才用的方法，但它的规则属于 v1 契约，现在就要定。
8. ShellAPI（`ui-core` 与 `apps/desktop` 之间的契约）：
   - `init` 和 `open` 改为异步；
   - `requestPrivileged` 的返回值；
   - `sendBlob`、`onCommand`、`onNavigate`；
   - `PrivilegedAction` 新增的种类。
9. 宿主进程的启动契约：
   - stdin 的 JSON 格式；
   - 退出码 6；
   - `--build-info`、`--test-config`。

**第 2 组：属于新增，但金样例和界面从 M1a 起就要依赖，建议一并冻结**

- 各条目的时间字段；
- `final_item_ids`；
- ToolCall 的 `query`、`url`、`parent_item_id`，以及必填字段表；
- `late_tool_output` 条目类型；
- `may_still_run` 与 `interrupt_stops_commands`；
- `project/trust` 与 `trust_state`；
- `confirmation_required`；
- `snapshot.recorded.stat`；
- 新增的 notice code 和 reason；
- `ui/navigate`；
- schema 的 `x-id`、`x-time`、`x-path` 注解。

**第 3 组：M1b 才做也不违反规则的**

- 审查文档；
- git 的用户档（自动档随第一个 git 调用一起做）；
- Codex 的拒绝逻辑（reason 的取值已经在第 2 组里定好）；
- 安装空窗；
- `pathForFile`；
- 签名后冒烟。

这些都是界面、内部实现或流水线的改动，不涉及协议形状。

**冻结机制**：M1a 的协议 crate 任务合并时，把 `protocol/schema/v1.json` 复制成 `protocol/released/v1-m1a.json`，由 `tools/schema-compat` 对它做方向敏感的比对。ShellAPI 的类型声明同样做一份快照比对。这样 M1b 期间出现任何破坏性改动，CI 都会直接失败。

**结论**：本次评审带来的协议改动全部集中在第 1、2 组，只要在本轮修订中写定、在 M1a 冻结前实现，就不违反"M1b 只加方法和事件"。