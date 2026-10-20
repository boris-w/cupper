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
