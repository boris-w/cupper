# T107 Electron 大版本跟进：44 → 45

- 派发日期：2026-11-09
- 执行者：子 agent（工程师，桌面主进程与构建）
- 分支：`build/T107-electron-45`
- 依赖：T099（已合并）。T103（SP-6）依赖本任务，经它影响 T152、T155、T108、T159

## 背景

桌面目前锁在 Electron 44.9.0。官方支持的最新大版本是 45：45.0.0 于 2026-10-21 发布，到今天已满两周；最新的补丁是 45.2.3（11-04）。SP-6（T103）要测的就是要交付的版本，所以要先升级。

## 范围

**做**（计划 T107 一行；设计 5.10.6；桌面 #14）：

1. `apps/desktop` 的 `electron` 升到 45 的最新补丁（精确版本），连同 electron-builder、`@electron/fuses` 等配套依赖里必须跟着升的。新依赖照许可证门禁和 pnpm 的最短发布时间检查。
2. 读 Electron 45 的 breaking changes，逐条对照我们用到的 API，结果写进 `notes.md`。重点核对：
   - fuses：Electron 44 有第 9 个 fuse（`WasmTrapHandlers`），45 可能又多了，或者改了默认值。T161 的扫描会把不认识的 fuse 判为违规，要按实际情况更新 5.7.9 的两列和扫描的期望值；
   - `webUtils.getPathForFile`（T142）、`sendInputEvent`（e2e 的输入）、`will-frame-navigate`（T131）、`utilityProcess`、窗口和菜单 API；
   - Q26：Playwright 的键盘输入不触发 `before-input-event`。T090 在 44 上复测过，45 上要再确认，结论记进 notes。
3. 跑全量检查：
   - PR 的 8 个检查；
   - 由 `main` 用分支触发 nightly 全集（含 release-config、selfuse、e2e、perf 只记录）；
   - 截图基线有变化时由 `ui-baseline` 生成，逐张核对原因。
4. **MAN-keyboard**：要作者用实体键盘确认，放在合并前。写好检查步骤，交给 `main` 约用户。

**不做**：SP-6 本身（T103）；为新版本的新功能加代码。

## 验收标准

- [ ] PR 的 8 个检查通过；nightly 全集在分支上通过。
- [ ] 截图基线的变化都有解释。
- [ ] MAN-keyboard 的检查步骤写好，交给 `main`。
- [ ] 汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：200；硬停止线：260（lock 和基线不计）。breaking changes 涉及的改动比预想的大时，先报给 `main`。

## 参考资料

- 设计：5.10.6（Electron 版本策略）、5.7.9（fuses）、5.7.8（安全加固的落点）、5.9.9。
- 卷宗：T074（fuses）、T090（Q26 复测）、T161（release-config 扫描）、T160（自用构建）。
- 交接事项：`docs/tasks/handoff-notes.md` 里与 fuse、Electron 相关的几行。

## 注意

- 本机只单份跑，不跑抢焦点、发真实按键的用例（守则第 10 条）；全量和 e2e 放到 runner 上。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
