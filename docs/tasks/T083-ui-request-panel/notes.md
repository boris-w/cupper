# T083 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T083 的行：T086（`ComposerArea` 的替换位置、`composer/keys.ts`）、T128（行类型注册表，可共用一个实例）、T080（`useStreams().session(id)`、`useEntry`）、T070（测试里的不可见字符用 `String.fromCodePoint` 生成）；另有"作者（MAN-keyboard）"一行：实体键盘确认四项，涉及请求面板的部分等本任务。任务书另列 T077（主进程认定真实输入、`digit` 证据）与 T134（`requestPrivileged` 的返回值）。
- `tests/ids.toml` 的 T083 段：UNIT-request-panel、UI-request-panel。
- 数据：会话视图的 `requests`（`RequestView {requested, resolved?}`），新请求进 `tail`（touch `tail`），解决时 touch `request:<id>`。请求类型 `permission`（`title`、`tool {tool_kind, command?, paths}`、`detail?`、`options`、`content_truncated`）、`question`（`prompt`、`fields[] {field_id, label, type, choices?, allow_custom, required}`、`unsupported_schema`）、`plan_confirm`（`plan`、`options`）、`auth_url` 和未知类型。选项 `{option_id, name, kind, requires, effect {scope, persistent, ends_turn?, ...}}`。
- 回答：`interaction/respond {session_id, request_id, request_digest, outcome}`，`outcome` 为 `selected {option_id}`、`answer {values}`、`dismissed`；输入证据由主进程按真实输入填写。特权选项（`requires: privileged`、`effect.scope: unknown`）只经 `requestPrivileged({kind: "interaction", ...})`。
- 目前没有共用的行类型注册表实例（T082 还没做）；请求记录行需要一个共用实例，由本任务在 `rows/` 下建，T082 照用。

## 规模估计（开工前）

按格式化后的写法估，不含 notes、lock、截图基线。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 面板外壳与接线 | `ComposerArea` 里替换 `Composer`；按创建顺序取待处理的请求；标题行与"关闭"（Esc 键帽）；外观同输入框 | 70 | — |
| 待命与激活 | RP-1 待命期（按键重新计时、`data-ready`）、RP-2 只认待命后的 `mousedown`、RP-3 三个条件与 `data-activation`、打字键撤销、第一下 ⌘Enter、不抢焦点、RP-8 连续请求重新待命、RP-9 有弹出层时不响应 | 140 | — |
| 选项列表 | RP-4 初始选中、RP-6 编号等于序号、方向键跳过效果未知的选项、数字键选中即提交、标注（持久、结束回合、效果未知、需要确认）、不可见字符转义 | 110 | — |
| 三类请求的内容 | 权限：按工具的问句、命令全文与展开、文件列表、"已截断"与展开前允许类不可选；计划确认：计划全文；提问：字段（单选带编号、多选复选框、文本）、多题切换、`unsupported_schema` 只显示"关闭"；未知类型与 `auth_url` 的占位 | 210 | — |
| 回答 | "提交""跳过""关闭"到回答的映射（纯函数）；`interaction/respond`；特权选项走 `requestPrivileged`；`user_activation_required` 时按 `onFocusChange` 的时间区分"请再按一次"与撤销；别处已处理时收起并 toast | 170 | — |
| RP-12 | "拒绝并附一条消息"：自由输入行；拒绝成功后才发跟进消息（`reply_to`，M1a 一律排队）；`already_resolved`、`expired` 时文字放回输入框并提示 | 80 | — |
| 请求记录行 | 共用的行类型注册表实例；"已批准：…""已拒绝""已询问 n 个问题""未回答""已失效" | 60 | — |
| 多请求 | "1 / 3" | 20 | — |
| 文案 | i18n 键 | 45 | — |
| UNIT-request-panel | 待命、焦点三条件、撤销、不抢焦点、初始选中、映射、第一下 ⌘Enter、编号与序号、特权与效果未知、"请再按一次"只在冷却期、RP-12 的顺序与失败 | — | 350 |
| UI-request-panel | 三类请求、待命 / 可以操作 / 手动激活 / 多请求，两种主题 | — | 120 |
| 合计 | | 905 | 470 |

合计约 **1,375 行**。以往的实际偏差在 −6% 到 +17%，推算约 1,375–1,600，超过约 1,500 的上限的可能性很大。按任务书，开工前停下汇报。

## 拆分建议

照任务书给的思路：

| 部分 | 内容 | 估计 |
|---|---|---|
| T083（面板与 RP-1 至 RP-9） | 面板外壳与接线、待命与激活、选项列表、三类请求的内容与占位、回答（映射、`respond`、特权、"请再按一次"、别处已处理）、文案；UNIT-request-panel 的对应部分；UI-request-panel（单个请求的各状态、两种主题） | 约 1,150（推算 1,150–1,350） |
| 新编号（RP-12、请求记录行、多请求） | "拒绝并附一条消息"的自由输入行与 RP-12 的顺序和失败处理；共用的行类型注册表实例与请求记录行；"1 / 3"与连续请求的截图；对应的单元用例和截图 | 约 250 |

- 两份都在 `request/` 目录里，后一份只在面板上加自由输入行和计数，改动面小，可以紧接着做。
- T091、T093 的 e2e 需要两份都在；T016 只依赖面板的结构和 DOM 标记，前一份合并就够。
- 不拆的话，上限需要提到约 1,700。

## 裁定（计划 v1.3.34）

