# T038 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- **协议缺一个字段**：`AuthElevationGetResult.verified`（`VerifiedRequester`）没有"是否本 App 自带的命令行工具"的字段，要加（例如 `bundled_cli: bool`），连带重新生成 TS 和 schema。
- **索引流的唯一写者是 `IndexActor`**：`elevation.requested`、`elevation.resolved`、`clients.updated` 都要由它写；`IndexPort::closed` 也挂在它上面。所以提权的状态放在 `IndexActor` 里最顺。它现在没有定时器（没有 `Alarm`、`TimerTable`），2 分钟过期、15 分钟授权、10 分钟静默要加一套。
- **连接的 scope 不能改**：`Dispatch` 持有 `initialize` 时定下的 `Principal`，下游的 `Call.principal` 也是它。授权要做成连接与提权状态之间共享的一个格子（例如 `Arc` 里的已授 scope），`Dispatch::admit` 和 `Call` 按"原 scope + 已授 scope"判定；`Clients::info` 的 `elevated` 也从这个格子取。
- **对端信息**：连接接受时已经拿到固定令牌（`AuditToken`，`conn.rs`），可以把它（pid、pid version）随 `Opened` 交给索引。
- **cupper-sys 缺的**：取控制终端（`kinfo_proc` 的 `e_tdev`）、签名身份（`SecCodeCopySigningInformation` 的 identifier 和 Team ID）；IT 里探针进程要自己 `setsid`，也要一个包装。都要写 `unsafe`，按约束放在 cupper-sys。
- **血缘的数据源**：打算用 `adapters` 表（`ended_at IS NULL` 的行，pid、pgid、`start_us`；适配器以 SETSID 拉起，会话号就是它的 pid）：与 `AgentRuntime` 解耦，重启后的新实例自然是新的一行，用 `start_us` 防 pid 复用。
- **IT-elevate 的探针**：假 agent 已有 `spawn_child`（同进程组）和 `spawn_group_child [program]`（新进程组，但固定传参 `3600`）。要让子进程以 CLI 身份连上宿主、发 `auth/elevate` 并把结果写到文件，需要一个只在测试构建里存在的小程序（用 `cupper-cli` 的客户端库），以及让假 agent 把参数（数据目录、结果文件、是否 `setsid`）传给它。
- **测试时钟**：进程级的宿主没有 `test/clock_advance`（T047 的），所以 IT 只能用 `test_config.durations` 把三个时长缩到几百毫秒、等回复自然到达。

## 开工前的规模估计

实现按以往经验乘 1.3，测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| cupper-sys | 控制终端、签名身份、`setsid` 包装 | 120 | 40 |
| 协议 | `VerifiedRequester` 加字段（生成物另计） | 5 | — |
| 提权纯核心 | 全局一个未决、10 分钟两次被拒后静默 10 分钟、2 分钟过期、按连接的授权与 15 分钟过期、断开即撤销；产出回复、事件、审计、定时器 | 200 | 120 |
| 血缘与核实信息 | 读 `adapters` 表、三类信号；可执行文件、签名、父进程链（含完整路径）、控制终端、"无法确认来源"、是否本 App 自带的 CLI（路径与 cdhash）；自述字段截断 | 110 | — |
| `IndexActor` 接线 | `auth/*` 三个方法（`auth/elevate` 的回复挂起）、定时器、写三类事件、审计、`closed` 撤销 | 150 | — |
| 网关 | 共享的已授 scope、`Dispatch` 与 `Call` 按它判定、`Opened` 带对端令牌、`clients.updated` 的 `elevated` | 80 | — |
| 时长、审计事件 | 三个时长登记；`AuditEvent` 的提权各类 | 30 | — |
| IT 探针与假 agent | 测试构建的探针程序；假 agent 传参与 `setsid` | 80 | — |
| IT-elevate | 真实宿主加假 agent：两类后代被拒；非后代进入未决、`elevation_get`、授权后能写而另一条连接不能；全局一个未决、静默、过期；断开与 15 分钟失效 | — | 250 |
| **合计** | | **约 775，乘 1.3 约 1,010** | **约 410** |

合计约 **1,420 行**，远超约 900 的停止线。按 brief 和守则第 8 条，开工前停下汇报，没有写代码。

## 拆分建议

| 拆分 | 内容 | 用例 | 估计 |
|---|---|---|---|
| **T038a 提权的状态与授权** | 提权纯核心；`IndexActor` 接线（三个方法、定时器、事件、审计、断开撤销）；网关的共享已授 scope 与 `clients.updated` 的 `elevated`；时长与审计事件。血缘和核实信息先经一个窄接口取（a 里给一个只读进程表的实现，不含签名、终端和自带 CLI 判定） | 新增单元编号（例如 UNIT-elevation-core）：限频、过期、授权与撤销；进程内的接线用例（授权后能写、另一条连接不能、断开即失效） | 约 800 |
| **T038b 血缘与核实信息、IT-elevate** | cupper-sys 的控制终端、签名身份、`setsid`；血缘三信号（`adapters` 表）；核实信息的完整内容与是否自带 CLI（协议加字段）；IT 探针与假 agent 的扩展；IT-elevate | IT-elevate | 约 700 |

