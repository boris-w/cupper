# 5.3 宿主协议 v1 草案

- 所属：`docs/02-design.md` 第 5 节
- 依据：ADR-04（含 SP-4a 修订）、ADR-05、ADR-06；技术选型讨论会架构师意见 ADR-04 一节（错误码表）；SP-1（选项与提问的实际形态）；SP-4a 的 `cupper-protocol`
- 性质：**只写契约**。Rust 类型是唯一来源（`crates/cupper-protocol`），由它导出 `protocol/schema/v1.json`。本节与导出的 schema 不一致时，以评审后修改的本节为准，再改代码

## 5.3.1 通用约定

| 项 | 约定 |
|---|---|
| 帧 | `u32 长度（大端）\| u8 帧类型 \| 负载`，整帧不超过 64,000 字节。`0x01` JSON 消息；`0x02` blob 分片（头部：32 字节 sha-256、8 字节偏移、1 字节标志，bit0 表示最后一片）；`0x03` 预留给二进制编码。JSON 消息不跨帧（ADR-04） |
| 信封 | JSON-RPC 2.0。宿主从不向客户端发请求，只发响应和通知 |
| ID | 带前缀的字符串，见 [5.4.4](04-data-model.md#544-id-规则)。客户端把 ID 当作不透明字符串 |
| 时间 | UTC 毫秒整数；时长也用毫秒 |
| 整数 | 全部不超过 2^53−1，schema 里用上下界表示，不用 `format: int64`（ADR-04 修订） |
| 文本流 | `item.delta` 只追加文本，不用偏移量 |
| 开放类型 | 事件信封的 `type` 是普通字符串；枚举和带 `type` 标签的联合类型都是开放的，生成器产出 `Unknown` 兜底分支；读取方忽略不认识的字段 |
| 大内容 | 转义后超过 16,000 字符的内联内容一律改成 blob 引用 `{type: "blob", blob: {hash, size, media_type}, preview}`，按需用 `blob/get` 拉取（ADR-06） |
| 命令 | 有副作用的方法都带 `command_id`（客户端生成的 UUIDv7）和可选的 `attestation`（M3 预留） |
| 文案 | 宿主只下发语义码和参数，不下发界面文案。例外：agent 自己给出的文本（选项名、工具标题、错误原文）原样转发，并按 1,000 字符截断 |

## 5.3.2 握手与版本协商

**第一帧：`host/hello`（宿主 → 客户端，通知）**。连接建立、宿主完成对端校验后立即发出。

```json
{"jsonrpc":"2.0","method":"host/hello","params":{
  "nonce":"<64 位十六进制>","protocol":{"min":1,"max":1},"host_version":"0.1.0"}}
```

**`initialize`（客户端 → 宿主，请求）**：必须是客户端的第一条消息。

| 参数 | 说明 |
|---|---|
| `protocol: {min, max}` | 客户端支持的主版本区间 |
| `nonce` | 原样回显 `host/hello` 里的 nonce（SP-4b 的固定令牌握手） |
| `client: {kind, name, version}` | `kind` 取 `desktop`、`cli`、`ios`、`test`（开放枚举） |
| `role` | 仅桌面端：`control`（主进程自用的控制连接）或 `window`（转发给某个窗口的数据连接），见 [5.7.1](07-desktop.md#571-进程与连接) |
| `auth` | 带 `type` 标签：`codesign`（无参数，宿主使用 accept 时的签名校验结论）；`peer`（只读 CLI）；`dev_key {key}`（仅 `dev-auth` 构建）；`harness {token}`（仅 `test-harness` 构建）；M3 的远程设备由远程桥在进程内接入，不经过这里 |
| `features: [string]` | 客户端支持的可选特性，见下表 |
| `coalesce_ms?` | 希望的发送时合并窗口，宿主截断到 0–1,000 |

| 结果 | 说明 |
|---|---|
| `protocol` | 选定的主版本：双方区间交集中的最大值 |
| `host: {host_id, version, weak_auth}` | `weak_auth` 为 true 表示宿主运行在退化的认证模式（开发构建或自行构建，ADR-05），客户端要显示"本地鉴权较弱" |
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
| 索引流 | `index` | 项目、未归档会话的摘要、agent 状态、宿主状态、已连接的客户端、提权请求、通知意图 |
| 会话流 | `session/<session_id>` | 会话配置与能力、回合、条目、交互请求、队列、快照记录 |

- **订阅**：`stream/subscribe {stream, after_seq?}`。`after_seq` 仍在日志保留范围内时，结果为 `mode: replay`，之后以 `stream/events` 补发；否则为 `mode: snapshot`，之后以若干页 `stream/snapshot` 下发状态（`at_seq` 为快照对应的 seq），再接增量。
- **快照的内容**是一串事件体，客户端用处理增量时同一个 `apply` 应用（ADR-06）。会话流的快照只覆盖窗口：最近 20 个回合（或最多 2,000 个条目）、进行中的回合、全部待处理的交互请求、配置、能力和队列。
- **历史**：`history/page {session_id, before?, max_bytes?}` 返回窗口之前的回合，内容同样是事件体序列，用同一个 `apply` 拼到视图前面；`next` 为空表示到头了。
- **连续性**：客户端检查 `event.from`（没有就取 `seq`）等于上一个事件的 `seq` 加 1，不满足就带着游标重新订阅（ADR-04）。
- **重置**：`stream/reset {stream, reason}` 表示宿主丢弃了这条订阅（`slow_consumer`：该连接的发送队列超出预算；`rebuilt`：宿主重建了这条流），客户端带游标重新订阅。
- **可合并的事件**：只有 `item.delta`。schema 中以 `x-mergeable: true` 标出。

## 5.3.4 方法清单

图例：**幂等**一栏，"命令"表示需要 `command_id`，宿主按账本去重；"天然"表示重复调用没有额外效果，不需要 `command_id`。**scope** 的含义见 [5.8.2](08-security.md#582-principal角色与-scope)。**里程碑**一栏标出最早需要的里程碑。

### 握手、生命周期、在场

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `host/hello` | 通知 宿主→客户端 | `nonce, protocol, host_version` | — | — | — | — | M1a |
| `initialize` | 请求 | 见 5.3.2 | 见 5.3.2 | `version_unsupported`、`unauthenticated` | — | 无 | M1a |
| `ping` | 请求 | — | `{}` | — | 天然 | 无 | M1a |
| `client/presence` | 通知 客户端→宿主 | `{foreground, session_id?}` | — | — | 天然 | read | M1a |
| `host/status` | 请求 | — | `{version, pending_upgrade, running_turns, pending_interactions, clients}` | — | 天然 | read | M1a |
| `host/prepare_upgrade` | 请求 | `{command_id, mode: wait\|force}` | `{state: waiting\|exiting, running_turns}` | `forbidden` | 命令 | privileged | M1b |
| `host/exit` | 请求 | `{command_id, force}` | `{}`，随后宿主退出 | `conflict`（有运行中的回合且未 force） | 命令 | privileged | M1a |
| `host/shutdown` | 通知 宿主→客户端 | `{reason: user_quit\|upgrade\|idle, restart_expected}` | — | — | — | — | M1a |

### 流与内容

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `stream/subscribe` | 请求 | `{stream, after_seq?}` | `{stream, mode, at_seq, head_seq}` | `not_found`、`rate_limited` | 天然 | read | M1a |
| `stream/unsubscribe` | 请求 | `{stream}` | `{}` | — | 天然 | read | M1a |
| `stream/events` | 通知 宿主→客户端 | `{stream, events: [Event]}` | — | — | — | — | M1a |
| `stream/snapshot` | 通知 宿主→客户端 | `{stream, at_seq, page, last, ops: [EventBody]}` | — | — | — | — | M1a |
| `stream/reset` | 通知 宿主→客户端 | `{stream, reason}` | — | — | — | — | M1a |
| `stream/ack` | 通知 客户端→宿主 | `{stream, seq}` | — | — | — | read | M3 |
| `history/page` | 请求 | `{session_id, before?, max_bytes?}` | `{ops: [EventBody], next?}` | `not_found` | 天然 | read | M1a |
| `blob/get` | 请求 | `{hash, offset?, length?}` | 先发 `0x02` 分片，再回 `{hash, size, offset, length, chunks}` | `not_found` | 天然 | read | M1a |
| `blob/put` | 请求 | 先发 `0x02` 分片，再请求 `{hash, size, media_type}` | `{hash}` | `too_large`、`conflict`（哈希或长度不符） | 天然（按内容寻址） | write | M1a |

### 项目与 agent

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `project/add` | 请求 | `{command_id, path}` | `{project_id}`（同一路径重复添加返回已有的 ID） | `not_found`（路径不存在） | 命令 | write | M1a |
| `project/remove` | 请求 | `{command_id, project_id}` | `{}`（项目下的会话一并归档） | `conflict`（有运行中的回合） | 命令 | write | M1b |
| `agents/capabilities` | 请求 | `{agent}` | 该 agent 最近一次的能力描述（[5.3.7](#537-会话能力描述)）及其采集时间 | `unavailable`（CLI 或运行时不可用，带 `reason`） | 天然 | read | M1a |
| `agents/refresh` | 请求 | `{agent}` | `{status}`：重新发现 CLI、检查版本；能力缓存为空时做一次探测 | `unavailable` | 天然 | write | M1a |
| `agents/restart` | 请求 | `{command_id, agent}` | `{}`：重启该 agent 的适配器，影响它的所有会话 | — | 命令 | write | M1b |

### 会话

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `session/create` | 请求 | `{command_id, agent, project_id, workspace: {kind: local} \| {kind: worktree, base_ref, branch_hint?}, config?: {permission_mode?, model?, reasoning_effort?}, message: PromptContent}` | `{session_id}`（已受理；工作区准备、agent 连接、第一个回合都在会话流里体现） | `not_found`、`unavailable`、`unsupported`（如非 git 项目选 worktree） | 命令 | write；配置含完全访问时 privileged | M1a（worktree 为 M1b） |
| `session/send` | 请求 | `{command_id, session_id, content: PromptContent, mode: queue\|steer, expected_turn_id?}` | `{disposition: started\|queued\|steered, turn_id?, queue_id?, fallback_reason?}` | `not_found`、`unavailable`、`too_large` | 命令 | write | M1a（steer 为 M1b） |
| `session/interrupt` | 请求 | `{command_id, session_id, expected_turn_id?}` | `{turn_id?}` | `conflict/turn_mismatch` | 命令 | write | M1a |
| `session/queue_resume` | 请求 | `{command_id, session_id}` | `{}`：恢复已暂停的队列 | — | 命令 | write | M1a |
| `session/queue_edit` | 请求 | `{command_id, session_id, expected_version, ops}` | `{version}` | `conflict/version_mismatch` | 命令 | write | M2（CP-8） |
| `session/set_config` | 请求 | `{command_id, session_id, option, value}` | `{config}` | `unsupported`、`agent_error` | 命令 | write；值为完全访问时 privileged | M1b |
| `session/rename` | 请求 | `{command_id, session_id, title}` | `{}` | — | 命令 | write | M1b |
| `session/archive` | 请求 | `{command_id, session_id, worktree: keep\|remove\|remove_case_only_dirty}` | `{worktree: kept\|removed\|blocked, blockers?}` | `conflict`（有运行中的回合） | 命令 | write | M1b |
| `session/restore` | 请求 | `{command_id, session_id}` | `{mode: full\|read_only}` | — | 命令 | write | M1b |
| `session/mark_read` | 请求 | `{session_id, up_to_seq}` | `{}` | — | 天然（只前移） | read | M1a |
| `session/get` | 请求 | `{session_id}` | 完整摘要和工作区信息 | `not_found` | 天然 | read | M1a |
| `session/capabilities` | 请求 | `{session_id}` | 能力描述 | `not_found` | 天然 | read | M1a |
| `session/list_archived` | 请求 | `{cursor?}` | `{sessions, next?}` | — | 天然 | read | M1b |

`PromptContent` 是内容块数组：`text {text}`、`image {blob}`、`file_ref {path}`（`@` 引用，相对工作区）、`review_comment {path, line_start, line_end, side, quote, comment}`（GT-2，宿主负责把评论排成给 agent 的文本）。单个 `text` 块超过帧预算时，客户端先 `blob/put`，再用 `text_blob {blob}` 引用。

### 交互请求

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `interaction/respond` | 请求 | `{command_id, session_id, request_id, request_digest, outcome}` | `{request_id}` | `conflict/already_resolved`（带 `resolved_by`）、`conflict/digest_mismatch`、`conflict/too_early`、`expired`、`forbidden` | 命令 | approve；选项 `requires: privileged` 时 privileged | M1a |
| `interaction/get` | 请求 | `{session_id, request_id}` | 完整的交互请求和当前状态（供主进程弹原生确认框） | `not_found` | 天然 | read | M1a |

### Git、工作区与文件（M1b）

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope |
|---|---|---|---|---|---|---|
| `git/status` | 请求 | `{session_id}` | `{branch, upstream?, ahead, behind, in_progress?, files: [{path, index, worktree}]}` | `unsupported/not_a_repo` | 天然 | read |
| `git/diff` | 请求 | `{session_id, scope: unstaged\|staged\|last_turn\|{turn: id}}` | `{diff_token, files: [{path, old_path?, change, additions, deletions, binary}], snapshot?: {status, excluded}, mixed_changes_warning}` | `unavailable/snapshot_missing` | 天然 | read |
| `git/file_diff` | 请求 | `{session_id, diff_token, path}` | `{path, hunks: [{hunk_id, unit: hunk\|mode\|whole_file, header, lines \| lines_blob}]}` | `conflict/stale_diff` | 天然 | read |
| `git/stage`、`git/unstage`、`git/revert` | 请求 | `{command_id, session_id, diff_token, target: {all} \| {paths} \| {hunk_ids}}` | `{}` | `conflict/stale_diff`、`conflict/outcome_unknown`、`git_error` | 命令 | write |
| `git/commit` | 请求 | `{command_id, session_id, message}` | `{oid}` | `git_error`（`hook_failed`、`nothing_to_commit`）、`conflict/outcome_unknown` | 命令 | write |
| `git/push` | 请求 | `{command_id, session_id, set_upstream?}` | `{remote, branch}` | `git_error`（`auth_failed`、`rejected`） | 命令 | write |
| `git/branches` | 请求 | `{project_id}` | `{current, local, remote}` | — | 天然 | read |
| `worktree/check` | 请求 | `{session_id}` | `{removable, blockers: [rule], warnings: {ignored_files, case_only_dirty}}` | — | 天然 | read |
| `files/search` | 请求 | `{session_id \| project_id, query, limit?}` | `{results: [{path, score, ranges_utf16}]}` | — | 天然 | read |
| `workspace/open` | 请求 | `{session_id, target: terminal\|editor\|finder}` | `{}` | `unavailable/app_not_found` | 天然 | ui_local |

`last_turn` 范围的结果总是带 `mixed_changes_warning: true`（本地模式下可能混入用户自己或其他会话的改动，GT-1 要求注明）；worktree 会话也带，但界面可以弱化显示。

### 设置、认证、诊断

| 方法 | 类型 | 参数要点 | 结果要点 | 可能的错误 | 幂等 | scope | 里程碑 |
|---|---|---|---|---|---|---|---|
| `settings/get` | 请求 | — | `{settings}` | — | 天然 | read | M1a |
| `settings/update` | 请求 | `{command_id, patch}` | `{settings}` | `forbidden`（安全相关的键） | 命令 | write；安全相关的键 privileged | M1a |
| `auth/elevate` | 请求 | `{scopes: [write\|approve], reason?}` | 用户决定后返回 `{granted, expires_at}`；最长等待 2 分钟 | `forbidden/lineage`、`rate_limited`、`timeout` | — | 无（仅 `cli`） | M1a |
| `auth/elevation_get` | 请求 | `{elevation_id}` | `{executable, signing, pid, parent_chain, scopes, reason}` | `not_found` | 天然 | privileged | M1a |
| `auth/elevation_respond` | 请求 | `{command_id, elevation_id, decision: grant\|deny, scopes}` | `{}` | `expired` | 命令 | privileged | M1a |
| `policy/describe` | 请求 | `{method, params}` | `{action, details}`：主进程弹原生确认框前，用它取得要确认的操作的语义描述 | — | 天然 | privileged | M1a |
| `diag/export` | 请求 | `{command_id, include_sessions?}` | `{blob}`（zip） | — | 命令 | ui_local | M2（NFR-12 P1） |
| `diag/recording_export` | 请求 | `{session_id}` | `{blob}`（SP-1 格式的 JSONL） | — | 天然 | 仅 `test-harness` 和 `dev-auth` 构建 | M1a |

## 5.3.5 事件类型目录

事件信封：`{seq, from?, type, v, ts, cause?, fallback?, data}`（ADR-06）。`cause` 为 `{command_id, principal}`，客户端据此认出自己命令的结果。下表中"可合并"指发送时合并。

### 索引流

| type | 可合并 | data 要点 | 何时写入 |
|---|---|---|---|
| `project.upserted` | 否 | `{project_id, name, path, is_git, added_at, position}` | 添加、改名、排序 |
| `project.removed` | 否 | `{project_id}` | 移除 |
| `session.upserted` | 否 | `SessionSummary {session_id, project_id, agent, title, title_source, workspace_kind, branch?, status, unread, pending_interactions, last_activity_at, error_code?, read_only}` | 状态、标题、分支、未读变化（`IndexActor` 合并 100 ms 内的连续变化） |
| `session.removed` | 否 | `{session_id, reason: archived}` | 归档 |
| `agents.updated` | 否 | `{agents: [AgentStatus]}`：每个 agent 的 CLI（路径、版本、与已验证范围的关系）、适配器（版本、运行时状态）、登录状态（只有类型，不含邮箱和组织）、资源占用；另含 git 版本和 git-lfs | 检测结果或采样变化 |
| `host.updated` | 否 | `{version, weak_auth, pending_upgrade, power_assertion}` | 变化时 |
| `clients.updated` | 否 | `{clients: [{conn_id, principal_kind, role?, client_name, client_version, since, auth: verified\|weak\|peer, elevated?: {scopes, expires_at}}]}` | 连接、断开、提权 |
| `elevation.requested` | 否 | `{elevation_id, client_name, scopes, requested_at}` | CLI 申请提权 |
| `elevation.resolved` | 否 | `{elevation_id, decision: granted\|denied\|expired}` | 用户决定或超时 |
| `notification.intent` | 否 | `{intent_id, session_id, reason: needs_action\|turn_completed\|error, request_id?}` | 见 [5.2.9](02-host.md#529-关注度与通知)；不进入快照 |

### 会话流

| type | 可合并 | data 要点 | 何时写入 |
|---|---|---|---|
| `session.configured` | 否 | `{agent, workspace: {kind, path, branch?, base_ref?}, config: {<option>: value}, read_only}`；后写的覆盖先写的 | 创建、配置变化、恢复 |
| `session.capabilities` | 否 | 能力描述（[5.3.7](#537-会话能力描述)） | agent 会话建立或 resume 后；能力变化时 |
| `workspace.preparing` | 否 | `{phase: worktree_add\|checkout\|refresh, progress?}` | 创建 worktree 期间 |
| `workspace.ready` / `workspace.failed` | 否 | `{path, branch?}` / `{code}` | 准备完成或失败 |
| `turn.started` | 否 | `{turn_id, origin: user\|queued\|steer_new_turn\|autonomous}` | 回合开始 |
| `turn.completed` | 否 | `{turn_id, status: completed\|cancelled\|failed\|interrupted, stop_reason?, duration_ms, usage?: {input, output, cached?, cost?}, error_code?}` | 回合结束 |
| `item.started` | 否 | `{item_id, turn_id?, kind, group_id?, ...}`（按 `kind` 见下表） | 条目开始 |
| `item.delta` | **是** | `{item_id, text}` | 文本流 |
| `item.updated` | 否 | `{item_id, ...要更新的字段}` | 工具参数补全、输出、diff 等 |
| `item.completed` | 否 | `{item_id, status: completed\|failed\|cancelled\|abandoned, ...最终字段}` | 条目结束 |
| `interaction.requested` | 否 | 见 [5.3.6](#536-交互请求模型) | agent 发起请求 |
| `interaction.resolved` | 否 | `{request_id, status: resolved\|cancelled\|expired, outcome?, resolved_by?: {principal_id, client_kind, client_name}, reason?}` | 处理、撤回、失效 |
| `queue.updated` | 否 | `{version, paused?: {reason}, entries: [{queue_id, preview, created_at, created_by}]}` | 入队、出队、暂停、恢复 |
| `snapshot.recorded` | 否 | `{turn_id, phase: start\|end, status: ok\|partial\|failed\|skipped, excluded: [{path, reason}]}`；tree id 不下发 | 拍完快照 |
| `usage.updated` | 否 | `{context_used, context_size, cost?}`；后写的覆盖先写的 | agent 上报用量（显示是 P1，数据从 M1 起记录） |

**条目类型**（`item.started.kind`）

| kind | 字段 | 说明 |
|---|---|---|
| `user_message` | `content: PromptContent`（文本走 delta），`steered`，`queue_id?` | 用户消息；Steer 进当前回合的标 `steered: true` |
| `agent_message` | 文本走 delta | Markdown |
| `thought` | 文本走 delta | 思考或推理摘要，可折叠（MS-2） |
| `tool_call` | `ToolCall`（见下） | 一次工具调用，按适配器的 `toolCallId` upsert |
| `plan` | `{source: entries\|markdown, entries?: [{content, status: pending\|in_progress\|completed, priority?}], markdown?: Content}` | MS-4。Claude 在 SDK 会话里只有计划模式的 Markdown（SP-1） |
| `notice` | `{code, params}`，`code` 取 `agent_restarted`、`session_resumed`、`compaction`、`upstream_retry`（带 `attempt`、`max`）、`late_output_discarded`、`snapshot_unavailable` 等 | 消息流中的提示（MS-5） |
| `error` | `{code, agent_code?, retryable, guidance, raw?}` | 统一的错误条目（ADR-06），`guidance` 是恢复指引的语义码，如 `run_login_in_terminal`、`upgrade_cli`、`quota_exhausted`、`retry` |

**`ToolCall` 字段**（宿主输出语义，客户端只负责排版，对齐会意见 1.3 第 8 条）

| 字段 | 说明 |
|---|---|
| `tool_kind` | 开放枚举：`read`、`edit`、`delete`、`move`、`search`、`execute`、`think`、`fetch`、`switch_mode`、`other` |
| `title` | agent 给的标题（截断） |
| `paths` | 涉及的路径，工作区内的写成相对路径 |
| `command?`、`cwd?` | 命令文本（Codex 的 `sh -c` 由适配器拆开） |
| `exit_code?`、`signal?` | 退出码，由提取层取得（[5.6.3](06-agent-driver.md#563-提取层claude-与-codex-对照)） |
| `output?` | `Content`：内联或 blob；`output_streaming` 标出输出是否流式到达 |
| `diffs?` | `[{path, change: add\|update\|delete, hunks?: Content, stat: {added, removed}}]`，来自 agent 的片段，只用于展示过程；回合级的变更以 git 为准 |
| `duration_ms` | 宿主按开始和结束时间计算 |
| `extraction` | `full`、`partial`、`none`：拿不到退出码或输出时降级显示（MS-3） |
| `group_id` | 同一回合内两段 agent 文字之间连续的工具调用共用一个组，客户端据此合并成一行摘要（截图笔记中的"编辑了文件、读取文件、运行了命令"） |

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
| `permission` | `{title, tool: {item_id?, tool_kind, command?, paths}, options: [Option], detail?: Content}` | `selected {option_id}`；`dismissed` |
| `question` | `{prompt, fields: [{field_id, label, type: single_choice\|multi_choice\|text\|boolean\|number, choices?: [{value, label, description?}], allow_custom, required}], unsupported_schema}` | `answer {values: {field_id: value}}`；`dismissed` |
| `plan_confirm` | `{plan: Content, options: [Option]}` | `selected {option_id}`；`dismissed` |
| `auth_url` | `{url, message}`（M1 不产生） | `opened`；`dismissed` |

**选项**：`Option { option_id, name, kind, effect, requires }`

| 字段 | 取值 |
|---|---|
| `kind` | ACP 原样的 `allow_once`、`allow_always`、`reject_once`、`reject_always`（开放枚举） |
| `effect.scope` | `once`、`session`、`persistent`、`mode_switch`、`reject`、`plan_accept`、`plan_revise` |
| `effect.persistent` | 是否会写入 agent 自己的权限规则或执行策略（AP-2 要求界面标明） |
| `effect.ends_turn` | 选它会不会结束整个回合（Codex #556） |
| `effect.mode_after?`、`effect.full_access` | 选它之后会话切换到的模式；是否等于完全访问 |
| `requires` | `approve` 或 `privileged`。`kind` 为 `allow_always` 的一律 `privileged`：非 UI principal 不能选（ADR-05），UI 也必须经主进程的原生确认框（ADR-03） |

**选项 ID 到生效范围的对照表**（SP-1 实测，claude-agent-acp 0.91.0、codex-acp 2.2.2）

对照表是数据，按锁定的适配器版本维护在驱动层（[5.6.4](06-agent-driver.md#564-交互请求的分类与选项映射)），每次升级适配器都用录制回归。

| agent | 场景 | option_id | ACP kind | scope | persistent | ends_turn | requires | 界面要点 |
|---|---|---|---|---|---|---|---|---|
| Claude | 文件编辑（Write、Edit、MultiEdit、NotebookEdit） | `allow-once` | allow_once | once | 否 | 否 | approve | — |
| Claude | 文件编辑 | `allow-with-updates` | allow_always | session | 否 | 否 | privileged | agent 原文如 "allow all edits in claude/ during this session" |
| Claude | 命令（Bash）及其他工具 | `allow-with-updates` | allow_always | persistent | **是** | 否 | privileged | **同一个 option_id，在命令上会写入持久规则**，按工具区分；无法判断时按 persistent 处理 |
| Claude | 任意 | `reject` | reject_once | reject | 否 | 否 | approve | 拒绝后回合继续 |
| Claude | 计划确认（ExitPlanMode，`switch_mode`） | `exit-plan-default` | allow_once | plan_accept，`mode_after: default` | 否 | 否 | approve | 之后的编辑逐个审批 |
| Claude | 计划确认 | `exit-plan-auto` | allow_always | mode_switch，`mode_after: auto` | 否 | 否 | privileged | 切换到低打扰的 Auto 模式（U6） |
| Claude | 计划确认 | `exit-plan-clear-auto` | allow_always | mode_switch，`mode_after: auto`，并清空上下文 | 否 | 否 | privileged | — |
| Claude | 计划确认 | `exit-plan-bypass` | allow_always | mode_switch，`full_access: true` | 否 | 否 | privileged | 等于切换到完全访问 |
| Claude | 计划确认 | `reject` | reject_once | plan_revise | 否 | 否 | approve | 继续规划 |
| Codex | 文件修改 | `allow_once` | allow_once | once | 否 | 否 | approve | — |
| Codex | 文件修改 | `allow_for_session` | allow_always | session | 否 | 否 | privileged | "don't ask again for these files" |
| Codex | 命令 | `allow_once` | allow_once | once | 否 | 否 | approve | — |
| Codex | 命令 | `accept_execpolicy_amendment` | allow_always | persistent | **是** | 否 | privileged | 写入 Codex 的执行策略 |
| Codex | 文件修改、命令 | `cancel` | reject_once | reject | 否 | **是** | approve | 界面写明"拒绝将结束本回合"（ADR-07） |
| Codex | 计划确认（`toolCallId` 以 `plan-review:` 开头） | `implement_plan` | allow_once | plan_accept | 否 | 否 | approve | 在同一个 prompt 里继续实施 |
| Codex | 计划确认 | `revise_plan` | reject_once | plan_revise | 否 | 未实测 | approve | 按"可能结束回合"提示，兼容套件补测 |
| 任意 | 对照表里没有的 option_id | 按 kind 兜底 | allow_once → once / approve；allow_always → persistent / privileged；reject_once → reject（Codex 视为 ends_turn）；reject_always → reject、persistent / approve | | | | | 同时记日志，提示更新对照表 |

**`actionable_by`**：所有选项都是 `privileged` 时只有 `desktop_ui`；否则为 `desktop_ui`、`cli`（已提权且含 `approve`）、`test_harness`，M3 加 `remote_device`。宿主在回答时还会再检查一遍（ADR-04）。

**摘要**：`d_` 加上 `{session_id, request_id, turn_id, request}` 规范 JSON（键排序）的 sha-256 前 32 位十六进制（SP-4a）。

**生命周期**：`pending` → `resolved`（有人回答）| `cancelled`（agent 撤回或回合被中断）| `expired`（所属的适配器实例退出或宿主重启，SEC-10）。

**提问的映射**（SP-1）：Claude 的 AskUserQuestion 每个问题是一组 `question_N`（`oneOf` 选项）加 `question_N_custom`（自由文本），映射成一个 `single_choice` 字段并设 `allow_custom: true`，回答时宿主拆回两个字段；Codex 的表单字段按 JSON Schema 类型映射。遇到无法表示的 schema，设 `unsupported_schema: true`，客户端只提供"跳过"。

## 5.3.7 会话能力描述

宿主统一提供（AG-4），新建会话页使用缓存的同一结构（ADR-09）。

```
SessionCapabilities {
  agent: "claude" | "codex",
  versions: { adapter, cli, cli_compat: verified | newer_untested | older_untested | unsupported },
  prompt:  { image, file_ref, max_image_bytes },
  turn:    { queue: true, steer: { supported, side_effects }, interrupt: true },
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
- `steer.side_effects` 为 true（Claude）时，界面默认仍然排队，插话需要用户明确选择（ADR-07）。
- `risk` 由宿主按配置项映射表给出（[5.6.5](06-agent-driver.md#565-配置项映射)），`full_access` 的值只能经原生确认框选择。

## 5.3.8 错误码

应用错误统一用 JSON-RPC `-32000`，按 `error.data.code` 分支；`data.reason` 进一步区分；`message` 是给开发者看的英文。错误码表沿用技术选型讨论会的版本（`architect.md` ADR-04 一节），本节补充 `reason` 清单和 M1 新增的码：

| code | 常见 reason | 能否重试 | 客户端处理 |
|---|---|---|---|
| `version_unsupported` | — | 否 | 提示升级 |
| `not_initialized` | — | 否 | 程序错误 |
| `unauthenticated` | `bad_nonce`、`signature_invalid`、`bad_key` | 否 | 断开 |
| `forbidden` | `scope`（`data.scope` 指出缺哪项）、`lineage`、`role`（需要控制连接）、`user_activation_required`（主进程本地产生） | 否 | CLI 申请提权；界面走原生确认 |
| `not_found` | — | 否 | 刷新 |
| `conflict` | `already_resolved`、`turn_mismatch`、`digest_mismatch`、`too_early`、`version_mismatch`、`stale_diff`、`outcome_unknown`、`turn_running` | 否 | 收起或刷新；`already_resolved` 显示"已在别处处理"（AP-4） |
| `expired` | `adapter_exited`、`host_restarted` | 否 | 收起卡片 |
| `unsupported` | `data.capability` 指出能力；`not_a_repo` | 否 | 隐藏功能 |
| `unavailable` | `cli_missing`、`cli_not_logged_in`、`cli_too_old`、`runtime_installing`、`runtime_failed`、`adapter_starting`、`adapter_crashed`、`pending_upgrade`、`quota_exhausted`、`app_not_found`、`snapshot_missing` | 视 reason | 显示恢复指引（AG-2、MS-5） |
| `agent_error` | `data.agent_code`、`data.agent_message`（截断、脱敏） | 否 | 显示 |
| `git_error` | `hook_failed`、`nothing_to_commit`、`auth_failed`、`rejected`、`lock_held`（M1 新增） | 视 reason | 显示 stderr 摘要 |
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
| 单个 delta（转义后） / 合并后 / 内联内容上限 | 16,000 / 32,000 / 16,000 字符 |
| 标题、名称截断 | 1,000 字符 |
| 写入时合并窗口 | 40 ms（宿主内部，ADR-06） |
| `coalesce_ms`（发送时合并） | 0–1,000；桌面默认 0 |
| `max_queue_bytes`（每连接的实时事件积压，超出即重置流） | 4 MiB |
| 每连接进行中的请求 / 订阅 | 64 / 64 |
| 快照窗口 | 20 个回合或 2,000 个条目 |
| `history/page` 单页 | 不超过 48,000 字节 |
| `blob/put` | 单个 32 MiB；每连接未完成上传合计 64 MiB；图片另受 agent 限制，默认 10 MiB |
| 每会话排队消息 | 50 条 |
| 命令记录保留 | 48 小时（ADR-04 下限 24 小时） |
| 提权 | 每次最长 15 分钟；每个可执行文件最多 3 个未决申请，10 秒内最多 1 次 |
| 中断兜底 / Steer 超时 | 10 s / 5 s |

## 5.3.10 兼容与代码生成

- 兼容性检查区分方向（ADR-04）：宿主发往客户端的消息，删除字段、必填改可选、收窄类型都是破坏性的；客户端发往宿主的消息，新增必填字段是破坏性的。
- 新增方法、事件类型、枚举值、可选字段都不升主版本；新的交互请求类型必须带特性门控。
- 生成链：Rust（serde + schemars）→ `protocol/schema/v1.json` → `tools/gen-protocol-ts`（hey-api 生成 TS 类型和 zod，非 strict）→ 后处理：已知事件类型的字面量列表和类型守卫、兜底成员用 `.loose()`、开放枚举写成 `(string & {})`（SP-4a 报告）。
- "类型已知但数据残缺"的事件：客户端降级成未知事件并记日志；CI 对金样例严格校验，不允许降级（ADR-04 修订）。
- iOS 怎样使用协议由 ADR-15 决定；M1 只保证 `cupper-protocol` 能编译到 iOS 模拟器、金样例里有未知值的样例。
