# SP-4a 汇报

- 执行者：子 agent（后端工程师，Rust）
- 分支：`spike/SP-4a-host-skeleton`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-ab25bd62e6b7234b4`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

> 编排者存档说明：正文按子 agent 的回复原样保留，只在"对用户配置的改动"一节把会话 ID 和临时目录名换成了概括写法，完整清单存放在不入库的 `.private/cleanup/SP-4a-leftovers.md`。

## 做了什么

在 `spikes/SP-4a-host-skeleton/` 下建了 Rust workspace，共 4 个 crate，Rust 代码约 7,000 行（含测试）。

- **`cupper-protocol`**（不做 I/O）
  - 帧格式按 brief：单帧不超过 64,000 字节，编码超限直接报错；有 blob 分片头。
  - JSON-RPC 信封，错误码表按 ADR-04。
  - 实现的方法：`initialize`、`stream/subscribe`、`session/create`、`session/send`、`session/interrupt`、`interaction/respond`、`blob/get`。实现的通知：`stream/events`、`stream/snapshot`、`stream/reset`。
  - 开放枚举用 `open_enum!` 宏，带 `Unknown(String)`；带 `type` 标签的联合类型用 serde 的 untagged 兜底成员。
  - 事件信封带 `from`（区间事件）、`v`、`cause`、`fallback`。
  - 发送时合并的规则和纯视图都在这里。快照就是一串能重建状态的事件体，客户端用同一个 `apply`。
  - 用 schemars 导出 `protocol/schema/v1.json`，有一致性测试。
- **`cupper-host`**
  - 临时数据目录，权限 0700，并检查属主。
  - 单实例用 `flock`（标准库 `File::try_lock`）。Unix 套接字权限 0600；路径超过 104 字节时退到 `/tmp/cupper-<uid>/<hash>/`。
  - 对端校验 uid。
  - SQLite（WAL）存事件日志，同一条流的 seq 连续；有视图检查点和保留窗口。
  - 订阅时游标仍在日志里就补发，太旧就给快照。
  - 写入时合并：首个片段立即发出，之后 40 ms 合并一次。
  - 每个连接一个发送队列，按字节计预算，超出就发 `stream/reset`。写任务把积压的片段合并成带 `from` 的区间事件。
  - 命令按 `(principal, command_id)` 幂等，结果持久化，并发重复会等第一次执行的结果。
  - 交互请求由会话 actor 串行处理，先到先得，同时校验摘要、`actionable_by`，并禁止非 UI principal 选 `allow_always`。
  - ACP 驱动用官方 `agent-client-protocol` 3.3.0（锁定 `=3.3.0`）。SDK 回调里只做无界 channel 发送，ACP 类型不出 `driver.rs`。
  - 拉起适配器用 `posix_spawn`：`SETPGROUP` + `CLOEXEC_DEFAULT` + 默认信号处置；环境变量走白名单，去掉 `CUPPER_*`；stderr 写日志。
  - 崩溃恢复：按 pid 和启动时间识别进程组并清理残留；运行中的回合记为 `interrupted`，待处理请求记为 `expired`；下一次 `send` 时用 `session/resume` 继续。正常退出时也按同样逻辑清理适配器。
  - 可以按配置显式选择权限模式和模型。
  - 认证只有 `test_harness`（仅在 `test-harness` 特性下编译，密钥经继承的 stdin 传入）和只读的 `cli`。
  - `unsafe` 只在 `sys.rs`。
- **`cupper-cli`**：客户端库加命令行。库负责分帧和请求关联；`Mirror` 检查"`from` 等于上一个 seq 加 1"。命令有 watch、status、create、send（可加 `--wait`）、interrupt、pending、approve、answer。
- **`cupper-fake-agent`**：用 SDK 的 agent 角色实现，prompt 文本就是剧本。可以模拟高速输出、大消息、工具调用、审批、提问、取消、resume、泄漏的孙进程、fd 和环境变量探测。SP-1 的录制文件当时不在 main 上，没有用。
- **其他**
  - `ts/`：TS 生成链。
  - `protocol/fixtures/events.json`：含未知值的样例，Rust 和 TS 都会解析。
  - `smoke/`：真实适配器的冒烟脚本。
  - `docs/tasks/SP-4a-host-skeleton/notes.md`：返工记录、测量、冒烟日志（路径都用占位符）。

## 关键取舍

- **快照用事件序列表达**，而不是另定义一套快照 schema。这样只有一套 `apply`，分页也很自然，"快照重建等于原状态"可以做成性质测试。代价是事件模型改动时快照格式跟着变。
- **宿主的投影就是协议里的纯视图**，放在内存里，按检查点加日志懒加载。冷启动只加载 index；长会话的内存问题留给 ADR-06。
- **一条流一把锁**，追加和订阅在锁内完成，补发由写任务在锁外从 SQLite 分页读。这样订阅和追加之间不会漏或重。
- **发送时合并不需要单独判断慢连接**：队列有积压时写任务自然会合并；`coalesce_ms` 由握手协商。
- **只保存成功的命令结果**：失败的命令重试时会重新执行。`conflict` 这类错误是确定性的，重试结果不变。
- **一个会话一个适配器进程**。ADR-09 未定，这是最简单的做法。

## 验证方式

**命令**（在 `spikes/SP-4a-host-skeleton` 下）：

```bash
cargo test --workspace --features cupper-host/test-harness
cargo clippy --workspace --all-targets --features cupper-host/test-harness   # 0 warning
cargo test --release -p cupper-host --features test-harness --test measure -- --ignored --nocapture --test-threads 1
cd ts && npm ci --ignore-scripts && npm run gen && npm run check
cargo build -p cupper-protocol --target aarch64-apple-ios-sim
smoke/claude-smoke.sh <adapter-dir>; smoke/claude-midturn.sh <adapter-dir>
```

**结果**：协议 crate 16 个测试，宿主单元/性质测试 5 个，端到端 9 个，全部通过；测量测试 2 个默认忽略。端到端完整跑了 10 遍，每次都全部通过。

| 验收项 | 结果 |
|---|---|
| 1. 两个客户端同时订阅，内容一致 | 通过。晚订阅者的快照与两者一致；40,000 字符的工具输出走 blob，`blob/get` 取回的内容正确 |
| 2. 审批先到先得 | 通过。两个连接同时批准，恰好一个成功；另一个收到 `conflict/already_resolved`，带 `resolved_by`。只读 principal 和选 `allow_always` 返回 `forbidden`，摘要不对返回 `conflict`。提问走同一路径 |
| 3. 断线重连补齐，不丢不重 | 通过。按游标补发，`Mirror` 检查不变式；补发中出现了合并后的区间事件。游标早于保留窗口时改发快照。协商了 200 ms 合并窗口的客户端状态也一致 |
| 4. 慢客户端不拖住 agent | 通过。一个回合约 36,000 个事件：没有慢客户端 0.86 s，有一个完全不读的客户端 1.00 s。慢客户端收到 reset，恢复读取后按游标补齐，状态一致 |
| 5. `kill -9` 后恢复，无残留进程 | 通过。泄漏的孙进程在重启时被清理；回合为 `interrupted`，请求为 `expired`，对过期请求作答返回 `expired`；resume 成功，用的是原来的 agent 会话 |
| 6. 命令重试不重复执行 | 通过。顺序重试、两个连接并发重试、宿主重启后重试都返回同一个结果，并标记 `replayed: true`；参数不同返回 `idempotency_mismatch`；最终只有 2 个回合 |
| 7. 帧不超过上限 | 通过。宿主层的性质测试用随机超大文本和大量转义字符跑完"规范化、合并、分帧、快照"全链路。端到端用 3 MB 消息、100 kB 标题、500 kB 工具输出，最大帧 49,053 字节；客户端拒绝发送超过 64 KB 的 prompt |
| 额外：适配器隔离 | 通过。适配器继承的 fd 只有 0、1、2，没有 `CUPPER_*` 和白名单外的变量。变异检查：去掉 CLOEXEC 后测试失败，泄漏 fd 16–19 |
| 额外：协议错误 | 通过。握手前发请求返回 `not_initialized`；未知会话 `not_found`；未知方法 -32601；第二个宿主实例退出码 3 |

**测量**（release 构建）：

| 项 | 结果 |
|---|---|
| 冷启动：拉起宿主到客户端收到 index 快照（20 个会话、60 个回合、`host.db` 8.7 MB，30 次） | p50 4.2 ms，**p95 4.6 ms**，max 5.5 ms（目标 p95 ≤ 400 ms） |
| 拉起宿主到套接字可连接 | p95 4.2 ms |
| 宿主已在运行：连接到收到快照 | p95 0.3 ms |
| 宿主常驻内存：空闲 | 6.3 MiB |
| 宿主常驻内存：挂 6 个假会话 | 8.7 MiB |
| 宿主常驻内存：6 个会话各跑一个 6,000 条目的回合 | 22.5 MiB（增长来自内存里的视图） |
| 宿主常驻内存：真实 Claude 会话 | 14.9 MiB |
| 对照：一个真实 Claude 会话的进程组 | 约 675 MiB（node、claude 加上用户环境里的 uv/python） |

冷启动不含签名校验（SP-4b）和系统首次运行新二进制时的安全评估。

**真实适配器冒烟测试**：claude-agent-acp 0.91.0 加本机 claude 2.1.296；模型 haiku；权限模式显式选 `default`；只在临时 git 仓库里跑；共 4 个回合。

- **运行 1**（新建、回合、审批、kill -9、重启、resume）
  - 新建会话 963 ms。
  - 2.35 s 后收到权限请求（Bash：`printf 'hi' > hello.txt …`），通过 cupper-cli 选了 allow-once，文件写入正确。
  - `kill -9` 宿主后重启，下一条消息走 `session/resume`。agent 正确回答 `hello.txt`，**resume 成功**。
- **运行 2**（在等待审批时 `kill -9`）
  - node 适配器退出了，但 **`claude` 进程（约 300 MB）还活着**。
  - 重启时按进程组清理掉它（`reaped_processes: 1`），回合为 `interrupted`，请求为 `expired`，会话状态为 `interrupted`，文件没有被创建。
  - resume 后 agent 回答 "No … because I cancelled the earlier command"。
- codex-acp 没有做。它是可选项，而且按编排者通知，它会改写 `~/.codex/config.toml`，与约束冲突。

**TS 生成链**：可用，但需要补三处。

- `@hey-api/openapi-ts` 0.99.0 + zod 4.6.7 + typescript 6.0.3，都装在本地。8 个含未知值的样例在 zod 中全部通过，分类与 Rust 一致。
- 带 `type` 标签的联合类型生成 `… | UnknownEvent`，开放枚举生成 `'a' | … | string`。
- 需要补的三处：
  1. **TS 收窄**：`e.type === 'item.delta'` 排除不掉兜底成员（报 TS18046），`Exclude<EventBody, UnknownEvent>` 得到 `never`。可行的写法是用已知类型的字面量列表做 `Extract`，再加一个 zod 类型守卫（`ts/scripts/check.ts`）。
  2. **zod 会剥离未知联合成员的字段**：兜底成员生成的是普通 `z.object`。
  3. **64 位整数会生成 `z.coerce.bigint()`**，而 TS 类型是 `number`。已在 Rust 的 schema 导出处改为不超过 2^53−1 的上下界解决。
- 还要配 `output.importFileExtension: '.ts'`。

**iOS 编译检查**：执行了 `rustup target add aarch64-apple-ios-sim`。`cupper-protocol` 的 debug 和 release 都一次编译通过，没有改代码。

## 返工记录摘要（ADR-02 依据，详见 notes.md）

- 时间：第一行代码 15:35，最后一个功能提交 16:22（本机时钟，AI agent 耗时，不能换算成人日）。
- **编译修复 3 轮，共 4 个错误，都是一两行就修好**：
  - proptest 里整数类型不明确；
  - ACP SDK 的 `ToolCallId` 只实现了 `From<&'static str>`，传入借用的 `&str` 报 E0521，共 2 处；
  - 闭包的错误类型推断不出。
