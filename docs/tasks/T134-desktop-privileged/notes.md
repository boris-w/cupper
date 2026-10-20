# T134 工作笔记

- 执行者：子 agent（工程师；T077 的执行者接着做）

## 开工前核对现状

- **真实输入**：`guardOf(webContents)?.take()`（T077）。Esc 由本任务判断：证据的 `key` 为 `escape` 时拒绝。
- **`policy/describe` 的参数**：
  - 宿主要的是被描述方法的**完整参数**（`Confirmable::parse` 按各方法的参数类型反序列化），所以主进程要先拼出整条命令（带 `command_id`），再取描述；确认后用同一个 `command_id` 发出。
  - 不需要原生确认的形式，宿主返回 invalid params；这时返回 `failed`，渲染进程应走普通路径。
- **各 action 的内容来源**：

  | action | 内容来源 | 发出的命令 |
  |---|---|---|
  | `interaction` | `interaction/get`（请求原文、选项的 `name` 和 `effect`、`request_digest`）+ `policy/describe` | `interaction/respond`，`input: {kind: "native_confirm"}` |
  | `set_config` | `policy/describe`（`full_access`：`{session_id, mode}`）+ `session/get`（标题） | `session/set_config` |
  | `create_session` | `policy/describe`（`{agent, project_id, mode}`） | `session/create`，结果带 `session_id` |
  | `trust_project` | `project/trust_info`（findings、`trust_digest`） | `project/trust`；遇到 `digest_mismatch` 重新取内容、再确认一次 |
  | `add_project` | 选择目录 → `project/add`（`auto_empty` 时直接完成）；其余同 `trust_project` | 同上，结果带 `project_id` |
  | `elevation` | `auth/elevation_get`（`verified`、`self_reported`、`scopes`） | `auth/elevation_respond`：`grant` 或 `deny`，`scopes` 不含没勾的 approve |
  | `confirm_config` | 见下面的缺口 | `session/confirm_config` |
  | `settings` | — | 宿主的 `settings/update` 到 T036 前都是 `unsupported`，返回 `failed` |

- **设计缺口：`confirm_config` 的内容从哪来**：
  - `session/confirm_config` 要 `config_digest`，可是 `PrivilegedAction` 的 `confirm_config` 只有 `session_id`；
  - `config_digest`、原因（`full_access_unconfirmed` 或 `tier_above_setter`）和当前配置只在会话流的 `session.configured` 事件里，没有读方法直接给；
  - `policy/describe` 的 `details` 也只回显 `{session_id, config_digest}`。
  - 可选做法：
    - (a) 主进程在 control 连接上用 host-client 的 `SessionView` 订阅一次会话流，等到 `live` 就取 `configured`，然后退订。内容来自宿主，接口不变，但要拉一次会话快照（约 25 行）。
    - (b) 给 `PrivilegedAction.confirm_config` 加 `config_digest`，由渲染进程从它的视图里给出。接口只加成员，但确认框显示不出档位和原因。
  - 我倾向 (a)。
- **`bundled_cli`**：`VerifiedRequester` 里还没有（T133 没有合并），按现有字段实现，写进交接。
- **T038 执行者的三点**：
  - `elevation_respond` 的 `scopes` 只取申请范围之内的，主进程只从申请的 `scopes` 里去掉没勾的 approve；
  - 已不在未决的申请，`elevation_get` 回 `not_found`，这时返回 `failed`；
  - 授权到期时间在 `clients.updated` 里，确认框不显示。

## 开工前的规模估计

行数指格式化后的新增行，含注释和空行。最近两个任务的实现部分实际是草估的 1.0–1.1 倍（T077、T115），之前一直按 1.3 计，下面两个都列出。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| privileged 框架 | 主进程的 IPC 处理（只答主文档）、真实输入与 Esc、一次一个（`busy`）、按 kind 冷却 2/10/60 s 与重新计数（连续 10 分钟没有取消）、时钟注入；preload 接上 | 110 |
| 各 action 取内容与发命令 | 上表七种，`add_project` 的三条分支、`digest_mismatch` 的重新确认；`confirm_config` 按 (a) | 250 |
| 确认框 | 每种 action 的 message、detail、按钮（默认按钮是取消、拒绝或不信任）；`escapeInvisible`；持久选项的文案规则（5.3.6：没有规则时写"由 agent 决定"，`interpreter`、`unknown` 的附加说明）；findings 列表与工作区外链接；提权框的核实与自述分开、approve 勾选项；`dialog.*` 在调用时取 | 160 |
| 文案 | `privileged` 命名空间（约 45 个键）和注册 | 60 |
| **实现小计** | | **580**；乘 1.1 约 **640**，乘 1.3 约 **755** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-privileged | `busy`；冷却逐次加长与重新计数；Esc；没有真实输入；七种 action 的确认框参数、默认按钮与发出的命令（替换确认框函数记录参数，mock 宿主）；`add_project` 三条分支；提权框的勾选项与 `scopes`；`settings` 返回 `failed` | 300 |

**合计约 940 行（乘 1.1）到 1,055 行（乘 1.3）**，贴着约 1,000 行的线。按任务书，开工前停下汇报，没有写代码。