采用拆分：本任务做面板与 RP-1 至 RP-9、三类请求的内容与占位、回答的映射（含特权、只在冷却期提示"请再按一次"、别处已处理时 toast）、文案；验收 UNIT-request-panel（不含 RP-12）、UI-request-panel（单个请求的各状态，两种主题）。上限约 1,300，硬停止线 1,400。RP-12、共用的行注册表实例与请求记录行、多请求的"1 / 3"归 T165。

## 进度（截图用例之前）

面板与 UNIT-request-panel 已提交，改动 **1,168 行**（实现约 859，测试 309）。

- `request/logic.ts`（纯函数）：`handled`（面板处理的三类）、`isPrivileged`（`requires: privileged` 或效果未知）、`isEnabled`（截断时只有拒绝类可选）、`initialSelection`（RP-4）、`move`（方向键跳过效果未知和不可选的项）、`skipOutcome`（不持久的拒绝类选项，否则 `dismissed`）、`panelKey`（数字、方向、Enter、⌘Enter、Esc、Tab、打字键）。
- `request/RequestPanel.tsx`：待命（500 ms，按键重新计时，`data-ready`；每个请求重新挂载，连续请求重新待命）、点击只认待命后按下的（RP-2）、激活（RP-3 三条件，`data-activation`，打字键撤销，Tab 或点击恢复，第一下 ⌘Enter 只提示）、有菜单或对话框时不响应（RP-9）、标题与"关闭"（Esc 键帽）、按类型分派。
- `request/kinds.tsx`：选项列表（编号等于序号、标注、↑↓ 键盘提示）、"跳过""提交"（Enter 键帽）、权限内容（按工具的问句、终端框可展开、文件、说明）、计划全文、提问（逐题、单选数字即选即进、多选复选框、自由输入"在此输入"、上一题 / 下一题、最后一题提交或 `dismissed`）。
- `request/respond.ts`：`interaction/respond`；缺真实输入时按窗口获得焦点的时间区分"请再按一次"（1 秒内）和撤销激活；其他失败 toast。窗口获得焦点的时间在 `ShellProvider` 里从第一次渲染起记录，面板出现前的激活也算得上。
- `ComposerArea`：有待处理的请求时显示面板（按创建顺序取第一个），否则显示输入框；输入框经 `activity` 报告焦点和最近一次输入的时间；面板显示过、后来被别的客户端处理的请求，收起并 toast"已在 {客户端} 处理"。

截断的内容（`content_truncated`）：只显示"已截断：内容没有完整显示，只能拒绝"，允许类选项不可选。展开全文要下载 blob，客户端还没有下载接口，留给后续（T164 一带）。

剩下 UI-request-panel：三类请求 × 待命、可以操作、手动激活，两种主题，估约 60–90 行，合计推算约 1,230–1,260，贴近约 1,300 的上限，按裁定先停下。

## 截图与收尾（裁定：按现在的范围写完）

- UI-request-panel（`tools/ui-screenshots/src/request.spec.ts`）：权限请求、键盘可操作（输入框有焦点时作为实时事件到达），亮色、暗色；同一请求等待 Tab 或点击（加载时就在）；计划确认；提问。每个用例都先断言 `data-ready` 和 `data-activation`。顺手修了两处显示：主按钮上的 Enter 键帽改用按钮自身颜色（原来在深色底上看不见），终端框加了边框（原来和输入框同色）。
- 规模：合计 **1,270 行**（实现约 900，测试约 370），在硬停止线 1,400 以内。
- 审阅截图：`screenshots/`，截图页换回系统字体后截取，1280 × 820，与 UI-request-panel 同样的五个场景。按 F2，没有查看本机 Codex App 包。

## "展开看全文"的接入点（交 T165 评估）

- 截断的判断：`request/RequestPanel.tsx` 的 `Body` 把 `request.content_truncated` 作为 `truncated` 传给 `OptionsKind`；允许类选项是否可选由 `request/logic.ts` 的 `isEnabled(option, truncated)` 决定（初始选中、方向键、数字键、点击都经过它）。
- 展开：`request/kinds.tsx` 的 `Terminal` 已有"展开 / 收起"。`blob` 类的 `Content`（命令 `tool.command`、说明 `detail`）现在由 `text()` 返回 `undefined`，不显示。接上 host-client 的 `getBlob` 后：`Terminal` 在展开时按 `BlobRef` 下载全文并显示（转义同样走 `escapeInvisible`），全部内容都看过之后，`Body` 改传 `truncated={false}`（例如在面板里记一个"已看过全文"的状态）。

## 需要作者用实体键盘确认的几项（MAN-keyboard，涉及请求面板的部分）

Playwright 和 `sendInputEvent` 都不经过系统的按键事件，下面几项要在 e2e 构建里用实体键盘确认：

1. **Enter 批准**：输入框有焦点、为空、2 秒没打字时来一个权限请求，等约半秒，按 Enter，选中的"允许"被批准。
2. **Esc 拒绝**：同样的请求按 Esc，回答的是不持久的拒绝选项；有"本会话内都允许"这类特权选项时，Esc 不会弹原生确认框。
3. **数字键选择并提交**：按选项的编号直接提交；按特权选项的编号弹原生确认框；按"效果未知"选项的编号没有反应。
4. **输入法**：在输入框里用中文输入法选词时按 Enter，不发送消息；在提问的"在此输入"里选词时按 Enter，不提交回答。
5. 另外两项（属于面板自己的规则，建议一并看）：输入框里正在打字时来请求，面板不抢焦点、按键不落到面板上；切到别的应用再切回来，1 秒内按 Enter 被拒时面板提示"请再按一次"，过了 1 秒再按就能提交。