- 宿主约 3,200 行第一次编译只有 1 个错误；CLI 和测试第一次就通过。**没有和借用检查器来回拉扯**，因为全部写成 actor 加 channel、短临界区加锁、不跨 await 持锁。
- clippy 和测试发现的设计问题：
  - 依赖的 serde_json 特性被合并，导致 schema 的键顺序变化，单独测协议 crate 能过，整个 workspace 不能过；
  - `RpcError` 太大，把 data 装箱；
  - 视图查重是 O(n²)，由 t4 的耗时发现；
  - BigInt 问题；
  - 正常退出时残留适配器记录；
  - 适配器在初始化之后才记入 SQLite；
  - Claude 会沿用用户自己的 `defaultMode`。
- 端到端测试第一次运行 8 个过 5 个，失败的 3 个都是测试本身写错（阈值估高、prompt 超过帧上限、重连时用了空的 mirror）。
- **判断：不满足 ADR-02 的改变条件**。Rust 的额外成本集中在三处：约 150 行 `unsafe` 系统调用、cargo 特性合并带来的隐性行为变化、ACP SDK 3.x 的新 API 要读源码文档。都不大。

## 对 ADR 的修改建议

**ADR-01（进程形态）**
- 写明"每个适配器一个进程组；拉起后立即记录 pid 和启动时间；启动时和正常退出时都按进程组清理"。实测在等待审批时 `kill -9`，`claude` CLI 会存活下来，这一条是必需的。
- 宿主侧冷启动 p95 只有 4.6 ms，400 ms 的预算大部分可以留给签名校验和 UI。建议冷启动只加载 index，会话视图懒加载。

