# T244 notes

## 开工前的估算（2026-11-10）

按模块估的新增行数，开工前报给编排者：

| 模块 | 估算 |
|---|---|
| `keys.ts` 改成判定"发送 / 排队"，Composer 改用 | 约 30 |
| 斜杠菜单：`extensions/slash.tsx` 约 95；弹出层按键逻辑抽成 `composer/popover.tsx` 约 40，fileref 改用约 10；扩展组件多传 `agent` 约 4；i18n 约 3 | 约 150 |
| ↑ 恢复上一条 | 约 40 |
| 组件测试 | 约 130 |
| e2e 两条 | 约 110 |
| 合计 | 约 460 |

不抽共用逻辑、在 slash.tsx 里照抄 T142 那段按键处理，约 435。编排者选了抽出共用逻辑，停止线放到 480，硬停止线 560。

## 规模第二次报告（写完斜杠菜单时）

推算到约 530，超过 480，停下来报告。编排者把停止线放到 540，硬停止线 600，三块都留在本任务里。超出的原因：

- `slash.tsx` 比估的多约 40 行：命令有两个来源，要两个组件。会话里随 `session.capabilities` 更新（`WithSession` 加 `useEntry`）；新建会话页按所选 agent 读 `agents/capabilities` 的缓存，而且只在输入 `/` 之后才请求，免得每次打开新建会话页都发一次。
- `popover.tsx` 比估的多约 28 行：除了按键处理，光标跟踪（`useCaret`）也是两个列表都要的，一起挪了过来。

## fileref 挪到共用弹出层，行为不变

T142 的 `@` 列表改用 `composer/popover.tsx`，这次挪动不改变任何行为：

- 光标跟踪、↑↓ 选、Enter 或 Tab 取、Esc 关闭、输入法组合时不拦按键，代码原样挪过去。唯一的区别是关闭函数改成经 ref 取最新的，所以 `word.start` 变了也不用重新挂监听，效果相同。
- 外观的类名原样搬进 `POPOVER`、`OPTION`；只有高度（`max-h-72`）留在 fileref，因为斜杠菜单按设计最高 320（`max-h-80`，约 10 行）。Tailwind 的类不看先后顺序，生成的样式一样。
- 验证：composer 的单测（含 `@` 的三条）没有改动，全部通过；sidebar 那组截图（含 `composer-file-ref`）本机对着现有基线全部通过。

## 做法和取舍

- **keys.ts**：`sends()` 换成 `sendAction()`，给出 `"send"`、`"queue"` 或不处理。偏好为 Enter 时，Enter 发送、⌘Enter（连同 ⌘⇧Enter）排队；偏好为 ⌘Enter 时，⌘Enter 发送、⌘⇧Enter 排队、Enter 换行。凡是以前 `sends()` 判为真的按键，现在都给出两者之一，所以 Composer 照旧提交，行为不变；T242 只需把运行中的 `"send"` 映射成插话。
- **斜杠菜单**：
  - 只在文字开头、光标还在 `/` 这个词里时打开（`^/(\S*)$`），所以 `@` 列表和它不会同时出现。
  - 搜索只看命令名：包含搜索词的都列出，以它开头的排前面。
  - 高度：设计写最高 320 或 10 行，一行约 32px，取 `max-h-80`。
  - 搜索词没有匹配时，Enter 不被菜单拦下，照常发送 `/xxx`，由 agent 自己解释，和 `@` 列表没有结果时一样。
- **↑ 恢复上一条**：从会话视图的各回合和 tail 里找最后一条 `user_message`，取其中的 `text` 块，用换行连起来。附件、图片、评论和 `text_blob` 都不恢复。条件是没有文字、没有附件、没有浮层和确认框、不在输入法组合中、没有按修饰键；找不到文字时不拦这个键。
- **安全**：命令名、说明和 `input_hint` 都作为 React 文本节点显示，没有拼 HTML；文案走 i18n（`composer:slash_title`、`composer:slash_none`）。ShellAPI 没有改。

## 验证

- 本机：ui-core 的 vitest 全部 134 条通过；i18n 的测试、`tsc --build`（ui-core、i18n、e2e、ui-screenshots、desktop）、eslint、prettier 都通过。sidebar 和 new-session 两组截图对着现有基线全部通过，基线没有变。
- e2e 两条只在 runner 上跑（守则第 10 条），由编排者触发 e2e-repeat。

## 实际规模

相对 T145 的合并提交 2ca56f14（`cargo xtask size --main 2ca56f14`）：+494 −123。直接对 main 算会把 T145 的改动也算进来。
