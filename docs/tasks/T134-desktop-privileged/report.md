# T134 汇报

- 执行者：子 agent（工程师；T077 的执行者接着做）
- 分支：`feat/T134-desktop-privileged`
- 完成日期：2026-10-20

## 做了什么

特权操作与原生确认框已全部接上：preload 的 `requestPrivileged`、七种 action 和 `add_project`、提权确认框。验收用例 UNIT-privileged 在 8 个用例里覆盖；`cargo xtask ci pr` 通过。规模新增 933 行、删除 20 行，在放宽后约 1,100 行的上限内。

**1. 入口**
- preload 的 `requestPrivileged` 经新的 `shell:privileged` 通道送到主进程，主进程只回应主文档。
- 一进来先 `guardOf(event.sender)?.take()`：不管后面是 `busy` 还是冷却，这次输入都算用掉（第 4 条）。
- 确认框挂在发起请求的窗口上。`dialog.showMessageBox`、`dialog.showOpenDialog` 都在调用时才取。`ShellAPI.pickDirectory` 和 `add_project` 共用同一个选目录函数。

**2. privileged 框架**（`apps/desktop/src/main/privileged/index.ts`）
- 没有真实输入，或证据的 `key` 是 `escape`：返回 `user_activation_required`。
- 同一时间只有一个确认框，其他请求返回 `busy`。
- 用户取消后，同一类操作依次冷却 2 s、10 s、60 s，之后保持 60 s，返回 `cooldown` 和 `retry_after_ms`。一次确认，或距上次取消满 10 分钟，重新计数。时钟可注入。
- 渲染进程传来的 action 先按 kind 核对字段，不符合就返回 `failed`（`invalid_params`）。

**3. 七种 action**（`actions.ts`；内容从宿主经 control 连接取，`host.ts`）

| action | 取内容 | 确认后发出 |
|---|---|---|
| `interaction` | `interaction/get` + `policy/describe` | `interaction/respond`，`input` 为 `native_confirm` |
| `set_config` | `policy/describe` + `session/get` | `session/set_config` |
| `create_session` | `policy/describe` | `session/create`，结果带 `session_id` |
| `confirm_config` | 按裁定 (a)，用 `SessionView` 订阅一次会话流，等到 `live` 取 `session.configured` 后退订；失败或 5 秒没到就返回 `failed`，不弹框 | `session/confirm_config` |
| `trust_project` | `project/trust_info` | `project/trust`；遇到 `digest_mismatch` 重新取内容、再确认 |
| `elevation` | `auth/elevation_get` | `auth/elevation_respond`：允许时发 `grant`，`scopes` 只含申请范围内的，approve 要勾选才有；拒绝时发 `deny` |
| `settings` | — | **返回 `failed`（`unsupported`）**：宿主的 `settings/update` 在 T036 之前都是 `unsupported` |

- 要经 `policy/describe` 的命令：先拼出整条命令，带上新的 `command_id`，再取描述；确认后用同一个 `command_id` 发出。
- 不需要原生确认的命令，宿主的 `policy/describe` 回 invalid params，原样作为 `failed` 返回。

**4. `add_project`**
- 选择目录 → `project/add`。宿主判定扫描范围里没有项目配置时（`auto_empty`）直接完成，不弹框。
- 有项目配置时，确认框逐项列出 findings，醒目标出链接到工作区外的目标，默认按钮是"不信任"。
- 用户信任后发 `project/trust`；遇到 `digest_mismatch` 就重新取内容，确认框注明"确认期间项目配置又变了"，再问一次。

**5. 确认框的文本**（`dialogs.ts`，文案在 i18n 新开的 `privileged` 命名空间）
- 默认按钮和按 Esc 的效果都是取消、拒绝或不信任。
- agent 和申请方的文本逐字显示，不可见字符用 `escapeInvisible` 转义。
- 持久选项按 5.3.6：没有规则文本和写入位置时写"由 agent 决定，以原文为准"；命令前缀是解释器、或者解析不出前缀时，各附一句风险说明。
- 提权框：先列宿主核实过的信息（可执行文件、签名或"未签名"、进程号、父进程链、"无法确认来源"、申请的权限），再列"申请方自述，未经核实"（名称、理由各截到 200 字）。approve 单独一个勾选项，默认不勾。

