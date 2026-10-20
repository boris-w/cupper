# T138 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前的规模估计

估计前读过：
- 任务书；
- `handoff-notes.md` 第一列含 T059 的行里与审计有关的部分（T088、T028、T044、T038、T047）；
- 设计 5.8.6、`04-data-model.md` 的 `audit` 表、`11-diagnostics.md` 的 host.log；
- T059 任务书末尾的拆分裁定；
- 代码：
  - `gateway/audit.rs`、`gateway/conn.rs`（`admit`、关闭时的审计）；
  - `supervisor/mod.rs`（`records`、`_audit`）；
  - 各产生点：会话 actor 的 `unconfirmed`、`tier_guard`，索引的 `elevations.rs`、项目信任的 `decide`、`rescanned`；
  - `index/projects.rs`（`git_of`、`locate`）、`git/repo.rs`、`git/rules.rs`（`Unsafe`）；
  - `store/retention.rs`（`Cleanup::Audit` 已有：90 天、每天一次）；
  - `diag/log.rs`、`supervisor/harness.rs`（`test/run_maintenance`）；
  - `cupper-sys` 的 `AuditToken::signing_identity`、`pid_path`；
  - `bin/gateway_fixture.rs`（审计行是 `Debug` 文本）。

行数指 rustfmt 展开后的行，含文档注释和空行。

### 盘点：5.8.6 的事件现在谁在产生

| 事件 | 产生者 | 本任务要做的 |
|---|---|---|
| `conn.accepted`、`conn.rejected`、`conn.initialized`、`conn.closed` | 网关 | 映射；`conn.accepted` 补对端信息 |
| `elevation.*` | 索引（T038） | 映射（6 种：申请、批准、拒绝、过期、因血缘被拒、被限频） |
| `interaction.responded` | 会话 actor（T088） | 映射 |
| `config.unconfirmed` | 会话 actor（T044） | 映射 |
| `config.confirmed` | 无 | 补产生者（任务书第 4 项） |
| `project.trust.*` | 无 | 补产生者（任务书第 4 项） |
| `git.unsafe_repo_config` | **无**：仓库档拒绝时只返回错误，不发审计 | 任务书第 2 项列了它，第 4 项没有，见待裁定 |
| `host.*` | **无** | 同上 |
| `privileged.executed` | 无（特权操作还没有实现） | 不做，交接 |
| `stream.subscribed`（不在 5.8.6 表里） | 网关 | 只写 host.log，不进表 |

`Cleanup::Audit`（保留 90 天，每天一次）在 `store/retention.rs` 里已经有了，`test/run_maintenance` 的 `audit` 也已接上。本任务只需在 IT 里验证，不用改 `retention.rs`。

### 实现

| 模块 | 内容 | 估计 |
|---|---|---|
| 正式格式（`gateway/audit.rs`） | `Formal`（事件名、principal、`peer_json`、`detail_json`）；全部事件的映射：conn 4 种（`CloseReason` 11 种取值的代码，`HostExiting` 记为 `host_exiting`）、`interaction.responded`、`config.*` 2 种、`elevation.*` 6 种、`project.trust.*` 3 种；`stream.subscribed` 只进 host.log；新增变体 `ConfigConfirmed`、`ProjectTrust` | 190 |
| 审计写入 task | 接收端 → 写线程插入 `audit` 行，同时 `tracing` 写 host.log；新的静止屏障种类（登记、`Participant::idle`）；写失败时记 warn | 60 |
| `conn.accepted` 补对端信息 | `admit` 里取 `pid_path`、`AuditToken::signing_identity`（identifier、Team ID）；`Accepted` 加字段；改掉"归 T038"的注释 | 25 |
| `config.confirmed` 产生者 | 会话离开 `config_unconfirmed` 时记离开的原因、实际模式与档位、设定者（同一步的 `SetBy`） | 25 |
| `project.trust.*` 产生者 | 索引：`project/trust` 成功记"信任"；`project/add`、`trust_info`、`rescanned` 转为 stale 记"变为 stale"；扫描失败记"扫描失败"；摘要取前 16 位 | 40 |
| 宿主组装 | `_audit` 接到写入 task | 10 |
| **实现小计** | | **350** |
| 另：`git.unsafe_repo_config` 产生者 | `ScanContext` 加审计发送端；`locate` 遇到 `unsafe_repo_config` 时记命令类别、原因与路径 | 25 |
| 另：`host.*` 产生者 | 启动（版本、弱认证）、有序退出（原因）；"进入待升级"的产生点在 T059 的自检里 | 20 |