b 依赖 a（IT-elevate 要走完整的提权流程）。a 在前、b 在后；如果想并行，b 可以先做 cupper-sys 的三个系统调用和血缘判定（纯函数加进程表），最后再接 a 的接口、写 IT。

另一种：不拆，上限提到约 1,500。

## 裁定后的范围（计划 v1.3.24）

T038 = 提权的状态与授权；血缘、核实信息、`bundled_cli`、cupper-sys 补充、探针与 IT-elevate 归 T133。上限约 900 行，硬停止线 1,000。

## 与 T133 的窄接口（先定下，T133 照此实现）

```rust
// crates/cupper-host/src/elevation.rs

/// What the host finds out about the process behind a connection that asks
/// for elevation (design 5.8.4). T133 implements it; until then the host
/// runs `FailClosed`.
pub trait Requesters: Send + Sync + fmt::Debug {
    /// Examines the peer that `token` names (the connection's pinned token).
    /// May read the process table, code signatures and the `adapters` table.
    fn examine(&self, token: AuditToken) -> BoxFuture<'static, Examined>;
}

pub enum Examined {
    /// None of the three signals matched: the dialog shows `verified`.
    Clear(VerifiedRequester),
    /// An adapter's descendant, or it cannot be ruled out (the process is
    /// gone, a lookup failed). `signals` go into the audit record, e.g.
    /// `same_session`, `same_group`, `ancestor`, `unchecked`.
    Descendant { signals: Vec<String> },
}

/// Until T133: every requester counts as a possible descendant, so
/// `auth/elevate` is `forbidden` (`lineage`) and nothing reaches the dialog.
pub struct FailClosed;
```

- `VerifiedRequester` 是协议里的类型（`methods::admin`），T133 给它加 `bundled_cli` 一类的字段时，接口不用变。
- 注入点：`IndexDeps.requesters: Arc<dyn Requesters>`；`supervisor` 里传 `Arc::new(FailClosed)`，T133 换成真实实现。
- 调用时机：`IndexActor` 收到 `auth/elevate` 后，在一个任务里调用 `examine`，结果回到 actor 再交给纯核心；核心先看血缘，再看限频。

## 共享的已授 scope

- 网关的 `Opened` 多带两项：连接的固定令牌（`token: AuditToken`）和一个共享格子（`granted: Arc<Granted>`）。`Dispatch` 与 `IndexActor` 拿的是同一个格子。
- **每次调用在受理时读这个格子**：`Dispatch::admit` 和交给执行者的 `Call.principal` 都是"原 scope 加上格子里的 scope"。撤销之后受理的调用按撤销后的 scope 判定；已经受理、正在执行的调用照常完成。
- `clients.updated` 的 `elevated` 也从这个格子取。

## 进度与停下的原因（第一个模块之后）

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） |
|---|---|---|---|
| 提权纯核心、`Requesters` 接口与 `FailClosed`、四个时长（`feat(host): the elevation core ...`） | 490 / 0 | 152 | 260 + 120 |

核心比估计多约 110 行，主要是 rustfmt 展开的结构体字面量和动作枚举。按剩下的部分逐项细估：

| 剩下的模块 | 内容 | 估计 |
|---|---|---|
| 网关 | 共享的已授 scope 格子；`Opened` 带令牌和格子；`Dispatch` 受理时合并 scope（`admit`、`begin`、`call`）；`Clients::info` 的 `elevated`；测试用 `Peer` 能指定连接号和格子 | 90 |
| 审计事件 | `AuditEvent::Elevation`（申请、批准、拒绝、过期、因血缘被拒、被限频、撤销） | 20 |
| `IndexActor` 接线 | `IndexDeps` 加 `requesters`、审计发送端；邮箱加 `Opened`、`Closed`、`Examined`；定时器（`Alarm`、`TimerTable`）；三个方法；把核心的动作落成回复、事件、审计、格子与 `clients.updated`；`Routes::index` 登记；`supervisor` 接线 | 200 |
| IT-elevation-grant | 进程内：索引、路由、两个 CLI `Peer` 与一个 control `Peer`，真实的固定令牌（socketpair 取自己的令牌），测试用 `Requesters`；授予后能写、另一条连接不能；断开与 15 分钟撤销；`ManualClock` | 200 |

**推算约 1,000 行**，超过约 900 的上限、碰到 1,000 的硬停止线，按裁定停下汇报。

## 第 2 次裁定后（上限约 1,150，硬停止线 1,250）

### 各模块的实际行数

