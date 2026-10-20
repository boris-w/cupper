# T163 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T163-audit-producers`
- 完成日期：2026-10-20

## 做了什么

T163 完成。`host.*`、`git.unsafe_repo_config`、`privileged.executed` 都有了产生者和正式格式；有序退出改成先关连接、排空审计写入、再停写线程，所以 `conn.closed{host_exiting}` 和 `host.exiting` 现在会进表；`admit` 里的 `verify` 移进了 `spawn_blocking`。

`cargo xtask ci pr` 第二次通过（rust 1,890、protocol 185、ts 451）。第一次失败的原因见"验证方式"。

**规模**：净增 403 行（增 472、删 69，只算 `crates/`），贴着约 400 的线，没到 470 的硬停止线。开工估计 300；写完功能时是 397；CI 失败后的用例修正又加了 6 行。多出来的部分：
- 写入 task 改成能排空后，原来的循环体挪进了 `store()`，增删各约 40 行，算是搬动；
- 三类新事件的正式格式比估计长；
- `privileged()` 的判定。

功能齐了之后没有再加东西。

1. **`host.*`**
   - `host.started`（带版本）在 `Host::start`；
   - `host.weak_auth` 只在 `dev-auth` 构建发出，与 `host.updated.weak_auth` 用同一个判断；
   - `host.pending_upgrade` 在监护执行 `Close(PendingUpgrade)` 时发出，自检和安装器报的 `BundleChanged` 都走这里；
   - `host.exiting` 在 `Host::exit` 开头发出，带退出原因代码（`requested`、`terminated`、`writer_died`、`idle`、`orphaned`、`upgrade`）和是否强制。
2. **有序退出的顺序**：发 `host.exiting` → 停各部分 → `close_all`（从最后挪到这里）→ 排空审计写入 → 写 `last_clean_exit_at`、停写线程。
   - 排空的做法：写入 task 用 `biased` 先取记录，排空请求只在没有待写记录时才回复。
   - 退出码、`last_clean_exit_at`、`orderly exit` 日志、`host/shutdown` 都没有变。
3. **`git.unsafe_repo_config`**
   - 发在 `index::projects::locate`，`project/add`、`trust`、`trust_info` 和会话重扫都经过这里；`ScanContext` 加了审计发送端。
   - 新加的 `GitError::refusal()` 给出原因和工作区内的文件或键。原因取值为 `writable`、`unresolved`、`origin`、`unnullable`、`malformed`、`required_filter`。
   - 命令类别目前只有 `rev_parse`。
4. **`privileged.executed`**
   - 网关在应答成功之后发出，带方法、principal、连接。
   - 记录范围：scope 为 `privileged` 的方法（只读的 `policy/describe`、`auth/elevation_get` 除外），以及 `settings/update` 带安全相关键的调用。
   - action 和 details 取自 `Confirmable::parse` 加 `describe(Facts::default())`，都是代码和取值，没有文案。需要会话事实的（如 `host/exit force` 看运行中的回合数），action 记为空。
5. **`admit` 的 `verify`**：移进 `spawn_blocking`，原处 `await` 结论。结论出来之前连接不往下走，所以未通过校验的连接不会提前被受理；检查本身出错时按"不是 UI"处理。

## 关键取舍

- **`project/add` 新路径扫描失败，不按路径记 `project.trust.scan_failed`。** 这时还没有项目，也就没有信任状态在变化；失败已经作为 `project/add` 的错误回给了调用者。5.8.6 的 `project.trust.*` 记的是项目信任的变化，按路径另记一笔还要为脱敏后的路径单独定格式，不值得。
- **会话层要 privileged 的操作没有记 `privileged.executed`**：切到 `full_access`、选 privileged 选项，由会话 actor 判定，网关看不到。见遗留问题第 1 条。
- **不写秘密**：新记录里只有版本、退出原因、命令类别、配置文件路径或键名（不含值）、方法和描述用的代码与取值，没有令牌、密钥或登录 shell 的环境。

## 验证方式

- **UNIT-audit-producers**（5 条）：
  - 三类新事件的正式格式；
  - 排空在之前发的记录都落表之后才返回；
  - 仓库档拒绝工作区内的 include 时，`git_of` 记一条 `git.unsafe_repo_config`；
  - `project/trust` 成功后记一条 `privileged.executed`（action 为 `trust_project`，带 details）；失败的调用和 `policy/describe` 不记。
- **进程级**：
  - IT-audit-retention 末尾：SIGTERM 有序退出之后，表里依次有 `host.exiting`（原因 `terminated`）和 `conn.closed`（原因 `host_exiting`）。开头还有 `host.started`，`dev-auth` 构建下另有 `host.weak_auth`。
  - IT-self-check 里查到 `host.pending_upgrade`。
  - 退出相关的 4 条 IT（audit-retention、self-check、host-exit、idle-exit）连跑 5 次全部通过；全部 `cupper-host` 测试通过；跑完后没有残留进程。
- **lint**：`cargo clippy -p cupper-host --all-targets` 在 `test-harness,dev-auth` 和默认特性下都无警告。
- **CI**：
  - 第一次失败在 IT-audit-retention：CI 用全部特性构建，`dev-auth` 打开后宿主启动时还会记 `host.weak_auth`，用例没有预期这一条。
  - 改成按 `dev-auth` 预期它之后重跑通过，这条产生点也因此在 CI 里得到了验证。
  - 摘要 `pr-7bbf9ccb0b20.json` 已提交；估计、统计和过程记录在 `notes.md`。

## 提交列表

```
3449aa6 docs(task): record T163 CI summary
7bbf9cc test(host): expect host.weak_auth from a dev-auth host in IT-audit-retention
70cdc9b docs(task): record T163 line counts and choices
654d1ea fix(host): verify the UI's signature off the async threads
6418e70 feat(host): record privileged.executed once a privileged call succeeds
63e7b69 feat(host): record git.unsafe_repo_config where the repo tier refuses
46d30ca feat(host): record host.* and store the exit's audit records
23c6988 docs(task): estimate T163 by module
```

## 遗留问题 / 范围外发现

1. **会话层的 privileged 操作还没有 `privileged.executed`。**
   - 涉及：用 `session/create`、`session/set_config` 切到 `full_access`；用 `interaction/respond` 选会写入权限规则的 privileged 选项。
   - 这些由会话 actor 用 `tier_change`、`policy::respond` 判定，网关拿不到会话事实。
   - 要记的话，应在会话 actor 判定通过处发出。建议交给下一个动会话策略的任务。
2. **`Gateway::serve` 也在异步线程上同步调用 Security 框架。** 它在接受连接的循环里同步调用 `SelfCheck::check`（T059），里面走 `file_cdhash`。这是与 T138 那次阻塞同类的问题：可能卡住接受循环和定时器。这次没有改，建议同样移进 `spawn_blocking`，并保持"自检在受理新连接之前"的顺序。
3. **`git.unsafe_repo_config` 目前只覆盖 `rev-parse`。** 仓库档的其他调用（worktree 等）还没有走带审计的入口，以后接进来时照 `locate` 的做法发出。
