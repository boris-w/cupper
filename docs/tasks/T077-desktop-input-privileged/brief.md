# T077 桌面主进程 IV：真实输入、特权操作与 Q26

- 派发日期：2026-10-20
- 执行者：子 agent（Electron 工程师）。T004、T005、T094、T028、T075、T007、T056、T115 的执行者接手（T075、T007 做过主进程）
- 分支：`feat/T077-desktop-input-privileged`
- 依赖：T076（已合并）。T090（e2e 基础设施）依赖本任务；本任务离剩余关键路径有 4 个单位的松弛

## 背景

T076 接好了 `windowBridge` 和 preload，但留了两处空位（交接）：
- `interaction/respond`、`git/revert` 现在照常放行，没有真实输入检查；
- preload 的 `requestPrivileged` 现在返回 `{status: "failed", error: "unsupported"}`。

本任务补上设计 5.7.8 的"真实输入"九条规则和特权操作（原生确认框），并在 Electron 44 上复测 Q26。

## 范围

**做**（计划 T077 一行；设计 5.7.8 的"真实输入"与"原生确认框"、5.7.2、5.7.3 的 `PrivilegedAction`、5.5 里 `add_project` 的时序图；Q26）：
1. **`inputGuard`**（5.7.8"真实输入"第 1–9 条）：
   - 只认主框架获得焦点时的输入：键盘用 `before-input-event`，鼠标用 `input-event` 的按下接松开；
   - 白名单：Enter、Space、Escape、⌘Enter，以及数字键 1–9（含小键盘），数字键写 `digit` 证据；
   - ↑ ↓ ← → 是导航键：本身不算真实输入，不触发第 3 条，但会清掉尚未用掉的输入；
   - 打字键之后 1 秒内的 Enter、Space、数字键不算；
   - 一次输入只能用一次；
   - 输入法组合中的按键不记录；
   - 窗口获得焦点后的冷却期；
   - 已放行的 `command_id` 表（第 9 条）。
   - 时钟可以注入。规则写成纯逻辑，用 Vitest 的假时钟测。
2. **证据写入**：在 `apps/desktop/src/main/bridge/window-bridge.ts` 的 `#send` 里，白名单检查之后、转发之前（T076 的交接）：
   - 任何出方向命令都用掉尚未消耗的输入；
   - `interaction/respond`、`git/revert` 要求有不超过 1 秒、尚未用掉的输入，没有就在本地返回 `user_activation_required`；
   - 主进程写入 `input: {kind, key?, digit?}`，覆盖渲染进程自己填的值。
   - `window.onFocusChange` 带上获得焦点的时间（第 7 条）。
3. **`privileged`**（第 8 条与"原生确认框"一行）：
   - preload 的 `requestPrivileged` 接上；要求真实输入，Esc 不能触发；
   - 同一时间只弹一个确认框，其他请求返回 `busy`；
   - 用户取消后同一类操作冷却 2 s、10 s、60 s，返回 `cooldown`；一次确认，或连续 10 分钟没有取消，重新计数；
   - 确认框的内容由主进程经 control 连接从宿主取：`policy/describe`、`auth/elevation_get` 等，各 action 的 `details` 键按 5.3.11 末尾"T031 实现时的裁定"（T031 的交接）；
   - agent 给出的文本逐字显示，不可见字符显示为转义；默认按钮是"取消"或"拒绝"；文案走 `@cupper/i18n`（主进程和审查文档共用一份文案）；
   - 确认后经 control 连接发出命令。
   - `PrivilegedAction` 的各种 kind 都要处理：`interaction`、`set_config`、`confirm_config`、`create_session`、`trust_project`、`elevation`、`settings`。宿主那边还没有方法的（例如 `settings`），返回 `failed` 并在汇报里列出。
4. **`add_project` 流程**（5.5 的时序图）：系统的选择目录对话框 → `project/add` → 有项目配置时，原生确认框逐项列出 findings，醒目标出来自工作区外的链接目标，默认按钮"不信任" → 用户信任时发 `project/trust`（带 `trust_digest`）；遇到 `digest_mismatch` 就重新取内容、再确认一次。
5. **提权确认框**：
   - 先列宿主核实过的信息，再列"申请方自述，未经核实"；
   - approve 单独一个勾选项，默认不勾；
   - "申请方不是本 App 自带的命令行工具"要醒目标出。这个字段由 T133 在 `VerifiedRequester` 上加（进行中）。T133 先合并就直接用；否则本任务按现有字段实现，把这一项写进交接。
6. **Q26 复测**（Electron 44）：
   - Playwright 的键盘输入是否触发 `before-input-event`；
   - `sendInputEvent` 是否触发它，页面收到的 keydown 是否 `isTrusted`；
   - `input-event` 的键盘事件运行时是否带键码；
   - `page.click()` 是否产生 `input-event`；
   - `webContents.focusedFrame` 判断主框架是否可靠。
   - 结论写进 `notes.md`，在汇报里列出，由编排者更新设计的 Q26。