### 测试

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-audit-format | 每种事件的事件名与记录内容；`conn.accepted` 带对端信息；无连接的会话审计也能落盘 | 130 |
| IT-audit-retention | 真实宿主、手动时钟：连接、初始化、关闭、项目信任产生审计；查 `audit` 表与 host.log；推进 91 天后 `test/run_maintenance audit`，旧记录清掉、新记录留下；宿主进程照 T008 修复 1 的守卫做法不留残留 | 100 |
| 产生点用例 | `config.confirmed`（会话 gating 用例）；`project.trust.*` 三种（索引用例） | 70 |
| **测试小计** | | **300** |
| 另：`git.unsafe_repo_config`、`host.*` 的用例 | git：建一个配置来源在工作区内的仓库；host：在 IT 里顺带断言 | 30 |

### 合计

| 范围 | 估计 | 按前几个任务的实测比（0.86–1.04） |
|---|---|---|
| 任务书第 1–4 项（不含 git、host 的新产生者） | 650 | 560–675 |
| 再加 `git.unsafe_repo_config`、`host.*` 的产生者 | 725 | 625–755 |

两种口径都贴着或超过约 650 的线，按任务书开工前停下汇报。拆分建议见汇报。

## 裁定后（方案 A，上限约 700、硬停止线 780）

### 统计（`git diff --numstat`，只算 `crates/`）

| 时点 | 增 | 删 | 净增 |
|---|---|---|---|
| 正式格式与会话侧（`config.*`） | 298 | 43 | 255 |
| 加上 `project.trust.*` 产生者 | 389 | 63 | 326 |
| 加上审计写入 task 与组装（实现完成） | 450 | 65 | 385 |
| 加上 UNIT-audit-format（压缩前 653，压缩后） | 658 | 65 | 593 |
| 加上 IT-audit-retention 与信任产生点的 IT | 734 | 66 | 668 |
| 定稿 | 738 | 67 | 671 |

实现约 380 行（估计 350），测试约 290 行（估计 300）。UNIT-audit-format 第一版有 218 行（rustfmt 把每个结构体展开），推算会到 800 上下、越过硬停止线，于是先压缩：用例改成单行的 JSON 文本比对，去掉一个重复的关闭原因，写入 task 的用例只留一条记录。之后推算在 680 左右，才接着写 IT。

### 过程记录

- **正式格式**在 `gateway/audit.rs`：
  - `AuditEvent::formal()` 给出事件名、principal、`peer_json`、`detail_json`；`kept()` 说明是否进表，`stream.subscribed` 只进 host.log。
  - `host.*`、`git.unsafe_repo_config`、`privileged.executed` 在 `formal()` 的文档里注明随产生者由 T163 加。
  - `CloseReason`、`RefusalKind`、`io::ErrorKind` 的代码按变体名转蛇形（`HostExiting` → `host_exiting`），UNIT-audit-format 核对。
- **审计写入**：`supervisor/audit.rs`，一个新的静止屏障种类 `audit`。
  - 在 `Participant::idle` 里等接收端，每条先写 host.log（`tracing` 事件，字段 `event`、`conn`、`principal`、`peer`、`detail`），再经写线程插入 `audit` 表；
  - 写失败时记 warn。
- **`conn.accepted`**：在 `admit` 里取 `pid_path` 和 `AuditToken::signing_identity`，记可执行文件路径、签名 identifier、Team ID；取不到时为空。
- **`config.*`**：
  - 两条都记实际模式与档位、设定者。离开时，设定者取同一步的 `SetBy`（`session/confirm_config` 或被准许的选项），没有时取 `TierGuard` 当前的设定者。
  - `config.confirmed` 的原因是离开的那一种。
  - T044 的 `ConfigUnconfirmed` 改成了 `{reason, config: ConfigState}`。
