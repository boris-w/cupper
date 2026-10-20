# T083 汇报（ui-core V：请求面板，面板与 RP-1 至 RP-9）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T083-ui-request-panel`
- 完成日期：2026-10-20

请求面板已完成：有待处理的请求时，它在 `ComposerArea` 里替换输入框，RP-1 至 RP-9 都已落实。UNIT-request-panel、UI-request-panel 通过，UI-csp-zero 为 0，`ci pr --record` 通过，摘要已提交。规模 1,270 行，在硬停止线 1,400 以内。RP-12、请求记录行和多请求的"1 / 3"按裁定归 T165。

## 做了什么

**面板外壳与接线**
- `ComposerArea` 里有待处理的请求时，按创建顺序取第一个显示面板，否则显示输入框。排队消息留在上方，草稿和附件留在 `useUi()`，切回输入框时原样还在。
- 面板外观与输入框一致；标题行右侧是"关闭"，带 Esc 键帽。
- 请求被别的客户端处理后，面板收起并 toast"已在 {客户端} 处理"。
- `ShellProvider` 从第一次渲染起记录窗口获得焦点的时间，所以面板出现之前那次窗口激活也能用来判断冷却期。

**RP-1 至 RP-9**（`request/RequestPanel.tsx`、`request/logic.ts`）
- RP-1 待命：500 ms 待命期，期间每次按键都重新计时并吞掉；结束后设 `data-ready`。
- RP-8 连续请求：每个请求单独挂载，下一个请求会重新待命。
- RP-2 点击：只认待命结束之后才按下的点击。
- RP-3 激活：
  - 自动获得焦点需要三条同时成立：焦点在输入框里、输入框没有文字和附件、2 秒内没有打字。成立时 `data-activation="auto"`，焦点移到选项列表；否则为 `manual`，提示"按 Tab 或点击以处理请求"，不抢焦点。
  - 打字键撤销键盘激活，按 Tab 或点击恢复。
  - 第一下 ⌘Enter 只提示"按 Enter 提交"，不提交。
- RP-4 初始选中：第一个既不是特权、也不是效果未知的选项。
- RP-5 按键：
  - 数字键选中并提交；
  - ↑↓ 移动选中，←→ 在提问的多个字段之间切换；
  - Enter 提交；
  - Esc 关闭；焦点在有内容的自由输入框里时，第一下 Esc 只把焦点移回选项列表。
- RP-6 编号与效果未知：编号等于选项在 `options` 里的序号，特权选项和效果未知的选项也占编号，不重新排序。方向键跳过效果未知的选项，它只能点击。
- RP-7 特权选项：只经 `requestPrivileged({kind: "interaction"})`，Esc 永远不会选中它。
- RP-9 遮挡：有菜单或对话框打开时，面板不响应按键。

**内容**（`request/kinds.tsx`）
- 权限：按工具写问句（运行命令、编辑文件、其他）；命令放在"终端"框里，可展开收起；另列文件和说明。
- 计划确认：显示计划全文。
- 提问：
  - 一次一个字段；
  - 单选按数字即选中并进入下一题，多选用复选框，另有"在此输入"；
  - 有上一题、下一题；最后一题提交 `answer`，一题都没答时回答 `dismissed`。
- 选项标注："效果未知""需要确认""会写入 agent 的权限规则""拒绝将结束本回合"。
- 只能关闭（回答 `dismissed`）的情况：`auth_url`、未知类型、`unsupported_schema` 的提问。
- agent 给出的文本一律经 `escapeInvisible` 转义，只作为文本节点渲染。

**截断的内容**：`content_truncated` 时显示"已截断：内容没有完整显示，只能拒绝"，允许类选项不可选。

**回答**（`request/respond.ts`）
- 跳过、关闭按 5.7.4 的表映射：选不持久的拒绝类选项，没有就回答 `dismissed`；提问的关闭一律是 `dismissed`。
- 回答经 `interaction/respond` 发出。
- 因缺真实输入被拒时：窗口获得焦点不到 1 秒，提示"请再按一次"；其他情况撤销键盘激活，回到手动激活。
- 其他失败弹 toast。

**DOM 标记**：`data-request-panel`、`data-request-kind`、`data-ready`、`data-activation`。

**文案**：新增 `request` 命名空间。

## 关键取舍

- **每个请求单独挂载**：靠 React 的 `key`，让连续请求天然重新待命（RP-8），不需要另外维护状态。
- **自动获得焦点的判断时机**：输入框在被面板替换之前，把焦点和最近一次打字的时间写进 `activity`，面板挂载时读取。原因是输入框卸载后已无法再判断"焦点曾在输入框里"。
- **窗口焦点时间放在 `ShellProvider`**：如果在面板里才订阅，面板出现之前的那次窗口激活就记不到，"请再按一次"会错判成撤销。
- **"效果未知"一律算特权选项**：键盘对它无效，只能点击，点击后走原生确认框。
- **截断内容只能拒绝**：比"允许但看不全"保守，符合安全 #6。展开全文需要 blob 下载，见下文的接入点。
- **`ComposerArea` 订阅整个会话视图**：新请求会落在回合里或 tail 里，各自 touch 不同的键，用整体订阅最不容易漏。代价是流式输出时这一区域每帧重绘一次。

