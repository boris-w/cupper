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
