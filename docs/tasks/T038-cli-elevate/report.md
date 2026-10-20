# T038 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T038-cli-elevate`（在 T054 的 worktree 里）
- 完成日期：2026-10-20

T038 已做完：提权的状态与授权全部接上，UNIT-elevation-core 和 IT-elevation-grant 都通过，`cargo xtask ci pr` 在干净的工作区里通过，摘要已提交。分支相对 main 不含 docs 新增 1,150 行、删除 43 行，刚好在约 1,150 的上限内。

需要你处理一件事：本分支的 `tests/ids.toml` 还是旧版，所以 CI 摘要的结果里没有 UNIT-elevation-core 和 IT-elevation-grant 这两条，但两条用例都实际跑过并通过。你把 main 合进来以后，告诉我一声，我在合并后的分支上重跑一次 CI 并重新记录。

## 做了什么

1. **纯核心**（上一轮已提交，这轮小改）：`ElevationCore` 管限频、过期和授权。全局同一时间只有 1 个未决申请；10 分钟内被拒 2 次后静默 10 分钟；2 分钟没有决定就过期；授权按连接记，最长 15 分钟，断开立即撤销。接线时把输出的动作 `Scopes` 改成 `Revoked(conn)`，批准只走 `Resolved::Granted { scopes, ends_in }`。与 T133 的窄接口（`Requesters`、`Examined`、`FailClosed`）没有改动。
2. **网关**：
   - 新增共享格子 `Granted`，记一条连接被授予的 scope 和到期时间；`Opened` 多带两项：连接的固定令牌和这个格子。
   - `Dispatch` 每次受理调用时，把格子里的 scope 并进 principal，权限判定和交给执行者的 `Call.principal` 都用合并后的结果；撤销后受理的调用按原 scope 判，已在执行的照常完成。
   - `clients.updated` 的 `elevated` 取自这个格子。
   - 审计事件新增 `AuditEvent::Elevation { step, scopes }`，`step` 为 `Requested`、`Resolved(id, granted|denied|expired)`、`Lineage(signals)`、`RateLimited` 之一。
3. **`IndexActor` 接线**（新文件 `index/elevations.rs`）：
   - `auth/elevate`：先校验申请范围，只接受 write、approve 或两者；在单独的任务里调 `Requesters::examine`，结果回到 actor 再交核心；回复挂起，等用户决定或过期。
   - `auth/elevation_get`：返回核心里未决申请的详情（核实信息、截到 200 字的自述、申请范围）。
   - `auth/elevation_respond`：命令的结果与 `elevation.resolved` 在同一个事务里提交；申请已不在未决时回 `expired`。
   - 定时器经 `Alarm` 接进 actor 的主循环；`elevation.requested` 和 `elevation.resolved` 写进索引流；授权或撤销时更新格子并刷新 `clients.updated`；每一步都写审计。
   - `Routes::index` 登记三个 `auth/*` 方法；`supervisor` 注入 `FailClosed` 和审计发送端。
   - 启动对账时，把上次运行遗留的未决申请记为 `expired`，否则横幅会一直挂着。
4. **测试**：
   - 网关测试用的 `Peer` 加了 `Peer::on(n, …)`（指定连接号）和 `open`、`close`（像读循环那样登记连接、通知索引）。
   - 新增 IT-elevation-grant。
   - 原有的启动对账用例补了"遗留申请被记为过期"。
   - UNIT-unsupported-set 的允许列表去掉了三个 `auth/*` 方法。

## 关键取舍

- **授权与固定令牌的绑定**：授权按连接记。连接的令牌在接受时固定，之后每次读都核对，换了进程连接就会断，所以按连接绑定就等于绑定了令牌。令牌只用来交给 `examine`。
- **15 分钟到期**：由索引的定时器撤销格子，不在 `Dispatch` 里比时间，这样网关不用读时钟。
- **权限矩阵不用改**：它本来就按 `principal.scopes` 判定，矩阵里已有 `cli + write`、`cli + approve` 两列。
- **`auth/elevate` 的各种回复**：
  - 被拒：回 `granted: []`，不是错误。
  - 过期：回 `timeout`。
  - 被限频：回 `rate_limited`，带 `retry_after_ms`；有未决申请时取它剩余的时间，静默期取静默剩余的时间。
  - 在 `FailClosed` 下：一律回 `forbidden`（`lineage`）。
