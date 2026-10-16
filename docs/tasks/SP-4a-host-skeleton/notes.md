# SP-4a 实测笔记

- 执行者：子 agent（后端工程师，Rust）
- 分支：`spike/SP-4a-host-skeleton`
- 日期：2026-10-16
- 代码：`spikes/SP-4a-host-skeleton/`

## 环境与版本

| 项 | 版本 |
|---|---|
| 系统 | macOS 26.4（arm64） |
| Rust | rustc / cargo 1.96.0（edition 2024，`rust-version = 1.89`，因为用了 `File::try_lock`） |
| ACP SDK | `agent-client-protocol` 3.3.0（锁定 `=3.3.0`），内部依赖 `agent-client-protocol-schema` 1.11.0 |
| 其他 crate | tokio 1.53、rusqlite 0.40（bundled SQLite）、schemars 1.2.3、serde_json 1、proptest 1、libc 0.2.190 |
| Node | 24.13.0 |
| TS 生成链 | `@hey-api/openapi-ts` 0.99.0、`zod` 4.6.7、`typescript` 6.0.3、`@types/node` 24.19.2（都装在 `ts/node_modules`，没有全局安装） |
| 真实适配器 | `@agentclientprotocol/claude-agent-acp` 0.91.0（装在临时目录）+ 本机 `claude` 2.1.296 |

## 怎么跑

```bash
cd spikes/SP-4a-host-skeleton
# 单元测试、性质测试、schema 一致性、端到端测试 1–7
cargo test --workspace --features cupper-host/test-harness
# 测量（release）
cargo test --release -p cupper-host --features test-harness --test measure -- --ignored --nocapture --test-threads 1
# schema 变更后重新生成
UPDATE_SCHEMA=1 cargo test -p cupper-protocol schema
# TS 生成链
cd ts && npm ci --ignore-scripts && npm run gen && npm run check
# iOS 编译检查
rustup target add aarch64-apple-ios-sim
cargo build -p cupper-protocol --target aarch64-apple-ios-sim
# 真实适配器冒烟（<adapter-dir> 里本地安装 claude-agent-acp）
cargo build --workspace --features cupper-host/test-harness
smoke/claude-smoke.sh <adapter-dir>
smoke/claude-midturn.sh <adapter-dir>
```

## 结构

| crate | 行数（含测试） | 内容 |
|---|---|---|
| `cupper-protocol` | 约 2,100 | 帧、JSON-RPC 信封、消息、事件模型、开放枚举宏、合并规则、纯视图（快照用）、游标不变式、socket 路径、schema 导出。无 I/O |
| `cupper-host` | 约 3,200（src）+ 810（tests） | 见下面的数据流。`unsafe` 只在 `sys.rs` |
| `cupper-cli` | 约 570 | 客户端库（分帧、请求关联、`Mirror` 检查不变式）+ 命令行 |
| `cupper-fake-agent` | 约 280 | 用官方 SDK 的 agent 角色写的假 agent；prompt 文本就是剧本 |

数据流：适配器 stdout → `driver`（ACP SDK 回调里只做无界 channel 发送）→ 会话 actor（`normalize` 写入时合并、交互请求）→ `hub`（分配 seq、写 SQLite、更新内存视图、推给各连接）→ 每个连接的 `outbox`（只加锁入队，超预算就 reset）→ 写任务（发送时合并成区间事件、按 64,000 字节分帧）→ socket。

几个关键做法：
- **快照 = 一串事件体**：`SessionView::to_ops()` 产出能重建当前状态的事件序列，客户端用同一个 `apply` 处理快照和后续事件。性质测试保证"快照重建 = 原状态"。
- **订阅与追加在同一把流锁下**：订阅时先入队响应，再入队快照页或"从日志补发 (a, head]"的占位项，再登记订阅者；之后的实时事件排在后面。补发由写任务从 SQLite 分页读，不占锁。
- **幂等**：`(principal, command_id)` → 参数哈希（排序键后的规范 JSON）+ 结果，存 SQLite；并发重复用 watch channel 等第一个执行完。只存成功结果。
- **先到先得**：会话 actor 串行处理 `interaction/respond`，第一个通过摘要、principal、选项检查的生效，其余返回 `conflict/already_resolved` 并带 `resolved_by`。
- **崩溃恢复**：适配器的 pid 和启动时间记在 SQLite；启动时按进程组清理（组长还活着必须启动时间一致，组员的启动时间不早于组长）；`running` 标记的会话把运行中的回合记为 `interrupted`、待处理请求记为 `expired`；下一次 `session/send` 时拉起新适配器并 `session/resume`。

## 返工记录（ADR-02 的依据）

