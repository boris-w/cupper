# T038 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 1,150 行、删除 43 行（不含 docs，测试约 340），正好在第 2 次裁定的上限约 1,150 之内。
  - 开工前估约 1,420 行，拆出 T133（计划 v1.3.24）。
  - 做完纯核心后推算约 1,000 行，放宽上限、改为 L（v1.3.25）。
- **CI**：执行者的摘要 `pr-b8a2d6ac4f53.json` 通过，但那时分支上的 `tests/ids.toml` 还是旧版，两个新编号没有出现在结果清单里。合并 main 之后，在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-6756f823927a.json` 通过（rust 1792、protocol 185、ts 374、license、testdata-scan），UNIT-elevation-core 和 IT-elevation-grant 都在结果清单里。
- **合并冲突**：index 的三个文件，由执行者解决，编排者提交。
  - 冲突来源：T100 在 main 上给索引 actor 加了状态核心和自己的闹钟、定时器表。
  - 合并后整个 actor 只有一个闹钟、一张 `TimerTable<Timer>`、一个等待点。`Timer` 分 `Status(Flush)` 和 `Elevation(..)` 两种，`Mail::Rung` 把到期的 key 分给对应的核心。符合静止屏障"一个等待点"的规则。
  - 执行者在合并中还发现并修掉了自己原来的一个缺陷：T100 之后 `test/seed` 也会调用 `rows::reconcile`，原写法会把还活着的提权申请记成过期，使视图和核心对不上。现在只在启动对账之后追加 `elevations::left_over`。

## 逐项意见

1. **纯核心**：
   - 全局一个未决；10 分钟内被拒 2 次后静默 10 分钟；2 分钟过期；按连接授权最长 15 分钟，断开即撤销；
   - 先查血缘、再查限频；
   - 只授予"申请过且被勾选"的 scope。
   - UNIT-elevation-core 三个用例覆盖。
2. **网关的已授 scope**：
   - 每条连接一个 `Granted` 格子，`Dispatch` 每次受理调用时把格子里的 scope 并进 principal。权限矩阵和交给执行者的 `Call.principal` 都用合并后的结果。
   - 撤销后受理的调用按原 scope 判，已在执行的照常完成（与裁定一致）。
   - 权限矩阵本来就有 `cli + write`、`cli + approve` 两列，不用改。
3. **授权与固定令牌**：连接的令牌在接受时固定、之后每次读都核对，换进程连接就会断，所以按连接绑定等于绑定了令牌。认可。
4. **`FailClosed`**：在 T133 接上之前，`auth/elevate` 一律回 `forbidden`（`lineage`）。main 上没有不经血缘检查就能进入确认的路径。
5. **回复**：
   - 被拒回 `granted: []`；过期回 `timeout`；
   - 限频回 `rate_limited` 和 `retry_after_ms`：有未决时取未决剩余时间，静默期取静默剩余时间；
   - 不认识的 `decision` 按拒绝。
   - 都符合 5.8.4 和协议。
6. **启动时遗留的未决申请记为 `expired`**：否则横幅会一直挂着。好的补充。

## 小问题（不阻塞）

- **到期只靠索引的定时器撤销格子**：网关不读时钟，自身不核对 `expires_at`。索引 actor 卡住或退出时，授权可能超过 15 分钟，直到连接断开。建议以后在 `Granted::widen` 里也按 `Clock` 核对 `expires_at`（网关拿一个 `Clock` 即可），作为第二道防线。交接给 T133：它正好要接 `Requesters`、改 supervisor。
- **`elevation.requested` 写入失败的边界情况**：之后的 `elevation.resolved` 在视图里找不到申请。很少见，记交接。
- **`gateway/audit.rs` 开头的注释**：还写着"对端的可执行文件路径、签名标识、Team ID 是 T038 的事"。`conn.accepted` 的这几项交给 T059（审计落盘与正式格式），T133 的签名身份函数可以直接用。

## 交接

- **T133**：
  - 实现 `Requesters`，在 supervisor 里把 `Arc::new(FailClosed)` 换掉；
  - `examine` 收到的是连接的固定令牌；
  - `Descendant { signals }` 的 `signals` 原样进审计；
  - 加上 `expires_at` 的第二道核对。
- **T134、T090**：
  - `elevation_respond` 中，不认识的 `decision` 按拒绝，`scopes` 只取申请范围之内的；
  - `elevation_get` 对已不在未决的申请回 `not_found`；
  - 到期时间在 `clients.updated` 的 `elevated.expires_at`。
