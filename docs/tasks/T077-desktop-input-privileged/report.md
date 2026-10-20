# T077 汇报

- 执行者：子 agent（工程师；T004、T005、T094、T028、T075、T007、T056、T115 的执行者接手）
- 分支：`feat/T077-desktop-input-privileged`
- 完成日期：2026-10-20

## 做了什么

按裁定收窄后的范围完成：真实输入的识别与证据写入、Q26 复测、MAN-keyboard 清单。新增 608 行、删除 21 行，在约 800 行的上限内。CI 通过。

**1. inputGuard**（`apps/desktop/src/main/input/guard.ts`，纯逻辑，时钟由调用方传入）
- 第 1 条：键盘只认 `before-input-event` 的 keyDown；鼠标认同一按钮的 mouseDown 接 mouseUp，相隔不超过 1 秒。两者都只在主框架有焦点时记录。
- 第 2 条：Enter、Space、Escape、⌘Enter、数字 1–9 算真实输入，数字键记成 `digit` 证据。
  - 带 ⌘、⌃、⌥ 的（⌘Enter 除外）和自动重复的不算。
  - ↑ ↓ ← → 不算真实输入，也不算打字，但会清掉还没用掉的输入。
- 第 3 条：其他键算打字，会清掉还没用掉的输入；打字后 1 秒内的 Enter、Space、数字键不算，⌘Enter 仍算。
- 第 6 条：输入法组合中的按键不记录，并按打字处理。
- 第 7 条：窗口获得焦点后 1 秒内的按键不算；获得或失去焦点时清掉还没用掉的输入。
- 第 4 条：`take()` 用掉一次输入，不超过 1 秒才返回证据。

**2. 接线**（`input/index.ts`）
- `watchInput(window, now)` 接 `before-input-event`、`input-event`（鼠标）、focus 和 blur。
- 判断主框架用 `focusedFrame.frameTreeNodeId` 与 `mainFrame.frameTreeNodeId` 比较，不依赖是否同一个对象。
- `FOCUS_CHANGED` 里的时间改由 inputGuard 的时钟给出，替换了 `shell.ts` 原来的 `followFocus`。
- 给 T134 的接口是 `guardOf(webContents)?.take()`。

**3. 证据写入与 `command_id` 重发表**（`window-bridge.ts` 的 `#send`，在白名单检查之后）
- **消耗**：任何命令都会用掉还没用掉的输入。这里的"命令"指幂等方式为 `command` 的方法；读请求和通知不消耗，理由见"关键取舍"。
- **`interaction/respond`、`git/revert`**：
  - 要求有输入，没有就在本地拒绝，返回 `forbidden/user_activation_required`；
  - 有输入时，主进程把证据写进 `input`，覆盖渲染进程自己填的值。
- **重发表**：
  - 按 `command_id` 记下参数哈希（去掉 `input`）和当时用的证据，收到宿主的响应就删除；
  - 上次转发后宿主没有回应（例如连接断了）时，同参数的重发不需要新输入；
  - 一个文档实例对应一个桥，文档变了表自然是空的。
- **入方向认出响应**：
  - 用 host-client 的 `FrameDecoder` 跟帧边界，只在重发表非空时才解析 JSON 帧取 `id`。
  - 您要求确认的两点：
    - 16 ms 攒批和超过 8 MB 暂停读取都没变，字节按原样转给窗口，已有的 UNIT-bridge-flow 用例照常通过；
    - 解析失败只换一个新解码器，不影响转发，有专门用例。

**4. Q26 复测**（Electron 44.9.0，playwright-core 1.63.0）
- 脚本 `q26/main.cjs` 和 `run.mjs` 与结果 `q26/result.json` 放在任务目录。
- playwright-core 装在临时目录，用完已删；脚本里没有本机路径和用户名。

| # | 结论 |
|---|---|
| 1 | Playwright 的键盘输入**不触发** `before-input-event`，但产生 `input-event` 的 keyDown 和 keyUp；页面收到的 keydown 是 `isTrusted: true` |
| 2 | `sendInputEvent` **触发** `before-input-event`（`key` 和 `code` 都是 `Enter`）；页面的 keydown 是 `isTrusted: true` |
| 3 | `input-event` 的键盘事件运行时**带键码**：有 `key`、`code`、`isComposing`、`isAutoRepeat`、修饰键、`location` 等字段，类型定义里没有。可以留作后备 |
| 4 | `page.click()` **产生**同一按钮（`left`）的 `mouseDown` 和 `mouseUp` |
| 5 | `focusedFrame` **可靠**：焦点在主文档时它和 `mainFrame` 是同一个对象，`frameTreeNodeId` 也相同；焦点在 iframe 时两者都不同 |

