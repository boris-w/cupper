# 5.4 数据模型

- 所属：`docs/02-design.md` 第 5 节
- 依据：ADR-01（数据目录）、ADR-04（ID、时间、命令保留）、ADR-06（存储与事件模型）、ADR-08（运行时目录）、ADR-10（快照对象库、worktree 位置）；SP-4a 的 `store.rs`
- 只有宿主读写这些数据（单写者）。桌面界面、CLI 只通过协议访问（NFR-1）

## 5.4.1 存储位置

```
~/Library/Application Support/cupper/            宿主数据目录，0700，属主检查（ADR-01）
├── host.db  (+ -wal, -shm)                      主数据库，SQLite WAL
├── raw.db   (+ -wal, -shm)                      原始 ACP 帧，单独一个库，方便按时间整体清理
├── host.lock                                    单实例 flock
├── run/host.sock                                套接字，0600（路径过长时退到每个用户自己的 DARWIN_USER_TEMP_DIR 下，5.2.13）
├── blobs/sha256/<前 2 位>/<hash>                 按内容寻址的 blob 文件
├── git/<repo_key>/objects/                       回合快照的私有对象库（ADR-10）
├── git/<repo_key>/ws/<workspace_key>/            快照档用的 GIT_DIR：白名单配置、info/、临时 index（5.2.10）
├── git/empty-hooks/                             空目录，0500；宿主自动调用 git 时 core.hooksPath 指向这里
├── runtime/<名称>/<版本>/  + .installed.json      Node 与适配器（ADR-08）；runtime/.staging/ 为安装中的临时目录
├── probe/<agent>/                                能力探测用的空目录，每次探测前重建（5.6.7）
├── backups/host-v<schema>-<时间>.db              迁移前的备份，保留最近 3 份
├── crash/                                       panic 报告
└── ui/                                          桌面端 Electron 的 userData（主进程启动时显式设置）
~/Library/Logs/cupper/                           0700；host.log、adapter-<agent>.log、main.log（轮转见 5.11）
~/.cupper/worktrees/<仓库名>-<短哈希>/<会话短名>/    worktree（ADR-10，路径不含空格，可在设置中修改）；根目录 0700
```

