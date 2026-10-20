# T093 汇报

- 执行者：子 agent（前端工程师；T085 的执行者接着做）
- 分支：`test/T093-e2e-interaction-project`
- 完成日期：2026-10-20

**结论**：
- 用例、三处缺陷修复、机器锁都已提交。
- 经新的持锁入口，有一段连续 10 次全绿。
- 带记录的 `ci pr`：除 e2e 外全过。e2e 的 `@focus` 段里 approve-enter-esc 失败，那时本机已锁屏，窗口既看不见也拿不到焦点。
- 有一件事需要你决定，写在最后一节。

## 做了什么

1. **六个编号的用例**（`tools/e2e/src/interaction.spec.ts`、`project.spec.ts`），全部用真实输入，并核对宿主写下的回答：
   - E2E-approve-enter-esc：Enter、方向键、数字键、Esc、"跳过""关闭"，以及点击选项和"提交"。
   - E2E-question-panel：单选、文本、多选。多选题由 `raw` 写的 `elicitation/create` 造出来。
   - E2E-persistent-option-native：原生确认框的取消和确认两个分支，nightly。
   - E2E-project-add-trust：添加项目选文件夹；信任确认框显示项目路径；未信任标记随信任消失。
   - E2E-new-session-agent-claude：选项目、Claude、本地，创建会话。
   - E2E-reject-with-message（两条）：拒绝成功后才发带 `reply_to` 的排队跟进；CLI 先批准时，文字接在草稿之后，提示"已在别处处理"。
2. **缺陷就地修**，都单独提交，不改接口：
   - 请求面板：等待 Tab 或点击时，第一下点击落空（bddc892）。
   - 文本题："提交"没有反应（fe4c80c）。
   - `ci pr` 的 smoke 没有把 `@focus` 用例分出来单独跑（35120fe）。
3. **机器锁**（3e8c9e0、0f911a8，设施修复，单列）：
   - 新增 `cargo xtask lock -- <命令>`：取机器锁，运行命令，透传退出码。
   - 持锁运行的命令带 `CUPPER_MACHINE_LOCK=held`，嵌套时不会再等锁。
   - `xtask ci` 里持锁的作业，拉起的每条命令都带这个变量。
   - `tools/e2e` 的 `test:e2e`（两段）和新增的 `test:e2e:one`，都经这把锁运行。
4. **一处用例时序**（ec1c120）：拒绝后让回合再问一次，保证跟进消息一定是在排队时到达。
5. **notes.md**（dc8a647、80d8a88）：缺陷、取舍、各用例时长、机器锁、以后 e2e 怎么跑、实际规模、连跑记录。

## 以后 e2e 怎么跑

- 整套，两段跑法：`pnpm test:e2e`，或 `pnpm --filter @cupper/e2e run test:e2e [-- <spec>]`。
- 单个 spec 或单条用例：`pnpm --filter @cupper/e2e run test:e2e:one <spec> -g <名称>`，不加 `--`。
- 不要直接运行 `playwright test`；别的写法套一层 `cargo xtask lock -- <命令>`。
- 带 `@focus` 的用例需要显示器开着、没有锁屏（见下文）。

## 关键取舍

- 多选题用 `raw` 造，"别处已处理"用暂扣动画帧复现，都只在用例里做，产品代码里没有测试分支。
- 机器锁的变量在 `JobCtx::command` 统一设置。
- approve-enter-esc 照你的裁定留在 `pr` 和 `@smoke`。

## 规模

相对分支起点（不含 docs），新增 570 行、删除 18 行：

| 部分 | 增 | 删 | 说明 |
|---|---|---|---|
| 用例 | 376 | 0 | 估计 545 |
| 两处界面修复 | 15 | 5 | |
| 两处界面修复的单元测试 | 47 | 0 | |
| smoke 分段 | 11 | 8 | |
| 机器锁（单列） | 121 | 5 | 实现约 68 行、单元测试约 53 行 |

三处缺陷修复都在每处 100 行的上限内。机器锁按实现算，也在上限内。

## 验证方式

