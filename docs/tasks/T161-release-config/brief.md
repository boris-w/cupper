# T161 release-config 检查

- 派发日期：2026-11-09
- 执行者：子 agent（工程师，构建与 CI）
- 分支：`ci/T161-release-config`
- 依赖：T160（已合并）。T179、T180、T182 依赖本任务

## 背景

设计 5.1.6 的 nightly 里有一项 `release-config`：用发布配置做一次不签名的构建，逐项核对发布产物的安全配置。`electron-builder.release.yml` 至今没有真正构建过。自用构建（T160）已经有逐个 Mach-O 检查 entitlements、读 fuse 线的做法，可以复用。

## 范围

**做**（计划 T161 一行；设计 5.1.6 `release-config`、5.7.9、5.10.2、5.10.7；P-11）：

1. **`xtask ci release-config`**，挂进 nightly。用发布配置做不签名的构建，然后逐项断言：
   - `@electron/fuses read` 的每一项都符合 5.7.9 发布一列。Electron 44 有第 9 个 fuse，@electron/fuses 1.8.0 不认识，读出来是 `undefined`，要明确处理，不能当作通过；
   - Info.plist 里有 `ElectronAsarIntegrity`；
   - `asarUnpack` 为空，原生扩展只经 `extraResources` 放在 `Resources/native/`；
   - asar 文件清单符合预期，没有测试、夹具、源码映射之类不该进产物的文件；
   - 带未知命令行开关启动时，应立即退出（E2E-SEC-switches）；
   - 扫描产物里的测试钩子标记（`__cupperE2E`、harness 等），一个都不能有。包括 T172 新加的 `dropReplyTo`，它只在 e2e 构建里注册；
   - 宿主和 CLI 的 `--build-info`（一行 JSON：`debug_assertions`、`features`、`name`、`version`）：`debug_assertions` 为 false、`features` 为空；
   - `cargo xtask runtime-pack --verify` 通过；
   - 两个 Info.plist 都不含用途说明（5.10.2）。`Cupper.app` 的那份由 electron-builder 产出，Electron 默认带着摄像头、麦克风、蓝牙、音频采集的用途说明。**这一项先只报告现状，不在本任务里删**，删不删、怎么删由 `main` 和安全评审定（交接事项 T160 一行）。
2. **自用构建也扫一遍**：
   - fuses 按 5.7.9 自用构建一列断言，`enableCookieEncryption` 关闭；
   - 测试钩子扫描、`--build-info` 同上。
3. **UNIT-release-config-scan**（已登记）：扫描逻辑的单元测试，构造违规的产物（多一个 fuse、带测试钩子标记、asar 里多了文件等），证明每一项都会失败。
4. 复用 `apps/desktop/src/test-entry-points.ts`、`apps/desktop/selfuse/build.test.ts` 里的做法，不要另写一份。

**不做**：
- 签名、公证（T180）；
- 发布流水线（T181）；
- 许可证门禁第二层（T182）；
- 删掉 Info.plist 里的用途说明（待定，见上）。

## 验收标准

- [ ] `release-config` 作为 nightly 的一项，在 GitHub runner 上通过。由 `main` 用分支手动触发 nightly，或者只跑这一项。
- [ ] UNIT-release-config-scan 在 PR 里通过，每一种违规都有用例证明会失败。
- [ ] E2E-SEC-switches 通过。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：450；硬停止线：550。开工前在 `notes.md` 里按检查项估行数，估完报给 `main`。

## 参考资料

- 设计：5.1.6（CI 作业表、nightly）、5.7.9（fuses 的发布与自用两列）、5.10.2（Info.plist、entitlements）、5.10.7（运行时清单）；P-11。
- 交接事项：`docs/tasks/handoff-notes.md` 里接收任务含 T161 的各行，包括第 9 个 fuse、`--build-info`、`buildInfo().features`、Info.plist、`vite build --mode local`、复用 T160 的做法。
- 卷宗：T160（自用构建）、T074（fuses）、T023（`--build-info`）、T073（原生扩展）、T005（Info.plist）。

## 注意

- 构建在本机只做一次，单份跑；不签名，不碰钥匙串。整套 nightly 放到 runner 上跑（守则第 10 条）。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
