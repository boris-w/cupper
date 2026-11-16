# T153 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`feat/T153-session-ui`（PR #124，577c4c42）
- 完成日期：2026-11-16

两个用例在 runner 上各连跑 10 次全部通过，分支 nightly 记下了 `ui_switch_next_needs_action`（p50 44 ms），PR 8 个检查全绿。规模 +632 −64，在调整后的停止线 650 以内。重命名、归档、在终端中打开三个快捷键没有接，归 T265。

## 做了什么

**1. 应用菜单（M1）**
- 先按 F2 静态读了 ChatGPT.app 的菜单模板和中文菜单文案：没有启动 App，临时目录已删，仓库里没有放任何解出的文件。
- 分组照 Codex：App 菜单、文件、编辑、视图、窗口。
  - 帮助菜单不加：Codex 的帮助菜单里全是 OpenAI 的链接。
  - 服务菜单不加：设计 5.7.8 规定不要。
- 重命名、归档、模型菜单、⌃1–9 做成隐藏的加速键，不在菜单上显示。Codex 也是这样处理的。
- `@cupper/i18n` 新增 `menu` 命名空间，包括顶层标题、全部角色项和缩放项，文案里带上 App 名字，菜单上不再有 Electron 的英文。
- 原来放在 `command` 命名空间里的三个缩放文案移到了 `menu`。

**2. 快捷键（M2）**
接上的命令：⌘.、⌘⌥A、⌘⌥[ / ⌘⌥]（以及 ⌘⌥← / ⌘⌥→）、⌃1–9、⌃Tab / ⌃⇧Tab、⌃⇧M。
- **侧边栏的上一个 / 下一个**：照 Codex，只走侧边栏上显示出来的行，到两头就停。
- **⌃1–9**：取显示出来的第 n 行。
- **⌘⌥A**：按侧边栏顺序找下一个需要处理的会话，到尾从头再找。"展开显示"后面的会话也算，跳过去时展开它所在的项目。
- **⌃Tab**：照 Codex 的"切换聊天…"。UI store 记最近访问的会话；按住 ⌃ 连按时沿开始那一刻的顺序走，⌃⇧Tab 往回走；⌃ 抬起、点击或窗口失焦时，停在哪个会话就算访问了哪个。所以单按一下就是在最近两个会话之间来回。
- **⌘.**：当前会话正在运行或在等待处理时中断，和停止按钮效果一样。
- **⌃⇧M**：在会话页上开关输入框的模型菜单。
- 侧边栏的顺序抽成了纯函数 `sidebar/order.ts`；"展开显示"的状态从组件的局部状态移到了 UI store，让快捷键和侧边栏看到的是同一组行。
- 接收命令的处理函数用 `useEffectEvent` 写，只订阅一次。

**3. PERF-ui-switch（M9）**
- 新增一项 `ui_switch_next_needs_action`：在十个等待请求的会话之间按"下一个需要处理的会话"绕圈。
- 第一圈不计，之后计 30 次。此时每个目标都是十次切换之前看过的，已经不在 LRU 里。

**4. 用例**
- E2E-shortcuts、E2E-next-needs-action，写在 `tools/e2e/src/shortcuts.spec.ts`，都标 `@focus`。

## 关键取舍

- **E2E 怎么触发快捷键**：在主进程里找到持有该加速键的菜单项，调用它的 `click()`，命令沿着和按键相同的菜单路径发出；按键匹配到菜单项是系统的事。菜单只把命令发给有焦点的窗口，所以两个用例都要 `@focus`。
- **⌃Tab 的"⌃ 抬起"**：用 `sendInputEvent` 发一个 `keyUp Control`，等页面确认收到后再按下一次。输入事件和 IPC 走两条通道，不等的话顺序没有保证。
- **PERF 的触发方式**：PERF 用例不拿焦点，菜单发不到窗口，所以照菜单的方式直接把命令发给窗口的 `shell:command`，从主进程开始计时。
- **重命名、归档、在终端中打开**：按你的更正，这三个由 T265 用它的 `useSessionActions` 自己接。我这边没有接，也没有放占位文件；菜单里照旧注册加速键、照旧发命令。
- **规模**：M2 超出估算，因为 ⌃Tab 的按住连按和侧边栏顺序的单测估少了。到 471 行时我停下汇报，你选了 A（停止线 650，硬线 750）。