- **单元测试与检查**：
  - ui-core 的 request 单元测试 19 个通过。两条新测试在去掉对应修复时失败。
  - xtask 单元测试 39 个通过；clippy、fmt、tsc、ESLint、Prettier 通过。
- **连跑**：都是两段跑法、同一份构建，每次都没有本 worktree 的残留进程、根目录和 plist。完整记录在 notes。
  - **连续 10 次全绿**：持锁入口的第 4 批 run 7–10，接第 5 批 run 1–6，时间是 19:34–19:50。
  - **失败与显示器状态有关**。本机 19:46:50 关显示器，19:51:50 起锁屏，此前 18:33–19:10 也关过显示器、锁过屏。
    - 失败几乎都落在这两段，但不是每次都失败。
    - 显示器开着的 19:10–19:46 之间跑了 12 次，只有 1 次失败：19:34 那次 persistent-option-native，标记为 `other-e2e=yes`。
    - 同一用例之后单独用 `--repeat-each 10` 重跑，10 次全过。
  - **失败时的样子**：面板的 `data-ready` 10 s 不出现（窗口被遮挡时计时器被节流）；或者回答没送达，面板撤回到"按 Tab 或点击以处理请求"（主框架拿不到焦点，按键、点击不算真实输入）。
  - **`other-e2e` 标记偏宽**：等锁的时间也算了进去，等锁时持锁的正是别人的 e2e，所以等过锁的几次都标成了 `yes`。
- **T167 的 `disconnectWindow`**：只在第 1 批 run 5 失败过一次（18:40，锁屏期间）。持锁之后的 20 次里没有再出现。
- **带记录的 `ci pr`**（a8c4247，摘要 `pr-80d8a88461a9.json`）：
  - rust 1912、protocol 185、ts 473、license 1、testdata-scan 3、ui 35，全部通过。
  - e2e 的非 `@focus` smoke 段通过。`@focus` smoke 段 5 条里，4 条通过，approve-enter-esc 失败：一个回答在 10 s 内没有送达宿主，与锁屏期间的失败一样。
  - 跑的时候屏幕从 19:51 起一直锁着，所以没有重跑。这份失败的摘要照实提交了。
- 交回前确认过：没有我起的脚本，也没有本 worktree 的 e2e 进程留着。

## 提交列表

```
a8c4247 docs(tasks): record T093 PR CI summary, e2e focus phase failed on a locked screen
80d8a88 docs(tasks): T093 run record and why the focus tests failed
dc8a647 docs(tasks): T093 findings, the machine lock, how to run e2e, actual size
0f911a8 fix(e2e): test:e2e and test:e2e:one hold the machine lock
3e8c9e0 feat(xtask): cargo xtask lock runs a command under the machine lock
ec1c120 test(e2e): the turn still runs when the rejection's follow-up comes
9858525 test(e2e): adding and trusting a project, and a new Claude session
b239adc test(e2e): request panel keys and clicks, questions, the native dialog, reject with a message
35120fe fix(xtask): ci pr runs smoke tests that need the focus last, one at a time
fe4c80c fix(ui-core): "提交" answers a question's typed text
bddc892 fix(ui-core): a click on a panel waiting for Tab or a click lands
9f1b1cc docs(task): estimate T093
```

## 遗留问题 / 需要你决定的事

1. **带记录的 CI 要在屏幕解锁时重跑**：`cargo xtask ci pr --record docs/tasks/T093-e2e-interaction-project/ci/`。可以你明天合并前跑，也可以让我在屏幕解锁后跑。本机现在锁着屏，我这边重跑大概率还会失败。
2. **锁屏、显示器睡眠时 e2e 怎么办**：`@focus` 用例在这种状态下会偶发失败，`ci pr` 的 smoke 现在也包含它们。请在下面选一种或组合：
   - (a) 只把"屏幕开着、没锁"写成 e2e 的前提；
   - (b) e2e 的全局 setup 检查锁屏，锁着就直接报"屏幕已锁，`@focus` 用例无法运行"，不让它看起来像用例出错；
   - (c) `test:e2e` 和 `ci e2e` 运行期间用 `caffeinate -d` 防止显示器睡眠，从而不触发之后的锁屏。已经锁屏时它帮不上忙。

   (b) 和 (c) 都是十几行的设施修改。你定下来之前我不动。
