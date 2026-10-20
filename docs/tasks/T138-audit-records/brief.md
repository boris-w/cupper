# T138 审计落盘与正式格式

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T037、T035、T096、T034、T047、T100 及两个修复任务的执行者接手
- 分支：`feat/T138-audit-records`
- 依赖：T037（已合并）。与 T059 并行；T099 依赖本任务

## 背景

T059 开工前拆成两份（计划 v1.3.31，见 `docs/tasks/T059-host-supervisor/brief.md` 末尾的补充裁定）。
- 网关、会话、索引已经产生各种审计记录，都经同一个发送端（`supervisor` 里的 `records`）发出；
- 但 `Host::start` 里的接收端（`_audit`）至今被丢弃，什么都没落盘。

本任务把审计接到写线程，按设计 5.8.6 落成正式格式，并补上几处还没有产生者的事件。

## 范围

**做**（计划 T138 一行；设计 5.8.6、5.2.13；`04-data-model.md` 的 `audit` 表）：
1. **落盘**：
   - 审计接收端接到写线程，写进 `audit` 表；
   - 同时写入 host.log（`tracing` 事件，字段按 `diag` 的约定，T028 的交接）；
   - 保留 90 天，清理接进现有的 Maintenance（`store/retention.rs`）。
2. **正式格式**（只用于 `audit` 表；网关夹具的审计行保留 `Debug` 文本，T037 的裁定）：把已有的审计事件都映射成 5.8.6 的事件名和记录内容：
   - `conn.accepted`、`conn.rejected`、`conn.initialized`、`conn.closed`（`CloseReason::HostExiting` 记为 `host_exiting`）；
   - `interaction.responded`（T088）；
   - `config.unconfirmed`、`config.confirmed`（T044；`AuditRecord.conn` 已是 `Option<ConnId>`，会话自己发起的没有连接）；
   - `elevation.*`（T038 的 `AuditEvent::Elevation` 各 `step`）；
   - `git.unsafe_repo_config`；`host.*`；
   - 5.8.6 表里其余的事件，有产生者的都映射。
3. **`conn.accepted` 补对端信息**（T038 的交接）：可执行文件路径、签名 identifier、Team ID。用 T133 在 `cupper-sys` 加的按审计令牌取签名身份的函数；`gateway/audit.rs` 开头"归 T038"的注释改掉。
4. **补上还没有产生者的事件**：
   - `config.confirmed`：会话离开 `config_unconfirmed` 时发出，记实际档位、设定者、原因；
   - `project.trust.*`：信任、变为 stale、扫描失败，记摘要的前 16 位。产生点在项目信任与重扫的路径上（T035、T109）。

**不做**：
- 监护、孤立、自检、清理残留（T059）。T059 不碰 `gateway/audit.rs`、审计写入、`store/retention.rs`；`Host::start` 里审计接收端的去向归本任务。
- 设置页的"连接与设备"界面（M1b）。

## 验收标准

- [ ] UNIT-audit-format：每种事件映射成正式格式（事件名、记录内容）；`conn.accepted` 带对端信息；没有连接的会话审计也能落盘。
- [ ] IT-audit-retention：真实宿主上产生几类审计后，`audit` 表里有正式格式的记录，host.log 里有对应事件；90 天清理经 `test/run_maintenance` 和手动时钟验证。
- [ ] `config.confirmed`、`project.trust.*` 的产生点有用例。
- [ ] 不睡眠；新增的 IT 连跑 10 次全绿。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T138 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T138-audit-records/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T059 的行里与审计有关的部分）

- T088：`InteractionResponded` 记为 `interaction.responded`；`Gateway::new` 接收宿主建好的审计发送端。
- T028：审计同时写 host.log。
- T044：`AuditRecord.conn` 是 `Option<ConnId>`。
- T038：`conn.accepted` 的对端信息；`elevation.requested` 写入失败时，之后的 `elevation.resolved` 在视图里找不到申请。看看审计这边要不要记一笔，在汇报里说明。
- T047：新的 actor 或 task 要遵守静止屏障规则（登记、`Participant::idle`、`Hold`）。

## 注意

- 审计里不写秘密：令牌、密钥、登录 shell 的环境都不能进表或 host.log（5.8.5、T028 的脱敏约定）。
- 代码约束：每条事件流只有一个写者；时间只经 `Clock`；错误用 thiserror。

## 规模

计划估为 M，T059 的执行者估约 435 行；本任务另加了两类产生者。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 650 行（包括贴线）时，必须先停下来汇报**；硬停止线 750。

## 参考资料

- 设计：`08-security.md` 5.8.5、5.8.6；`02-host.md` 5.2.13；`04-data-model.md`（`audit` 表）；`11-diagnostics.md`（host.log）。
- `docs/tasks/` 下 T037、T088、T028、T044、T038、T133 的 `notes.md` 和 `review.md`。
- `crates/cupper-host/src/gateway/audit.rs`、`supervisor/`、`store/retention.rs`、`diag/`。

## 工作方式

在原来的 T037 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T138-audit-records main`。T008 修复 1 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计 650–725 行（按实测比 560–755），贴线，在写代码之前停下。

**裁定：方案 A（计划 v1.3.32）。**
- **本任务做第 1–4 项**：
  - 审计写入 task（写 `audit` 表和 host.log）；
  - 全部现有事件的正式格式；
  - `conn.accepted` 补对端信息；
  - `config.confirmed` 与 `project.trust.*` 的产生者；
  - UNIT-audit-format、IT-audit-retention 和产生点的用例。
  - 上限约 700 行，硬停止线 780；推算贴线也先停下。
- **90 天清理已有**（`Cleanup::Audit`）：不改 `retention.rs`，只在 IT-audit-retention 里验证。
- **`host.*`、`git.unsafe_repo_config`、`privileged.executed` 都还没有产生者**，交给新增的 T163（依赖 T059 和本任务）。本任务只为已有产生者的事件做映射；给这三类留好正式格式的位置即可，不必先加 `AuditEvent` 变体。
- **`elevation.requested` 写入失败的边界情况**：审计在索引写批次之前单独发出，审计表里仍有记录，不用另记一笔。认可。
