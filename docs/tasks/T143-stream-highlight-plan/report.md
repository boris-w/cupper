# T143 汇报

- 执行者：子 agent（前端工程师，ui-core 消息流）
- 分支：`feat/T143-stream-highlight-plan`（PR https://github.com/boris-w/cupper/pull/46，头提交 0bff31f）
- 完成日期：2026-11-03

T143 的界面部分做完了：代码高亮、计划条目、计划药丸，以及错误条目里的恢复指引。PR #46 在 0bff31f 上 8 项检查全部通过。E2E-error-guidance 按你的裁定移到 T226，原因是宿主目前不写 `error` 条目。

## 做了什么

1. **代码高亮**（MS-1）
   - 依赖：`@shikijs/primitive`、`@shikijs/engine-javascript`、`@shikijs/langs`，都是 4.5.0、MIT，锁了精确版本。
   - 用 JavaScript 正则引擎，不加载 WASM，CSP 一点没放宽。
   - 懒加载：Shiki 和每种语法都是动态 import。release 构建里它们各自成块，入口块里只留引用。支持 37 种语言、18 个别名，其余语言照原样显示。
   - 时机：streamdown 的 `useIsCodeFenceIncomplete()` 为真（围栏还没闭合）时原样显示，围栏闭合后高亮一次。结果按"语言 + 文本"缓存 200 条，行滚出再滚回时不重做。超过 5 万字符的代码块不高亮。
   - 配色：自定义主题给 5.7.10 的七个类别各一个占位色，占位色只用来查类别。页面上只出现固定的 `hl-*` class，颜色取已有的 `--color-code-*` token，亮暗两套都由 token 给出。没有内联样式，也没有 CSS 自定义属性。
   - 语言：sanitize 的 schema 只放行已知语言的 `language-*` class，用来选语法，不写进 DOM。
2. **计划**（MS-4）
   - 有步骤的计划显示为编号步骤，每步一个 16 的状态图标（未开始、进行中、完成）。不认识的状态显示为未开始。
   - 只有 Markdown 的计划显示为"计划"摘要条，进行中写"编写计划"并带闪光，可以展开。blob 形式的计划在展开时才下载。
3. **计划药丸**（F3）
   - 最后一个回合在运行、并且有带步骤的计划时，显示"第 n / m 步"。n 取进行中的那一步；没有进行中的，取第一个未完成的；全部完成取 m。
   - 点击后滚到计划条目，并停止贴底。药丸用 `--shadow-pill` 和 elevated 底色。
   - 药丸放在新组件 `<ComposerOverlays/>` 里，"回到底部"按钮也挪了进去，两者在同一行居中并排，不会重叠。
4. **恢复指引**（MS-5）：错误条目用警告框，guidance 映射成框里的按钮或文字，下一节列出每种的入口。

## 每种 guidance 的入口

| guidance | 界面 | 入口归属 |
|---|---|---|
| `retry` | 会话最后一个回合里是"重试"按钮；更早的回合里只写"重新发送消息即可重试。" | 本任务 |
| `run_login_in_terminal` | 文字，加上 agent 的登录命令（Claude 是 `claude`，Codex 是 `codex login`）和"复制命令"按钮 | "打开终端"的入口在 T153（`workspace/open` 在 T126） |
| `upgrade_cli`、`install_cli` | 只有文字 | 设置页的 agent 状态在 T151 |
| `quota_exhausted` | 只有文字 | 没有对应入口 |
| `change_model` | 只有文字 | 模型选择器在 T141 |
| `restart_agent` | 只有文字 | `agents/restart` 的界面入口在 T151 |
| `none`、未知取值 | 不加任何内容 | 没有对应入口 |

**retry 怎么重发**：经 `session/send`（`mode: queue`，与输入框相同），重发这个回合里第一条非插话的用户消息。内容块原样发出，文字、`@` 文件、图片、长文本 blob、审查评论都一起重发。图片和长文本是已经上传过的 blob 引用，不重新上传。按钮只出现在会话最后一个回合里；重发后新回合开始，旧错误不再是最后一回合，按钮随之消失。发送失败时弹 toast，与输入框的提示相同。

## 关键取舍

- **用 `@shikijs/primitive`，不用 `shiki` 或 `@shikijs/core`**：`shiki` 带 WASM 引擎和全部主题；core 带 `codeToHtml` 用的 hast-util-to-html。这些都用不上。
- **不用 Shiki 的双主题输出**：它靠内联样式和 CSS 变量。改成"占位色 → 固定 class"，颜色只来自 5.7.10 的表。
- **`<ComposerOverlays/>` 的位置**：药丸和"回到底部"都要操作消息列表的滚动，所以由列表在自己框的底部绘制，没有做成 SessionPage 的直接子节点。这样不需要跨组件传滚动接口，看上去仍在输入框正上方。
- **类别到 TextMate scope 的对应**（例如 storage 算 keyword，`entity.name` 和 `support.function` 算 name）是我定的，运算符和标点保持正文颜色。

## 验证方式

