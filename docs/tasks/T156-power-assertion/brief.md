# T156 防休眠（NFR-13）

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主与 cupper-sys）
- 分支：`feat/T156-power-assertion`
- 依赖：T099（M1a 已退出）。T179（M1b 退出验收）依赖本任务

## 背景

NFR-13 是 M1 里唯一顺带做的 P1：agent 在干活或在等用户回答时，Mac 不能因为用户闲置而睡眠。设计 5.2.13 已定规则：有进行中的回合或待处理的交互请求时，持有 `PreventUserIdleSystemSleep` 断言，没有时立即释放；有序退出时释放电源断言。协议里 `host.updated` 早已带 `power_assertion` 字段（5.3），宿主目前一律写 `false`（`crates/cupper-host/src/supervisor/mod.rs`）。

## 范围

**做**（计划 T156 一行，5.2.13 的"防休眠"和"有序退出"）：
1. `cupper-sys`：IOKit 的 `IOPMAssertionCreateWithName` / `IOPMAssertionRelease` 的安全封装（`unsafe` 只在 `cupper-sys`），断言名称不含用户数据。
2. `Supervisor`：按"进行中的回合 + 待处理的交互请求"引用计数，从 0 变 1 时取得断言，回到 0 时立即释放；有序退出按 5.2.13 的顺序释放。计数的增减来自现有的状态变化（回合开始与结束、请求出现与了结），不另起轮询。决策逻辑写成纯核心，核心里不做 I/O。
3. `host.updated` 的 `power_assertion` 如实反映当前是否持有断言（变化时下发）。
4. 用例 **IT-power-assertion**（已登记，挂在 T156 下）：回合进行中持有、回合结束释放；只有待处理请求时也持有；宿主异常结束（崩溃、SIGKILL）后系统不残留断言（进程退出时系统自动回收，要有用例或证据）；`power_assertion` 随之变化。能在 runner 上核对系统层面的断言（例如 `pmset -g assertions` 里按进程号查）最好；做不到时说明替代的证据。

**不做**：设置页的"防休眠"开关。5.7.4 设置页"通用"里列了防休眠，界面归 T147；5.2.13 还没写宿主读哪个设置项。本任务按"总是开启"实现，开关怎么接（设置键、默认值、关掉时立即释放）写成建议放进汇报，由编排者补设计。空闲退出与待升级的改动也不做。

## 验收标准

- [ ] IT-power-assertion 通过，覆盖上面四种情形。
- [ ] `host.updated.power_assertion` 与实际持有状态一致。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：400；硬停止线：500

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.13（启动、空闲退出、有序退出、防休眠）、5.2.2 的 `Supervisor` 一行、5.3 的 `host.updated`、5.1.7（`unsafe` 只在 cupper-sys）。
- 计划：T156 一行，1.6（编号）。
- 卷宗：T037、T059（宿主生命周期 I、II，Supervisor 的纯核心）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；用例只用假 agent。
- 本机只单份跑（守则第 10 条）。本机跑用例时会短暂持有断言，这没问题；但不要让断言在测试结束后残留。
- 时间经 `Clock`；子进程经 `cupper-sys::spawn`；每条事件流只有一个写者。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
