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
