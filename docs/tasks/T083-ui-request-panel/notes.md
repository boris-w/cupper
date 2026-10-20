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