- **CI**：PR #46 在 0bff31f 上 8 项检查全部通过：https://github.com/boris-w/cupper/actions/runs/41599210548 。你重开 PR 之前的那次运行（41597235482）里只有 rust 失败，是 T224 正在修的偶发失败。
- **四个用例**：
  - UI-code-highlight：截图（亮、暗），外加组件测试：
    - 七个类别的映射；
    - 只出现 `hl-*` class，没有 style 属性，也没有 `language-*` class；
    - 流式中围栏未闭合时不高亮，闭合后高亮；
    - 未知语言保持原样。
  - UI-plan：截图分两组。一组是步骤加药丸：先断言计划已滚出视口，点药丸后回到视口，"回到底部"按钮出现。另一组是 Markdown 摘要条展开。组件测试覆盖状态图标和不认识的状态、摘要条和"编写计划"、药丸 n/m 的取法。
  - UI-error：截图覆盖每种 guidance 加一个未知取值。组件测试覆盖：
    - 每种 guidance 的文字；
    - none 和未知取值不加内容；
    - Codex 的登录命令和"复制命令"；
    - retry 只出现在最后一个回合，重发的内容块原样。
  - E2E-error-guidance：按裁定移到 T226，本任务不交。
- **安全约束**：
  - 没有 innerHTML 一类写法。
  - 每个截图用例都在主文档的 CSP 下断言违规数为 0。
  - Shiki 相关的包里扫过，没有 `eval` 或 `new Function`。
  - agent 给出的字符串不进入样式。
  - ui-core 没有 import electron 或 `node:*`。
  - 文案都走 i18n，字体子集检查通过。
- **本机**：
  - ui-core 100 个单元测试、41 个截图用例、prettier、全仓库 eslint、`tsc --build` 都通过；trace-ids check 也一致。
  - `vite build --mode release` 加 license-gate 没有问题。
  - 都只跑一次，没有跑性能或高负载测试。

## 截图基线（按计划 1.5）

- **改动的原有基线**：stream-markdown 的亮、暗两张。原因只有代码高亮：Markdown 样例里那个 ```sh 代码块现在着色了，其余区域不变。
- **新增 8 张**：stream-code、stream-plan-steps、stream-plan-markdown、stream-error，各有亮、暗两张。
- **来龙去脉**：
  1. 一开始我在本机生成基线。你指出基线只能由 runner 上的 ui-baseline 生成，于是换成运行 41595337219 的构件。构件里只有 stream-code 两张和本机不同；meta.json 的 macOS 由 15.7.9 变为 15.8.2（runner 镜像升级）。其余基线逐字节相同，没有动。
  2. stream-code 不同的原因：样例代码块的注释里有中文，截图页的等宽字体栈没有中文字形，这些字退到了系统中文字体，而系统中文字体随 macOS 版本不同。
  3. 按方案 (a)，截图页 `fonts.css` 的 `--font-mono` 在 Menlo 之后、`monospace` 之前接上夹具里的 Noto 子集，产品的字体栈没改。现有基线里，代码中有中文的只有 stream-code。
  4. 再用运行 41596611977 的构件更新这两张。现在 runner 的图和本机渲染只在约 100 个像素上差一级色阶，低于比对阈值。
- 这些基线之后由 T152 统一更新（计划 1.5）。

## 提交列表

```
0bff31f test(ui): stream-code baselines with Chinese in the fixture font
9769f4b test(ui): Chinese in code takes the fixture font in the screenshots
8161584 test(ui): the T143 baselines from the ui-baseline run on the runner
6360bd6 docs(task): T143 notes
fe6845c feat(ui-core): recovery steps in the error box
bf93293 feat(ui-core): plan rows, and the plan pill above the composer
42c1553 feat(ui-core): highlight finished code blocks with Shiki
```

**规模**：`cargo xtask size` 为 +972 −40，其中实现约 550、测试约 420。越过原停止线 900 时我停下报告过，你把停止线放宽到 1000，硬停止线 1100 不变。分支上的任务书还是旧的，所以工具仍按 900 报 `!!`。

## 遗留问题 / 范围外发现

- **宿主不写 `error` 条目**：已由 T226 处理，E2E-error-guidance 跟着移过去。
- **"回到底部"按钮的两个旧问题（T082 起就有，我没改）**：
  - 用的 `bg-bg` 没有对应 token，背景是透明的，应该是 `bg-surface` 一类；
  - 尺寸是 32，设计写的是 28。
- **设计修改建议**：
  - 5.7.5 组件树：注明 `<ComposerOverlays/>` 由消息列表在底部绘制，"回到底部"也在其中，与药丸同行并排。
  - 5.7.10 代码高亮表：补一句"只取这七类，运算符和标点用正文色"，并把类别对应的 TextMate scope 记下来（现在在 `rows/highlight.ts`），以后 T106 核对时有据可查。
  - 5.7.6 长会话：补上实现细节——高亮结果按文本缓存、超过 5 万字符的代码块不高亮。
  - 计划 1.8 截图基线：写明截图页的字体栈要整套固定（无衬线和等宽都带夹具的中文子集），代码里出现中文时才不依赖系统字体。
- **代码块按钮**：没有做代码块的复制按钮之类的控件，任务书没有要求。

notes.md（规模估计、实现记录和 guidance 入口表）已在分支上提交，位置 `docs/tasks/T143-stream-highlight-plan/notes.md`。