时间线（本机时钟）：读材料约 15:28–15:35；第一行代码 15:35；最后一个提交 16:22。协议、假 agent、宿主、CLI、端到端测试、测量、TS 链、冒烟脚本都在这段时间内完成。这是 AI 编码 agent 的耗时，不能直接换算成人日；可比较的是"时间花在哪里"。

### 编译错误（rustc 报错）

| 轮次 | 位置 | 错误 | 原因 | 修复 |
|---|---|---|---|---|
| 1 | 协议 crate 的 proptest | E0689 整数类型不明确 | `let mut pos = 0` 后调用 `saturating_add` | 加类型标注，1 行 |
| 2 | 假 agent | E0521 借用逃逸 ×2 | ACP SDK 的 `ToolCallId` 只实现了 `From<&'static str>`，传入非 `'static` 的 `&str` 被推断成 `'static` | `.to_owned()`，2 处 |
| 3 | 宿主 `sys.rs` | E0282 需要类型标注 | 用闭包包住 `posix_spawn` 的一串 `?`，错误类型推断不出 | 标注 `io::Result<pid_t>`，1 行 |

- 协议（约 2,100 行）第一次就编译通过（只有上面测试里的 1 处）；宿主（约 3,200 行）第一次编译只有上面 1 处错误；CLI、端到端测试、测量测试第一次编译即通过。
- **没有一次和借用检查器的拉锯**。原因是写法上全部是"actor + channel + 短临界区的 `std::sync::Mutex`"，没有跨 await 持锁、没有自引用结构。
- 另有一次是我自己用脚本改文件时匹配错了位置（把 `impl Driver` 里的方法替换掉了），导致"未闭合的分隔符"，用 `git checkout` 恢复后重做。这不算语言问题。

### clippy 与设计层面的修正

| 发现方式 | 问题 | 处理 |
|---|---|---|
| clippy `result_large_err` | `RpcError` 160 字节（`ErrorData` 里的 map 被 `preserve_order` 变成 IndexMap） | `data: Option<Box<ErrorData>>` |
| schema 一致性测试 | **单独测协议 crate 通过，整个 workspace 测试失败**：ACP SDK 打开了 serde_json 的 `preserve_order` 特性，cargo 特性合并后 `Map` 的键顺序变了，导出的 schema 随之变化 | 导出时递归排序键。这是 Rust 生态特有的坑：一个依赖的特性会改变另一个 crate 的行为 |
| t4（慢客户端）的耗时 | 第二个回合比第一个慢 1.8 倍。`SessionView` 对 `item.started` 的查重是线性扫描，整段会话是 O(n²) | 加 `item_id → 位置` 的索引（不序列化、不参与相等比较、按长度自动重建）。修复后 0.86 s vs 1.00 s |
| TS 生成链 | `u64`/`i64` 字段在 zod 里成了 `z.coerce.bigint()`，TS 类型却是 `number` | schema 导出时把 `format: uint64/int64` 换成 JS 安全的上下界（≤ 2^53−1），和 ADR-04 的"seq 不超过 2^53"一致 |
| 冒烟测试 | Claude 适配器会沿用用户 Claude 设置里的 `defaultMode`，resume 时还会沿用记录里的模式 | 驱动层在 `session/new` 和 `session/resume` 之后都按配置显式选择权限模式（找不到就拒绝启动会话） |
| 冒烟测试 | 正常退出时 SQLite 里留下适配器记录（actor 先停了，收不到进程退出事件） | 退出流程复用启动时的按进程组清理逻辑 |

### 测试本身的错误（不是宿主的问题）

端到端测试第一次运行 8 个过了 5 个，失败的 3 个都是测试写错：
- t3 的事件数阈值估高了（写入时合并后事件比预想少）；
- t7 想通过 prompt 发 100 KB 标题，被**客户端**按 64 KB 帧上限拒绝（这正是协议规定的行为：长 prompt 要走 `blob/put`）；改由假 agent 产生长标题；
- t5、t3（两处）重连时新建了空的 mirror 却按游标续传，状态当然不全；改为带着旧 mirror 重连。

### 卡点

没有真正卡住的地方。花时间最多的是读 ACP SDK 3.x 的新 API（`Client.builder().on_receive_*().connect_with()`、回调在分发循环里执行、`block_task` 的含义），大约 10 分钟（估计）。SDK 的 `concepts` 文档写得很清楚，按"回调里只做 channel 发送、长任务 spawn 出去"的模式写下来就没有出问题。

### 对 ADR-02"改变条件"的判断