- **`decision` 不认识的值**按拒绝处理，偏保守。
- **申请方在检查期间断开**：丢弃这次申请，不生成未决。
- **审计范围**：授权到期或随断开结束不单独记，因为 5.8.6 的表里没有这一项，断开已有 `conn.closed`。
- **规模**：IndexActor 接线比估计多，主要是 rustfmt 把结构体字面量拆成多行。为此压缩过一轮：路由改成按 `auth/` 前缀判断，格子写入合成一个 `grant` 方法，审计合并成 `Resolved`。

更详细的取舍记在 `docs/tasks/T038-cli-elevate/notes.md`。

## 验证方式

- **UNIT-elevation-core**（3 个用例，`ManualClock`）：
  - 同一时间一个未决，第二个申请被限频；未决过期；断开时未决随之过期；血缘检查先于限频。
  - 被拒 2 次后静默；两次拒绝相隔超过 10 分钟时不累计。
  - 只授予申请过的 scope；15 分钟或断开时撤销。
- **IT-elevation-grant**（进程内）：真实的 `host.db`、索引流和账本，`ManualClock`；两条 CLI 连接和一条 control 连接都用 `Peer`；`Requesters` 用测试实现，一律判"不是后代"。覆盖：
  - 授权前 CLI 发 `project/add` 被拒（`forbidden`/`scope`）。
  - 已有未决时，另一条连接的申请被限频，`retry_after_ms` 为 120,000。
  - `elevation_get` 给出核实信息，自述被截到 200 字。
  - 批准时多勾的 `approve` 不授予，CLI 收到 `granted: ["write"]` 和 `expires_at`。
  - 这条连接的 `project/add` 成功，另一条仍被拒；`clients.updated` 里这条连接的 `elevated` 正确。
  - 15 分钟后撤销；再次授予后，断开即撤销。
  - 审计记录的顺序和内容。
- `cargo clippy -p cupper-host --all-targets` 在不开 feature 和 `--all-features` 下都没有告警；`cargo test -p cupper-host` 两种 feature 组合都通过。
- 在干净的工作区里 `cargo xtask ci pr --record docs/tasks/T038-cli-elevate/ci/` 通过：rust 1737、protocol 185、ts 353、license、testdata-scan 全部通过。只跳过了 sensitive-words，因为本机没有词表。摘要是 `pr-b8a2d6ac4f53.json`。
- **验收里没体现的一点**：如开头所说，摘要结果不含这两个新用例编号。这是因为编号只登记在 main 上已更新的 `tests/ids.toml` 里，本分支还是旧版；不是用例没跑。

## 提交列表

```
a1d8ee9 docs(task): T038 CI summary
b8a2d6a docs(task): T038 counts and choices after the second ruling
32618a6 feat(host): the index serves CLI elevation
fd8bead feat(host): connections admit calls with the scopes granted to them
950b4b6 docs(task): T038 counts so far and why work stopped
657db49 feat(host): the elevation core and its interface to the requester check
2a30122 docs(task): T038 interface with T133 and the shared granted scopes
d112618 docs(task): T038 size estimate and split proposal
```

规模（整个分支相对 main，不含 docs）：新增 1,150、删除 43，其中测试约 340。

## 遗留问题 / 范围外发现

- **同步 main**：我没有合并 main。需要你把 main 合进本分支，`tests/ids.toml` 的两个新编号才会被识别。我改了 `supervisor/mod.rs`：把审计 channel 的创建挪到第 7 步之前，并加了 `requesters`、`audit` 两个字段。正在并行的 T037 也改 supervisor，合并时可能要手工处理冲突。
- **交接给 T133**：
  - 只需实现 `Requesters`，然后在 `supervisor` 里把 `Arc::new(FailClosed)` 换掉。
  - `examine` 收到的是连接的固定令牌。
  - 返回 `Descendant { signals }` 时，`signals` 会原样进审计。
  - 给 `VerifiedRequester` 加 `bundled_cli` 字段不影响本任务的代码；它出现在 `elevation_get` 的结果里，由宿主原样转出。
- **交接给 T077、T090**：
  - `elevation_respond` 中，`decision` 不认识的值按拒绝处理；`scopes` 只取申请范围之内的。
  - `elevation_get` 对不在未决状态的申请回 `not_found`。
  - 授权的到期时间在 `clients.updated` 的 `elevated.expires_at` 里。
- **范围外发现**：
  - `gateway/audit.rs` 开头的注释写着"对端的可执行文件路径、签名标识和 Team ID 是 T038 的事"，拆分后应改为 T133 或另行指定。我没动它。
  - 极少见的边界情况：如果某次 `elevation.requested` 写入失败，之后对应的 `elevation.resolved` 在折叠视图时会找不到该申请而失败，CLI 的回复照常发出。我没有专门处理。