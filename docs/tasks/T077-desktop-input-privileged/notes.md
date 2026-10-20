# T077 工作笔记

- 执行者：子 agent（工程师；T004、T005、T094、T028、T075、T007、T056、T115 的执行者接手）

## 开工前核对现状

- **`window-bridge.ts`（T076）**：
  - 入方向的字节不解析，16 ms 攒一批整块转给窗口。
  - 规则 9 要求"收到任何响应就删除重发表里的记录"，所以桥要能认出发给自己的响应。做法：用 host-client 的 `FrameDecoder` 跟住帧边界；只在重发表非空时解析 0x01 帧的 JSON，取 `id`。
  - 重发表放在桥里：桥按文档实例建、随导航关闭，"文档实例变化时清空"自然成立。
- **`shell.ts`**：`followFocus` 已经在 `FOCUS_CHANGED` 里带上时间（`Date.now()`）。要改成和 inputGuard 用同一个可注入的时钟。
- **preload**：`requestPrivileged` 现在直接返回 `{status: "failed", error: "unsupported"}`。要新开一个 IPC 通道，主进程只答主文档（同 `shell.ts` 的 `checked`）。
- **`dialogs.ts`**：已有的写法是在调用时取 `dialog.showMessageBox`、文案走 i18n，确认框照这个写。
- **协议**：
  - `policy/describe` 返回 `{action, details}`，`details` 的键按 T031 的裁定；
  - `interaction/get` 给出请求原文和选项（`name`、`effect`）；
  - `project/add` 返回 `trust: {state, findings, trust_digest}`，`project/trust_info` 用于重新确认；
  - `auth/elevation_get` 返回 `verified`、`self_reported`、`scopes`；
  - `VerifiedRequester` 还没有 `bundled_cli`（T133 没有合并），按现有字段实现，这一项写进交接；
  - 宿主没有"设置"的方法，`settings` 返回 `failed`。
- **i18n**：现有 namespace 是 `command`、`host`、`update`；确认框的文案新开一个 `privileged`。
- **Playwright**：仓库里没有装（e2e 基础设施是 T090）。Q26 复测要在临时目录装 `playwright-core`，用它的 `_electron` 拉起 Electron 44；复测脚本放在本目录作为物料。

## 开工前的规模估计

行数指格式化后的新增行，含注释和空行。"草估"是逐模块的估计；实现按近几个任务的实际偏差乘 1.3。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| inputGuard（纯逻辑） | 第 1–7 条：按键分类（白名单、数字键、导航键、打字键、修饰键）、鼠标按下接松开、只认主框架焦点、窗口激活后的冷却、消耗与 1 s 时效；时钟可注入 | 150 |
| inputGuard 接线 | `before-input-event`、`input-event`（鼠标）、`focus`/`blur`、`focusedFrame`；`FOCUS_CHANGED` 改用同一个时钟 | 45 |
| 证据写入（`window-bridge.ts` 的 `#send`） | 任何出方向命令都消耗输入；`interaction/respond`、`git/revert` 没有输入时在本地返回 `user_activation_required`；覆盖写入 `input`；`command_id` 重发表（参数哈希），入方向用 `FrameDecoder` 认出响应后删除 | 80 |
| privileged 框架 | 一次一个（`busy`）；按 kind 冷却 2/10/60 s 与重新计数；真实输入（Esc 不行）；IPC 与 preload | 90 |
| 各 action 的取内容与发命令 | `interaction`、`set_config`、`confirm_config`、`create_session`（`policy/describe`，`interaction` 另取 `interaction/get`）；`trust_project` 与 `add_project`（选目录、`project/add`、findings、`digest_mismatch` 后重新取内容再确认）；`elevation`（核实与自述、approve 单独勾选）；`settings` 返回 `failed` | 230 |
| 确认框的文本 | 按 action 组织 message 和 detail；不可见字符转义；默认按钮是取消或拒绝；工作区外的链接目标醒目标出；`dialog.*` 在调用时取 | 90 |
| 文案 | 新 namespace `privileged.json`（约 45 个键）和注册 | 60 |
| **实现小计** | | **745**；乘 1.3 约 **970** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| UNIT-input-guard | 九条规则各一组；数字键证据；导航键清掉未用的输入、其后的 Enter 仍算；输入法；冷却；`command_id` 重发表 | 220 |
| 证据写入（桥） | 需要真实输入的方法；覆盖渲染进程填的 `input`；消耗；重发表随响应删除、跨连接保留 | 110 |
| UNIT-privileged | `busy`；冷却逐次加长与重新计数；Esc；没有输入；各 kind 的确认框参数与默认按钮（替换确认框函数记录参数）；`add_project` 三条分支；elevation 勾选项；`settings` | 260 |
| **测试小计** | | **590** |

另有不计入产品代码的物料：Q26 复测脚本约 80 行；MAN-keyboard 清单写在本文件。