- **`project.trust.*`**：
  - `trusted`：`project/trust` 成功之后记录；
  - `stale`：`project/add`（已有项目）、`project/trust_info`、`IndexActor::rescanned` 让受信任的项目变为 stale 时记录；
  - `scan_failed`：`project/trust`、`trust_info` 的扫描失败（项目 ID 来自参数），以及会话重扫时扫描本身失败（`Rescan.scan_failed`）时记录。
  - 新路径的 `project/add` 扫描失败时还没有项目，不记。
- **不写秘密**：
  - 握手失败只记错误码和 `data`，不记 `message`，因为参数非法时 serde 的报错可能引用客户端发来的内容；
  - 令牌、密钥、登录 shell 的环境本来就不在任何审计事件里。
- **`elevation.requested` 写入失败**：审计在索引写批次之前单独发出，审计表里照样有，不另记（编排者已认可）。
- **IT-audit-retention 的 90 天**：
  - 不能直接把时钟推 91 天：每小时一次的账本清理让 `test/clock_advance` 要逐步走两千多个期限，约 30 s；而且途中每天的审计清理会先把旧记录删掉，`test/run_maintenance` 就验证不到。
  - 改为：宿主运行时直接插入两条旧记录，一条早于 90 天 1 ms，一条晚 1 ms。先 `run_maintenance` 一次，只剩后者；手动时钟推进 2 ms 后再跑一次，后者也被删，新近的连接记录留着。
- **进程卫生**：新 IT 的宿主进程包在 `Running` 守卫里，drop 时结束。临时让用例中途 panic，没有留下宿主进程。
- **`conn.closed`（`host_exiting`）只进 host.log**：有序退出时 `close_all` 排在写线程停止之后，这些记录到达时表已经写不进去了。要进表，就得调整退出顺序：先关连接、排空审计写入，再停写线程。这一步属于 `Host::run`（T059），而 T163 加 `host.exiting` 时也需要同样的排空，交接给 T163。

## 合并 main 后的修复：IT-idle-exit

- **现象**：合并 T059、T129 之后，`it_idle_exit_once_nobody_is_connected_for_the_idle_time` 每次都失败：客户端还连着，宿主就以 `idle` 退出了。
- **原因**：
  - `admit` 里新加的 `AuditToken::signing_identity()` 在异步工作线程上同步执行。对体积大、未正式签名的程序（这里是 lifecycle 测试二进制），一次要约 600 ms；临时加的日志显示前后相差 627 ms。
  - 这段时间里宿主的定时器跟着停了，监护的观察本该每 20 ms 一次，实际只在启动时和阻塞结束后各发生一次；
  - 而且客户端在 `admit` 结束、握手完成之前，还不算进 `gateway.clients()`；
  - 这条 IT 把空闲时长设成 300 ms，所以宿主看到的是一段超过 300 ms 的空闲，就退出了。
- **修法**（修在 T138 这一边，不动 T059、T129 的代码）：
  - `conn.accepted` 改为带上钉住的 `AuditToken`，路径、identifier、Team ID 一开始为空；
  - 审计写入 task 收到 `conn.accepted` 时，经 `spawn_blocking` 调用 `AuditEvent::read_peer()` 读出它们，按记录顺序写入；
  - 读完路径之后再核对令牌（`unique_id`），对不上就不记路径。
  - 连接不再等这次查询，异步线程也不再阻塞。
  - 代价：对端如果在写入 task 读之前就退出了，这三项为空。写入 task 正常是在接受连接后毫秒级就处理到这条记录的。
- **修复的规模**：增 59、删 18，净增 41。
- **验证**：
  - IT-idle-exit 连跑 5 次全部通过，IT-audit-retention 和全部 `cupper-host` 测试通过；
  - UNIT-audit-format 改为对本进程取令牌、`read_peer` 后核对路径就是本测试二进制、identifier 是字符串。
- **CI 的插曲**：第一次重跑时，xtask 的 `it_host_bundle_layout_of_a_self_use_build` 报"building cupper-host failed (exit status: 101)"。当时磁盘只剩 3.8 GB，是环境问题：删掉本 worktree 的增量编译缓存（约 50 GB）后重跑，通过。