## 关键取舍

- **`confirm_config` 的内容从会话流取**（裁定 (a)）：要拉一次会话快照，换来确认框的内容全部来自宿主。
- **`add_project` 时拒绝信任**：项目已经加上了，所以返回 `confirmed` 加 `{project_id, trusted: false}`，不计冷却。选目录时取消才算 `cancelled`。
- **提权框拒绝时也通知宿主**（发 `deny`）：否则申请要挂着等 2 分钟过期。拒绝计冷却。
- **失败的形式**：宿主的错误按语义码返回 `{code, reason?}`；连接断开返回 `connection_lost`；其他异常返回 `unexpected`。

## 验证方式

**UNIT-privileged**：8 个用例，在 `privileged.test.ts`。用 mock 宿主加真实的 HostClient，确认框函数替换成记录调用参数的版本。

| 用例 | 覆盖 |
|---|---|
| 真实输入与一次一个 | 没有输入、Esc → `user_activation_required`；一个框没关时再请求 → `busy` |
| 冷却 | 2、10、60、60 s 逐次加长，别的 kind 不受影响；满 10 分钟后从头计；一次确认后从头计 |
| `interaction` | 请求原文和选项名（含双向控制符，显示为转义）；按钮"允许、拒绝"、默认拒绝；规则未知和解释器前缀两句说明；`respond` 与描述用同一个 `command_id`、带 `native_confirm`；选项不存在时返回 `failed` |
| `set_config`、`create_session` | 显示宿主描述的模式和会话标题；取消时不发命令；`create_session` 返回 `session_id` |
| `confirm_config` | 从会话流快照里取 digest 和原因；订阅失败时返回 `failed`，不弹框 |
| `elevation` | 核实信息在自述之前；"无法确认来源"；自述转义并截到 200 字；approve 默认不勾；`grant` 的 `scopes` 不含没勾的 approve；拒绝时发 `deny` |
| `add_project` 三条分支 | `auto_empty` 不弹框；有配置时逐项列出、标出工作区外链接、默认"不信任"；`digest_mismatch` 后重新取内容并再确认；选目录时取消 |
| `settings` 与格式不对的 action | 返回 `failed`（`unsupported`、`invalid_params`） |

- i18n 的用例改为包含新的命名空间。
- `cargo xtask ci pr --record docs/tasks/T134-desktop-privileged/ci/` 通过，摘要 `pr-02e9cc8b7c4d.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1776 通过 |
  | protocol | 185 通过 |
  | ts | 382 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |

- 规模：实现约 640 行（草估 580），测试约 295 行（草估 300）。逐模块的数字在 `notes.md`。

## 提交列表

```
b459606 docs(task): record the T134 pr CI summary
02e9cc8 docs(task): record the T134 decisions and line counts
3ad1417 feat(desktop): privileged actions behind native confirmation dialogs
b2920be feat(i18n): the text of the privileged actions' native dialogs
929ecd0 docs(task): estimate T134 and raise the confirm_config gap before starting
```

## 遗留问题 / 范围外发现

1. **`bundled_cli` 还没接上**（交接给 T133 之后）：
   - `VerifiedRequester.bundled_cli` 还不存在（T133 没有合并），提权框里"申请方不是本 App 自带的命令行工具"这一项没有做。
   - T133 合并后，在 `dialogs.ts` 的 `elevationBox` 里加一行醒目提示即可，文案加在 `privileged` 命名空间。
2. **宿主还没有方法的 action**：只有 `settings`。T036 接上 `settings/update` 后，按 `policy/describe`（`security_setting`）→ 确认框 → `settings/update` 补上，结构与 `set_config` 相同。
3. **`trust_project` 的确认框里没有项目路径**：现有的读方法都不给路径（只有 `project_id`），框里只列 findings。需要的话，可以让 `project/trust_info` 带上路径。
4. **`create_session` 的确认框只显示 agent 和模式**：`policy/describe` 的 `details` 有 `project_id`，没有项目名或路径，同上。
5. **分支没有同步 main**：基于 f8d5dec，`c6a3f51`（本任务的补充裁定）不在分支上。合并时 brief 以 main 为准。需要同步时请告诉我。