不满足改变条件。编译修复 3 轮、4 个错误，全部是一两行的类型标注或所有权转换；没有借用检查器拉锯；时间主要花在设计和测试上。Rust 带来的额外成本集中在三处：
1. 系统调用要写 `unsafe`（`posix_spawn` 带 `CLOEXEC_DEFAULT`/`SETPGROUP`、`proc_pidinfo`、`proc_listpgrppids`、`killpg`、`waitpid`），约 150 行，集中在 `sys.rs`；单实例锁用标准库的 `File::try_lock` 即可。
2. cargo 特性合并带来的隐性行为变化（上面的 `preserve_order`），需要靠测试兜住。
3. ACP SDK 的 API 风格较新（3.x），网上的例子多是 0.x 的 `ClientSideConnection`，要读源码里的文档。

## 测量

release 构建，`tests/measure.rs`。

**冷启动**（数据目录有 20 个会话、60 个回合，`host.db` 约 8.7 MB；30 次）：

| 区间 | p50 | p95 | max |
|---|---|---|---|
| 拉起宿主 → 套接字可连接 | 3.9 ms | 4.2 ms | 5.2 ms |
| 拉起宿主 → 客户端收到 index 快照（冷启动） | 4.2 ms | 4.6 ms | 5.5 ms |
| 宿主已在运行：连接 → index 快照 | 0.2 ms | 0.3 ms | 0.3 ms |

不含：签名校验（SP-4b）、首次运行新二进制时系统的安全评估、崩溃后的恢复（只有 `running` 会话需要加载视图）。会话视图是懒加载的，冷启动只加载 index。

**常驻内存**（`ps` 的 RSS）：

| 状态 | 宿主 RSS |
|---|---|
| 空闲，1 个客户端订阅 index | 6.3 MiB |
| 6 个假会话（6 个适配器在跑） | 8.7 MiB |
| 6 个会话各跑完一个约 110 kB、6,000 个条目的回合，全部被订阅 | 22.5 MiB |
| 真实 Claude 会话（冒烟测试中，1 个会话） | 14.9 MiB |

增长主要来自内存里的会话视图（每个条目一份状态）。长会话需要 ADR-06 定义视图的淘汰或分页。参照：6 个假适配器共 45 MiB；一个真实 Claude 会话的进程组（node 适配器 + claude + 用户环境里的 uv/python 子进程）合计约 675 MiB。

**慢客户端（t4）**：一个回合约 36,000 个事件。没有慢客户端 0.86 s；有一个完全不读的客户端 1.00 s。慢客户端的发送队列超过 256 KiB 后收到 `stream/reset`，恢复读取后按游标补齐，状态与快客户端一致。

**帧大小**：t7（3 MB 消息、100 kB 标题、500 kB 工具输出）中客户端见到的最大帧 49,053 字节。

## 真实适配器冒烟测试

claude-agent-acp 0.91.0 + 本机 claude，模型 haiku，权限模式显式选 `default`，临时 git 仓库。共 4 个回合（两次运行各 2 个）。

**运行 1（`smoke/claude-smoke.sh`）：新建 → 回合 → 审批 → kill -9 → 重启 → resume**

- 新建会话（拉起适配器、initialize、session/new、选模式、选模型）：963 ms。
- 发出"创建 hello.txt"后 2.35 s 收到权限请求，标题是 `printf 'hi' > hello.txt && cat hello.txt`。选项有三个：`allow-once`（allow_once）、`allow-with-updates`（allow_always，"Yes, and allow access to repo/ and … commands"）、`reject`。通过 cupper-cli 选了 `allow-once`。
- 回合完成（4.5 s，`end_turn`），文件内容是 `hi`。
- 适配器进程组：node、claude，以及 claude 拉起的 uv/Python/python（来自用户自己的 Claude 环境）。宿主 RSS 14.9 MiB。
- `kill -9` 宿主：此时没有运行中的回合，整组进程在 1 秒内都因 stdin EOF 自行退出。
- 重启宿主（含 CLI 轮询）389 ms；下一条消息触发 `session/resume`（约 0.9 s），回合 1.8 s，回答 `hello.txt`。**resume 成功，对话记忆完整。**

**运行 2（`smoke/claude-midturn.sh`）：在等待审批时 kill -9**

- 等待审批时进程组同上。
- `kill -9` 宿主 1 秒后：node 适配器退出了，**`claude` 进程（约 300 MB）还活着**。
- 重启宿主：恢复报告 `reaped_processes: 1, interrupted_turns: 1, expired_interactions: 1`；该进程被清理；index 中会话状态为 `interrupted`；会话流里回合是 `interrupted`（`host_restarted`）、请求是 `expired`；文件没有被创建。
- 继续对话（resume）：问"hello.txt 是否创建了"，agent 答 "No … because I cancelled the earlier command"。resume 后 agent 把未回应的工具调用视为已取消。

