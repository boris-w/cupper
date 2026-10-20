# T163 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**（修复 1 轮）

## 核对

- **规模**：净增 403 行（`crates/` 内增 472、删 69），贴着约 400 的线，没到 470 的硬停止线。超出部分主要是把审计写入 task 的循环体挪进 `store()`（增删各约 40 行的搬动）和三类正式格式；功能齐了之后没有再加东西。
- **CI**：执行者的摘要 `pr-7bbf9ccb0b20.json` 通过；第一次失败是 CI 用全部特性构建、`dev-auth` 下多一条 `host.weak_auth`，用例改成按特性预期，产生点也因此在 CI 里验证了。修复第 1 轮之后合并 main，`ci pr --record` 通过：`pr-b9d3be0a17c0.json`（rust 1892、protocol 185、ts 459、license、testdata-scan、e2e 5；ui 按路径过滤跳过）。
- **修复第 1 轮**（编排者提出，执行者汇报的遗留第 2 条）：`Gateway::serve` 在接受循环里同步调用 `SelfCheck::check`（内含 `file_cdhash`），与 T138 那次阻塞同类。执行者查遍宿主，共挪了三处到 `spawn_blocking`：接受循环的自检（原地 `await`，"自检在受理新连接之前"的顺序不变）、适配器启动前的自检、适配器启动前对 node 与 bundle 的 sha256 校验（node 约 100 MB，T129 加的）。失败与 join 失败都按原有语义拒绝（`bundle_changed`、`RuntimeFailed`），不放行。增 21、删 10；退出与自检相关的 5 条 IT 连跑 3 次通过。

## 逐项意见

1. **有序退出的顺序**：发 `host.exiting` → 停各部分 → `close_all` → 排空审计 → 写 `last_clean_exit_at`、停写线程。排空的正确性靠两点：审计通道是无界 FIFO，排空之前发的记录都已在通道里；写入 task 用 `biased` 先取记录，排空请求只在通道暂时为空时回复。写线程已经死掉（`WriterDied`）时 `submit` 报错、记一行警告后继续，排空不会挂住。退出码、`last_clean_exit_at`、`orderly exit` 日志都没变。
2. **`privileged.executed`**：在网关应答成功之后记，失败的调用和只读的 `policy/describe`、`auth/elevation_get` 不记；action 与 details 取自 `Confirmable::parse` 加 `policy::describe`，只有代码和取值，没有文案。安全相关的 `settings/update` 也记。
3. **`git.unsafe_repo_config`**：发在 `index::projects::locate`，`project/add`、`trust`、`trust_info` 和会话重扫都经过这里；`GitError::refusal()` 给出原因与工作区内的文件或键，不含值。
4. **`admit` 的 `verify`**：移进 `spawn_blocking`，结论出来之前连接不往下走；检查出错时按"不是 UI"处理，是失败即关闭的一侧。
5. **`project/add` 新路径扫描失败不另记一笔**：理由成立。那时没有项目、也没有信任状态在变化，失败已作为 `project/add` 的错误回给调用者。
6. **不写秘密**：新记录里只有版本、退出原因、命令类别、文件路径或键名、方法与描述用的代码，没有令牌、密钥或登录 shell 的环境。

## 记下的点

- **会话层的 `privileged.executed`**（遗留第 1 条）：经 `session/create`、`session/set_config` 切到 `full_access`，以及经 `interaction/respond` 选写入权限规则的特权选项，要在会话 actor 判定通过处发出。并进 T169（计划 v1.3.39，新增 UNIT-audit-session-privileged）。
- **`git.unsafe_repo_config` 只覆盖 `rev-parse`**（遗留第 3 条）：仓库档的其他调用接进来时照 `locate` 的做法发出，记交接。