7. **MAN-keyboard**：写出人工验证清单（实体键盘确认 Enter 批准、Esc 拒绝、数字键选择并提交、输入法选词时不触发），由作者执行。本任务不能代做，汇报里列为待作者确认。

**不做**：
- 渲染进程的请求面板与 RP-1 至 RP-12（T083）；
- e2e 用例与辅助函数（T090、T092）；
- 系统通知（T150）。

## 验收标准

- [ ] UNIT-input-guard：
  - 九条规则各有用例；数字键证据；
  - ↑ ↓ 之后的 Enter 仍算真实输入，↑ ↓ 之前那一下输入被清掉；
  - 输入法（`isComposing: true` 的 Input 对象）；
  - 冷却期；`command_id` 重发表（收到任何响应就删除，文档实例变化时清空）。
- [ ] UNIT-privileged：
  - 一次一个（`busy`）；冷却逐次加长与重新计数；
  - Esc 不能触发；没有真实输入时返回 `user_activation_required`；
  - 确认框的默认按钮与内容（用可替换的确认框函数记录调用参数）；
  - `add_project` 的三条分支（`auto_empty`、信任、`digest_mismatch` 后重新确认）。
- [ ] 主进程在调用时才取 `dialog.showMessageBox`、`dialog.showOpenDialog` 这类函数，不在模块加载时解构保存（CLAUDE.md 的代码约束）。
- [ ] Q26 的五项结论写进 `notes.md`。
- [ ] MAN-keyboard 的清单写好，汇报里列为待作者确认。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T077 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T077-desktop-input-privileged/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T077 的行；开工前逐行读一遍）

- T031：原生确认框按 5.3.11 末尾"T031 实现时的裁定"里各 action 的 `details` 键取内容，文案由界面按语义码组织。
- T076：真实输入检查的位置（`window-bridge.ts` 的 `#send`）；preload 的 `requestPrivileged` 现状。

## 注意

- 生产代码里不加任何测试分支：e2e 走的是生产的 inputGuard（5.7.8 测试一节）。
- 不运行真实的 Claude Code、Codex CLI；需要宿主时用 mock 宿主或假 agent。
- 不读、不改用户的 `~/.claude`、`~/.codex`。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超线就开工前停下汇报，并给出拆分建议**（例如 inputGuard 与证据写入一份，privileged 与 `add_project` 一份）。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条），推算贴线也要停下。

## 参考资料

- 设计：`07-desktop.md` 5.7.2、5.7.3、5.7.8；`05-flows.md` 里添加项目与信任的时序图；`03-protocol-v1.md` 5.3.11；`08-security.md` 5.8.4（提权确认框的内容）；`13-open-issues.md` Q26。
- `docs/tasks/` 下 T074、T075、T076、T007、T031 的 `notes.md` 和 `review.md`。
- `apps/desktop/src/main/`、`apps/desktop/src/preload/`、`packages/shell-api/`。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T077-desktop-input-privileged main`。T115 的分支已经合并，不要再往它上面提交。该 worktree 的 `target/` 刚清理过；本任务主要是 TS，但 `cargo xtask ci pr` 第一次会重新构建 Rust。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,335–1,560 行，在写代码之前停下，建议拆成两份。

**裁定：采用两分法（计划 v1.3.26）。**
- **T077 收窄为真实输入与 Q26**：
  - inputGuard 的纯逻辑与接线（`before-input-event`、鼠标、焦点、`focusedFrame`）；
  - `#send` 里写入证据；`command_id` 重发表；
  - 获得焦点的时间改用同一时钟；
  - 给 T134 用的"消耗一次真实输入"接口；
  - Q26 复测；MAN-keyboard 清单。
  - 验收：UNIT-input-guard、桥的证据写入用例、Q26 结论、MAN-keyboard 清单。
  - 规模改为 M，上限约 800 行，硬停止线 900；推算贴线也先停下。
- **新增 T134（特权操作与原生确认框）**：
  - privileged 框架（一次一个、冷却、真实输入）；
  - 七种 action 各自的取内容与发命令；
  - `add_project` 流程；确认框文本；文案；接上 preload。
  - UNIT-privileged 由本任务移过去。
  - T134 依赖 T077；T016、T090 改为也依赖 T134。
  - T077 合并后由同一个执行者接着做。
- `tests/ids.toml` 已在 main 上改好：T077 段留 UNIT-input-guard、MAN-keyboard，UNIT-privileged 在 T134 段。

**对核对结果的答复**：
1. **入方向认出宿主的响应**：用 host-client 的 `FrameDecoder` 跟住帧边界，只在重发表非空时才解析 JSON 取 `id`。认可。要确认两点：
   - 跟帧边界不改变现有的 16 ms 攒批和流控（"超过 8 MB 暂停读取"）；
   - 解析失败不影响转发。
2. **获得焦点的时间改用可注入的时钟**：认可。
3. **Q26 复测在临时目录装 `playwright-core`**：认可。请求里不带个人信息；复测脚本作为物料放任务目录（脚本里不写本机路径和用户名），临时目录用完删除。
4. **`bundled_cli`、`settings`**：都归 T134，届时按 T133 是否已合并处理。