**合计约 1,335 行**（草估）到 **1,560 行**（实现乘 1.3），贴着约 1,500 行的线，偏高的估计超线。按任务书，开工前停下汇报，没有写代码。

## 拆分建议

按任务书举的拆法：

| 拆分 | 模块 | 用例 | 估计 |
|---|---|---|---|
| **T077a 真实输入与证据写入** | inputGuard 纯逻辑与接线；`#send` 里的证据写入与 `command_id` 重发表；`FOCUS_CHANGED` 的时间；Q26 复测；MAN-keyboard 清单 | UNIT-input-guard；桥的证据写入用例 | 实现约 275，乘 1.3 约 360；测试约 330；合计约 **690** |
| **T077b 特权操作与原生确认框** | privileged 框架（一次一个、冷却、真实输入）；各 action 的取内容与发命令；`add_project` 流程；确认框文本；`privileged` 文案；preload 接上 | UNIT-privileged | 实现约 470，乘 1.3 约 610；测试约 260；合计约 **870** |

两份的先后关系：
- T077b 要用 T077a 的"消耗一次真实输入"接口：`requestPrivileged` 要求有不超过 1 秒、尚未用掉的输入，Esc 除外。
- 所以 T077a 先做，T077b 接着做。
- T090（e2e 基础设施）依赖的是真实输入的整条链路，T077a 完成后就能开始；e2e 的确认框用例要等 T077b。

## 裁定后的范围与估计（2026-10-20）

编排者采用两分法（计划 v1.3.26）：本任务收窄为真实输入与 Q26，上限约 800 行，硬停止线 900；特权操作归新的 T134。

| 模块 | 草估 |
|---|---|
| inputGuard 纯逻辑（含给 T134 的 `take()`） | 150 |
| 接线（`before-input-event`、`input-event`、焦点、`focusedFrame`；`FOCUS_CHANGED` 用同一时钟） | 45 |
| 证据写入与 `command_id` 重发表（`#send`；入方向用 `FrameDecoder` 认响应） | 80 |
| **实现小计** | **275**，乘 1.3 约 **360** |
| UNIT-input-guard | 220 |
| 桥的证据写入用例 | 110 |
| **合计** | 约 **605–690** |

两点约束（编排者要求确认）：
- 跟帧边界只是旁听：入方向的字节照旧进 16 ms 的批，按原样转给窗口，流控（超过 8 MB 暂停读取）不变；
- 解析失败只影响重发表，不影响转发。

## 实现中定下的几点

- **"命令"的范围**：
  - 第 4 条说"任何一条出方向命令都会用掉尚未消耗的输入"。这里的"命令"取协议里幂等方式为 `command` 的方法，也就是带 `command_id` 的那些；读请求和通知不消耗输入。
  - 原因：渲染进程在后台不断发读请求（订阅、翻页），要是读请求也消耗输入，用户按下 Enter 和 `interaction/respond` 发出之间，输入可能已经被一条读请求用掉。
- **按键分类**：
  - 输入法组合中的键算打字键：不记录，并且按第 3 条压住其后 1 秒内的 Enter、Space 和数字键，用 Enter 选词之后紧接着的 Enter 也不算。
  - 单独的修饰键（Shift、⌘、⌃、⌥、CapsLock、Fn）什么都不改变，否则按 ⌘Enter 时先到的 ⌘ 会清掉输入。
  - 带 ⌘、⌃、⌥ 的白名单键（⌘Enter 除外）和自动重复的白名单键不记录，也不算打字。
  - Shift 不影响判断：Shift+数字得到的是符号，本来就不在白名单里。
- **焦点不在主框架时**（例如在审查文档里），按键和点击整个不处理，也不算打字。这与设计 RP-11 的说明一致："审查文档里的按键主进程不记录"。
- **激活冷却**：只压键盘。窗口激活的那次点击按 macOS 默认不送到页面（`acceptFirstMouse` 为 false），所以点击不受冷却限制。
- **重发表**：
  - 只记需要真实输入的两个方法（`interaction/respond`、`git/revert`）。记录的内容是：参数哈希（去掉 `input` 后按键名排序的 JSON 的 sha-256）和当时用的证据；重发时写回原来的证据。
  - 记录在桥里：一个文档实例一个桥，文档变了，表自然是空的。
  - 换连接时，等待中的请求 `id` 清空，记录保留。这正是规则 9 要放行的情形：连接断了，宿主还没回应。
- **入方向的帧**：
  - `FrameDecoder` 只是旁听。字节照旧进 16 ms 的批，按原样转给窗口，流控不变：已有的 UNIT-bridge-flow 用例照常通过。
  - 解析失败只换一个新的解码器，不影响转发，有用例覆盖。
  - 代价：每个入方向字节在主进程里多复制一次（`FrameDecoder` 的拼接）。留给 T078 的测量闸门一并评估。
- **给 T134 的接口**：
  - `guardOf(webContents)?.take()`：用掉一次输入；不超过 1 秒时返回证据 `{kind, key?, digit?}`，否则返回 `undefined`。
  - "Esc 不能触发特权操作"由 T134 自己判断：看取到的证据里 `key` 是不是 `escape`。

