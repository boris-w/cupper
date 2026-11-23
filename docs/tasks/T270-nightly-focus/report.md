# T270 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`fix/T270-nightly-focus`
- 完成日期：2026-11-23

## 做了什么

T270 做完了：nightly 里两个 focus 用例反复失败，原因都在测试，产品不用改。两项各连跑 20 次通过；分支上的 nightly `job=e2e` 94 个用例全部通过；PR #130 的 8 个检查也都通过了。

- **E2E-NFR1-3**（`7ff40989`）：
  - **原因**：焦点从侧边栏的会话按钮出发，要按 11 下 Tab 才能进请求面板，测试最多只按 10 下。
  - **多出来的一下从哪来**：T265 给侧边栏每个会话行加了"会话菜单"按钮（fd9864c5）。对照通过的 577c4c42，顶栏那三个按钮当时就有；T266 加的标题是 span，不进 Tab 顺序。
  - **为什么不改产品**：按 RP-3，焦点在别处时面板不抢焦点，用户按 Tab 或点击进入面板；Tab 走的是文档顺序（侧边栏 → 顶栏 → 消息流 → 面板），这是对的。
  - **改法**：没有调大上限，而是改成沿页面实际的焦点顺序一直按 Tab，直到面板进入 `data-activation="auto"`。如果焦点回到已经经过的元素还没进面板，就报错，并列出经过的每一站。调用方式仍是 `activatePanel(cupper)`，T170 不用改。
- **E2E-presence-suppresses-notification**（`7b0ed8a8`）：
  - **现象**：三晚都挂在同一步。点了审查文档里的"收起"以后，frame 的 `hasFocus()` 一直是 false，而且"收起"按钮仍是展开状态，说明这次点击没有生效。
  - **原因**：pierre 在 hunk 到达和高亮完成之后会重建文件头。如果按下鼠标后、松开前文件头被换掉，click 不会触发，焦点也跟着丢了。和 T260 查到的是同一个过程。
  - **改法**：照 T260，点击之前先等 `[data-line] span[style]` 出现，也就是文档画完。没有加 sleep。
- **E2E-review-comment**（只出现过一次）：那一晚高亮 20 秒都没画完，和 T260 修的不是同一种情况。按任务书只记录，没有修。
- **快照里"没有名字的按钮"**：它是 `packages/ui-core/src/rows/basic.tsx` 的 `TurnHeaderRow`，即"用时 {时长}"那个折叠按钮。我在本机用一次性探针核对过（没有提交）：`ariaSnapshot()` 读到它的名字是"用时 0 秒"，只是 error-context 用的 AI 模式快照不打印这类名字。不是缺陷。

## 关键取舍

- 不调大 Tab 上限，改成"绕一圈"作为停止条件：以后页面上再加按钮，测试照样能通过；面板真的到不了时，报错会写清焦点经过了哪里。
- presence 用例只等界面状态（高亮的行出现），不加固定的等待时间。

## 验证方式

- **验收标准**
  - **两项都给出了原因，各自在 runner 上连跑 20 次通过**：
    - E2E-NFR1-3：运行 45117822336，修的两个 focus 用例共 40 passed，同组的 @smoke 用例 20 passed。
    - presence：运行 45117830471，20 passed。
  - **分支上的 nightly e2e 通过**：
    - 第一次（45121719952）focus 部分全部通过，但 rest 部分 E2E-SEC-review-iso 失败了一次：拿不到 diff，面板显示"无法加载变更"。这个用例和本任务的改动无关。
    - 把 review-iso 单独连跑 20 次（45124199800），20 次全部通过，你按偶发处理。
    - 合并 main 后重跑（45126730426）：`94 passed, 0 failed`，`test-e2e` 和 `test-e2e-focus` 都通过。
  - **没有为通过而加 sleep**：只等界面状态。
  - **PR 上 8 个检查全部通过**：PR #130 https://github.com/boris-w/cupper/pull/130 ，运行 https://github.com/boris-w/cupper/actions/runs/45125865786 （头 f822202d，已含 T271 的修复）。
- **本机检查**：只跑了 tools/e2e 的 tsc、eslint、prettier。focus 用例本机没有跑。
- **规模**：+30/−8 行，停止线 150。

## 提交列表

```
f822202d Merge branch 'main' into fix/T270-nightly-focus（编排者合入 main，含 T271）
7b0ed8a8 test(e2e): the presence case clicks the review document once it is drawn
7ff40989 test(e2e): activatePanel follows the page's focus order to the panel
7064e44e docs(task): T270 causes and estimate
```

## 遗留问题 / 范围外发现

- **E2E-review-comment**：只出现过一次，那一晚高亮 20 秒都没画完。如果再出现，要查审查文档的高亮 worker 为什么会卡住。
- **E2E-SEC-review-iso**：只出现过一次"无法加载变更"，连跑 20 次没有复现。你已按偶发处理，并记在评审里。
- **pierre 重建文件头时会吞掉点击**：hunk 到达、高亮完成后，pierre 会重建文件头。真实用户如果正好在这一刻点了"收起"，这一下点击也会丢。影响很小，记下来，交给以后做审查文档的任务评估。
- **e2e 不录 trace**（`retries: 0`，失败时也不留 trace）：这次只能靠 error-context 的快照推断原因。是否在失败时保留 trace，可以考虑。