3. **其他 agent**：在本分支合并之前，它们的 `test:e2e` 还不持锁。

**我先停在这里，等你的回复。**

## 修复第 2 轮（锁屏的处理，编排者转达用户的决定：(b) 加 (c)）

T093 修复第 2 轮：(c) 防止显示器睡眠、(b) 锁屏时报错，都已提交在 `test/T093-e2e-interaction-project` 上。按你说的没有跑带记录的 CI。没锁的状态实测通过；锁屏和显示器睡着两种状态，都没在新代码上实测成。

**改了什么**
- **(c) 防止显示器睡眠**（7fa83db，增 9 删 5）：
  - `test:e2e`、`test:e2e:one` 改为 `cargo xtask lock -- caffeinate -d ...`；`xtask ci` 的 e2e 作业里，两段 playwright 也套上 `caffeinate -d`；同步更新了 `playwright.config.ts` 的说明。
  - `caffeinate -d <命令>` 在命令结束前一直阻止显示器空闲睡眠，并原样返回命令的退出码，实测 `exit 3` 得 3。它随命令一起结束，不会留下进程。
- **(b) 锁屏时报错**（6328de5，增 37 删 3，`tools/e2e/src/setup.ts`）：
  - globalSetup 开跑前检查屏幕。遇到锁屏、主显示器睡着、或本登录会话不在前台，直接抛错，信息说明是屏幕的原因、`@focus` 用例无法运行，并提示解锁屏幕或唤醒显示器后重跑。
- **notes.md**（cac74c9）："以后 e2e 怎么跑"补上"屏幕没锁、显示器醒着"这条前提，并记下本轮的检测方法和实测情况。

**检测方法**
- 用 `osascript -l JavaScript` 调 CoreGraphics，一次约 70 ms，不需要额外权限：
  - `CGSessionCopyCurrentDictionary()` 里 `CGSSessionScreenIsLocked` 为 true：锁屏；
  - `kCGSSessionOnConsoleKey` 不为 true：本登录会话不在前台；
  - `CGDisplayIsAsleep(CGMainDisplayID())` 为真：主显示器睡着。

**实测结果**
- **没锁、显示器醒着**：
  - 检查得到 `{"locked":false,"asleep":false,"away":false}`；
  - 经 `test:e2e:one` 跑 E2E-question-panel（`@focus`）通过，跑完没有留下 caffeinate；
  - xtask 单元测试 39 个、`pnpm lint` 都通过。
- **锁着**：没能在新代码上实测。
  - 今晚 19:51 到 20:56 锁屏期间，我用 `ioreg -n Root -d1 -a` 看到过 `CGSSessionScreenIsLocked = true`，解锁后这个键消失。这与新检测读的是同一份会话字典。
  - 新的检测脚本是解锁后才写的。要实测，得锁一次屏，再由用户输密码解开，所以我没有自己锁屏。
- **显示器睡着**：没能实测成。
  - 我用 `pmset displaysleepnow` 让显示器睡，但它同一秒就被唤醒了（日志里 21:02:12 关、21:02:12 开）。检查仍得到 `asleep:false`，smoke 用例照常跑过。
  - 看来用户正在用这台机器，所以没有再试。

**行数**（设施修复，单列）
- (c)：增 9 删 5；
- (b)：增 37 删 3。

**提交**
```
cac74c9 docs(tasks): T093 e2e needs an unlocked screen; the display check and caffeinate
6328de5 fix(e2e): refuse to run on a locked screen or a sleeping display
7fa83db fix(e2e): keep the display awake while the e2e suite runs
```

**需要你决定**：锁屏这条怎么实测。可以请用户在方便时锁一次屏再解开；我在锁屏期间跑一次检查，再跑一次 `test:e2e:one`，确认它会以屏幕原因报错，大约需要用户离开 1 分钟。也可以接受上面 `ioreg` 的旁证，不再实测。你定。

交回前确认过，没有我起的进程留着。
