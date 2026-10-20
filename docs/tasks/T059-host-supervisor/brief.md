# T059 宿主生命周期 II：监护、孤立、自检与审计落盘

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037 的执行者接手，因为 `Host::start`、`Host::exit`、`supervisor::Part` 都是它写的
- 分支：`feat/T059-host-supervisor`
- 依赖：T037（已合并）。T062（宿主集成测试 I）依赖本任务

## 背景

T037 开工前按守则第 8 条拆成两半（计划 v1.3.9）：T037 做启动串联、有序退出和 `host/status`、`host/exit`；监护者的其余职责拆到本任务。T037 留下的接入点写在 `docs/tasks/T037-host-lifecycle/notes.md`"给其他任务的接入点"。

## 范围

**做**（计划 T059 一行；设计 5.2.13、5.5.6、5.8.1"`dev_key` 只接受拉起宿主的那个进程"、5.8.6；5.2.13 末尾"T037 开工前的裁定"）：
1. **监护者纯核心**：输入是连接数、进行中的回合、待处理请求、排队消息、安装器状态、父进程状态、自检结果、时钟；输出是"是否接受新会话和新回合"和"何时退出、以什么原因退出"。不做 I/O，不 await（代码约束）。规则：
   - **空闲退出**（非后台模式）：没有客户端连接、没有进行中的回合、没有待处理请求、没有排队消息、安装器空闲，持续 10 分钟（经 `durations.rs`，接 `test_config.durations`）后有序退出；
   - **孤立**（`dev-auth` 构建）：拉起宿主的父进程退出后，不再开始新回合，运行中的回合结束后有序退出；
   - **待升级**：拒绝新建会话和新回合（`unavailable`，`reason: pending_upgrade`）；没有运行中的回合、待处理的交互和排队消息时立即退出，不看连接，也不等 10 分钟（T037 开工前的裁定）。
2. **接进 `Host::run`**：空闲、孤立、待升级作为新的 `Exit` 取值，从 `Host::run` 的等待里出来，复用 `Host::exit`。`host/shutdown` 的 `reason` 按 `Exit` 取值：空闲用 `idle`，待升级用 `upgrade`，孤立按裁定用 `user_quit`。"是否接受新会话和新回合"要经会话端口生效，接口留给会话管理器调用（会话管理器由 T055 组装进宿主，本任务用测试里的替身验证）。
3. **`dev-auth` 构建下监视父进程**：父进程退出时通知监护核心进入孤立。启动时 ppid 已经是 1 的情况 T037 是否已处理，请核对；没有的话按 5.8.1 以错误码退出。
4. **自检**：每次有新连接（可以加在 `Gateway::serve` 里 accept 之后）、每次拉起子进程之前，比较 `current_exe()` 文件的 `file_cdhash` 和 `own_cdhash()`；不一致或文件不存在时进入待升级。`own_cdhash()` 失败时不做自检（T037 开工前的裁定）。`host/status` 的 `pending_upgrade` 改取自检结果。
5. **按 `adapters` 表清理残留**：TERM → 限时等待 → KILL 的例程，每 50 ms 轮询一次，两个时长登记进 `durations.rs`；没有 ACP 连接，所以跳过 ADR-09 的前两步。做成 T055 可以复用的函数。清理完才启动 Maintenance（排在 `maintenance.spawn` 之前）。适配器后代的进程组表由 T055 补上。
6. **审计落盘与正式格式**（5.8.6）：
   - 把 `Host::start` 里丢弃的审计接收端（`_audit`）接到写线程，写进 `audit` 表（保留 90 天，清理接进现有的 Maintenance），同时写入 host.log（T028 合并之前先只写表，留好调用点）；
   - 正式格式只用于 `audit` 表，网关夹具的审计行保留 `Debug` 文本（T037 开工前的裁定）；
   - `CloseReason::HostExiting` 记为 `host_exiting`；
   - 把已有的审计事件都映射成 5.8.6 的事件名和记录内容。
7. **在 `Host::start` 里接上流订阅**（编排者指定，T019、T037 都留了这一步）：调用 `routes.streams(streams, readers)`，删掉 `supervisor/mod.rs` 里 `streams` 上的 `#[expect(dead_code)]`。

**不做**：
- 防休眠（T156）；资源采样（T055 起）；
- 会话管理器、`AgentRuntime` 的组装（T055）；索引（T034）；
- 进程级的孤立用例 IT-host-orphaned（T062）。

## 验收标准

- [ ] 新增 UNIT-orphan-policy：监护核心的各条规则用表驱动覆盖（空闲计时被任一活动打断后重新计时、孤立时拒绝新回合但等运行中的回合结束、待升级时立即退出的条件、几种状态同时出现时的优先级）。
- [ ] 新增 IT-self-check：替换磁盘上的可执行文件（或让它不存在）之后，新连接触发待升级，新建会话被拒（`pending_upgrade`），`host/status.pending_upgrade` 为 true，空闲时退出。
- [ ] 新增 IT-idle-exit：真实宿主进程在空闲时长（经 `test_config.durations` 缩短）之后有序退出，客户端收到 `host/shutdown{reason: idle}`；期间有连接或活动时不退出。
- [ ] 清理残留的例程有用例：用 `setsid` 拉起的测试进程组，TERM 不退出时会被 KILL。
- [ ] 审计：`audit` 表里有正式格式的记录，90 天清理有用例。
- [ ] `stream/subscribe` 在真实宿主上可用（IT-lifecycle 或本任务的用例里订阅一次即可）。
- [ ] 不睡眠：核心和宿主内的用例用 `ManualClock`；进程级用例经 `test_config.durations` 缩短时长。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T059 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T059-host-supervisor/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T037：空闲、孤立、待升级作为新的 `Exit` 取值；`reason` 按 `Exit` 取值；接上 `_audit`；清理残留排在 `maintenance.spawn` 之前；`pending_upgrade` 改取自检结果。
- T019、T037：`routes.streams(..)` 和删掉 `#[expect(dead_code)]`。
- T088（并行进行中）：会新增一种带 `ConnId` 的网关审计记录 `interaction.responded`，经同一个审计 channel 发出。两边谁后合并，谁负责把它映射进正式格式；编排者合并时会核对。

## 注意

- 代码约束：`unsafe` 只在 `cupper-sys`；子进程一律经 `cupper_sys::spawn`；时间和定时只经 `Clock`；有状态的决策逻辑写成纯核心。
- 清理残留的用例只能杀测试自己拉起的进程组，**绝不能向测试之外的进程发信号**；用例结束后确认没有残留进程。
- 自检用例替换的是测试自己复制出来的宿主可执行文件，不碰 `target/` 里共享的产物。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍、组合测试按 1.5 倍计。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出上限时，开工前就停下来汇报，并给出拆分建议**（例如把"审计落盘"单独拿出来）。

## 参考资料

- 设计：`02-host.md` 5.2.13（含末尾"T037 开工前的裁定"）；`05-flows.md` 5.5.6；`08-security.md` 5.8.1、5.8.6；`06-agent-driver.md` 5.6.8；`04-data-model.md`（`audit`、`adapters` 表）。
- ADR-01、ADR-09。
- `docs/tasks/T037-host-lifecycle/`（`notes.md` 的接入点、`review.md`）。

## 工作方式

在原来的 T037 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T059-host-supervisor main`。T037 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
