# T134 桌面主进程 V：特权操作与原生确认框

- 派发日期：2026-10-20
- 执行者：子 agent（Electron 工程师）。T077 的执行者接着做
- 分支：`feat/T134-desktop-privileged`
- 依赖：T077（合并中）。T016（契约冻结）、T090（e2e 基础设施）依赖本任务

## 背景

T077 开工前拆成两份（计划 v1.3.26）：
- T077 已做完真实输入：inputGuard、证据写入、`command_id` 重发表、Q26。
- 本任务在它之上做特权操作与原生确认框。

本任务的内容就是原 T077 任务书（`docs/tasks/T077-desktop-input-privileged/brief.md`）范围第 3–5 项。preload 的 `requestPrivileged` 现在仍返回 `{status: "failed", error: "unsupported"}`（T076 的交接）。

## 范围

**做**（计划 T134 一行；设计 5.7.8 的"原生确认框"与"真实输入"第 8 条、5.7.3 的 `PrivilegedAction`、5.5 里添加项目与信任的时序图、5.8.4 的确认框内容）：
1. **`privileged` 框架**：
   - preload 的 `requestPrivileged` 接上；
   - 要求真实输入：用 T077 的 `guardOf(event.sender)?.take()` 取证据；证据的 `key` 是 `escape` 时拒绝，Esc 不能触发特权操作；
   - 同一时间只弹一个确认框，其他请求返回 `busy`；
   - 用户取消后，同一类操作冷却 2 s、10 s、60 s，返回 `cooldown`；一次确认，或连续 10 分钟没有取消，重新计数；
   - 时钟可以注入。
2. **七种 action**（`interaction`、`set_config`、`confirm_config`、`create_session`、`trust_project`、`elevation`、`settings`）：
   - 各自经 control 连接从宿主取内容（`policy/describe`、`auth/elevation_get` 等），`details` 键按 5.3.11 末尾"T031 实现时的裁定"（T031 的交接）；
   - 确认后经 control 连接发出命令。
   - 宿主还没有方法的（例如 `settings`）返回 `failed`，在汇报里列出。
3. **原生确认框**：
   - 默认按钮是"取消"或"拒绝"；
   - agent 给出的文本逐字显示，不可见字符用 i18n 包已有的 `escapeInvisible` 转义；
   - 持久选项的文案规则见 5.3.6；
   - 文案走 `@cupper/i18n`（新开 `privileged` 命名空间；主进程和审查文档共用一份文案）；
   - 主进程在调用时才取 `dialog.showMessageBox` 这类函数（照 `dialogs.ts` 的写法）。
4. **`add_project` 流程**：
   - 系统的选择目录对话框 → `project/add`；
   - 扫描范围里没有文件时，宿主直接记为 `trusted`（`auto_empty`）；
   - 有项目配置时，原生确认框逐项列出 findings，醒目标出来自工作区外的链接目标，默认按钮"不信任"；
   - 用户信任时发 `project/trust`（带 `trust_digest`）；遇到 `digest_mismatch` 就重新取内容、再确认一次。
5. **提权确认框**（5.8.4）：
   - 先列宿主核实过的信息，再列"申请方自述，未经核实"；
   - approve 单独一个勾选项，默认不勾；
   - "申请方不是本 App 自带的命令行工具"醒目标出：`VerifiedRequester.bundled_cli` 由 T133 加（进行中）。T133 先合并就直接用；否则按现有字段实现，这一项写进交接。

**不做**：渲染进程的请求面板（T083）；e2e 用例（T090、T092）。

## 验收标准

- [ ] UNIT-privileged：
  - 一次一个（`busy`）；冷却逐次加长与重新计数；
  - Esc 不能触发；没有真实输入时返回 `user_activation_required`；
  - 确认框的默认按钮与内容：用可替换的确认框函数记录调用参数；
  - `add_project` 的三条分支：`auto_empty`、信任、`digest_mismatch` 后重新确认。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T134 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T134-desktop-privileged/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T077、T134 的行；开工前逐行读一遍）

- T031：原生确认框按 5.3.11 末尾"T031 实现时的裁定"里各 action 的 `details` 键取内容，文案由界面按语义码组织。
- T076：preload 的 `requestPrivileged` 现状。
- T077：`guardOf(event.sender)?.take()`；Esc 由本任务判断。

## 注意

- 生产代码里不加任何测试分支。
- 不运行真实的 Claude Code、Codex CLI；需要宿主时用 mock 宿主或假 agent。
- 不读、不改用户的 `~/.claude`、`~/.codex`。

## 规模

计划估为 L，T077 的执行者估约 870 行（实现约 610、测试约 260）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,000 行（包括贴线）时，必须先停下来汇报**；硬停止线 1,100。

## 工作方式

T077 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T134-desktop-privileged main`。T077 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 940–1,055 行，贴线；另外发现 `confirm_config` 的内容没有来源。在写代码之前停下汇报。

**1. `confirm_config` 采用 (a)**：主进程在 control 连接上用 host-client 的 `SessionView` 订阅一次会话流，等快照到齐（`live`）后取 `session.configured`（`config_digest`、需要确认的原因、当前配置），再退订。
- 理由：原生确认框的内容必须来自宿主，不能由渲染进程给出。(b) 让渲染进程提供 `config_digest`，确认框也显示不出档位和原因，失去原生确认的意义。
- `ShellAPI` 接口不变。已在设计 5.7.3 的 `PrivilegedAction` 处注明。
- 订阅失败或超时就返回 `failed`，不弹框。

**2. 规模：放宽，不拆分**：上限约 1,100 行，硬停止线 1,200；推算贴线也先停下。剩下的模块彼此依赖紧，拆开要多一轮派发，得不偿失（与 T038 的第 2 次裁定同理）。

**3. 核对结果都认可**：
- `policy/describe` 要完整参数：先拼出整条命令（带 `command_id`），取描述后用同一个 `command_id` 发出；
- 各 action 的内容来源如 `notes.md` 所列；
- `settings` 在 T036 之前返回 `failed`；
- `bundled_cli` 按现有字段实现、写进交接；
- T038 执行者提的三点按你的安排处理。