**ADR-02（实现语言）**
- 维持 Rust。
- 建议写进 CLAUDE.md 的约束：
  - ACP SDK 的回调里只做 channel 发送，长任务 spawn 出去；
  - SDK 锁定精确版本；
  - 导出给其他语言的产物不能依赖 serde_json 的 map 顺序；
  - `unsafe` 只在 `sys`。
- 注意 ACP Rust SDK 有些枚举是封闭的（`StopReason`、`PermissionOptionKind`、`ToolCallStatus`），适配器发来未知值时整条消息会解析失败。可以向上游反馈。目前它不算"落后于 TS SDK"。

**ADR-04（协议）**
1. 写明"所有整数不超过 2^53−1"，schema 导出用上下界代替 `format: uint64/int64`，并加 CI 检查（已有 `integers_are_js_safe`）。
2. 消息清单补上 `stream/snapshot`（快照分页，内容是事件序列）；事件信封里的 `id` 可以去掉，它和 (host_id, stream, seq) 重复。
3. TS 生成链需要大约 0.5–1 人日补一个后处理或 hey-api 插件：
   - 生成已知类型的字面量列表和类型守卫；
   - 兜底成员用 `.loose()`，保留未知字段；
   - 开放枚举用 `(string & {})`。
4. 决定"已知类型但数据残缺"怎么处理。Rust 和 zod 都会悄悄降级成未知事件。客户端可以容忍，但 CI 金样例要做严格校验，客户端最好打日志。
5. `blob/put` 要在 M1 实现：64 KB 上限对粘贴的长 prompt 立刻生效。
6. 命令的参数摘要必须基于排序键后的规范 JSON。