## 验证方式

- **本机**：
  - `pnpm lint` 通过。
  - ui-core 176 项、desktop 127 项（另有 2 项跳过）、i18n 11 项通过。
  - `trace-ids check` 通过（461 个 ID 与文档一致）。
  - e2e 的 tsc 通过。
  - 本机没跑 E2E 和 PERF。
- **验收标准**：
  - [x] 两个 E2E 在 runner 上各连跑 10 次，日志里都是 "10 passed"：
    - E2E-shortcuts：https://github.com/boris-w/cupper/actions/runs/43826668924
    - E2E-next-needs-action：https://github.com/boris-w/cupper/actions/runs/43826672524
  - [x] PERF-ui-switch 的新项在分支 nightly 里有记录（https://github.com/boris-w/cupper/actions/runs/43826675957 ，e2e 作业 92 项通过、0 失败）。四项结果：

    | 项目 | p50 | p95 | max |
    |---|---|---|---|
    | `ui_switch_next_needs_action` | 44 ms | 57 ms | 75 ms |
    | `ui_switch_not_in_lru` | 62 ms | 176 ms | 281 ms |
    | `ui_switch_in_lru` | 24 ms | 38 ms | 53 ms |
    | `ui_switch_from_notification` | 247 ms | 255 ms | 255 ms |

    "下一个"同样不在 LRU 里，却比 `not_in_lru` 快。原因应该是等待请求的会话只有几行，而 `not_in_lru` 用的是 5 个回合×50 项的预置会话。这一项只做记录。
  - [x] 截图基线：截图会拍到的界面没有改，没有跑 ui-baseline。
  - [x] PR 上 8 个检查全部通过：https://github.com/boris-w/cupper/pull/124
- **规模**：`cargo xtask size` 为 +632 −64，在 650 以内。brief 里的停止线还写着 500，所以 size 会报超线；你说合并时改成 650。

## 提交列表

```
577c4c42 docs(task): T153 notes on the E2E cases and the PERF item
1df399dc test(perf): PERF-ui-switch times the next session that needs action
ac9963a2 test(e2e): E2E-shortcuts and E2E-next-needs-action
538a207d refactor(ui-core): read the shortcuts' state through an effect event
16187123 docs(task): T153 notes on the split, Codex's menu and the shortcuts
32b1fdea feat(ui-core): wire the shortcuts that move between sessions
1596546d feat(desktop): group the menu as Codex's, with cupper's text for its roles
```

## 遗留问题 / 范围外发现

- **归 T265**：`archive`（⌘⇧A）、`rename`（⌘⌥R）、`open_in_terminal`（⌘⌥T）三个快捷键的处理，以及 E2E-shortcuts 里对它们的测试。
- **合并冲突**：T265 也改了 `sidebar/parts.tsx`，谁后合并都会有冲突。我这边改了三处：
  - `SHOWN_SESSIONS` 和两个排序函数移到了 `sidebar/order.ts`；
  - "展开显示"改读 UI store 的 `expanded`；
  - 去掉了 `useState`。
- **已由你处理**：设计 5.7.7 补一句"菜单按 Codex 分组"（你合并时加）；项目行上那句注释 "The menu's items come with T153." 已转给 T265。
- **Codex 键位差异，只记录**：较新的 Codex 把 ⌃Tab 改成了切换标签，最近会话的切换默认不再占按键。cupper 按设计 5.7.7 保留 ⌃Tab 切换最近会话；M1 只有一个标签，不冲突。