对设计的影响：现有测法成立。e2e 的按键经 `sendInputEvent` 发出，走生产代码的 inputGuard；Playwright 的键盘输入不会被记成真实输入。请据此更新设计里 Q26 这一行。

**5. MAN-keyboard 清单**：写在 `notes.md` 里，有四项：Enter 批准、Esc 拒绝、数字键选择并提交、输入法选词时不触发。另附两项可以顺手确认的。**待作者确认**；其中涉及请求面板的部分要等 T083 做出面板才能完整执行。

## 关键取舍

- **"命令"取幂等方式为 `command` 的方法**：渲染进程在后台不断发读请求（订阅、翻页）。如果读请求也消耗输入，用户按下 Enter 到 `respond` 发出之间，输入可能已经被一条读请求用掉。
- **按键边界情况**：
  - 单独的修饰键不改变任何状态，否则 ⌘Enter 里先到的 ⌘ 会清掉输入。
  - 焦点不在主框架时，按键和点击整个忽略，也不算打字，与设计 RP-11 的说明一致。
  - 激活冷却只针对键盘：激活窗口的那次点击按 macOS 默认不送到页面。
- **重发时写回原来的证据**：重发表只记需要真实输入的两个方法。
- **入方向旁听的代价**：每个入方向字节在主进程里多一次复制（`FrameDecoder` 的拼接），留给 T078 的测量闸门一并评估。

## 验证方式

- **UNIT-input-guard**：在 Vitest 的假时钟上，九条规则里本任务负责的部分各有用例：
  - 白名单与数字键证据；修饰键与重复键；主框架与点击；
  - ↑ ↓ 之后的 Enter 仍算、之前那一下被清掉；
  - 打字后 1 秒；一次只能用一次与 1 秒时效；输入法；激活冷却。
  - 另有一条接线用例：焦点时间来自同一时钟，审查文档有焦点时不记录，`frameTreeNodeId` 相同的另一个对象仍算主框架。
- **桥的证据写入**：
  - `respond` 和 `revert` 带上主进程写的证据，或在本地被拒；
  - 命令消耗输入，读请求和通知不消耗；
  - 没有回应的命令换连接后重发不需要新输入，收到回应后就要了，新文档实例的表是空的；
  - 读不了的帧照样按原样转给窗口。
  - 已有的白名单用例改为带上输入，使需要输入的两个方法也能通过。
- **`cargo xtask ci pr --record docs/tasks/T077-desktop-input-privileged/ci/` 通过**，摘要 `pr-a55a5a3a47cf.json` 已提交。

  | 部分 | 通过数 |
  |---|---|
  | rust | 1754 |
  | protocol | 185 |
  | ts | 366 |
  | license | 1 |
  | testdata-scan | 3 |

- **规模**（相对分支起点）：新增 608 行、删除 21 行，其中测试 348 行。逐模块的数字在 `notes.md`。

## 提交列表

```
0a0bb87 docs(task): record the T077 pr CI summary
a55a5a3 docs(task): record the T077 decisions, the Q26 retest and MAN-keyboard
03ed6a3 feat(desktop): write the input evidence into the window's commands
16e9215 feat(desktop): record each window's real input in an input guard
d624891 docs(task): estimate the narrowed T077
f82f3a2 docs(task): estimate T077 and propose a split before starting
```

## 遗留问题 / 范围外发现

1. **给 T134**：
   - 用 `guardOf(event.sender)?.take()` 取证据；Esc 不能触发特权操作，由 T134 判断证据的 `key` 是不是 `escape`；
   - `bundled_cli` 和 `settings` 按裁定归 T134。
2. **MAN-keyboard 待作者执行**，执行后按 `tests/ids.toml` 末尾的格式登记证据。
3. **分支没有同步 main**：基于 2273343，没有包含 0a53f70 的计划 v1.3.26 和 `tests/ids.toml` 的改动。本分支 T077 段里还留着 UNIT-privileged 一行，合并时以 main 为准。需要我同步时请告诉我。