**ADR-05（本地传输与认证）**
- 回退套接字目录要检查属主和 0700，防止被其他用户抢先创建（spike 已实现）。
- `test_harness` 密钥经 stdin 传入可行。
- uid 校验用 tokio 的 `peer_cred()`，不需要 `unsafe`。
- 适配器隔离已有自动化测试：继承的 fd 只有 0、1、2，环境变量走白名单。

**ADR-06（事件模型与存储，待起草）**
- 采用：同一条流的 seq 连续；SQLite WAL；回合结束时写视图检查点加保留窗口；快照由视图生成。
- 阈值参考：单个 delta 的转义后长度不超过 16,000，合并后不超过 32,000；内联内容超过 16,000 改为 blob；标题截断到 1,000 字符。
- 需要补的：
  - 内存里的视图会随历史增长（6 个会话、每个 6,000 个条目时 22.5 MiB），需要淘汰空闲会话的视图，长会话的快照要分页或只给最近若干回合；
  - 回合外的输出（`available_commands_update`、自主回合）目前只记成 notice；
  - 原始 ACP 帧没有保存；
  - 断电后的持久性没有测（`synchronous=NORMAL`）。

## 对用户配置的改动（编排者脱敏摘要）

- `~/.claude`：没有修改任何配置文件。Claude Code 自己留下了两个测试会话的记录目录。`~/.claude.json` 的 `projects` 条目数前后都是 58，没有新增。是否清理由用户决定。
- `~/.codex`：没有运行 codex，也没有改动。
- 工具链：执行了 `rustup target add aarch64-apple-ios-sim`（brief 允许）。npm 包全部装在本地目录，没有全局安装。
- 留下了两个临时目录，包含数据目录、测试仓库和日志；适配器安装在本会话的 scratchpad 里。没有残留进程。