## 验证方式

- **UNIT-request-panel**（9 条，jsdom，MockHost 连真的 `HostClient`，用假时钟控制待命与冷却），覆盖：
  - 纯规则：初始选中、方向键跳过、跳过的映射（持久拒绝时回答 `dismissed`）、按键分类；
  - 待命期按键重新计时，待命前按下的点击无效；
  - 自动获得焦点的三个条件，以及有草稿、刚打字、焦点在别处时一律为手动；
  - 打字键撤销，获得焦点后恢复；
  - 编号与序号：按效果未知选项的编号无效，按特权选项的编号走 `requestPrivileged`，按拒绝选项的编号直接回答；
  - 第一下 ⌘Enter 只提示，↓ 之后提交；Esc 回答不持久的拒绝，从不触发特权选项；
  - "请再按一次"只在冷却期内出现，冷却期外撤销激活；
  - 提问逐题作答，`unsupported_schema` 只能关闭；
  - 别的客户端处理后收起并 toast。
- **UI-request-panel**（5 张）：权限请求在键盘可操作状态下的亮色和暗色；同一请求等待 Tab 或点击的状态；计划确认；提问。每个用例都先断言 `data-ready` 和 `data-activation` 的值。看图时修了两处：主按钮上的 Enter 键帽原来在深色底上看不见；终端框原来和输入框同色，加了边框。
- **其他检查**：UI-csp-zero 为 0；T136 的规则通过；其余截图基线没有变化。
- **审阅截图**在 `docs/tasks/T083-ui-request-panel/screenshots/`：截图页换回系统字体后截取，1280 × 820，与 UI-request-panel 相同的五个场景。
- **F2**：没有查看本机的 Codex App 包。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T083-ui-request-panel/ci/`，全部通过，摘要 `pr-df356e106411.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,845 通过 |
  | protocol | 185 通过 |
  | ts | 440 通过 |
  | license | 通过 |
  | testdata-scan | 通过 |
  | ui | 21 通过 |

  e2e 作业还没实现。用 `git merge-tree` 试合最新的 main，没有冲突。
- **规模**：1,270 行（实现约 900，测试约 370；不含 notes、lock、截图基线）。

## 提交列表

```
d27886b docs(T083): record the ci pr summary
df356e1 docs(T083): record the screenshots, the expand access point and the keyboard checks
d73d880 test(ui): add UI-request-panel
9c27d21 docs(T083): record the panel and the size before the screenshots
fa7eb6f feat(ui-core): add the request panel
b5c617d docs(T083): record the checks, the size estimate and a split proposal
```

## 需要作者用实体键盘确认的几项（MAN-keyboard）

Playwright 和 `sendInputEvent` 都不经过系统的按键事件，以下几项要在 e2e 构建里用实体键盘确认：

1. **Enter 批准**：输入框有焦点、为空、2 秒没打字时来一个权限请求，等约半秒按 Enter，选中的"允许"被批准。
2. **Esc 拒绝**：按 Esc 回答的是不持久的拒绝选项。有"本会话内都允许"这类特权选项时，Esc 不会弹原生确认框。
3. **数字键选择并提交**：按选项编号直接提交；按特权选项的编号弹原生确认框；按"效果未知"选项的编号没有反应。
4. **输入法**：在输入框里用中文输入法选词时按 Enter，不发送消息；在提问的"在此输入"里选词时按 Enter，不提交回答。
5. **建议一并看的两项**（面板自己的规则）：
   - 正在输入框里打字时来请求，面板不抢焦点，按键不会落到面板上；
   - 切到别的应用再切回来，1 秒内按 Enter 被拒时面板提示"请再按一次"，过了 1 秒再按就能提交。

清单同时记在 `docs/tasks/T083-ui-request-panel/notes.md`，可以并入 T077 notes 里的那份 MAN-keyboard 清单。

## 遗留问题 / 给 T165 的接入点

- **"展开看全文"**：
  - 截断与否在 `request/RequestPanel.tsx` 的 `Body` 里判断，它把 `content_truncated` 作为 `truncated` 传给 `OptionsKind`。
  - 允许类选项能不能选，由 `request/logic.ts` 的 `isEnabled(option, truncated)` 决定；初始选中、方向键、数字键、点击都经过它。
  - `request/kinds.tsx` 的 `Terminal` 已有"展开 / 收起"。`blob` 类的内容（`tool.command`、`detail`）现在 `text()` 返回 `undefined`，不显示。
  - 接上 host-client 的 `getBlob` 之后的做法：`Terminal` 展开时按 `BlobRef` 下载全文，同样经 `escapeInvisible` 转义后显示；全部内容都看过以后，`Body` 改传 `truncated={false}`，例如在面板里记一个"已看过全文"的状态。
- **T165 的其余部分**：
  - RP-12 的自由输入行加在 `OptionsKind` 的选项列表之后，编号为"选项数 + 1"，数字键聚焦它。回答走现成的 `answer`，成功后再发跟进消息。
  - 请求记录行用 T128 的注册表，新建共用实例。
  - "1 / 3"：`ComposerArea` 已经算出待处理请求的列表 `pending(view)`，把长度和序号传给面板的标题行即可。