**本机留下的记录**（不删除，由用户决定）：
- Claude 会话记录：`~/.claude/projects/<编码后的临时目录路径>/` 下各一个 `.jsonl` 和一个 `memory` 目录，会话 ID `c069bae1-824e-4a32-812e-b8cee8cc046e`、`99216f59-af74-4dc8-9940-94b7762fb86b`。
- `~/.claude.json` 的 `projects` 条目数前后都是 58，没有新增。没有修改 `~/.claude` 下的任何配置文件。
- 临时目录：`<tmp>/tmp.nhaaWW72Fj`、`<tmp>/tmp.59qn4vLHJX`（含数据目录、测试仓库、宿主日志）；适配器安装在会话的临时草稿目录。

**codex-acp 没有做**：它是可选项；SP-4b 发现 codex 在新目录运行时会向 `~/.codex/config.toml` 写入信任项，与"不修改 `~/.codex`"冲突。

## TS 生成链

- `ts/scripts/gen.mjs` 把 `protocol/schema/v1.json` 的 `$defs` 包成 OpenAPI 3.1 的 `components.schemas`，交给 `@hey-api/openapi-ts`（插件 `@hey-api/typescript` + `zod`）。生成 3 个文件，约 1,150 行。没有远程请求（只解析本地 `$ref`）。
- 带 `type` 标签的联合类型：生成 `{type: 'a'; data: A} | … | UnknownEvent`，zod 是 `z.union([...])`（不是 `z.discriminatedUnion`，因为兜底成员的 `type` 是 `string`）。开放枚举生成 `'running' | … | string`，zod 是 `z.union([z.enum([...]), z.string()])`。
- `protocol/fixtures/events.json` 的 8 个样例（未知事件类型、已知事件里的未知枚举值、未知联合成员、未知字段、数据残缺的已知类型）在 Rust 和 zod 里都解析成功，分类一致。
- 发现的问题：
  1. **TS 收窄**：`if (e.type === 'item.delta') e.data.text` 编译失败（`TS18046: 'e.data' is of type 'unknown'`），因为兜底成员的 `type: string` 也能匹配。`Exclude<EventBody, UnknownEvent>` 得到 `never`（每个已知成员都能赋给兜底类型）。可行的写法是 `Extract<EventBody, {type: 已知类型字面量}>` 加一个用 zod 校验的类型守卫（见 `ts/scripts/check.ts`）。已知类型的字面量列表最好由生成器产出。
  2. **zod 会丢掉未知联合成员的字段**：兜底成员生成的是普通 `z.object({type})`，默认剥离其他字段（`{"type":"oauth","url":…}` 解析后只剩 `type`）。TS 类型里有索引签名，zod 却没有 `.loose()`。
  3. 开放枚举在 TS 里折叠成 `string`，失去补全；可以改为 `(string & {})` 的写法，hey-api 不支持，需要后处理或自写插件。
  4. 64 位整数会生成 BigInt（已在 schema 导出处解决，见上）。
  5. `Option<T>` 生成 `T | null` 的可选字段（schemars 的 `type: [..., "null"]`），客户端要同时处理缺省和 `null`。
  6. 生成的 `index.ts` 默认用不带扩展名的相对导入，在 `nodenext` 下报错，需要 `output.importFileExtension: '.ts'`。
- 另一个一致性问题：Rust 侧（serde 的 untagged 兜底）和 zod 侧，"已知类型但数据残缺"的事件都会悄悄降级成未知事件，而不是报错。宽容读取的语义是一致的，但服务端的 bug 会被掩盖，CI 的金样例测试需要覆盖。

## iOS 编译检查

`rustup target add aarch64-apple-ios-sim`（新增了这个 target 的 rust-std），`cargo build -p cupper-protocol --target aarch64-apple-ios-sim` 的 debug 和 release 都一次通过，没有改代码。依赖只有 serde、serde_json、schemars、sha2、hex、thiserror。

## 其他发现

- ACP Rust SDK 的部分枚举是封闭的（`StopReason`、`PermissionOptionKind`、`ToolCallStatus`），未知值会导致整条消息反序列化失败；另一些用了 `#[serde(other)]`/`untagged` 兜底（`ToolKind`、`SessionConfigOptionCategory`、elicitation）。驱动层把它们映射到我们自己的开放类型，ACP 类型不出 `driver.rs`。
- SDK 的回调在分发循环里执行，回调里等待会卡住整条连接。权限请求的 `Responder` 是 `Send` 的，可以存起来，等客户端回答后在另一个任务里回应。
- 回合外的 agent 输出（如 `session/new` 之后的 `available_commands_update`）目前只记成 `agent.notice`；自主回合（Claude #1280）需要 ADR-06 定义"回合外条目"。