## Q26 复测（Electron 44.9.0，playwright-core 1.63.0）

脚本和原始结果在 `q26/`：
- `main.cjs`：一个带按钮、输入框和同源 iframe 的窗口，记录 `before-input-event`、`input-event`，以及页面 keydown 的 `isTrusted`。
- `run.mjs`：用 Playwright 的 Electron 支持拉起它，逐项操作，输出 `result.json`。

运行方式：`node run.mjs <装了 playwright-core 的目录> <Electron 可执行文件>`。复测时 playwright-core 装在临时目录，用完已删。

| # | 问题 | 结论 |
|---|---|---|
| 1 | Playwright 的键盘输入是否触发 `before-input-event` | **不触发**。`page.keyboard.press("Enter")` 没有产生 `before-input-event`；但产生了 `input-event` 的 keyDown 和 keyUp，页面收到的 keydown 是 `isTrusted: true`。与 Electron 34 上的结论相同 |
| 2 | `sendInputEvent` 是否触发它，页面的 keydown 是否 `isTrusted` | **触发**。`sendInputEvent({type: "keyDown", keyCode: "Return"})` 产生了 `before-input-event`，`key`、`code` 都是 `Enter`；页面收到的 keydown 是 `isTrusted: true`。`input-event` 里对应的是 `rawKeyDown` |
| 3 | `input-event` 的键盘事件运行时是否带键码 | **带**。字段有 `key`、`code`、`isComposing`、`isAutoRepeat`、`shift`、`control`、`alt`、`meta`、`location`、`modifiers`（另有内部的 `_modifiers`），类型定义里只有 `type` 和 `modifiers`。可以留作后备，但设计仍只用 `before-input-event` 识别键盘 |
| 4 | `page.click()` 是否产生 `input-event` | **产生**。同一按钮（`left`）的 `mouseDown`、`mouseUp` 各一次 |
| 5 | 用 `webContents.focusedFrame` 判断主框架是否可靠 | **可靠**。焦点在主文档的输入框时，`focusedFrame` 与 `mainFrame` 是同一个对象，`frameTreeNodeId` 也相同；焦点在 iframe 的输入框时两者都不同（`url` 为 `about:srcdoc`）。实现按 `frameTreeNodeId` 比较，不依赖对象同一 |

对设计的影响：
- e2e 的按键照设计经 `sendInputEvent` 发出，走生产的 inputGuard；Playwright 的键盘输入不会被记成真实输入。
- 点击用 `page.click()`。

## MAN-keyboard（待作者确认）

每次升级 Electron 大版本，用实体键盘在开发构建里各做一次，结果按 `tests/ids.toml` 末尾的格式登记证据（`versions = { electron = "<当前版本>" }`）。

前提：请求面板（T083）和 e2e 之外的真实会话；可以用假 agent 产生一个权限请求。下面两件事要等 T083 做出请求面板才能完整执行，本任务只能把清单写好：
- 用 Enter 和数字键提交请求面板；
- 面板上的输入法选词。

1. **Enter 批准**：
   - 面板出现后，等待命期过去（约 0.5 秒），按 Enter；
   - 选中的选项被提交，宿主接受；
   - 在主进程日志或宿主审计里能看到这次回答带着 `input: {kind: "key", key: "enter"}`。
2. **Esc 拒绝**：
   - 面板出现后按 Esc，请求以拒绝或 `dismissed` 结束；
   - 带 `input.key: "escape"`。
3. **数字键选择并提交**：
   - 按某个选项的编号（例如 2）；
   - 提交的就是 `options` 里第 2 项，带 `input: {kind: "key", key: "digit", digit: 2}`；
   - 按一个效果未知的选项的编号，不起作用。
4. **输入法选词时不触发**：
   - 切到中文输入法，在输入框里输入拼音，面板出现时用 Enter 选词；
   - 选词不会提交面板，也不会被记作真实输入；
   - 选词后立即按 Enter，也不算（1 秒内）。

另外两项可以顺手确认：
- 窗口刚激活（从别的 App 切回来）1 秒内按 Enter 被拒，面板提示"请再按一次"；
- 刚在输入框打完字立即按 Enter，不被记作真实输入。

## 实际规模

| 模块 | 新增 / 删除 | 草估 |
|---|---|---|
| inputGuard 纯逻辑、接线、焦点时间（提交 16e9215，不含测试） | 179 / 11 | 195 |
| 证据写入与重发表（提交 03ed6a3，不含测试） | 82 / 5 | 80 |
| 测试（UNIT-input-guard 的纯逻辑和接线；桥的证据写入；已有用例的调整） | 348 / 6 | 330 |
| **合计**（相对分支起点） | **608 / 21** | 约 605–690 |

Q26 的复测脚本和结果是物料，在 `q26/` 下，不计入上表。