## 提交列表

```
0c84dc5 docs(task): record isolation check and spawn-record fix in SP-4a notes
2827c3e fix(spike): record adapter process right after spawn
f07fcc7 test(spike): verify adapter fd and env isolation
7154b2c docs(task): add SP-4a notes with rework record and measurements
4f05fc6 feat(spike): add real-adapter smoke scripts and reap adapters on shutdown
6b2be62 feat(spike): pin agent permission mode via ACP config options
15d349c feat(spike): generate TS types and zod validators from the schema
c7bb77f test(spike): add cold start and memory measurements
02e83ad test(spike): add end-to-end tests 1-7 and frame-limit property test
25e51e6 feat(spike): add cupper-host and cupper-cli
fd4a363 feat(spike): add scripted fake ACP agent
f591b1d feat(spike): add cupper-protocol framing, messages and event model
<编排者提交：notes 脱敏、存档本报告>
```

## 遗留问题 / 范围外发现

1. 没有实现：`blob/put`、`history/page`、steer 与排队、`session/load` 回退、`stream/ack`、`auth/elevate`、`ping`、`host/shutdown` 通知、空闲退出、多个会话共用一个适配器进程、保存原始 ACP 帧。
2. SQLite 调用是同步的，跑在 async 工作线程上。单次很短，但在事件路径上；M1 应改为专门的存储线程或批量写入。
3. 交互请求只允许出现在回合内，回合外的会被取消。提问只映射了单字段表单，Claude 的多问题 AskUserQuestion 没有用真实适配器验证。
4. codex-acp 没有测（原因见上）。
5. 慢客户端的内存上限大约是队列预算的 2 倍（写任务手上还有一批）；队列按连接计预算，不按流。
6. `sys.rs` 用了 libproc，只适用于 macOS（符合 macOS 14+ 的定位）。孤儿进程的判定是启发式的：组长复用了 pid 时不杀，组员的启动时间不能早于组长。
7. 范围外：claude 在用户环境中会拉起 uv/python 子进程，一个会话进程组约 675 MiB。按会话起进程的拓扑在内存上代价很高，ADR-09 需要考虑。
