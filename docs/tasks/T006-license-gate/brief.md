# T006 许可证门禁第一层

- 派发日期：2026-10-26
- 执行者：子 agent（前端工程师）。T084、T118、T128、T081、T082、T085、T093 的执行者接手（熟悉 pnpm 工作区和 Vite 的构建）
- 分支：`ci/T006-license-gate`
- 依赖：T004、T074（已合并）。T099 依赖本任务

## 背景

ADR-11 规定：Anthropic 的 SDK 和 Claude Code 二进制绝不进入仓库和产物。现在 `xtask ci license` 只跑 `cargo deny`，管 Rust 的许可证和安全公告；JS 侧还没有门禁。本任务做第一层：按锁文件和构建的 metafile，求出每个 JS 入口的**生产依赖闭包**，在源头上拦住。最终产物扫描是第二层，归 T182。

## 范围

**做**（计划 T006 一行；设计 5.1.6 的 license、ADR-11）：
1. **`tools/license-gate` 第一层**：
   - 按 `pnpm-lock.yaml` 和 Vite/esbuild 的 metafile，求每个 JS 入口（桌面主进程、preload、渲染进程，以及其他会进产物的入口）的生产依赖闭包；
   - 闭包里出现 Anthropic 的 SDK（`@anthropic-ai/*`）或 Claude Code 就失败，并给出依赖路径，例如 `a → b → @anthropic-ai/sdk`；
   - 运行时组件（`runtime/` 的清单里随 App 带的 Node、适配器）的许可证要齐全，缺了就失败。
2. **只算生产依赖**：开发和测试才用的包不算，例如 `lightningcss`（MPL-2.0，vite 的依赖）、`argparse`（Python-2.0，经 hey-api、electron-builder 引入）。算进去会误报（T070、T015 的交接）。
3. **codex-acp 内联依赖的许可证**：声明按上游 v2.2.2 标签的 `package-lock.json` 生成。SP-2 的锁文件是按版本范围解析的，与上游实际内联的版本不同（T004 的交接，明细见 `docs/tasks/T004-runtime-pack/notes.md`）。
4. **负面样例**：入口闭包里加一个 `@anthropic-ai/sdk`（假包或 fixture）时门禁失败；运行时组件缺许可证时失败。
5. **接入 `xtask ci license`**：和 `cargo deny` 并列一步，摘要里看得到。
6. **顺带**（交接里点名给本任务）：`cargo deny` 拉 advisory 数据库时，网络出错整个作业就失败。可以评估拉取失败时用已缓存的数据库继续、在摘要里标记为过期，或者有限次重试。改不改、怎么改，写进汇报。注意区分两种失败：拉取失败可以重试，真的有安全公告（"advisories FAILED"）不能被吞掉，2026-10-26 的 RUSTSEC-2026-0382 就是后者。

**不做**：最终产物扫描（T182）。

## 验收标准

- [ ] UNIT-license-gate-negative：两类负面样例都失败，并给出依赖路径或缺失的组件。
- [ ] 当前 main 上的门禁通过，没有误报。
- [ ] `xtask ci license` 里能看到这一步。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T006 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T006-license-gate/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T006 的行）

- T070：`lightningcss` 只在开发和测试时用。
- T015：`argparse@2.0.1` 只在开发时用。
- T004：codex-acp 内联依赖的许可证声明怎么生成。
- 编排者：`cargo deny` 拉 advisory 数据库的网络失败。

## 注意

- 门禁只做静态分析，不运行真实的 Claude Code 或任何 SDK。
- 负面样例不能真的从 npm 装 Anthropic 的包：用本地 fixture 或伪造的锁文件片段。

## 规模

- 停止线：850；硬停止线：950

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b ci/T006-license-gate main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
