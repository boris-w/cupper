# T008 驱动层 II-b：适配器的生命周期

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T088、T097、T055 的执行者接手，因为 `AgentRuntime` 是它写的
- 分支：`feat/T008-adapter-lifecycle`
- 依赖：T055（合并中）。T063（宿主集成测试 II）依赖本任务

## 背景

T055 开工前拆成三份（计划 v1.3.18）：T055 做了运行时核心与宿主组装；本任务做适配器实例的生命周期：重启退避、关闭顺序与后代进程组、资源采样。T009（能力与状态）与本任务并行，由另一位执行者做。

## 范围

**做**（设计 5.6.1、5.6.8；ADR-09；T055 的交接）：
1. **重启退避与 `crashed`**：现在没有退避，下一次 attach 立即拉起新实例。加退避与 `crashed` 状态（时长接 `test_config.durations`，登记进 `durations.rs`）。
2. **关闭顺序按 5.6.8**：先 `session/close`，drop 连接关 stdin，**发信号前枚举全部后代**（`with_descendants()`），按各自的进程组发信号，`Clock` 限时等待并用 `is_alive()` 核对，最后对还在的组 `Kill`（T020 的做法）；`stop` 要限时（现在不限时）。`setsid` 的孙进程拿着 stdout 时实例一直不结束，靠这里的后代进程组清理解决。
3. **资源采样**：每 30 秒采样一次各适配器进程组的常驻内存，记下后代的进程组和 `claude` 进程数（Q43），提供采样接口（`rss_samples` 注入供测试）；适配器自己先退出时，后代只能靠采样时记下的组清理。
4. **stderr 尾部进 `error` 条目**：每个实例的 stderr 尾部（64 KB，在 `Instance.tail`）放进 `AdapterExited`，供 `error` 条目的错误详情（MS-5）。
5. **假 agent 的 `setsid` 孙进程**：给假 agent 加一个用 `setsid` 拉起子进程的剧本变体（现有的 `spawn_child` 只在自己的进程组里拉起），供 IT-K21。

**不做**：能力缓存、去重、`agents/*`（T009）；进程级的 K 清单其余用例（T063）。

## 验收标准

- [ ] IT-K21-reap-group（补"后代在另一个进程组"：假 agent 用 `setsid` 拉起子进程，关闭后它也被清理）、IT-adapter-restart（用例直接驱动 `AgentRuntime`：崩溃后按退避重启、连续崩溃进入 `crashed`）。
- [ ] 关闭顺序有用例：`session/close` 先发、Term 不退出时被 Kill、`stop` 限时返回。
- [ ] 资源采样有用例（注入的 `rss_samples`），`claude` 进程数计入。
- [ ] 不睡眠：核心和宿主内的用例用 `ManualClock`；进程级用例经 `test_config.durations` 缩短时长。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T008 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T008-adapter-lifecycle/ci/` 通过，摘要随任务提交。

## 注意

- 发信号的用例只能向测试自己拉起的进程组发，绝不向测试之外的进程发信号；用例结束后确认没有残留进程。
- 不运行真实的 Claude Code CLI；用例一律用假 agent（按 T055 定下的路径约定）。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；`unsafe` 只在 `cupper-sys`。

## 规模

计划估为 M，T055 的执行者估约 650 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 800 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.1、5.6.8；`02-host.md` 5.2.13（资源采样）；`08-security.md` 5.8.5。
- ADR-09；`docs/tasks/T055-agent-runtime/notes.md`（给 T008 的交接）、`docs/tasks/T020-sys-process/`。

## 工作方式

T055 合并之后，在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T008-adapter-lifecycle main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 760 行，按偏差推算约 830，贴线，停下汇报，建议把资源采样拆出去（约 230 行）。

**裁定：采用方案二，不拆，上限放宽到约 850 行，硬停止线 900。** 资源采样单独成任务不值得，而且拆开后适配器意外退出时接不上后代进程组的记录。

执行者的 4 个问题：
1. **后代进程组存在哪里**：在 `adapters` 表加一列 `groups TEXT`（`[[pgid, start_us], …]`），同时改 `0001` 和设计 04 的 SQL（授权，做法同 T043 加 `reply_to`）。宿主崩溃后的清理（T059）要靠它。
2. **孙进程**：假 agent 用标准库的 `process_group(0)` 让孙进程进入新的进程组并继承 stdout。假 agent 是测试二进制，不受"宿主、CLI、原生扩展的子进程一律经 `cupper_sys::spawn`"的约束；宿主清理新进程组和新会话的方式完全一样。不扩展 `cupper-sys` 的标准流。
3. **重启时机**：有 attach 在等时按退避定时拉起，否则等下次使用再拉起，两种情况都遵守退避。
4. **`crashed` 之后的恢复**：`agents/restart` 是 M1b 的方法（设计 5.3.4），归 T111；本任务只留一个清除 `crashed` 的方法。

运行时的用例一律用 `ManualClock`、先等 `next_deadline()` 看到定时器设好再推进时钟（`host-release` 特性组里 `test_config.durations` 用不了）：认可。
