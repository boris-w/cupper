# T163 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；
- 设计 5.8.6、5.2.13；
- 代码：
  - `supervisor/mod.rs`（`Host::start`、`run`、`exit`）、`supervisor/watch.rs`（`Action::Close(PendingUpgrade)`）、`supervisor/audit.rs`（T138 的写入 task）；
  - `gateway/dispatch.rs`（`answer`、`admit`）、`gateway/conn.rs`（`admit` 里的 `verify`）、`gateway/mod.rs`（`close_all`）；
  - `policy/describe.rs`（`Confirmable`、`describe`、`Facts`）、`policy/method.rs`（`settings_update`、`security_settings`）；
  - `index/projects.rs`（`Context`、`locate`）、`git/repo.rs`（`GitError::Unsafe`、`RequiredFilter`）、`git/rules.rs`（`Unsafe` 各变体）。

行数指 rustfmt 展开后的行，含文档注释和空行。

### 实现

| 模块 | 内容 | 估计 |
|---|---|---|
| `host.*` | `AuditEvent::Host` 与映射：`host.started`（版本）、`host.weak_auth`（`dev-auth` 构建）、`host.exiting`（`Exit` 的取值、是否强制）、`host.pending_upgrade`；产生点在 `Host::start`、`Host::exit`、监护的 `Close(PendingUpgrade)`（`WatchDeps` 加审计发送端） | 60 |
| 有序退出的顺序 | `Host::exit` 先关连接（`close_all` 前移），再排空审计写入（写入 task 加一个排空请求通道：清空已到的记录再回复），最后写 `last_clean_exit_at`、停写线程；退出码、`orderly exit` 日志不变 | 35 |
| `git.unsafe_repo_config` | `AuditEvent::GitUnsafe` 与映射（命令类别、原因、工作区内的路径或键）；`ScanContext` 加审计发送端，索引和会话重扫两处构造跟着改；`locate` 遇到 `Unsafe`、`RequiredFilter` 时发出 | 45 |
| `privileged.executed` | `AuditEvent::Privileged` 与映射（方法、action、details、principal 与连接）；网关在受理 privileged 的调用成功之后发出：方法的 scope 是 `privileged`（只读的 `policy/describe`、`auth/elevation_get` 除外），或 `settings/update` 带安全相关的键；action 与 details 用 `Confirmable::parse` 加 `describe`（不需要会话事实的那些） | 45 |
| `admit` 的 `verify` | 移进 `spawn_blocking` 并在原处等它的结果：结论出来之前连接不往下走，不会提前受理 | 10 |
| **实现小计** | | **195** |

### 测试（UNIT-audit-producers）

| 项 | 内容 | 估计 |
|---|---|---|
| 正式格式 | 新事件进 UNIT-audit-format 的表 | 25 |
| `host.*` | 进程级：有序退出后表里有 `host.started`、`conn.closed{host_exiting}`、`host.exiting`（并进 IT-audit-retention）；IT-self-check 里查 `host.pending_upgrade` | 20 |
| `git.unsafe_repo_config` | 配置来源在工作区内的仓库，`git_of` 被拒，审计里有一条 | 30 |
| `privileged.executed` | 网关用例：`project/trust` 一类的 privileged 调用成功后有一条，带 action 与 details；只读的 `policy/describe` 没有 | 30 |
| **测试小计** | | **105** |

### 合计

约 300 行，在约 400 的上限之内，开工。

### 不做的一项

**`project/add` 新路径扫描失败，不按路径记 `project.trust.scan_failed`。**
- 这时还没有项目，也就没有信任状态在变化；
- 失败已经作为 `project/add` 的错误回给了调用者；
- 5.8.6 的 `project.trust.*` 记的是项目信任的变化，按路径另记一笔，还要为脱敏后的路径单独定格式，不值得。

## 统计（`git diff --numstat`，只算 `crates/`）

| 提交 | 增 | 删 |
|---|---|---|
| `host.*` 与有序退出的审计（含三类新事件的正式格式） | 303 | 56 |
| `git.unsafe_repo_config` | 60 | 0 |
| `privileged.executed` | 90 | 6 |
| `admit` 的 `verify` 移进 `spawn_blocking` | 13 | 7 |
| **合计** | **466** | **69** |

净增 397 行，贴着约 400 的线。估计是 300，多出来的主要在三处：
- 写入 task 改成能排空之后，原来的循环体挪进了 `store()`，增删各 40 行左右，算是搬动；
- 三类新事件的正式格式比估计长；
- `privileged()` 的判定。

写到这里功能已经齐了，没有继续加东西。

## 过程记录

- **有序退出**：`Host::exit` 的顺序改为：
  1. 发 `host.exiting`（`Exit` 的代码与是否强制）；
  2. 停各部分；
  3. `close_all`（从最后挪到这里）；
  4. 排空审计写入：写入 task 带 `biased` 先取记录，排空请求只在没有待写记录时才回复；
  5. 写 `last_clean_exit_at`、停写线程。
  - 退出码、`orderly exit` 日志、`host/shutdown` 都没变；IT-host-exit、IT-idle-exit、IT-self-check 照常通过。
- **`host.*`**：
  - `host.started` 带版本；`host.weak_auth` 只在 `dev-auth` 构建发出（与 `host.updated.weak_auth` 同一个判断）；
  - `host.pending_upgrade` 在监护执行 `Close(PendingUpgrade)` 时发出，自检和安装器的 `BundleChanged` 都走这里；
  - `host.exiting` 在 `Host::exit` 开头发出。
- **`git.unsafe_repo_config`**：发在 `index::projects::locate`。`project/add`、`trust`、`trust_info` 和会话重扫都经过这里，`ScanContext` 加了审计发送端。
  - 原因与工作区内的文件或键由新加的 `GitError::refusal()` 给出：`writable`、`unresolved`、`origin`、`unnullable`、`malformed`、`required_filter`。
  - 命令类别目前只有 `rev_parse`：仓库档别的调用（worktree 等）还没有经过审计的入口，以后接进来时照这里做。
- **`privileged.executed`**：网关在应答成功之后发出。
  - 范围：scope 为 `privileged` 的方法，只读的 `policy/describe`、`auth/elevation_get` 除外；以及 `settings/update` 带安全相关键的调用。
  - action 和 details 用 `Confirmable::parse` 加 `describe(Facts::default())`，只取不需要会话事实的那些；`host/exit force` 一类要看运行中回合数的，action 记为空。
  - 会话层要 privileged 的操作（切到 `full_access`、选 privileged 选项）由会话 actor 判定，网关看不到，这次没有记，交接。
- **`admit` 的 `verify`**：移进 `spawn_blocking`，原处 `await` 结论，结论出来之前连接不往下走；检查本身出错按"不是 UI"处理。
- **另一处同步调用 Security 框架的地方**：`Gateway::serve` 在接受连接的循环里同步调用 `SelfCheck::check`（T059），它走 `file_cdhash`。我没有改，在汇报里提出。