| 提交 | 内容 | 新增 / 删除 | 其中测试 |
|---|---|---|---|
| `feat(host): the elevation core ...` | 纯核心、`Requesters` 接口与 `FailClosed`、四个时长、UNIT-elevation-core | 490 / 0 | 152 |
| `feat(host): connections admit calls with the scopes granted to them` | 网关的已授 scope 格子（`Granted`）；`Opened` 带固定令牌和格子；`Dispatch` 受理时合并；`clients.updated` 的 `elevated`；审计事件 `Elevation` | 92 / 18 | 1 |
| `feat(host): the index serves CLI elevation` | `IndexActor` 接线（`index/elevations.rs`）、`Routes::index` 登记三个方法、`supervisor` 传 `FailClosed` 与审计发送端、启动时把上次遗留的未决申请记为过期；核心的 `Scopes` 动作改成 `Revoked`；测试用 `Peer` 能指定连接号并像读循环那样开、关连接；IT-elevation-grant | 601 / 58 | 206 |

整个分支相对 main（不含 docs）：**新增 1,150、删除 43**，其中测试约 340（`elevation/tests.rs`、`index/tests.rs` 新增部分、`Peer` 的扩展）。没有超过约 1,150 的上限。IndexActor 接线比估计（200）多，主要是 rustfmt 的结构体字面量换行；为此压缩过一轮（`serves` 换成 `auth/` 前缀、格子写入合并成一个 `grant`、审计里把批准、拒绝、过期合成 `Resolved(id, decision)`）。

### 做法与取舍

- **授权绑定连接与固定令牌**：授权按 `ConnId` 记在核心里，格子随连接走。连接的令牌在接受时固定，之后每次读都核对（5.8.1，`conn.rs`），换了进程连接就断，所以按连接绑定也就绑定了令牌；令牌本身随 `Opened` 交给索引，只用来交给 `Requesters::examine`。
- **scope 合并的位置**：`Dispatch` 每次受理时把格子里的 scope 并进 principal（`Granted::widen`），`policy::method`、`settings_update` 等规则和交给执行者的 `Call.principal` 都用合并后的。权限矩阵本来就按 `principal.scopes` 判（矩阵里有 `cli + write`、`cli + approve` 两列），不用改。
- **15 分钟到期**：由索引的定时器撤销（`Revoked` 清空格子），不在 `Dispatch` 里比时间，免得网关也要读时钟。定时器响到格子清空之间只有一次邮箱往返。
- **核心的动作调整**：原来批准时发 `Scopes { scopes }`、撤销时发空的 `Scopes`；接线时发现格子要的到期时间在 `Resolved::Granted { ends_in }` 里，于是批准只走 `Resolved`，撤销单独成 `Revoked(conn)`。这只是核心的输出，不是与 T133 的窄接口，`Requesters`、`Examined`、`FailClosed` 没有变。
- **`auth/elevate` 的 scope**：只接受 `write`、`approve`（或两者），其他（如 `privileged`）一律 `invalid_params`；批准时核心再取"申请的"与"勾选的"交集。
- **`auth/elevation_respond` 的 `decision`**：开放枚举里不认识的值按拒绝处理（保守）。命令的结果和 `elevation.resolved` 同一个事务提交（5.2.3 规则 3）；申请已不在未决时回 `expired`。
- **回复的形状**：批准回 `granted` 与 `expires_at`；拒绝回 `granted: []`（不是错误）；2 分钟无决定回 `timeout`；被限频回 `rate_limited` 带 `retry_after_ms`（未决期间取未决申请剩余的时间，静默期取静默剩余的时间）；`FailClosed` 下一律 `forbidden`（`lineage`）。
- **申请方中途断开**：检查回到 actor 时连接已关就丢弃，不生成未决；未决期间断开，核心把它记为过期（`elevation.resolved` 为 `expired`）。
- **重启遗留**：上次运行留在索引视图里的未决申请，启动对账时写 `elevation.resolved`（`expired`），否则横幅会一直挂着。
- **审计**：`AuditEvent::Elevation { step, scopes }`，`step` 为 `Requested`、`Resolved(id, granted|denied|expired)`、`Lineage(signals)`、`RateLimited`；申请与被拒记申请的范围，批准记实际授予的范围。授权到期或随断开结束不单独记（5.8.6 的表里没有，`conn.closed` 已有断开）。

### IT-elevation-grant（进程内）

真实的 `host.db`、索引流与账本，`ManualClock`；两条 CLI 连接和一条 control 连接都是网关测试用的 `Peer`（直接驱动 `Dispatch`），`Peer::open`、`Peer::close` 像读循环那样把连接登记到网关的客户端表并通知索引；固定令牌取本进程自己的（socketpair）；`Requesters` 用测试实现，一律判"不是后代"。覆盖：未授权时 `project/add` 被拒（`forbidden`/`scope`）；申请后另一条连接的申请被限频（`retry_after_ms` 为 120,000）；`elevation_get` 给出核实信息和截到 200 字的自述；批准时勾选了未申请的 `approve` 也只授予 `write`，CLI 收到 `granted` 和 `expires_at`；这条连接的 `project/add` 成功、另一条仍被拒；`clients.updated` 里这条连接的 `elevated`；15 分钟后撤销；再次授予后断开即撤销；审计记录的顺序与内容。拒绝、2 分钟过期和静默的判定在 UNIT-elevation-core 里。
