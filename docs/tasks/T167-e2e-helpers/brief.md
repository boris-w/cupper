# T167 e2e 辅助函数

- 派发日期：2026-10-20
- 执行者：子 agent（测试工程师）。T090 的执行者接着做
- 分支：`feat/T167-e2e-helpers`
- 依赖：T090（合并中）。T091、T092、T093 依赖本任务。**本任务在剩余关键路径上**：T167 → T091 → T099

## 背景

T090 开工前估约 1,545 行，按方案一拆成两份（计划 v1.3.36）：T090 收窄为套件骨架、夹具、CLI 包装、原生确认框和 `xtask ci e2e`；其余辅助函数归本任务。T090 写到上限附近时，又把当时没有用例用到的 CLI `repl`、`watch`、以 `cli` 身份连接，以及 `showMessageBoxSync` 的替换拿掉，留给本任务跟用到它们的辅助一起加。

你在 T090 的 `notes.md` 里已经写过这些辅助的做法和估计（模块 6–14），本任务照那份来。

## 范围

**做**（计划 T167 一行；设计 5.9.4 的环境、5.7.8 的"测试"、5.9.9；T090 补充裁定的七条确认）：
1. **`__cupperTest`**：
   - ui-core 的 `mount` 加可选的 `onClient(client)`，只在 e2e 构建的渲染进程入口传入，挂上 `window.__cupperTest`，导出 `client.counters` 和连接状态（T087 的交接；视图的导出由 T091 按需要补）；
   - 发布构建经 Vite `define` 把这段去掉。发布构建的配置还没有，用现有的非 e2e 构建（例如 dev）证明产物里没有 `__cupperTest`，做成构建检查或用例（设计 5.10.7 的产物扫描也查这个名字）。
2. **CLI 的 `clock-advance <ms>`**：只在 test-harness 构建里有，发 `test/clock_advance`；有编译期门控。
3. **CLI 包装补齐**：`repl`（按 `repl.exit` 分条）、`watch` 加 `until(谓词)`、以 `cli` 身份连接（不带 key，提权要用）。
4. **按窗口断连**：主进程里只在 e2e 构建存在的入口，断开指定窗口的连接，等它重连。
5. **事件屏障**：经宿主的静止屏障与 `test/clock_advance`；会话流上等事件、索引上等会话出现（手动时钟下要先推进超过 100 ms，T046 的交接）。
6. **`kill -9` 宿主**：从 `status --json` 取 pid，按 T090 的进程表确认是本用例的宿主再发信号，等它消失、等 UI 拉起新宿主（pid 变化）。
7. **CLI 提权**：`cli` 身份在 `repl` 里 `elevate`，在侧边栏横幅点"查看"，替换后的确认框批准。照 IT-elevate、IT-cli-commands 的做法。
8. **外发采样**：沿 ppid 找出 App 的进程树（含宿主、适配器），采样并解析。只给采样器；`nettop` 能否作验收归 T092（Q27）。
9. **按键**：经 `webContents.sendInputEvent` 发出 keyDown、char、keyUp，支持修饰键和键名，走生产的 inputGuard（Q26 的结论）。
10. **请求面板**：等 `data-ready`、按数字键、按 Tab 激活。T083、T165 已合并或合并中，自检用真实面板。
11. **原生对话框补齐**：`showMessageBoxSync` 的替换；`dialog.showOpenDialog` 的替换与记录（添加项目时选文件夹，T090 交给 T093 的那一条，放在本任务的辅助里，T093 只写用例）。

**不做**：NFR-1 的五个用例（T091）；安全 e2e（T092）；交互与项目 e2e（T093）。

## 验收标准

- [ ] 每个辅助至少被一条自检用例调用过、确认能用。自检不登记编号，写进 `notes.md`；`@smoke` 不扩大，自检放在每晚的全集里。
- [ ] 证明非 e2e 构建的产物里没有 `__cupperTest` 和按窗口断连的入口。
- [ ] 新增的用例连跑 10 次全绿（workers 为 2）；跑完没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在。
- [ ] `cargo xtask ci e2e` 通过。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T167-e2e-helpers/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T090、T167、e2e 的行；开工前逐行读一遍）

- T090：夹具、进程表（pid 加启动时间）、CLI 包装、`Dialogs`。
- T046、T133：CLI 的退出码；`--json` 下 `elevate` 被拒时输出两行；提权探针。
- T038、T134：`auth/elevation_respond` 的取值；授权到期在 `clients.updated` 的 `elevated.expires_at`。
- T077：Q26 的结论；按键经 `sendInputEvent`。
- T083：请求面板的 DOM 标记（`data-request-panel`、`data-request-kind`、`data-ready`、`data-activation`）。
- T086：输入框的 DOM 标记（`data-composer`、`data-send-state`、`data-esc-armed`）。
- T059：中断回合的回复可能因待升级退出而丢失，不要断言一定收到。
- T008 修复 1、T063：进程级用例不留残留进程的守卫做法。

## 注意

- 不运行真实的 Claude Code、Codex CLI，不登录；agent 一律用假 agent。
- 生产代码里不加测试分支；测试用的入口只在 e2e 构建（或 test-harness 构建）里存在。
- 不读、不改用户的 `~/.claude`、`~/.codex`；e2e 的数据目录全部在临时目录里。
- 外发采样只采本用例的进程树，不碰别的进程。

## 规模

M。T090 的估计是实现约 425、测试约 225，加上从 T090 拿掉的部分和 `showOpenDialog`，约 760。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 850 行（包括贴线）就先停下汇报**；硬停止线 950。每完成一个模块就用 `git diff --numstat` 统计一次（守则第 8 条）。

## 工作方式

T090 合并之后，在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T167-e2e-helpers main`。T090 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
