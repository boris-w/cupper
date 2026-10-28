# 5.3 宿主协议 v1 草案

- 所属：`docs/02-design.md` 第 5 节
- 依据：ADR-04（含 SP-4a 修订）、ADR-05、ADR-06；技术选型讨论会架构师意见 ADR-04 一节（错误码表）；SP-1（选项与提问的实际形态）；SP-4a 的 `cupper-protocol`；设计评审会（`docs/meetings/2026-10-16-design-review/`）
- 性质：**只写契约**。Rust 类型是唯一来源（`crates/cupper-protocol`），由它导出 `protocol/schema/v1.json`。本节与导出的 schema 不一致时，以评审后修改的本节为准，再改代码
- 冻结：本节的契约在 M1a 协议 crate 合并时冻结，之后 M1b 只加方法、事件、可选字段和开放枚举的取值。必须在冻结前定下来的改动清单和冻结机制见 [5.12.5](12-milestones.md#5125-契约冻结清单)

## 5.3.1 通用约定

| 项 | 约定 |
|---|---|
| 帧 | `u32 长度（大端）\| u8 帧类型 \| 负载`，整帧不超过 64,000 字节。`0x01` JSON 消息；`0x02` blob 分片（头部：32 字节 sha-256、8 字节偏移、1 字节标志，bit0 表示最后一片）；`0x03` 预留给二进制编码。JSON 消息不跨帧（ADR-04） |
| 信封 | JSON-RPC 2.0，每帧一个对象，不支持批量数组。宿主从不向客户端发请求，只发响应和通知 |
| ID | 带前缀的字符串，见 [5.4.4](04-data-model.md#544-id-规则)。客户端把 ID 当作不透明字符串 |
| 时间 | UTC 毫秒整数；时长也用毫秒。界面要显示的时间（回合、条目的开始和结束）都写在事件的 `data` 里，不依赖信封上的 `ts`，因为快照和历史只下发事件体（桌面 #7） |
| 整数 | 全部不超过 2^53−1，schema 里用上下界表示，不用 `format: int64`（ADR-04 修订） |
| 文本流 | `item.delta` 只追加文本，不用偏移量；用 `field` 区分正文和命令输出 |
| 可选字段 | 非必填属性只会缺省，不为 `null`：schema 里不带 `null`，宿主也不发 `null`（Rust 字段都带 `skip_serializing_if`）。值本身可以为 `null` 的，在它自己的类型里声明，如 `CliPath`（`null` 表示自动发现）。上行方向宽容，宿主把 `null` 读作缺省（T016） |
| 开放类型 | 事件信封的 `type` 是普通字符串；枚举和带 `type` 标签的联合类型都是开放的，生成器产出 `Unknown` 兜底分支；读取方忽略不认识的字段 |
| 大内容 | 转义后超过 16,000 字符的内联内容一律改成 blob 引用 `{type: "blob", blob: {hash, size, media_type}, preview}`，按需用 `blob/get` 拉取（ADR-06） |
| 规范 JSON | 所有摘要（交互请求的 `digest`、账本的 `params_hash`、确认用的 `config_digest`）都对 RFC 8785（JCS）规范化后的 JSON 做 sha-256，保留完整的 256 位。Rust、TS 和 M3 的 Swift 都按 JCS 实现，金样例覆盖数字和 Unicode 转义（安全 #21） |
| 命令 | 有副作用的方法都带 `command_id`（客户端生成的 UUIDv7）和可选的 `attestation`（M3 预留）。`attestation` 签名的对象预先定为 `{host_id, session_id, request_id, request_digest, option_id, command_id, ts}` 的 JCS 形式 |
| 文案 | 宿主只下发语义码和参数，不下发界面文案。例外：agent 自己给出的文本（选项名、工具标题、错误原文）原样转发，标题和名称按 1,000 字符截断。**权限请求里要用户批准的内容（命令、路径、详情）不截断**，规则见 [5.3.6](#536-交互请求模型) |

## 5.3.2 握手与版本协商

**第一帧：`host/hello`（宿主 → 客户端，通知）**。连接建立、宿主完成对端校验后立即发出。

```json
{"jsonrpc":"2.0","method":"host/hello","params":{
  "nonce":"<64 位十六进制>","protocol":{"min":1,"max":1},"host_version":"0.1.0"}}
```

**`initialize`（客户端 → 宿主，请求）**：必须是客户端的第一条消息；**每条连接只接受一次**，再收到就返回 `forbidden`（`reason: already_initialized`）并断开（安全 #4）。

| 参数 | 说明 |
|---|---|
| `protocol: {min, max}` | 客户端支持的主版本区间 |
| `nonce` | 原样回显 `host/hello` 里的 nonce（下文记作 `h_nonce`，SP-4b 的固定令牌握手） |
| `client: {kind, name, version}` | `kind` 取 `desktop`、`cli`、`ios`、`test`（开放枚举） |
| `role` | 仅桌面端：`control`（主进程自用的控制连接）或 `window`（转发给某个窗口的数据连接），见 [5.7.1](07-desktop.md#571-进程与连接)。只有 `desktop_ui` 能声明，确定后不可变 |
| `auth` | 带 `type` 标签：`codesign`（无参数，宿主使用 accept 时的签名校验结论）；`peer`（只读 CLI）；`dev_key {c_nonce, proof}`（仅 `dev-auth` 构建）；`harness {c_nonce, proof}`（仅 `test-harness` 构建）；M3 的远程设备由远程桥在进程内接入，不经过这里。`proof` 的算法和信道绑定见 [5.8.1](08-security.md#581-认证握手固定令牌) |
| `features: [string]` | 客户端支持的可选特性，见下表 |
| `coalesce_ms?` | 希望的发送时合并窗口，宿主截断到 0–1,000 |

| 结果 | 说明 |
|---|---|
| `protocol` | 选定的主版本：双方区间交集中的最大值 |
| `host: {host_id, version, weak_auth, data_epoch}` | `weak_auth` 为 true 表示宿主运行在退化的认证模式（开发构建或自行构建，ADR-05），客户端要显示"本地鉴权较弱"，发布版客户端拒绝连接。`data_epoch` 见 [5.3.3](#533-流与同步) |
| `host_proof?` | 退化模式下宿主对客户端的证明（[5.8.1](08-security.md#581-认证握手固定令牌)），客户端校验失败就断开 |
| `principal: {id, kind, role?, scopes}` | `id` 是稳定 ID（`pr_desktop`、`pr_cli`、`pr_test`） |
| `features` | 双方都支持的特性 |
| `limits` | 见 [5.3.9](#539-限额) |

**版本规则**（ADR-04）：主版本只在破坏性变更时加 1；宿主同时支持 N 和 N−1；没有交集时返回 `version_unsupported`，`data.supported` 给出宿主的区间。

**特性（v1 起始集合）**

| 特性 | 含义 |
|---|---|
| `stream.coalesce` | 接受带 `from` 的区间事件 |
| `blob.get`、`blob.put` | blob 下载、上传 |
| `history.page` | 会话历史向前分页 |
| `interaction.permission`、`interaction.question`、`interaction.plan_confirm` | 客户端能渲染这几类交互请求 |
| `interaction.auth_url` | 能处理需要打开授权链接的提问（M1 不产生，预留） |
| `stream.ack` | M3 远程连接启用 |

宿主新增的交互请求类型必须先有对应的特性。客户端没有声明某个特性时，仍然会收到这类事件（流对所有人相同），只是显示占位："请在桌面端处理"。

## 5.3.3 流与同步

| 流 | 名称 | 内容 |
|---|---|---|
| 索引流 | `index` | 项目（含信任状态）、未归档会话的摘要、agent 状态、宿主状态、已连接的客户端、提权请求、通知意图 |
| 会话流 | `session/<session_id>` | 会话配置与能力、回合、条目、交互请求、队列、快照记录 |

**订阅**：`stream/subscribe {stream, after_seq?, epoch?}`

| 情况 | 结果 |
|---|---|
| `epoch` 等于宿主当前的 `data_epoch`，`after_seq` 不大于 head，并且仍在日志保留范围内 | `mode: replay`，之后以 `stream/events` 补发 |
| 没带游标；或者游标早于保留范围（`reason: cursor_expired`） | `mode: snapshot` |
| `epoch` 与宿主的 `data_epoch` 不同（`reason: epoch_changed`） | `mode: snapshot` |
| `after_seq` 大于宿主的 head（`reason: cursor_ahead`） | `mode: snapshot` |

- 结果为 `{stream, mode, reason?, at_seq, head_seq, epoch}`。`mode: snapshot` 时，之后以若干页 `stream/snapshot` 下发状态（`at_seq` 为快照对应的 seq），再接增量。
- **客户端收到 `mode: snapshot` 时**丢弃这条流的视图和游标，从快照重建。`epoch_changed` 之后 seq 可能被重用，Rust 的 `Mirror` 和 TS 的 `host-client` 都整体重置，不报"seq 重复"。
- **`data_epoch`**：宿主在"上次异常退出，并且之后系统重启过"时把它加 1，因为 `synchronous=NORMAL` 下断电可能丢掉最近的事务，seq 会被重用。判定规则见 [5.2.8](02-host.md#528-事件存储)。

**快照的内容**是一串事件体，客户端用处理增量时同一个 `apply` 应用（ADR-06）。

- 会话流的快照覆盖窗口：最近 20 个已结束的回合（或最多 2,000 个条目）、进行中的回合（全部条目，靠 `stream/snapshot` 分页）、全部待处理的交互请求、配置、能力和队列。
- 条目和交互请求按原始的相对顺序排列（视图按 seq 把它们放在同一个有序序列里），请求在消息流中留下的记录行位置不会丢（桌面建议 #24；v1.4 起待处理的请求显示在请求面板里，[5.7.4](07-desktop.md#574-窗口与布局)）。
- 视图生成的快照和回合快照，都按 delta 的上限切分长文本，保证每个事件体都放得进一帧。

**历史**：`history/page {session_id, cursor?, max_bytes?}` → `{ops: [EventBody], next?}`（桌面 #11）

- 返回窗口之前的内容，同样是事件体序列，用同一个 `apply` 应用；视图按 `turn.started` 里的 `turn_no` 排序，所以较早的回合会插到前面。
- 游标 `{turn_no, op_index?}`。回合之间向前翻：没有 `op_index` 时，从 `turn_no` 之前的那个回合开始。一个回合放不进一页时，按事件体的顺序从前往后分页：`next` 带同一个 `turn_no` 和下一页的起点 `op_index`，取完这个回合再继续向前翻。
- `next` 为空表示到头了。客户端对还没取完的回合显示占位"本回合还未加载完"。

**连续性**：客户端检查 `event.from`（没有就取 `seq`）等于上一个事件的 `seq` 加 1，不满足就带着游标重新订阅（ADR-04）。

**重置**：`stream/reset {stream, reason}` 表示宿主丢弃了这条订阅（`slow_consumer`：该连接的发送队列超出预算；`rebuilt`：宿主重建了这条流），客户端带游标重新订阅。

**可合并的事件**：只有 `item.delta`，按 `(item_id, field)` 合并。schema 中以 `x-mergeable: true` 标出。

## 5.3.4 方法清单

图例：**幂等**一栏，"命令"表示需要 `command_id`，宿主按账本去重；"天然"表示重复调用没有额外效果，不需要 `command_id`。**scope** 的含义见 [5.8.2](08-security.md#582-principal角色与-scope)，逐格的权限矩阵见 [5.8.3](08-security.md#583-权限矩阵)。**里程碑**一栏标出最早需要的里程碑。

### 握手、生命周期、在场

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `host/hello` | 通知 宿主→客户端 | `nonce, protocol, host_version` | — | — | — | — | M1a |
| `initialize` | 请求 | 见 5.3.2 | 见 5.3.2 | `version_unsupported`、`unauthenticated`、`forbidden/already_initialized` | — | 无 | M1a |
| `ping` | 请求 | — | `{}` | — | 天然 | 无 | M1a |
| `client/presence` | 通知 客户端→宿主 | `{foreground, session_id?}` | — | — | 天然 | read | M1a |
| `host/status` | 请求 | — | `{version, pending_upgrade, running_turns, pending_interactions, clients}`；test-harness 构建另带 `pid` | — | 天然 | read | M1a |
| `host/prepare_upgrade` | 请求 | `{command_id, mode: wait\|force}` | `{state: waiting\|exiting, running_turns}` | `forbidden` | 命令 | privileged | M1b |
| `host/exit` | 请求 | `{command_id, force}` | `{}`，随后宿主退出 | `conflict`（有运行中的回合且未 force） | 命令 | privileged | M1a |
| `host/shutdown` | 通知 宿主→客户端 | `{reason: user_quit\|upgrade\|idle, restart_expected}` | — | — | — | — | M1a |
| `ui/navigate` | 通知 宿主→客户端（只发给 control 连接） | `{session_id, request_id?}`：用户点了宿主弹出的系统通知 | — | — | — | — | M1b（AP-3） |

### 流与内容

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `stream/subscribe` | 请求 | `{stream, after_seq?, epoch?}` | `{stream, mode, reason?, at_seq, head_seq, epoch}` | `not_found`、`rate_limited` | 天然 | read | M1a |
| `stream/unsubscribe` | 请求 | `{stream}` | `{}` | — | 天然 | read | M1a |
| `stream/events` | 通知 宿主→客户端 | `{stream, events: [Event]}` | — | — | — | — | M1a |
| `stream/snapshot` | 通知 宿主→客户端 | `{stream, at_seq, epoch, page, last, ops: [EventBody]}` | — | — | — | — | M1a |
| `stream/reset` | 通知 宿主→客户端 | `{stream, reason}` | — | — | — | — | M1a |
| `stream/ack` | 通知 客户端→宿主 | `{stream, seq}` | — | — | — | read | M3 |
| `history/page` | 请求 | `{session_id, cursor?: {turn_no, op_index?}, max_bytes?}` | `{ops: [EventBody], next?: {turn_no, op_index?}}` | `not_found` | 天然 | read | M1a |
| `blob/get` | 请求 | `{hash, offset?, length?}` | 先发 `0x02` 分片，再回 `{hash, size, offset, length, chunks}` | `not_found` | 天然 | read | M1a |
| `blob/put` | 请求 | 先发 `0x02` 分片，再请求 `{hash, size, media_type}` | `{hash}` | `too_large`、`conflict`（哈希或长度不符） | 天然（按内容寻址） | write | M1a |

### 项目与 agent

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `project/add` | 请求 | `{command_id, path}` | `{project_id, trust: {state, findings, findings_omitted?, trust_digest}}`（同一路径重复添加返回已有的 ID；扫描范围里没有文件时直接为 `trusted`，否则为 `untrusted`） | `not_found`（路径不存在） | 命令 | write | M1a |
| `project/trust` | 请求 | `{command_id, project_id, trust_digest}` | `{trust_state}` | `conflict/digest_mismatch`（确认之后项目配置又变了）、`unavailable/scan_failed` | 命令 | privileged（只能经原生确认，[5.2.6](02-host.md#项目信任)） | M1a |
| `project/trust_info` | 请求 | `{project_id}` | `{path, state, findings, findings_omitted?, trust_digest, diff_from_trusted?, diff_omitted?}`：重新扫描，供主进程弹原生确认框；`path` 是添加项目时解析出的目录（T135），确认框据此显示是哪个项目 | `not_found` | 天然 | read | M1a |
| `project/remove` | 请求 | `{command_id, project_id}` | `{}`（项目下的会话一并归档） | `conflict`（有运行中的回合） | 命令 | write | M1b |
| `agents/capabilities` | 请求 | `{agent}` | 该 agent 最近一次的能力描述（[5.3.7](#537-会话能力描述)）及其采集时间 | `unavailable`（CLI 或运行时不可用，带 `reason`） | 天然 | read | M1a |
| `agents/refresh` | 请求 | `{agent}` | `{status}`：重新发现 CLI、检查版本；能力缓存为空时做一次探测 | `unavailable` | 天然 | write | M1a |
| `agents/restart` | 请求 | `{command_id, agent}` | `{}`：重启该 agent 的适配器，影响它的所有会话 | — | 命令 | write | M1b |

### 会话

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `session/create` | 请求 | `{command_id, agent, project_id, workspace: {kind: local} \| {kind: worktree, base_ref, branch_hint?}, config?: {permission_mode?, model?, reasoning_effort?}, message: PromptContent}` | `{session_id}`（已受理；第一条消息与受理同事务落盘；工作区准备、agent 连接、第一个回合都在会话流里体现） | `not_found`、`unavailable`（含 `project_untrusted`）、`unsupported`（如非 git 项目选 worktree） | 命令 | write；放宽档位时按档位规则 | M1a（worktree 为 M1b） |
| `session/send` | 请求 | `{command_id, session_id, content: PromptContent, mode: queue\|steer, expected_turn_id?, reply_to?}` | `{disposition: started\|queued\|steered, turn_id?, queue_id?, fallback_reason?}`；队列暂停时（例如 `config_unconfirmed`、`project_config_changed`）照常排队 | `not_found`、`unavailable`、`too_large` | 命令 | write | M1a（steer 为 M1b） |
| `session/interrupt` | 请求 | `{command_id, session_id, expected_turn_id?}` | `{turn_id?}` | `conflict/turn_mismatch` | 命令 | write | M1a |
| `session/queue_resume` | 请求 | `{command_id, session_id}` | `{}`：恢复已暂停的队列 | `conflict/config_unconfirmed`、`unavailable/project_untrusted` | 命令 | write | M1a |
| `session/queue_edit` | 请求 | `{command_id, session_id, expected_version, ops: [QueueOp]}` | `{version, steer?: {disposition: steered\|queued, turn_id?, fallback_reason?}}` | `conflict/version_mismatch`、`not_found`（`queue_id` 不在队列里）、`unsupported`（M1 里的 `remove`、`move`） | 命令 | write | M1b（`replace`、`steer`，用户决定 F3）；M2（`remove`、`move`，CP-8） |
| `session/set_config` | 请求 | `{command_id, session_id, option, value}` | `{config}` | `unsupported`、`agent_error`、`forbidden/scope` | 命令 | write；放宽档位时按档位规则（[5.8.2](08-security.md#582-principal角色与-scope)） | M1b |
| `session/confirm_config` | 请求 | `{command_id, session_id, config_digest}` | `{}`：确认会话当前的档位，解除 `config_unconfirmed`（[5.2.6](02-host.md#配置项)） | `conflict/digest_mismatch`（确认期间配置又变了） | 命令 | privileged（只能经原生确认） | M1a |
| `session/rename` | 请求 | `{command_id, session_id, title}` | `{}` | — | 命令 | write | M1b |
| `session/archive` | 请求 | `{command_id, session_id, worktree: keep\|remove\|remove_case_only_dirty}` | `{worktree: kept\|removed\|blocked, blockers?}` | `conflict`（有运行中的回合） | 命令 | write | M1b |
| `session/restore` | 请求 | `{command_id, session_id}` | `{mode: full\|read_only}` | — | 命令 | write | M1b |
| `session/mark_read` | 请求 | `{session_id, up_to_seq}` | `{}` | — | 天然（只前移） | read | M1a |
| `session/get` | 请求 | `{session_id}` | 完整摘要和工作区信息 | `not_found` | 天然 | read | M1a |
| `session/capabilities` | 请求 | `{session_id}` | 能力描述 | `not_found` | 天然 | read | M1a |
| `session/list_archived` | 请求 | `{cursor?}` | `{sessions, next?}` | — | 天然 | read | M1b |

`PromptContent` 是内容块数组：`text {text}`、`image {blob}`、`file_ref {path}`（`@` 引用，相对工作区）、`review_comment {path, line_start, line_end, side, quote, comment}`（GT-2，宿主负责把评论排成给 agent 的文本）。单个 `text` 块超过帧预算时，客户端先 `blob/put`，再用 `text_blob {blob}` 引用。v1.4 起，桌面端把行内评论作为输入框的附件，随下一条消息以 `review_comment` 块发出（GT-2 v1.3，[5.7.4](07-desktop.md#574-窗口与布局)）。

`session/send` 的 `reply_to`（v1.5）：桌面端"拒绝并附一条消息"时，填被拒绝的 `request_id`（[5.7.8](07-desktop.md#578-安全加固的落点) 的 RP-12）。宿主核对这个请求属于同一会话并且已经 resolved，然后把它写进 `user_message` 条目和排队条目，只用于显示"附于对 … 的拒绝"；不改变排队和插话的规则。

`QueueOp`（v1.4 定义，对应用户决定 F3 中"排队消息的编辑和插话"）：

- `{op: replace, queue_id, content: PromptContent}`：改写一条排队消息；
- `{op: steer, queue_id}`：把这条消息从队列里取出，立即插进当前回合，规则同 `session/send` 的 `mode: steer`。回退时这条消息留在原位，结果里的 `steer` 带 `disposition: queued` 和 `fallback_reason`。`steer` 只能单独出现在 `ops` 里；
- `{op: remove, queue_id}`、`{op: move, queue_id, before_queue_id?}`：属于 CP-8（M2），M1 返回 `unsupported`。

### 交互请求

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `interaction/respond` | 请求 | `{command_id, session_id, request_id, request_digest, outcome, input?}`；`input: {kind: click\|key\|native_confirm, key?: enter\|space\|escape\|cmd_enter\|digit, digit?}` 由桌面主进程写入（`digit` 是 v1.4 为请求面板的数字键新增的，取 1–9，宿主核对它与所选选项的序号相符），来自 window 角色时必须有，缺了就拒绝（[5.7.8](07-desktop.md#578-安全加固的落点)），其他 principal 的这个参数被忽略 | `{request_id}` | `conflict/already_resolved`（带 `resolved_by`）、`conflict/digest_mismatch`、`conflict/too_early`、`expired`、`forbidden`（含 `user_activation_required`）、`not_found`（会话里没有这个请求） | 命令 | approve；选项 `requires: privileged` 时 privileged；带 `mode_after` 的选项另按档位规则 | M1a |
| `interaction/get` | 请求 | `{session_id, request_id}` | 完整的交互请求和当前状态（供主进程弹原生确认框） | `not_found` | 天然 | read | M1a |

### Git、工作区与文件（M1b）

全部在宿主的 git 服务里按三档调用执行（[5.2.10](02-host.md#5210-git-与-worktree-服务)）。

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope |
|---|---|---|---|---|---|---|
| `git/status` | 请求 | `{session_id}` | `{branch, upstream?, ahead, behind, in_progress?, files: [{path, index, worktree}]}` | `unsupported/not_a_repo`、`git_error/unsafe_repo_config` | 天然 | read |
| `git/diff` | 请求 | `{session_id, scope: unstaged\|staged\|last_turn\|{turn: id}}` | `{diff_token, files: [{path, old_path?, change, additions, deletions, binary, filter_unsupported?}], snapshot?: {status, excluded}, stat: {files, added, removed}, caveats: [mixed_changes\|late_tool_effects]}` | `unavailable`（`snapshot_missing`、`index_unmerged`） | 天然 | read |
| `git/file_diff` | 请求 | `{session_id, diff_token, path}` | `{path, hunks: [{hunk_id, unit: hunk\|mode\|whole_file, header, lines \| lines_blob}]}`；`filter_unsupported` 的路径没有可操作的 hunk | `conflict/stale_diff` | 天然 | read |
| `git/stage`、`git/unstage`、`git/revert` | 请求 | `{command_id, session_id, diff_token, target: {all} \| {paths} \| {hunk_ids}, input?}`；`git/revert` 来自 window 角色时必须带 `input`（同 `interaction/respond`） | `{}` | `conflict/stale_diff`、`conflict/outcome_unknown`、`git_error`（含 `unsafe_repo_config`、`filter_unsupported`）、`forbidden`（含 `user_activation_required`） | 命令 | write |
| `git/commit` | 请求 | `{command_id, session_id, message}` | `{oid}` | `git_error`（`hook_failed`、`nothing_to_commit`）、`conflict/outcome_unknown` | 命令 | write |
| `git/push` | 请求 | `{command_id, session_id, set_upstream?}` | `{remote, branch}` | `git_error`（`auth_failed`、`rejected`） | 命令 | write |
| `git/branches` | 请求 | `{project_id}` | `{current, local, remote}` | — | 天然 | read |
| `worktree/check` | 请求 | `{session_id}` | `{removable, blockers: [rule], warnings: {ignored_files, case_only_dirty}}` | — | 天然 | read |
| `files/search` | 请求 | `{session_id \| project_id, query, limit?}` | `{results: [{path, score, ranges_utf16}]}` | — | 天然 | read |
| `workspace/open` | 请求 | `{session_id, target: terminal\|editor\|finder}` | `{}` | `unavailable/app_not_found` | 天然 | ui_local |

`git/diff` 的 `caveats`：

- `mixed_changes`：`last_turn` 和 `turn:<id>` 范围总是带（本地模式下可能混入用户自己或其他会话的改动，GT-1 要求注明）；worktree 会话也带，但界面可以弱化显示。
- `late_tool_effects`：这个回合里有 `late_tool_output` 条目，或者回合进行期间还有 `may_still_run` 的命令。界面注明"包含取消后仍在运行的命令造成的改动"（纪要 D4）。

### 设置、认证、诊断、测试

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `settings/get` | 请求 | — | `{settings}` | — | 天然 | read | M1a |
| `settings/update` | 请求 | `{command_id, patch}` | `{settings}` | `forbidden`（安全相关的键） | 命令 | write；安全相关的键 privileged | M1a |
| `auth/elevate` | 请求 | `{scopes: [write\|approve], reason?}` | 用户决定后返回 `{granted, expires_at}`；最长等待 2 分钟 | `forbidden/lineage`、`rate_limited`、`timeout` | — | 无（仅 `cli`） | M1a |
| `auth/elevation_get` | 请求 | `{elevation_id}` | `{verified: {executable, signing, pid, parent_chain, has_tty, origin_unverifiable}, self_reported: {client_name, reason}, scopes}` | `not_found` | 天然 | privileged | M1a |
| `auth/elevation_respond` | 请求 | `{command_id, elevation_id, decision: grant\|deny, scopes}` | `{}` | `expired` | 命令 | privileged | M1a |
| `policy/describe` | 请求 | `{method, params}` | `{action, details}`：主进程弹原生确认框前，用它取得要确认的操作的语义描述 | — | 天然 | privileged | M1a |
| `diag/export` | 请求 | `{command_id, include_sessions?}` | `{blob}`（zip） | — | 命令 | ui_local | M2（NFR-12 P1） |
| `diag/recording_export` | 请求 | `{session_id}` | `{blob}`（录制格式的 JSONL，[5.9.3](09-testing.md#593-录制与金样例)） | — | 天然 | 仅 `test-harness` 和 `dev-auth` 构建 | M1a |
| `test/clock_advance`、`test/run_maintenance`、`test/seed` 等 | 请求 | 见 [5.2.16](02-host.md#5216-时间与可测性)、[5.9.9](09-testing.md#599-测试替身与注入点) | — | — | — | test（仅 `test-harness` 构建） | M1a |

## 5.3.5 事件类型目录

事件信封：`{seq, from?, type, v, ts, cause?, fallback?, data}`（ADR-06）。`cause` 为 `{command_id, principal}`，客户端据此认出自己命令的结果。下表中"可合并"指发送时合并。

### 索引流

| type | 可合并 | data 要点 | 何时写入 |
|---|---|---|---|
| `project.upserted` | 否 | `{project_id, name, path, is_git, added_at, position, trust_state}`（`trust_state` 见 [5.2.6](02-host.md#项目信任)） | 添加、改名、排序、信任状态变化 |
| `project.removed` | 否 | `{project_id}` | 移除 |
| `session.upserted` | 否 | `SessionSummary {session_id, project_id, agent, title, title_source, workspace_kind, branch?, status, unread, pending_interactions, pending_kind?, last_activity_at, error_code?, read_only}`；`pending_interactions` 是待处理请求的数量；`pending_kind`（v1.4）是最早一个待处理请求的类别：`permission`、`plan_confirm` 为 `approval`，`question`、`auth_url` 为 `input`，侧边栏据此显示"等待批准"或"需要用户输入" | 状态、标题、分支、未读变化（`IndexActor` 合并 100 ms 内的连续变化） |
| `session.removed` | 否 | `{session_id, reason: archived}` | 归档 |
| `agents.updated` | 否 | `{agents: [AgentStatus]}`：每个 agent 的 CLI（路径、版本、与已验证范围的关系）、适配器（版本、运行时状态）、登录状态（只有类型，不含邮箱和组织）、资源占用、配置目录的校验结果；另含 git 版本和 git-lfs | 检测结果或采样变化 |
| `host.updated` | 否 | `{version, weak_auth, pending_upgrade, power_assertion, last_exit_abnormal?}` | 变化时 |
| `clients.updated` | 否 | `{clients: [{conn_id, principal_kind, role?, client_name, client_version, since, auth: verified\|weak\|peer, elevated?: {scopes, expires_at}}]}` | 连接、断开、提权 |
| `elevation.requested` | 否 | `{elevation_id, client_name, scopes, requested_at}` | CLI 申请提权 |
| `elevation.resolved` | 否 | `{elevation_id, decision: granted\|denied\|expired}` | 用户决定或超时 |
| `notification.intent` | 否 | `{intent_id, session_id, reason: needs_action\|turn_completed\|error, request_id?}` | 见 [5.2.9](02-host.md#529-关注度与通知)；不进入快照 |

### 会话流

| type | 可合并 | data 要点 | 何时写入 |
|---|---|---|---|
| `session.configured` | 否 | `{agent, workspace: {kind, path, branch?, base_ref?}, config: {<option>: value}, read_only, project_config: loaded\|ignored, confirmation_required?: {reason: full_access_unconfirmed\|tier_above_setter, config_digest}}`；后写的覆盖先写的 | 创建、配置变化、恢复、项目信任状态变化 |
| `session.capabilities` | 否 | 能力描述（[5.3.7](#537-会话能力描述)） | agent 会话建立或 resume 后；能力变化时。只在内容变化时写（v1.6）：Claude 每个会话和每个回合开始都推一次完整的命令列表（22–32 KB），内容相同的不写（[5.6.2](06-agent-driver.md#562-驱动接口)） |
| `workspace.preparing` | 否 | `{phase: worktree_add\|checkout\|refresh, progress?}` | 创建 worktree 期间 |
| `workspace.ready` / `workspace.failed` | 否 | `{path, branch?}` / `{code}` | 准备完成或失败 |
| `turn.started` | 否 | `{turn_id, turn_no, origin: user\|queued\|steer_new_turn\|autonomous, started_at}`；`turn_no` 是会话内从 1 递增的序号，视图按它排序 | 回合开始 |
| `turn.completed` | 否 | `{turn_id, status: completed\|cancelled\|failed\|interrupted, stop_reason?, ended_at, duration_ms, usage?: {input, output, cached?, cost?}, error_code?, final_item_ids: [item_id]}`；`final_item_ids` 的判定见下文 | 回合结束 |
| `item.started` | 否 | `{item_id, turn_id?, kind, started_at, group_id?, ...}`（按 `kind` 见下表） | 条目开始 |
| `item.delta` | **是** | `{item_id, field: text\|output, text}`：`text` 是消息、思考的正文，`output` 是工具的命令输出 | 文本流 |
| `item.updated` | 否 | `{item_id, ...要更新的字段}` | 工具参数补全、输出状态、diff 等 |
| `item.completed` | 否 | `{item_id, status: completed\|failed\|cancelled\|abandoned, ended_at, may_still_run?, ...最终字段}` | 条目结束 |
| `interaction.requested` | 否 | 见 [5.3.6](#536-交互请求模型) | agent 发起请求 |
| `interaction.resolved` | 否 | `{request_id, status: resolved\|cancelled\|expired, outcome?, resolved_by?: {principal_id, client_kind, client_name}, reason?}` | 处理、撤回、失效 |
| `queue.updated` | 否 | `{version, paused?: {reason: turn_cancelled\|turn_failed\|turn_interrupted\|config_unconfirmed\|project_config_changed}, entries: [{queue_id, preview, content, reply_to?, created_at, created_by}]}`；`content` 是完整的 `PromptContent`（v1.4，编辑排队消息时用） | 入队、出队、暂停、恢复、编辑 |
| `snapshot.recorded` | 否 | `{turn_id, phase: start\|end, status: ok\|partial\|failed\|skipped, reason?, excluded: [{path, reason}], stat?: {files, added, removed}}`；`stat` 只在 `phase: end` 时有，是相对起始快照的统计；`reason` 取 `timeout`、`unsafe_repo_config` 等；tree id 不下发 | 拍完快照 |
| `usage.updated` | 否 | `{context_used, context_size, cost?}`；后写的覆盖先写的 | agent 上报用量（显示是 P1，数据从 M1 起记录） |

**条目类型**（`item.started.kind`）

| kind | 字段 | 说明 |
|---|---|---|
| `user_message` | `content: PromptContent`（文本走 delta），`steered`，`queue_id?`，`reply_to?`（v1.5） | 用户消息；Steer 进当前回合的标 `steered: true` |
| `agent_message` | 文本走 delta | Markdown |
| `thought` | 文本走 delta | 思考或推理摘要，可折叠（MS-2） |
| `tool_call` | `ToolCall`（见下） | 一次工具调用，按适配器的 `toolCallId` upsert |
| `late_tool_output` | `{ref: {item_id, turn_id, title, command?}, output: Content, output_state, exit_code?, signal?}`，输出走 `field: output` 的 delta | 被取消回合里仍在运行的命令后来产生的输出和结束状态（Q14，纪要 D4，规则见 [5.2.6](02-host.md#晚到的工具输出)）。每个原工具只有一个这样的条目 |
| `plan` | `{source: entries\|markdown, entries?: [{content, status: pending\|in_progress\|completed, priority?}], markdown?: Content}` | MS-4。Claude 在 SDK 会话里只有计划模式的 Markdown（SP-1） |
| `notice` | `{code, params}`，`code` 取 `agent_restarted`、`session_resumed`、`compaction`、`upstream_retry`（带 `attempt`、`max`）、`snapshot_unavailable`、`repo_config_changed`、`unsafe_repo_config`、`project_config_changed`、`config_unconfirmed` 等 | 消息流中的提示（MS-5） |
| `error` | `{code, agent_code?, retryable, guidance, raw?}` | 统一的错误条目（ADR-06），`guidance` 是恢复指引的语义码，如 `run_login_in_terminal`、`upgrade_cli`、`quota_exhausted`、`retry` |

**`ToolCall` 字段**（宿主输出语义，客户端只负责排版，对齐会意见 1.3 第 8 条）

| 字段 | 说明 |
|---|---|
| `tool_kind` | 开放枚举：`read`、`edit`、`delete`、`move`、`search`、`execute`、`think`、`fetch`、`switch_mode`、`other` |
| `title` | agent 给的标题（截断）；只在没有结构化字段时作为界面的后备 |
| `paths` | 涉及的路径，工作区内的写成相对路径 |
| `query?` | 搜索词（`search`） |
| `url?` | 访问的地址（`fetch`） |
| `command?`、`cwd?` | 命令文本（Codex 的 `sh -c` 由适配器拆开） |
| `exit_code?`、`signal?` | 退出码，由提取层取得（[5.6.3](06-agent-driver.md#563-提取层claude-与-codex-对照)） |
| `output?` | `Content`：输出在 16,000 字符以内时内联，超过后改为 blob |
| `output_state` | `none`（还没有输出）、`streaming`（经 `field: output` 的 delta 追加）、`truncated`（超过内联上限后停发 delta，宿主每秒至多一次用 `item.updated` 更新 `output_tail` 和 `output_bytes`）、`complete`（结束时给出完整输出：内联或 blob 引用加尾部预览）。Claude 的输出不流式，结束时一次给出（K22，桌面 #10） |
| `output_tail?`、`output_bytes?` | `truncated` 时的尾部预览（最后 4,000 字符）和已收到的总字节数 |
| `diffs?` | `[{path, change: add\|update\|delete, hunks?: Content, stat: {added, removed}}]`，来自 agent 的片段，只用于展示过程；回合级的变更以 git 为准 |
| `parent_item_id?` | 子代理产生的工具调用指向父条目（MS-6 为 P1，字段从 M1 起预留，提取层能取到就填） |
| `duration_ms` | 宿主按开始和结束时间计算 |
| `extraction` | `full`、`partial`、`none`：拿不到退出码或输出时降级显示（MS-3） |
| `group_id` | 活动组，规则见下文 |

**按 `tool_kind` 必须给出的字段**（提取层和金样例都以此为准；拿不到时 `extraction` 降为 `partial`，界面退回 `title`）

| `tool_kind` | 必须的字段 | 界面的单行摘要（示意） |
|---|---|---|
| `read` | `paths` | 已读取 a.rs |
| `edit`、`delete`、`move` | `paths` | 已编辑 a.rs |
| `search` | `query`；`paths` 为搜索范围 | 已在 src 中搜索 foo |
| `execute` | `command`、`cwd`；结束后 `exit_code` | 已运行 cargo test |
| `fetch` | `url` | 已访问 example.com |
| `think`、`switch_mode`、`other` | `title` | 原样显示 |

**活动组**（`group_id`，桌面 #9）：同一回合内连续的 `tool_call` 共用一个组，组内的 `thought` 不打断分组，显示在组内；其他任何条目（`agent_message`、交互请求、`error`、`notice`、`plan`、Steer 插进来的用户消息、`late_tool_output`）都会结束当前的组。`late_tool_output` 永远不并入活动组。

**最终回答**（`turn.completed.final_item_ids`，桌面 #8）：宿主在回合结束时判定。

- 判定只把 `tool_call` 和组内的 `thought` 算作"过程条目"；`late_tool_output`、`notice`、`error`、`plan`、交互请求都不参与判定。
- 最终回答是最后一个过程条目之后的全部 `agent_message`；回合里没有过程条目时，是全部 `agent_message`。Claude 的一次回答常拆成多个 `message_id`，所以是列表。
- 回合以工具调用结束、后面没有文字时，`final_item_ids` 为空，界面只显示折叠标题。
- Codex 如果经 `_meta` 透出了 `agentMessage.phase`，以它为准（是否透出，未核实，待定问题 Q31）。
- 哪些条目显示在回合折叠之外，见 [5.7.4](07-desktop.md#574-窗口与布局)。

## 5.3.6 交互请求模型

**`interaction.requested` 的 data**

```
{ request_id, turn_id?, item_id?, created_at,
  digest,                       // 宿主计算，客户端原样回显（ADR-04）
  actionable_by: [principal_kind],
  request: Permission | Question | PlanConfirm | AuthUrl | Unknown }
```

| 请求类型 | 字段 | 回答 `outcome` |
|---|---|---|
| `permission` | `{title, tool: {item_id?, tool_kind, command?, paths}, options: [Option], detail?: Content, content_truncated}` | `selected {option_id}`；`dismissed` |
| `question` | `{prompt, fields: [{field_id, label, type: single_choice\|multi_choice\|text\|boolean\|number, choices?: [{value, label, description?}], allow_custom, required}], unsupported_schema}` | `answer {values: {field_id: value}}`；`dismissed` |
| `plan_confirm` | `{plan: Content, options: [Option]}` | `selected {option_id}`；`dismissed` |
| `auth_url` | `{url, message}`（M1 不产生） | `opened`；`dismissed` |

**要用户批准的内容不截断**（安全 #6）：`permission` 的 `tool.command`、`tool.paths`、`detail` 都完整下发，不受 1,000 字符的截断规则约束。转义后超过 16,000 字符的改为 blob 引用加预览，并设 `content_truncated: true`；界面标"已截断"，展开看到全文后批准按钮才可用（[5.7.4](07-desktop.md#574-窗口与布局)）。摘要覆盖完整内容（blob 按哈希计入）。

**选项**：`Option { option_id, name, kind, effect, requires }`

| 字段 | 取值 |
|---|---|
| `name` | agent 给的原文，逐字下发，不截断 |
| `kind` | ACP 原样的 `allow_once`、`allow_always`、`reject_once`、`reject_always`；**未知的 kind 原样下发**（分接层为了让 SDK 能解析而做的改写不会出现在协议里，[5.6.10](06-agent-driver.md#5610-线路分接层)） |
| `effect.scope` | `once`、`session`、`persistent`、`mode_switch`、`reject`、`plan_accept`、`plan_revise`、`unknown` |
| `effect.persistent` | 是否会写入 agent 自己的权限规则或执行策略（AP-2 要求界面标明） |
| `effect.ends_turn` | 选它会不会结束整个回合（Codex #556） |
| `effect.mode_after?`、`effect.tier_after?` | 选它之后会话切换到的模式，以及这个模式的档位（[5.6.5](06-agent-driver.md#565-配置项映射)） |
| `effect.rule?` | 持久选项将写入的规则：`{text?, location?, prefix?, prefix_risk: normal\|interpreter\|unknown}`。适配器在 `_meta` 里透出了规则或写入位置就填；`prefix` 从选项原文或 `rawInput` 里解析；前缀是解释器一类（`bash`、`sh`、`zsh`、`python*`、`node`、`env`、`sudo`、`xargs` 等）时为 `interpreter`，解析不出前缀时为 `unknown` |
| `requires` | `approve` 或 `privileged`。`kind` 为 `allow_always` 的、`effect.scope` 为 `unknown` 的一律 `privileged`：非 UI principal 不能选（ADR-05），UI 也必须经主进程的原生确认框（ADR-03）。带 `tier_after` 的选项另按档位规则判定（[5.8.2](08-security.md#582-principal角色与-scope)） |

持久选项的原生确认框逐字显示选项原文；`effect.rule` 里没有规则或写入位置时，明确写出"具体规则和写入位置由 agent 决定，以原文为准"，不留空；`prefix_risk` 为 `interpreter` 时附加"等于允许任意命令"，为 `unknown` 时附加"可能放行更宽的范围"（安全确认）。同时按待定问题 Q7 向上游反馈，请适配器在 `_meta` 里透出规则和写入位置。

**选项 ID 到生效范围的对照表**（SP-1 实测，claude-agent-acp 0.91.0、codex-acp 2.2.2）

对照表是数据，按锁定的适配器版本维护在驱动层（[5.6.4](06-agent-driver.md#564-交互请求的分类与选项映射)），每次升级适配器都用录制回归；CI 检查锁定版本的录制里出现的每个 `option_id` 都在表里（[5.9.5](09-testing.md#595-兼容矩阵)）。

| agent | 场景 | option_id | ACP kind | scope | persistent | ends_turn | requires | 界面要点 |
|---|---|---|---|---|---|---|---|---|
| Claude | 文件编辑（Write、Edit、MultiEdit、NotebookEdit） | `allow-once` | allow_once | once | 否 | 否 | approve | — |
| Claude | 文件编辑 | `allow-with-updates` | allow_always | session | 否 | 否 | privileged | agent 原文如 "allow all edits in claude/ during this session" |
| Claude | 命令（Bash）及其他工具 | `allow-with-updates` | allow_always | persistent | **是** | 否 | privileged | **同一个 option_id，在命令上会写入持久规则**，按工具区分；无法判断时按 persistent 处理。原文里带命令前缀 |
| Claude | 任意 | `reject` | reject_once | reject | 否 | 否 | approve | 拒绝后回合继续 |
| Claude | 计划确认（ExitPlanMode，`switch_mode`） | `exit-plan-default` | allow_once | plan_accept，`mode_after: default` | 否 | 否 | approve | 之后的编辑逐个审批 |
| Claude | 计划确认 | `exit-plan-auto` | allow_always | mode_switch，`mode_after: auto`（elevated） | 否 | 否 | privileged | 切换到低打扰的 Auto 模式（U6）；比经模式选择器切换更严，不放宽 |
| Claude | 计划确认 | `exit-plan-clear-auto` | allow_always | mode_switch，`mode_after: auto`（elevated），并清空上下文 | 否 | 否 | privileged | — |
| Claude | 计划确认 | `exit-plan-bypass` | allow_always | mode_switch，`mode_after: bypassPermissions`（full_access） | 否 | 否 | privileged | 等于切换到完全访问 |
| Claude | 计划确认 | `reject` | reject_once | plan_revise | 否 | **是** | approve | 界面写明"拒绝将结束本回合"，用户另发 prompt 写修改意见。真实 CLI 拒绝后以 `cancelled` 结束回合，工具调用标 `nonExecutionKind: user-rejected`（T066，2.1.296；原先写的"否、继续规划"与实测不符） |
| Codex | 文件修改 | `allow_once` | allow_once | once | 否 | 否 | approve | — |
| Codex | 文件修改 | `allow_for_session` | allow_always | session | 否 | 否 | privileged | "don't ask again for these files" |
| Codex | 命令 | `allow_once` | allow_once | once | 否 | 否 | approve | — |
| Codex | 命令 | `accept_execpolicy_amendment` | allow_always | persistent | **是** | 否 | privileged | 写入 Codex 的执行策略 |
| Codex | 文件修改、命令 | `cancel` | reject_once | reject | 否 | **是** | approve | 界面写明"拒绝将结束本回合"（ADR-07） |
| Codex | 计划确认（`toolCallId` 以 `plan-review:` 开头） | `implement_plan` | allow_once | plan_accept | 否 | 否 | approve | 在同一个 prompt 里继续实施 |
| Codex | 计划确认 | `revise_plan` | reject_once | plan_revise | 否 | 未实测 | approve | 按"可能结束回合"提示，兼容套件补测 |
| 任意 | 对照表里没有的 option_id，kind 已知 | 按 kind 兜底 | allow_once → once / approve；allow_always → persistent / privileged；reject_once → reject（Codex 视为 ends_turn）/ approve；reject_always → reject、persistent / approve | | | | | 同时记日志，提示更新对照表 |
| 任意 | **kind 未知**（安全 #5） | — | 原值 | unknown | 是 | 未知 | privileged | `actionable_by` 只有 `desktop_ui` 和 `test_harness`；界面显示 agent 原文并标"效果未知"，不套用拒绝的样式，不响应 Enter 和 Esc |

**`actionable_by`**：所有选项都是 `privileged` 时为 `desktop_ui` 和 `test_harness`；否则为 `desktop_ui`、`cli`（已提权且含 `approve`）、`test_harness`，M3 加 `remote_device`（v1.8 后 T042 裁定：`test_harness` 总在其中，与 5.8.3 一致，取值不随构建变化；回答照样走完整的检查链）。宿主在回答时还会再检查一遍（ADR-04）。

**摘要**：`d_` 加上 `{session_id, request_id, turn_id, request}` 经 JCS 规范化后的 sha-256，完整 64 位十六进制。

**生命周期**：`pending` → `resolved`（有人回答）| `cancelled`（agent 撤回或回合被中断）| `expired`（所属的适配器实例退出或宿主重启，SEC-10）。Codex 的 `threadStatus` 变成 idle 不会让挂着的请求失效或被收起（Q11，纪要 D3）。

**提问的映射**（SP-1）：Claude 的 AskUserQuestion 每个问题是一组 `question_N`（`oneOf` 选项）加 `question_N_custom`（自由文本），映射成一个 `single_choice` 字段并设 `allow_custom: true`，回答时宿主拆回两个字段；Codex 的表单字段按 JSON Schema 类型映射。遇到无法表示的 schema，设 `unsupported_schema: true`，客户端只提供"跳过"。

## 5.3.7 会话能力描述

宿主统一提供（AG-4），新建会话页使用缓存的同一结构（ADR-09）。

```
SessionCapabilities {
  agent: "claude" | "codex",
  versions: { adapter, cli, cli_compat: verified | newer_untested | older_untested | unsupported },
  prompt:  { image, file_ref, max_image_bytes },
  turn:    { queue: true, steer: { supported, side_effects }, interrupt: true, interrupt_stops_commands },
  session: { resume, load_history, fork },          // load_history、fork 为 P1
  config:  [ ConfigOption ],
  commands:[ { name, description, input_hint? } ],  // 斜杠命令，来自 agent（CP-3）
  interactions: { reject_ends_turn, question_forms, plan_confirm },
  extraction:   { exit_code, output_streaming, diff: fragment | per_hunk | none, thinking, plan: entries | markdown_only, run_state: agent_reported | inferred },
}
ConfigOption {
  id: "permission_mode" | "model" | "reasoning_effort" | "plan_mode" | <其他原样透传的 id>,
  agent_option_id, current,
  values: [ { value, name, description?, risk: normal | elevated | full_access } ],
}
```

- 不支持的功能不出现（AG-4）：例如 `steer.supported` 为 false 时，界面不显示"插话"。
- `steer.side_effects` 为 true（Claude）时，界面仍默认插话（v1.4：PRD CP-6 v1.3 与用户决定 F5），副作用在设置"跟进消息默认排队"的说明里和第一次插话时提示；用户可以把默认行为改成排队（[5.7.4](07-desktop.md#574-窗口与布局)）。宿主只在客户端明确要求 `steer` 时插话（ADR-07）。
- `turn.interrupt_stops_commands` 为 false（Codex，K3）时，中断后还在运行的命令记为 `abandoned` 加 `may_still_run`，界面立刻提示"命令可能仍在运行"（纪要 D4）。
- `risk` 由宿主按配置项映射表给出（[5.6.5](06-agent-driver.md#565-配置项映射)），档位有序：`normal` < `elevated` < `full_access`。谁能放宽到哪一档见 [5.8.2](08-security.md#582-principal角色与-scope)。

## 5.3.8 错误码

应用错误统一用 JSON-RPC `-32000`，按 `error.data.code` 分支；`data.reason` 进一步区分；`message` 是给开发者看的英文。错误码表沿用技术选型讨论会的版本（`architect.md` ADR-04 一节），本节补充 `reason` 清单和 M1 新增的码：

| code | 常见 reason | 能否重试 | 客户端处理 |
|---|---|---|---|
| `version_unsupported` | — | 否 | 提示升级 |
| `not_initialized` | — | 否 | 程序错误 |
| `unauthenticated` | `bad_nonce`、`signature_invalid`、`bad_key`、`bad_proof` | 否 | 断开 |
| `forbidden` | `scope`（`data.scope` 指出缺哪项）、`lineage`、`role`（需要控制连接）、`already_initialized`、`user_activation_required`（主进程本地产生，宿主也会产生） | 否 | CLI 申请提权；界面走原生确认，或提示"请再按一次" |
| `not_found` | — | 否 | 刷新 |
| `conflict` | `already_resolved`、`turn_mismatch`、`digest_mismatch`、`too_early`、`version_mismatch`、`stale_diff`、`outcome_unknown`、`turn_running`、`config_unconfirmed` | 否 | 收起或刷新；`already_resolved` 显示"已在别处处理"（AP-4） |
| `expired` | `adapter_exited`、`host_restarted` | 否 | 收起请求面板 |
| `unsupported` | `data.capability` 指出能力；`not_a_repo` | 否 | 隐藏功能 |
| `unavailable` | `cli_missing`、`cli_not_logged_in`、`cli_too_old`、`runtime_installing`、`runtime_failed`、`adapter_starting`、`adapter_crashed`、`pending_upgrade`、`quota_exhausted`、`app_not_found`、`snapshot_missing`、`index_unmerged`、`project_untrusted`、`scan_failed` | 视 reason | 显示恢复指引（AG-2、MS-5） |
| `agent_error` | `data.agent_code`、`data.agent_message`（截断、脱敏） | 否 | 显示 |
| `git_error` | `hook_failed`、`nothing_to_commit`、`auth_failed`、`rejected`、`lock_held`、`unsafe_repo_config`、`repo_config_changed`、`filter_unsupported` | 视 reason | 显示 stderr 摘要或原因 |
| `idempotency_mismatch` | — | 否 | 程序错误 |
| `too_large` | — | 否 | 改用 blob |
| `rate_limited` | 带 `retry_after_ms` | 是 | 退避 |
| `timeout` | `steer`、`cancel` | 是 | — |
| `shutting_down` | `upgrade`、`user_quit`、`idle` | 是 | 重连 |
| `internal` | — | 有限次 | 收进诊断信息 |

## 5.3.9 限额

在 `initialize` 结果的 `limits` 中下发，客户端以下发值为准。

| 项 | 值 |
|---|---|
| `max_frame_bytes` | 64,000 |
| 单个 delta（转义后） / 合并后 / 内联内容上限 | 16,000 / 32,000 / 16,000 字节（按转义后的 UTF-8 字节计，32,000 个中文字符放不进 64,000 字节的帧；T010） |
| 标题、名称截断 | 1,000 字符（权限请求要批准的内容除外，[5.3.6](#536-交互请求模型)） |
| 写入时合并窗口 | 40 ms（宿主内部，ADR-06） |
| `coalesce_ms`（发送时合并） | 0–1,000；桌面默认 0 |
| `max_queue_bytes`（每连接的实时事件积压，超出即重置流） | 4 MiB |
| 每连接进行中的请求 / 订阅 | 64 / 64 |
| 快照窗口 | 20 个已结束的回合或 2,000 个条目，加上进行中的回合 |
| `history/page` 单页 | 不超过 48,000 字节 |
| `blob/put` | 单个 32 MiB；每连接未完成上传合计 64 MiB；图片另受 agent 限制，默认 10 MiB |
| 每会话排队消息 | 50 条 |
| 命令记录保留 | 48 小时（ADR-04 下限 24 小时） |
| 提权 | 每次最长 15 分钟；非 UI 的申请全局同一时间最多 1 个未决；10 分钟内被拒 2 次后静默 10 分钟（[5.8.4](08-security.md#584-cli-提权)） |
| 中断兜底 / Steer 超时 | 10 s / 5 s |

## 5.3.10 兼容与代码生成

- 兼容性检查区分方向（ADR-04）：宿主发往客户端的消息，删除字段、必填改可选、收窄类型都是破坏性的；客户端发往宿主的消息，新增必填字段是破坏性的。
- 新增方法、事件类型、枚举值、可选字段都不升主版本；新的交互请求类型必须带特性门控。
- schema 注解：`x-scope`、`x-idempotent`（每个方法）；`x-mergeable`（可合并的事件）；`x-id`、`x-time`、`x-path`（字段的性质，金样例的规范化按注解进行，新加的字段自动纳入，[5.9.3](09-testing.md#593-录制与金样例)）。`IndexView`、`SessionView` 的序列化格式也导出进 schema，供 TS 与 Rust 视图对比（测试 #16）。
- 生成链：Rust（serde + schemars）→ `protocol/schema/v1.json` → `tools/gen-protocol-ts`（hey-api 生成 TS 类型和 zod，非 strict）→ 后处理：已知事件类型的字面量列表和类型守卫、兜底成员用 `.loose()`、开放枚举写成 `(string & {})`（SP-4a 报告）。T015 实现时另补了四处，在转成 OpenAPI 或后处理时修正 hey-api 的行为：`$ref` 旁的关键字改写成 `allOf`（否则内部标签联合的成员丢失标签）；`true` 子 schema 改成 `{}`；接受其他字段的对象（如 `ErrorData`）加 `.loose()`；嵌套开放联合的兜底成员只接受联合不认识的标签，事件层的兜底接受一切并由 `isKnownEvent` 判定（与 Rust 的严格已知标签和 `Tolerant` 一致）。每处改写都检查命中，hey-api 升级后输出一变就失败。`v1.json` 是加了缩进的 JCS：键序、数值和字符串按 RFC 8785，只多空白，去掉空白即 JCS 原文。
- **账本回放**：`command` 方法的结果在账本回放时多一个 `replayed: true`（[5.2.5](02-host.md#525-策略与命令账本)）。几个方法共用 `Empty` 这类结果类型，所以不在结果类型上加字段，而是在方法目录里把结果写成 `allOf: [<结果类型>, Replayed]`（T016）。
- **冻结**：契约在 M1a 冻结（T016），之后只能附加，机制见 [5.12.5](12-milestones.md#5125-契约冻结清单)。
- "类型已知但数据残缺"的事件：客户端降级成未知事件并记日志；CI 对金样例严格校验，不允许降级（ADR-04 修订）。
- iOS 怎样使用协议由 ADR-15 决定；M1 只保证 `cupper-protocol` 能编译到 iOS 模拟器、金样例里有未知值的样例。

## 5.3.11 事件与领域类型的补充（v1.7，T011 规划）

T011 开工前做规划时，发现 5.3.5–5.3.7 有几处没有写定。下面是编排者的裁定，T011、T017 按此实现。依据是 `docs/tasks/T011-protocol-events/notes.md`。

**解码与兼容**

- **开放联合**：不用派生的 `#[serde(tag)]` 加 `untagged` 兜底。实测发现，类型已知、数据残缺的值会悄悄落进 `Unknown`。改用 `open_union!` 宏：`Serialize` 和 `JsonSchema` 照旧派生（schema 是 `anyOf`）；`Deserialize` 由宏生成，标签已知时按变体严格解析，标签未知时才落进 `Unknown`，并原样保留数据。
- **严格模式**：`Strictness::{Strict, Tolerant}`，作用于 `Event<EventBody>` 和 `EventBody`。
  - `Strict`：任何一层"类型已知、数据残缺"都直接报错，用于 CI 的金样例。
  - `Tolerant`：整个事件降级成 `Unknown`，并把错误交给调用方记日志（ADR-04 修订第 4 条）。
  - 嵌套联合遇到未知标签时，照常落进各自的 `Unknown`。
- **一个 `EventBody` 覆盖两条流**，由 `stream_kind()` 给出每种事件属于哪条流。
- **`fallback{kind, summary}`**：`kind` 是开放枚举，取客户端已经会画的占位形状（`item`、`hidden` 等）。`summary` 是给开发者看的英文短句，最多 120 个字符，不含 agent 给的文本和路径。界面显示本地化的通用占位，`summary` 只出现在 tooltip 和诊断信息里。
  - 理由：只有不认识这个 `type` 的旧客户端才会用到 `fallback`；新类型带来的语义码，旧客户端同样没有翻译。

**字段与类型**

| # | 位置 | 裁定 |
|---|---|---|
| 1 | `notification.intent.intent_id`、条目的 `group_id` | 新增前缀 `n_`、`g_`，后接 UUIDv7（5.4.4） |
| 2 | blob 的 `hash` | 新增 `BlobHash` 类型，带 `x-id`（5.4.4） |
| 3 | `late_tool_output.output` | 改为可选。条目开始时还没有输出，输出走 `field: output` 的 delta，与 `ToolCall.output?` 一致 |
| 4 | `Option.effect.ends_turn` | 类型为 `Option<bool>`，不出现表示未知。用于效果未知的选项，以及未实测的 Codex `revise_plan` |
| 5 | `item.updated`、`item.completed` 中要更新的字段 | 共用展开的 `ItemPatch`：各类条目可以更新的字段都写成可选字段，不带 `kind`；同名字段在各类条目里类型相同 |
| 6 | `workspace.failed` | `{code: ErrorCode, reason?: ErrorReason}`，与错误对象的写法一致 |
| 7 | `turn.completed.error_code`、`SessionSummary.error_code`、`error` 条目的 `code` | 开放枚举 `FailureCode`：5.6.2 中 `AgentError` 的分类（`not_logged_in`、`cli_too_old`、`session_not_found`、`process_exited`、`restartable`、`model_unsupported`、`quota_exhausted`、`other`），再加宿主侧的 `adapter_exited`、`host_restarted` 等 |
| 8 | `AgentStatus` | `{agent, state, cli?: {path, version?, compat, install}, adapter: {version, runtime, failure?}, auth: {state, kind?, plan?}, rss_bytes?, config_dir}`。登录状态只有类型和套餐名，不含邮箱、组织 |
| 9 | 用量里的 `cost` | `{amount, currency}`，形状与 ACP 的 `usage_update` 相同 |
| 10 | `session.configured.config` 的值、提问回答的 `values` | 配置项的值是字符串，ACP 的配置项是 select 类型。回答的值是带类型的 `AnswerValue`（字符串、数字、布尔、字符串数组），数字用 `serde_json::Number`，保证整数往返时不会变成浮点数 |
| 11 | `queue.updated` 条目的 `created_by` | `PrincipalId` |

**取值补充（T011 实现时，2026-10-19）**：设计只列了字段，没写取值的地方，按下面实现。

| 类型 | 取值 |
|---|---|
| `AgentState` | `unavailable`、`idle`、`starting`、`running`、`crashed` |
| `CliInstall` | `native`、`npm_shim`、`unsupported_install` |
| `ConfigDirState` | `default`、`ok`、`rejected` |
| `AuthState` | `logged_in`、`logged_out`、`unchecked`。不用 `unknown`，因为 `open_enum!` 已经占用了 `Unknown` 这个变体名 |
| `DiffSide` | `old`、`new` |
| `FailureCode` | 第 7 条列出的取值，加上 `adapter_crashed`、`cli_missing`、`runtime_failed` |
| `FallbackKind` | `item`、`hidden` |
| `CliCompat` | 不新增取值；读不到版本号时记为 `unsupported` |

**T017 实现时的裁定与取值（2026-10-19）**

- 权限请求的 `tool.paths` 超出预算时，把完整列表放进 `detail`（blob），`paths` 只保留前若干项，并设 `content_truncated`。
- `user_message` 的 `content` 始终完整，超长的文本用 `text_blob` 引用，不拆成 `item.delta`。拆成 delta 只用于 agent 一侧的条目（`agent_message`、`thought`、工具输出）。
- `ItemPatch` 的语义：出现的字段整体替换，没出现的保持不变；没有"清空字段"的写法。客户端按 `output_state` 这类状态字段来判断，例如输出完成后不会清掉 `output_tail`。
- `item.started` 的写法是 `{item_id, turn_id?, started_at, group_id?, kind, ...}`，条目联合展开在外壳字段旁边。`group_id` 放在外壳上，因为组内的 `thought` 也要带它。
- `duration_ms` 放在 `item.completed` 上，不作为 `ToolCall` 的字段。
- 权限请求里的 `tool.command` 是 `Content`，不是字符串，因为超长时要改成 blob（第 1 组第 6 条）。
- `request_digest`：`turn_id` 缺省时省略这个键，不写 `null`。

| 类型 | 取值 |
|---|---|
| `Guidance` | `retry`、`run_login_in_terminal`、`upgrade_cli`、`install_cli`、`quota_exhausted`、`change_model`、`restart_agent`、`none` |
| `StopReason` | ACP 的 `end_turn`、`max_tokens`、`max_turn_requests`、`refusal`、`cancelled`，另加宿主的 `host_timeout` |
| `SnapshotReason` | `timeout`、`unsafe_repo_config`、`not_a_repo`、`git_failed` |
| `ExclusionReason` | `too_large`、`nested_repo`、`large_untracked_dir`、`filter_unsupported` |
| `ResolutionReason` | `withdrawn`、`turn_cancelled`、`adapter_exited`、`host_restarted`；T048 附加 `turn_ended`（回合结束时还挂着的请求，见下文"T040 实现时的裁定"） |
| `PlanPriority` | `high`、`medium`、`low`（ACP） |
| `workspace.preparing.progress` | 0–100 的百分比 |
| `effect.rule.location` | 字符串，不用 `WirePath`：适配器给的是 `localSettings` 这类语义值 |
| `effect.scope`、`prefix_risk` 的线上值 `unknown` | 是已知取值，分别对应变体 `EffectScope::Unclassified`、`PrefixRisk::Unparsed`（`Unknown` 已被 `open_enum!` 的兜底变体占用） |

**T014 实现时的裁定（2026-10-19）**

视图规则。TS 的 reducer（T072）必须照做；视图 JSON 两端比对时按深度相等，不看键顺序。

- **没有 `turn_id` 的条目和请求**：
  - 有进行中的回合时并入它；否则放进 `tail`。下一个**新**回合（`turn_no` 比已有的都大）开始时，把 `tail` 收进这个回合。
  - 并入后，视图把条目或请求的 `turn_id` 改成这个回合的 ID，这样回合快照和历史页能把它放回原处。请求的 `digest` 保持原值。
- **窗口**（5.2.8）：
  - 从新到旧数已结束的回合，最多 20 个，条目合计不超过 2,000 个；遇到第一个放不下的就停。只数 item，不数交互请求。
  - 进行中的回合和 `tail` 永远保留。
  - 最新的一个已结束回合总是保留，即使它一个就超过 2,000 个条目。否则历史页送来的回合会被当成新回合，把 `tail` 收走。
- **`detached`**：
  - 被淘汰的回合里仍待处理的请求，放进 `detached`，快照照样带上。
  - 请求解决后，下一次 `trim` 把它删掉。
  - 它的回合经历史页回来时，重复的 `interaction.requested` 把它放回原位，并保留期间的状态。
- **视图 JSON**：
  - 缺省的可选值不出现；map、列表、字符串总是出现。
  - 条目是 `ItemView {item, text, output, end?}`：delta 拼出的文本单独存放，输出完成后不清空，界面按 `output_state` 选择显示哪个。
- **`ItemPatch`**：只设置这一类条目有的字段；未知类型的条目把全部字段并进去。
- **订阅**：
  - 判断顺序是先比 epoch，再看 `cursor_ahead`，最后看 `cursor_expired`。
  - 只要是 `snapshot`，不论原因（包括客户端不认识的 mode），客户端都丢弃这条流的视图和游标，用快照页重建。
  - `stream/reset` 保留游标。
  - `apply` 报 `ViewError` 时，客户端不带游标重新订阅。
- **快照 ops**：
  - 长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta`；
  - 超大的 `item.started` 把可选字段逐个拆成 `item.updated`；
  - 单个 op 不超过 `max_history_page_bytes`（48,000）。
  - 快照页和 `history/page` 都按 JSON 字节贪心分页，`op_index` 是页的起点。
- **宿主要守的不变式**（T026、T043）：
  - 引用某个回合的事件都写在这个回合的 `turn.completed` 之前，包括 `snapshot.recorded` 的 `phase: end`、被放弃条目的 `item.completed`、它的请求的 `interaction.resolved`。
  - 回合快照在 `turn.completed` 时生成，随后调用 `trim`。
- **回合外的交互请求**（编排者裁定）：
  - 交互请求一律属于某个回合。请求到来时没有进行中的回合，宿主按 K9 开一个 `autonomous` 回合，把请求放进去。
  - 回合在它的请求全部解决之前不结束。中断时请求标 `cancelled`，适配器退出时标 `expired`，这两条已有。
  - 因此宿主写出的 `interaction.requested` 总带 `turn_id`。协议里它仍是可选字段（开放性），视图对缺省的情况按上面的规则处理。
  - 没有 `turn_id` 的条目，只剩宿主的 notice 这类。

**T012 开工前的裁定（2026-10-19）**

方法的声明方式：
- `Method` 加 `MILESTONE` 和 `ERRORS`，`Notification` 加 `MILESTONE`，都不给默认值，漏写就编译不过。
  - schema 里写出 `x-milestone`，取值为 5.3.4 里程碑一栏的最早值；
  - 写出 `x-errors`，取值为 5.3.4"可能的错误"一栏，用 `code/reason` 写法。
  - 所有方法都可能返回的错误，写在模块文档里，不逐个列。所有方法都可能返回的有：`not_initialized`、`forbidden/scope`、`invalid_params`、`rate_limited`、`shutting_down`、`internal`；命令另有 `idempotency_mismatch`。
- 宽容解码：`stream/events.events`、`stream/snapshot.ops`、`history/page.ops` 的类型是 `Vec<Raw<T>>`。线上是原样的 JSON，schema 转发成 `T` 的 schema；客户端逐个按宽容模式解码。
- 方法目录：按名字排序的三张 entry 表，分别是请求、客户端通知、宿主通知。T015 从这里导出。
- `nonce`、`proof` 用字符串，字段上标 `x-id`。格式校验和转成字节归 T030。
- 命令方法的参数直接带 `command_id`，不用 `flatten`。

5.3.4 没写死的取值：
- `findings` 条目：`{kind, path, target?, outside_workspace, detail}`。`kind` 是开放枚举：`hook`、`env`、`api_key_helper`、`status_line`、`mcp_server`、`skill`、`agent`、`command`、`codex_config`、`unparsed`（解析不了的文件，`detail` 写原因；它的完整内容照样计入摘要，T096）。
- `project/trust_info.diff_from_trusted`：按文件列出 `{path, change}`，`change` 复用 `DiffChange`（`add`、`update`、`delete`）。为此 T035 在信任时另存每个文件的哈希清单，按 P-4 写进 `0001` 的 `projects` 表。
- 帧上限（T034 的裁定）：结果里的 findings 和差异合计不超过 48,000 字节。单条 `detail` 先按字符边界截到 2,000 字节，末尾加 `…`，再按顺序装入，装不下就停；`findings_omitted`、`diff_omitted` 给出没装下的条数，确认框据此写"另有 N 项未列出"。摘要仍覆盖全部内容。
- `fallback_reason`（插话退回排队的原因，5.2.6）：`unsupported`、`turn_mismatch`、`not_prompting`、`prompt_required`、`timeout`、`agent_error`。
- `session/archive.blockers`：ADR-10 的 7 条规则各一个取值；T013 的 `worktree/check` 复用同一个枚举。
- `host/status.clients`：`ClientInfo` 的列表（与 `clients.updated` 相同），不只是个数。
- 补充错误码：
  - `project/trust_info` 加 `unavailable/scan_failed`，因为它要重新扫描；
  - `session/create` 加 `forbidden/scope`，因为 `config` 同样受档位规则约束（5.8.2）。
- `session/queue_edit` 的 scope 按 5.8.3 中 `session/send` 那一行。

**T040 实现时的裁定（2026-10-19）**

- **回合结束时还挂着交互请求**：prompt 已经正常返回，但还有请求没有解决。
  - 回合的结局一确定，`TurnMachine` 就发 `Settle`。由交互代理（T042）把挂着的请求标为 `cancelled`，并对 ACP 应答 `Cancelled`。之后才拍结束快照、写 `turn.completed`。
  - 这时的 `ResolutionReason` 用新增的取值 `turn_ended`，由 T048 加进 `cupper-protocol`。不借用 `withdrawn`，因为它专指 agent 用 `$/cancel_request` 撤回。
  - 中断和适配器退出沿用已有的取值：`turn_cancelled`；`adapter_exited`，状态为 `expired`。
- **队列暂停怎样解除**：
  - 空闲时用户发新消息，这条消息立即开回合，同时解除暂停；它正常结束后，排队的消息接着出队。
  - 也可以调用 `session/queue_resume` 解除暂停。
  - 队列为空时不进入暂停。
- **agent 自己取消**（#556 的推广）：宿主没有中断，prompt 却返回 `cancelled`，就当作 agent 自己取消，不暂停队列。
- **Cancelling 状态下的结果**：处于 Cancelling 时，结果只要不是错误，一律记 `cancelled`；是错误就记 `failed`。
- **结束快照**：只有发过 prompt 的回合才拍。
- **驱动接口**（`driver::api`）：
  - Prompt 带 `PromptTag`，用来认出迟到的结果；
  - agent 的请求用 `AskId` 指代，ACP 的 responder 留在驱动实现里，回答走 `DriverCommand::Answer`；
  - Prompt 的内容直接用协议的 `PromptContent`，由 T055 转成 ACP 的内容块；
  - `TurnMachine` 发 `Action::Attach(AttachTarget)`，由会话 actor 补上工作区、配置和信任状态。
- **队列满**（编排者裁定）：排队消息已到 5.3.9 的上限（每会话 50 条）时，`session/send` 和 `session/queue_edit` 返回 `too_large/queue_full`，队列不变。

**T018 实现时的取值（2026-10-19）**

- `agents/capabilities` 的结果是 `{cached?: {capabilities, captured_at}}`。能力缓存为空时没有 `cached`，客户端据此调用 `agents/refresh`。
- `session/get` 的结果是 `{summary, workspace, archived_at?}`。
- `session/list_archived` 用结构化游标 `{archived_at, session_id}`。
- `interaction/get` 的结果是 `{requested, status, outcome?, resolved_by?, reason?}`。
- `interaction/respond.input` 是 `InputEvidence {kind, key?, digit?}`，`digit` 只接受 1–9（`KeyDigit`）。T013 的 `git/revert.input` 复用同一个类型。
- `trust_digest` 用 `Digest` 类型（`d_` 加 64 位十六进制），T035 要按这个形式输出。
- 每个方法各自一个 Params 类型，形状相同也不共用。
- 权限矩阵新增格子 `Sec9`，含义是 scope 允许，但按 SEC-9 由策略拒绝。它出现在 remote_device 那一列，涉及 `project/add`、`project/remove`，以及 `session/set_config`、`rename`、`restore`、`agents/refresh`、`agents/restart` 所在的行。

**T013 实现时的取值（2026-10-19）**

- 对 5.3.4 的更正：
  - `git/status.branch`、`git/branches.current` 改为可选，因为 detached HEAD 时没有分支；
  - `diag/export.include_sessions` 是会话 ID 的列表，依据 5.11.3"用户逐个勾选会话"；
  - `diag/recording_export` 的 scope 为 `ui_local`，它是唯一与 5.8.3 那一行逐格相符的 scope。
- 按构建门控的方法：
  - 新增注解 `x-builds`，取值是宿主的 cargo 特性名，例如 `["test-harness", "dev-auth"]`，没有门控时不写；
  - `diag/recording_export` 在 test-harness 和 dev-auth 构建里有，`test/*` 只在 test-harness 构建里有；
  - 宿主在其他构建里不注册这些方法。
- 阻塞原因枚举 `WorktreeBlocker`，与 ADR-10 的清理规则一一对应：`modified_files`、`untracked_files`、`operation_in_progress`、`foreign_lock`、`head_only_commits`、`unpushed_commits`、`initialized_submodules`。`worktree/check` 和 `session/archive` 共用。
- `session/archive` 的形状：
  - 参数 `{command_id, session_id, worktree: keep|remove|remove_case_only_dirty}`；
  - 结果 `{worktree: kept|removed|blocked, blockers?}`；
  - 错误码 `conflict/turn_running`。
- git 方法里的两个参数：
  - `git/diff.scope` 取 `"unstaged" | "staged" | "last_turn" | {"turn": id}`；
  - `git/stage`、`unstage`、`revert` 的 `target` 取 `{"all": true} | {"paths": [...]} | {"hunk_ids": [...]}`。
  - 两者都是封闭的，只由客户端发送，宿主拒绝不认识的取值。
  - `input` 复用 `InputEvidence`。
- 设置：
  - 用 5.2.11 的点号扁平键，`agents.<agent>.cli_path` 按 agent 展开；
  - patch 里的 `null` 只用来清空可为空的键。
- `policy/describe.action` 是开放枚举，7 个取值与 5.8.3 的"原生确认"格子一一对应；`details` 是不带文案的语义参数，每种 `action` 带哪些键归 T031。
- `test/*` 的参数：`clock_advance {ms}`、`run_maintenance {task}`、`seed {profile}`，T047 可以细化。
- 权限矩阵的新格子：
  - `Input`：window 的请求必须带 `input`；
  - `Build`：只在特定构建里有；
  - `CliOnly`：`auth/elevate` 不要求 scope，策略对 `cli` 以外的 principal 一律拒绝。

**T030 开工前的裁定（2026-10-19）**

- **各构建开放的认证**，按 5.8.1 的表推定：

  | 认证 | 发布（无特性） | `dev-auth` | `test-harness` | e2e（两者都开） |
  |---|---|---|---|---|
  | `codesign` | 开放 | 不开放 | 开放 | 不开放 |
  | `peer` | 开放 | 开放 | 开放 | 开放 |
  | `dev_key` | 不开放 | 开放 | 不开放 | 开放 |
  | `harness` | 不开放 | 不开放 | 开放 | 开放 |

  - `weak_auth` 等于是否开了 `dev-auth`。
  - UI 的签名要求在 T180 带上 Team ID 之前为空，所以在那之前 `codesign` 认证全部失败。这不影响 M1a：开发、自用和 e2e 构建里，UI 都用 `dev_key`。
- **握手失败一律先回错误，再断开**。错误码如下：
  - nonce 不符：`unauthenticated/bad_nonce`；
  - 签名结论不是 UI：`unauthenticated/signature_invalid`；
  - 本构建没有这种认证、没有载入密钥，或者 `dev_key` 的出示方不是拉起宿主的进程：`unauthenticated/bad_key`；
  - 证明格式错误或对不上：`unauthenticated/bad_proof`；
  - 非 UI 的客户端声明了角色，或者 UI 没有声明角色：`forbidden/role`。
- **写循环**：
  - 实时事件以编好的帧进入发送队列，按字节预算；超出预算时按流重置。
  - 发送时按 `coalesce_ms` 合并、补发（`Replay`），归 T033。
- **两个时长**：
  - 关闭宽限 2 s；
  - accept 出错后的退避 50 ms。
  - 两者都登记在 `durations.rs`。
- **审计**：
  - 网关把审计记录发进 channel。写进 `audit` 表和 host.log 的接线，归 T037。
  - 对端可执行文件的路径、签名 identifier、Team ID，归 T038，它的提权确认框同样要用。
- **握手期限**（编排者裁定，T030 汇报时提出）：宿主发出 `host/hello` 后，客户端须在 10 s 内发来 `initialize`，否则宿主断开连接，审计原因记 `handshake_timeout`。期限登记在 `durations.rs`，经 `Clock` 计时，由 T039 实现。

**T031 实现时的裁定（2026-10-19）**

- **拒绝码**：
  - SEC-9（remote_device 的收窄）和 `CliOnly` 的拒绝用 `forbidden/role`；
  - 档位规则的拒绝用 `forbidden/scope`，`data.scope` 写明要的是 `approve` 还是 `privileged`。
- **手机与 `permission_mode`**：手机可以改 `permission_mode`，但只能收紧或保持（MB-9；5.8.2 档位表）。所以 SEC-9 下手机能改的配置项是模型、推理强度，以及收紧或保持的 `permission_mode`。
- **400 ms 最短回答间隔**：
  - 用宿主的 `MonoTime` 计，从交互代理收到请求时开始算，不用线上的 `created_at`；
  - `MIN_ANSWER_DELAY` 是安全阈值，写成常量，不登记到 `durations.rs`，测试配置不能覆盖。
- **一律按 privileged 处理的选项**：`allow_always`、kind 未知、effect 未知的选项，不管对照表怎么写，都按 privileged 处理。
- **档位规则的边界**：
  - 映射里没有的模式按 `full_access` 处理（5.6.5）；
  - 当前档位未知时按 `normal` 处理；
  - 只有 `mode_after`、没有 `tier_after` 的选项，按 `full_access` 处理。
- **window 的按键**：
  - Esc 只能用于 `dismissed`，或 effect 为 `reject`、`plan_revise` 的选项；
  - `digit` 不能用来 dismiss；
  - 提问时只接受 `answer`；
  - role 未知的 desktop 连接按 window 处理；
  - `git/revert` 来自 window 时，只接受点击、Enter、Space、⌘Enter 或原生确认。
- **信任门控**：
  - 已信任的项目，加载项目配置；
  - 未信任或 stale 的项目：Claude 只加载用户配置，Codex 回 `unavailable/project_untrusted`；
  - 信任状态未知或 agent 未知，一律拒绝。
- **`policy/describe` 各 action 的 `details` 键**（不带文案，T077 的原生确认框按这些键取内容）：
  - `trust_project`：`{project_id, trust_digest}`；
  - `confirm_config`：`{session_id, config_digest}`；
  - `full_access`：
    - 来自 `session/create`：`{agent, project_id, project_path, mode}`（`project_path` 由 T135 加，宿主不认识该项目时为 `null`）；
    - 来自 `session/set_config`：`{session_id, mode}`；
    - 来自 `interaction/respond`：与 `privileged_option` 相同，另加 `mode`；
  - `privileged_option`：`{session_id, request_id, option_id, kind, effect}`；
  - `security_setting`：`{settings}`，只含安全相关的键和新值；
  - `grant_elevation`：`{elevation_id, scopes}`；
  - `interrupt_turns`：`{running_turns}`。只有 `host/exit` 带 `force`、`host/prepare_upgrade` 为 `mode: force`，并且确实有运行中的回合时，才是这个 action；
  - 这个形式下不需要确认的命令，返回 invalid params。

**T033 开工前的裁定（2026-10-19）**

- **UNIT-unsupported-set 的允许集合**：每一项都写明原因。可以返回 `unsupported` 的方法有：
  - M1b、M2、M3 的方法；
  - 按构建门控、本构建没有的方法；
  - T183 之前的 `host/prepare_upgrade`；
  - 执行者还没合并的 M1a 方法，并写明由哪个任务接上。
  - 每个任务接上执行者时，删掉自己那几行。到 T099 时，表里不应再有 M1a 方法。
- **`policy/describe` 的 `Facts`**：全部由会话端口的 `facts(&Confirmable)` 提供，`running_turns` 也由会话管理给出，不另设接口。
- **每连接在途上限**：第 65 个在途请求回 `rate_limited`，不带 `retry_after_ms`，因为在途数要靠这条连接自己的回复来释放，与时间无关。
- **订阅与 `history/page` 的 `not_found`**：会话流的 head 为 0 就回 `not_found`，不查 `sessions` 表（T019）。
- **补发与裁剪的竞态**：补发时读到缺口，就摘掉订阅、清掉这条流排队中的实时项，发 `stream/reset`，`reason` 沿用 `rebuilt`。客户端带游标重新订阅，会拿到 `cursor_expired` 快照（T019）。
- **订阅日志（Q16）**：网关把 `stream.subscribed` 发进审计 channel，T037 只把它写进 host.log，不写 `audit` 表（T019）。

**T041 实现时的裁定（2026-10-19）**

- **Ending 期间到达的更新**：
  - prompt 已经返回、回合正在收尾时，agent 发来的更新先由合并器暂存；
  - 裁判（T058）看到后先保持队列（`HoldQueue`）；
  - 等这个回合的 `turn.completed` 写出后，立即开一个自发回合来承载这些更新，排在任何排队消息之前。
  - 理由：
    - 这是 agent 自发的工作（K9）；
    - 放进正在收尾的回合，会破坏"Settle 之后不再有新条目"和回合快照不可变；
    - 放进出队的回合，又会在 agent 还在工作时发出新 prompt。
- **自发回合的结束**（T058 实现）：没有运行中的工具，也没有待处理的请求时，下面任一情况发生就结束：
  - 收到带 `cost` 的 `usage_update`；
  - thread 变为 idle；
  - thread 已经是 idle 时工具结束，对应晚到输出；
  - 静默 10 s，但 thread 已知在运行时不算。

  只有靠静默结束的自发回合会保持队列：agent 再静默 30 s 才放开出队。用户在空闲时发起回合，也会解除保持。
- **出队保护的范围**：只保护自动出队。用户在空闲时直接发的消息不受保护，立即开回合，用户的明确操作优先。自发工作不是 prompt，所以这不违反 K1。
- **晚到条目跨回合**："每个原工具只有一个晚到条目"按回合理解。晚到条目所在的回合先结束时，这个条目标为 `abandoned`，之后的输出在下一个回合里另建一个晚到条目，不往已经写下快照的回合里追加。
- **超长输出**：输出超过内联上限时转 blob、`truncated` 状态、每秒更新尾部，这几项都需要 I/O，归会话 actor（T043）。合并器只写内联文本。