- **Electron 的 userData 必须显式设到 `cupper/ui/`**。Electron 默认用 `~/Library/Application Support/<productName>`，产品名 "Cupper" 和宿主目录 "cupper" 在大小写不敏感的 APFS 上是同一个目录，不设置就会混在一起。
- 上面的目录树以发布构建为例。四种构建的数据互不相通（v1.3，种类见 [5.7.9](07-desktop.md#579-构建种类)）：

  | 构建 | 数据目录 | 日志目录 | worktree 根目录 |
  |---|---|---|---|
  | 发布 | `cupper/` | `~/Library/Logs/cupper/` | `~/.cupper/worktrees/` |
  | 自用 | `cupper-local/` | `~/Library/Logs/cupper-local/` | `~/.cupper-local/worktrees/` |
  | 开发 | `cupper-dev/` | `~/Library/Logs/cupper-dev/` | `~/.cupper-dev/worktrees/` |
  | e2e | 每个用例一个临时目录 | 同一临时目录下 | 同一临时目录下 |

## 5.4.2 SQLite 表结构

全部表由宿主的迁移脚本创建（[5.4.6](#546-迁移)）。时间列都是 UTC 毫秒；JSON 列存 UTF-8 文本。连接参数：`journal_mode=WAL`、`synchronous=NORMAL`、`checkpoint_fullfsync=ON`（`pending_external` 那一批另用 `FULL` 加 `fullfsync`，[5.2.8](02-host.md#528-事件存储)）、`foreign_keys=ON`、`busy_timeout=5000`。

### host.db

```sql
-- 键值元数据：schema_version、layout_version、host_id、created_at、last_start_at、last_clean_exit_at、
--             data_epoch、boot_session_uuid（data_epoch 的判定见 5.2.8）
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;

CREATE TABLE projects (
  project_id     TEXT PRIMARY KEY,
  path           TEXT NOT NULL UNIQUE,     -- realpath
  name           TEXT NOT NULL,
  git_common_dir TEXT,                     -- 非 git 项目为 NULL
  repo_key       TEXT,                     -- sha256(git_common_dir) 前 16 位，快照对象库的目录名
  position       INTEGER NOT NULL,
  added_at       INTEGER NOT NULL,
  removed_at     INTEGER,
  trust_state    TEXT NOT NULL DEFAULT 'untrusted',  -- untrusted | trusted | stale（5.2.6 项目信任）
  trust_digest   TEXT,                     -- 信任时的摘要：工作区到 git 根目录各级的 .claude/、.mcp.json、.codex/，链接按目标内容（5.2.6）
  trusted_at     INTEGER,
  trusted_by     TEXT                      -- pr_desktop、pr_test，或 auto_empty（扫描范围里没有任何文件）
);

CREATE TABLE sessions (
  session_id        TEXT PRIMARY KEY,
  project_id        TEXT NOT NULL REFERENCES projects(project_id),
  agent             TEXT NOT NULL,           -- claude | codex
  agent_session_id  TEXT,                    -- ACP 的 sessionId；resume 时使用
  title             TEXT,
  title_source      TEXT NOT NULL DEFAULT 'none',  -- none | prompt | agent | user（用户改过的标题不被 agent 覆盖）
  workspace_kind    TEXT NOT NULL,           -- local | worktree
  workspace_path    TEXT NOT NULL,           -- realpath（SP-1：Codex 按 cwd 过滤要求真实路径）
  worktree_branch   TEXT,
  base_ref          TEXT,
  base_oid          TEXT,
  config_json       TEXT NOT NULL DEFAULT '{}',    -- 当前的模式、模型、推理强度
  initial_prompt_json TEXT,                  -- 第一条消息，随受理事务写入，第一个回合开始时清空（5.2.6）
  mode_set_by       TEXT,                    -- 设定当前审批档位的 principal 稳定 ID；默认档位记为创建者（5.2.6）
  full_access_confirmed_at INTEGER,          -- 经原生确认进入 full_access 的时间
  state             TEXT NOT NULL,           -- idle | running | error | interrupted；供崩溃恢复使用的粗粒度状态
  read_only         INTEGER NOT NULL DEFAULT 0,    -- 分支已不存在的已恢复会话（PS-3）
  queue_version     INTEGER NOT NULL DEFAULT 0,
  queue_paused      TEXT,                    -- 暂停原因；NULL 表示未暂停
  last_read_seq     INTEGER NOT NULL DEFAULT 0,    -- 全局已读位置（U4）
  last_turn_end_seq INTEGER NOT NULL DEFAULT 0,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  archived_at       INTEGER
);
CREATE INDEX sessions_by_project ON sessions(project_id, archived_at, updated_at DESC);

CREATE TABLE turns (
  session_id      TEXT NOT NULL,
  turn_id         TEXT NOT NULL,
  turn_no         INTEGER NOT NULL,          -- 会话内从 1 递增，客户端视图按它排序（history/page 拼接用）
  origin          TEXT NOT NULL,             -- user | queued | steer_new_turn | autonomous
  status          TEXT NOT NULL,             -- running | completed | cancelled | failed | interrupted
  first_seq       INTEGER NOT NULL,          -- 会话流中 turn.started 的 seq
  last_seq        INTEGER,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER,
  stop_reason     TEXT,
  snapshot_start  TEXT,                      -- git tree id（只在宿主内部使用，不下发）
  snapshot_end    TEXT,
  snapshot_status TEXT,                      -- ok | partial | failed | skipped
  usage_json      TEXT,
  PRIMARY KEY (session_id, turn_id)
) WITHOUT ROWID;
CREATE INDEX turns_by_time ON turns(session_id, started_at);
CREATE UNIQUE INDEX turns_by_no ON turns(session_id, turn_no);

-- 回合结束时写入：重建这个回合所需的事件体序列。会话流的日志被裁剪后，历史从这里来（history/page）
CREATE TABLE turn_snapshots (
  session_id TEXT NOT NULL,
  turn_id    TEXT NOT NULL,
  turn_no    INTEGER NOT NULL,               -- history/page 按它向前翻页
  v          INTEGER NOT NULL,               -- ops 的版式版本，目前为 1（T026）
  ops        TEXT NOT NULL,                  -- JSON 数组：{type, v, data}，每个 op 带自己类型的 v，读时逐个升级
  bytes      INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn_id)
) WITHOUT ROWID;
CREATE UNIQUE INDEX turn_snapshots_by_no ON turn_snapshots(session_id, turn_no);

-- 事件日志：同一条流 seq 连续（ADR-06）
CREATE TABLE events (
  stream TEXT NOT NULL,                      -- index | session/<session_id>
  seq    INTEGER NOT NULL,
  type   TEXT NOT NULL,
  v      INTEGER NOT NULL,
  ts     INTEGER NOT NULL,
  json   TEXT NOT NULL,                      -- 完整的事件信封
  PRIMARY KEY (stream, seq)
) WITHOUT ROWID;

-- 每条流一个检查点：窗口视图 + 对应的 seq
CREATE TABLE checkpoints (
  stream     TEXT PRIMARY KEY,
  at_seq     INTEGER NOT NULL,
  view_v     INTEGER NOT NULL,
  view       TEXT NOT NULL,
  created_at INTEGER NOT NULL
) WITHOUT ROWID;

-- 交互请求：M1 打下 SEC-10 的数据基础（绑定到具体请求和所属的适配器实例）
CREATE TABLE interactions (
  request_id       TEXT PRIMARY KEY,
  session_id       TEXT NOT NULL,
  turn_id          TEXT,                     -- 回合外的请求为 NULL
  item_id          TEXT,
  kind             TEXT NOT NULL,            -- permission | question | plan_confirm | auth_url
  digest           TEXT NOT NULL,
  status           TEXT NOT NULL,            -- pending | resolved | cancelled | expired
  adapter_instance TEXT NOT NULL,
  request_json     TEXT NOT NULL,
  outcome_json     TEXT,
  resolved_by      TEXT,                     -- principal 稳定 ID 和客户端名
  reason           TEXT,
  created_at       INTEGER NOT NULL,
  resolved_at      INTEGER
);
CREATE INDEX interactions_pending ON interactions(session_id) WHERE status = 'pending';

CREATE TABLE queue_entries (
  queue_id     TEXT PRIMARY KEY,
  session_id   TEXT NOT NULL,
  position     INTEGER NOT NULL,
  content_json TEXT NOT NULL,
  principal    TEXT NOT NULL,
  command_id   TEXT NOT NULL,
  created_at   INTEGER NOT NULL
);
CREATE INDEX queue_by_session ON queue_entries(session_id, position);

-- 命令幂等（ADR-04）
CREATE TABLE commands (
  principal   TEXT NOT NULL,                 -- 稳定 ID：pr_desktop | pr_cli | pr_test | pr_dev_<设备>
  command_id  TEXT NOT NULL,
  method      TEXT NOT NULL,
  params_hash TEXT NOT NULL,                 -- 去掉 command_id、attestation、input 后，按 RFC 8785 规范化的 JSON 的 sha-256
  state       TEXT NOT NULL,                 -- done | pending_external
  result      TEXT,
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (principal, command_id)
) WITHOUT ROWID;
CREATE INDEX commands_by_time ON commands(created_at);

-- 适配器进程：拉起后立即写入，供崩溃后按进程组清理（ADR-01 的 SP-4a 修订）
CREATE TABLE adapters (
  instance_id     TEXT PRIMARY KEY,
  agent           TEXT NOT NULL,
  pid             INTEGER NOT NULL,
  pgid            INTEGER NOT NULL,
  start_us        INTEGER NOT NULL,          -- 进程启动时间，用来识别 pid 复用
  adapter_version TEXT NOT NULL,
  node_version    TEXT NOT NULL,
  cli_path        TEXT,
  cli_version     TEXT,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER,
  exit_status     TEXT
);
CREATE INDEX adapters_live ON adapters(agent) WHERE ended_at IS NULL;

-- 每种 agent 最近一次的能力描述（ADR-09，新建会话页使用）
CREATE TABLE agent_capabilities (
  agent           TEXT PRIMARY KEY,
  adapter_version TEXT NOT NULL,
  cli_version     TEXT,
  captured_at     INTEGER NOT NULL,
  json            TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE blobs (
  hash         TEXT PRIMARY KEY,               -- sha-256 十六进制
  size         INTEGER NOT NULL,
  media_type   TEXT,
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL
) WITHOUT ROWID;
-- 引用关系，供回收使用；owner 为 session_id
CREATE TABLE blob_refs (
  hash  TEXT NOT NULL,
  owner TEXT NOT NULL,
  PRIMARY KEY (hash, owner)
) WITHOUT ROWID;

CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,                    -- JSON
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

-- 审计日志（5.8.5）
CREATE TABLE audit (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  event       TEXT NOT NULL,
  conn_id     TEXT,
  principal   TEXT,
  peer_json   TEXT,                            -- pid、可执行文件路径、签名 identifier、Team ID、错误类别
  detail_json TEXT
);
CREATE INDEX audit_by_ts ON audit(ts);

-- P1（ADR-06 会话内搜索）：回合结束时写入
-- CREATE VIRTUAL TABLE item_fts USING fts5(text, session_id UNINDEXED, turn_id UNINDEXED, item_id UNINDEXED);
```

### raw.db

```sql
CREATE TABLE raw_frames (
  instance_id      TEXT NOT NULL,              -- 适配器实例
  seq              INTEGER NOT NULL,           -- 该实例内递增
  ts               INTEGER NOT NULL,
  dir              TEXT NOT NULL,              -- send | recv | recv_nonjson | stderr | note
  agent_session_id TEXT,                       -- 从 params.sessionId 提取，方便按会话导出
  frame            TEXT NOT NULL,              -- 已做结构化脱敏（只处理账号字段和已知格式的凭据，5.11.2）
  PRIMARY KEY (instance_id, seq)
) WITHOUT ROWID;
CREATE INDEX raw_by_ts ON raw_frames(ts);
CREATE INDEX raw_by_session ON raw_frames(agent_session_id, ts);
```

`diag/recording_export` 把原始帧导出成录制文件。录制格式（头行、版本号、`t` 的含义、`dir` 的取值、note 行、多实例分段）的规格见 [5.9.3](09-testing.md#593-录制与金样例)；SP-1 的录制一次性转换成这个格式。

## 5.4.3 数据放在哪里

| 数据 | 位置 | 写者 | 说明 |
|---|---|---|---|
| 事件日志 | `events` | 每条流的唯一写者 | 客户端看到的一切都来自这里 |
| 流的当前状态 | 内存视图 + `checkpoints` | 同上 | 会话视图只保留窗口 |
| 会话历史 | `turn_snapshots`（+ 窗口内的 `events`） | 会话 actor | **历史只以宿主的记录为准**（ADR-06），不依赖适配器回放 |
| 会话、项目元数据 | `sessions`、`projects` | 会话 actor / `IndexActor` | 用于列表、恢复、路径查找 |
| 回合索引 | `turns` | 会话 actor | 历史分页、"最近一回合"的 diff、崩溃恢复 |
| 交互请求 | `interactions` + 事件 | 会话 actor | 表用于查询和恢复，事件用于下发 |
| 排队消息 | `queue_entries` + `queue.updated` | 会话 actor | 宿主重启后队列还在 |
| 命令结果 | `commands` + 内存表 | 账本（经写线程） | 与命令产生的事件同事务 |
| 进程记录 | `adapters` | `AgentRuntime` | 崩溃后清理 |
| 能力缓存 | `agent_capabilities` | `AgentRuntime` | — |
| 设置 | `settings` | 设置模块 | 界面偏好不在这里 |
| 审计 | `audit` + `host.log` | 网关、策略 | — |
| 图片、长输出、长 diff | `blobs/` 文件 + `blobs` 表 | 写线程 | — |
| 回合快照的 git 对象 | `git/<repo_key>/objects/` | git 服务 | 用户的 `.git` 不写入任何东西（ADR-10）；快照档在 `git/<repo_key>/ws/` 下用 cupper 自己的 GIT_DIR（[5.2.10](02-host.md#5210-git-与-worktree-服务)） |
| 原始 ACP 帧 | `raw.db` | 写线程 | 14 天 |
| Node 与适配器 | `runtime/` | 运行时安装器 | 版本目录写入后不再修改 |
| worktree | `~/.cupper/worktrees/` | git 服务 | 用户的代码，不属于宿主数据，清理按 ADR-10 的规则 |
| 界面偏好 | Electron userData（`cupper/ui/`） | 桌面主进程 | 发送快捷键、主题覆盖、面板尺寸、草稿 |

## 5.4.4 ID 规则

| 对象 | 格式 | 生成方 | 说明 |
|---|---|---|---|
| 宿主 | `h_` + UUIDv4 | 首次运行 | 随机，不含时间；M3 跨电脑时用 `(host_id, id)` 区分 |
| 项目、会话、回合、条目、交互请求、排队消息、适配器实例、提权申请 | `p_`、`s_`、`t_`、`i_`、`r_`、`q_`、`a_`、`e_` + UUIDv7（32 位十六进制，不带连字符） | 宿主的 `IdGen`（时间取自 `Clock`，[5.2.16](02-host.md#5216-时间与可测性)） | 按时间有序，B 树局部性好（SP-4a 的 `new_id`） |
| principal | `pr_desktop`、`pr_cli`、`pr_test`、`pr_dev_<设备 ID>` | 宿主 | 稳定，不随连接变化（命令幂等的键） |
| 连接 | `c_` + 宿主本次运行内递增的整数 | 宿主 | 只用于审计和 `clients` 列表 |
| command_id | UUIDv7 字符串，最长 64 字符 | 客户端 | 宿主只校验长度和字符集 |
| 摘要 | `d_` + 64 位十六进制（完整的 sha-256，规范 JSON 按 RFC 8785） | 宿主 | [5.3.6](03-protocol-v1.md#536-交互请求模型) |
| hunk | `hk_` + sha-256(路径、hunk 头、内容、所在 diff 的 token) 前 24 位 | 宿主 | 按内容寻址 |
| diff token | `dt_` + 随机 | 宿主 | 指向 `DiffCache` 里的一次 diff |
| blob | sha-256 十六进制 | 宿主（上传时校验客户端给的值） | 按内容寻址 |
| 通知意图、活动组 | `n_`、`g_` + UUIDv7 | 宿主的 `IdGen` | v1.7 补充（T011 规划）：`notification.intent.intent_id`、条目的 `group_id` |
| blob 的 `hash` 字段 | 同上一行，协议类型为 `BlobHash`，带 `x-id` 注解 | — | v1.7 补充：内容里有每次运行都不同的值时哈希也会变，金样例按 `x-id` 规范化 |

- 工具调用的条目用宿主生成的 `i_` ID，适配器的 `toolCallId` 只在驱动层的映射表里作 upsert 键（ADR-06 的"工具调用 ID 用 toolCallId"在这里细化为"upsert 键"）。原因是 `toolCallId` 只在一个 agent 会话内唯一，而且不同适配器的格式不同。
- agent 侧的 ID（`agent_session_id`、`toolCallId`）不出现在协议里。

## 5.4.5 保留与回收

| 数据 | 保留规则 | 回收时机 | 里程碑 |
|---|---|---|---|
| `events` | 每条流至少保留检查点之后的全部事件，加上检查点之前最近 10,000 条或 7 天（取多者），供断线续传 | 每个回合结束写检查点时裁剪 | M1a |
| `turn_snapshots`、`turns` | 会话存在期间一直保留 | 删除会话时（M1 没有删除功能） | M1a |
| `checkpoints` | 每条流只有一行 | 覆盖写 | M1a |
| `commands` | 48 小时 | 启动时和每小时 | M1a |
| `interactions` | 与会话相同 | — | M1a |
| `raw.db` | 14 天（ADR-06），总大小超过 512 MB 时先删最旧的 | 每小时；空闲时 `VACUUM` | M1a |
| `audit` | 90 天 | 每天 | M1a |
| `adapters` | 运行中的实例，加上最近 100 条已结束的记录 | 启动时 | M1a |
| blob 文件 | 有引用就保留 | 删除会话时按 `blob_refs` 回收；没有引用且超过 1 天的孤儿文件在启动时删除 | 孤儿清理 M1a；按会话回收 M2 |
| git 快照对象 | 每个会话最近 50 个回合，或 30 天内的回合 | M2：把仍需要的 tree 用 `rev-list --objects` 重新打包进新的对象库，再替换旧库。M1 只累积，大小在设置页显示；快照会复制没被 gitignore 的未跟踪文件（可能含密钥），所以 M1b 的设置页提供"清除快照对象"，清除后旧回合的 diff 显示"快照不可用"（安全建议 #15） | 清除入口 M1b；按规则回收 M2（NFR-11 为 P1） |
| `backups/` | 最近 3 份 | 迁移成功后 | M1a |
| 运行时旧版本 | 没有适配器实例在使用时 | 启动时 | M1a |
| 日志 | 见 [5.11](11-diagnostics.md) | 轮转 | M1a |

磁盘占用（NFR-11，P1）：设置页显示 `host.db`、`raw.db`、blob、快照对象、worktree 的大小，并提供清理入口。

## 5.4.6 迁移

- **数据库版本**：`meta.schema_version`。迁移脚本以 `migrations/NNNN_<名称>.sql` 编进宿主二进制，按顺序执行。**每个脚本在自己的事务里同时更新 `schema_version`**，在两个脚本之间崩溃，重启后从下一个脚本继续，不会重跑已经执行过的脚本（测试 #11）。
- **启动时**：
  1. 版本比本程序支持的新：以退出码 4 退出，界面提示"数据来自更新的版本，不支持降级"（NFR-10），并提供"打开数据目录"。
  2. 版本较旧：先用 `VACUUM INTO 'backups/host-v<旧版本>-<时间>.db'` 备份（已有同一版本的备份时不重复备份），再逐个执行迁移脚本。`raw.db` 是可丢弃的诊断数据，结构变化时直接重建，不迁移。
  3. 迁移失败：回滚当前事务，以退出码 6 退出（与"数据目录不可用"的 5 区分），界面给出备份位置和诊断包入口。
- **事件升级器**（ADR-06、NFR-10）：每种事件带 `v`。宿主读取旧版本的事件（日志、检查点、回合快照）时，按类型逐级升级到当前版本后再使用或下发，客户端永远只看到当前版本。升级器放在宿主的 `store::upcast`，每个升级器都有"旧样例 → 新样例"的单元测试。
- **视图检查点**带 `view_v`。版本不一致时丢弃检查点，用最近 20 个回合快照加其后的事件重建视图。
- **非数据库的文件**（blob、runtime、快照对象库）的布局版本写在 `meta.layout_version`，变化时由迁移脚本旁的 Rust 迁移函数处理。
