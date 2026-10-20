# T093 工作笔记

- 执行者：子 agent（前端工程师；T085 的执行者接着做）

## 开工前核对现状

- **假 agent 能造的请求**：
  - `permission <id>`：Claude 的三个选项依次是 1 `allow-once`（`allow_once`）、2 `allow-with-updates`（`allow_always`，宿主标为持久、`requires: privileged`）、3 `reject`（`reject_once`）。Claude 拒绝后回合继续。
  - `question <prompt>`：一个必填的文本字段。
  - `form <q>=<a>|<b> ...`：Claude 的 AskUserQuestion 形状，每题是单选加 `_custom` 文本，非必填。
  - 多选：假 agent 没有命令。改用 `raw` 直接写一条 `elicitation/create`（字符串 id，数组字段带 `anyOf`）。宿主照常回答，假 agent 的 SDK 收到不认识的 id 只记一条警告。答案从宿主的 `interaction.resolved` 核对，这正是"宿主收到正确的答案"。
- **核对手段**：按 T167 的辅助，`events()`、`untilEvent()` 只读打开 `host.db`，用 `interaction.requested`、`interaction.resolved` 的 `outcome` 核对每一下按键或点击的映射。
- **真实输入**：`interaction/respond` 和特权操作都要真实输入。凡是按键或点击的用例都打 `@focus`。新建会话（`session/create`）不要求真实输入，可以不打。
- **"别处已处理"的竞争**：CLI 先批准后，窗口收到 `interaction.resolved` 就把面板收起，用户根本来不及提交。要复现"窗口还没看到结果、用户已经提交"，用例在页面里暂时扣住 `requestAnimationFrame`（视图的重渲染合并在动画帧里，T080），等用户提交、宿主回 `already_resolved` 之后再放开。这只改用例所在页面的全局函数，不进产品代码。
- **信任**：夹具的仓库只有 README，添加时宿主自动信任（`auto_empty`）。用例先写一个 `.claude/commands/*.md`，添加时才会弹信任确认框。先选"不信任"，侧边栏出现"未信任"；再从会话页的未信任警告框点"信任此项目"，确认框里显示项目路径，选"信任"后标记消失。
- **`ci pr` 只跑 `@smoke`，而且两个 worker 并行**：带 `@focus` 的用例如果也打 `@smoke`，会在 `ci pr` 里和别的用例并行，被抢焦点。需要的话按"缺陷就地修"把 `ci pr` 的 smoke 也分两段（预计 10 行以内，单独提交）。

## 开工前的规模估计

| 用例 | 内容 | 草估 |
|---|---|---|
| 公共部分 | 打开会话、等第 n 个请求、取回答结果 | 50 |
| E2E-approve-enter-esc | Enter、↓↑ 加 Enter、数字键、Esc、点"跳过"、点"关闭"、点选项、点"提交"，逐个核对宿主收到的 `outcome` | 110 |
| E2E-question-panel | 单选（数字键）加"跳过"非必填题、文本（打字后 Enter）、多选（数字键勾选后 Enter） | 90 |
| E2E-persistent-option-native | 按持久选项的编号弹原生确认框；取消后请求仍在，冷却过后点击再弹、确认后宿主收到这个选项 | 60 |
| E2E-project-add-trust | 侧边栏添加项目、选文件夹、信任确认框里有路径；先不信任，再从警告框信任，标记消失 | 70 |
| E2E-new-session-agent-claude | 新建会话页选第二个项目、选 Claude、看到"本地"，发出第一条消息，进入会话页；索引里的会话带对应的项目、agent、工作区 | 55 |
| E2E-reject-with-message | 成功路径：编号聚焦自由输入行、打字后 Enter 只提示"点击提交"、点击后先拒绝再发带 `reply_to` 的跟进；别处已处理：文字接在草稿之后、提示、不发跟进 | 110 |
| **用例小计** | | **545** |
| 缺陷就地修 | 目前未知；按每处 100 行的上限另计 | 0–60 |
| **合计** | | 约 **545–605** |

在约 850 行的线内。每完成一个用例统计一次。

## 做的过程中的发现与取舍

- **缺陷 1：手动激活的面板，第一下点击会落空**（`fix(ui-core)` bddc892）。
  - 现象：面板处于"按 Tab 或点击以处理请求"时，点"跳过""关闭"或某个选项都没有反应。E2E-approve-enter-esc 的键盘步骤全过，第一个点击步骤超时。
  - 原因：按下鼠标时面板立即激活，并把底部这行提示拿掉。面板贴着页面底部，变矮以后按钮下移，松开鼠标时指针已经不在按钮上，`click` 落到面板本身。用户实际点击也会这样。
  - 修法：按下时只激活，提示等这次点击结束后再拿掉。新增单元测试：按下之后提示还在，点击完成后才消失，并且点击发出了回答。去掉修复后这个单元测试失败。
- **缺陷 2：文本题点"提交"没有反应**（`fix(ui-core)` fe4c80c）。
  - 现象：提问的文本字段打完字点"提交"，什么也不发。原因是"提交"去提交选中的选项，而文本字段没有选项。
  - 修法：输入框里有文字时，"提交"和 Enter 一样用这段文字回答。多选题带自由输入时，文字和已勾选的项一起作为一个数组提交，这是宿主映射自定义答案的方式；原来 Enter 只单独发出文字，宿主会以"不符合字段"拒绝。新增单元测试覆盖文本题点"提交"和多选题加文字两种情况；去掉修复后失败。
- **缺陷 3：`ci pr` 的 smoke 不分 `@focus`**（`fix(xtask)` 35120fe）。`ci pr` 只按 `--grep @smoke` 用两个 worker 跑。打了 `@smoke` 又打 `@focus` 的用例会和别的用例并行，被抢焦点。现在 smoke 也分两段：先跑不带 `@focus` 的，再单 worker 跑两者都带的（`--grep "(?=.*@smoke)(?=.*@focus)"`，已用 `--list` 核对过匹配结果）。
- **多选题**：假 agent 的 `form` 只有单选，所以用 `raw` 写一条带字符串 id 的 `elicitation/create`。宿主照常建请求、照常回答；假 agent 的 SDK 收到不认识的 id 只记警告（`incoming_actor` 的 unknown id 分支）。答案以宿主的 `interaction.resolved` 为准。
- **"别处已处理"的竞争**：用例先暂扣页面的 `requestAnimationFrame`，再让 CLI 批准、用户点"提交"，之后放开。视图的重渲染合并在动画帧里，扣住以后面板还停在旧状态，复现的正是"用户提交时窗口还没画出结果"。如果扣不住，面板会在点击前收起，用例会因为找不到按钮而失败，所以这一步本身也有校验。
- **新建会话的索引**：会话页已经显示 agent 的回答时，CLI 的 `sessions` 可能还没有这条会话（索引稍后才写出）。并行跑时出现过一次，改为轮询。
- **`@smoke` 与时长**（单 worker，本机）：

  | 用例 | 标签 | 时长 |
  |---|---|---|
  | E2E-approve-enter-esc | `@focus @smoke` | 13–16 s（8 个请求，键盘步骤各要约 1.6 s 的待命期和激活冷却） |
  | E2E-question-panel | `@focus @smoke` | 7–8 s |
  | E2E-persistent-option-native | `@focus`（nightly） | 约 6 s（含 2 s 取消冷却） |
  | E2E-reject-with-message（两条） | `@focus @smoke` | 各 5–7 s |
  | E2E-project-add-trust | `@focus @smoke` | 约 2 s |
  | E2E-new-session-agent-claude | `@smoke` | 约 1.5 s（不需要真实输入，可以并行） |

  都在"较快"的范围内，没有提议改成 nightly。最慢的是 approve-enter-esc，`ci pr` 的 smoke 因此多出约 40 s。

## 机器锁（修复第 1 轮）

- **问题**：别的 worktree 直接跑 playwright，不经 `xtask ci`，也就不持有机器锁。几个 App 窗口同时开在同一个位置，互相遮挡、抢焦点。
  - 本任务看到的症状：请求面板 10 s 内进不了可操作状态（被遮挡的窗口里计时器被节流）、回答迟到、真实输入不被记录。
  - 第二批连跑时，同一时段确实看到 T043 的 worktree 在直接跑 playwright。
- **修法**：
  - `cargo xtask lock -- <命令…>`（`crates/xtask/src/ci/lock.rs`）：取机器锁，运行命令，返回它的退出码；命令被信号结束时返回 128 加信号值。
  - 持锁期间运行的命令都带 `CUPPER_MACHINE_LOCK=held`，`xtask lock` 看到它就不再取锁，直接运行，避免嵌套死锁。`xtask ci` 里持锁的作业（ui、e2e、perf）经 `JobCtx::command` 给它拉起的每条命令都设这个变量。
  - `tools/e2e` 的两个入口都经 `cargo xtask lock` 运行（见下）。
- **单元测试**：
  - 退出码透传，含被信号结束的情况；子命令看得到 `CUPPER_MACHINE_LOCK=held`；命令结束后锁就释放。
  - 已持锁时不再取锁：测试先占住锁，带 `held` 运行的命令立即返回。
  - 持锁作业的命令带上这个变量。
- **规模**：新增 121 行、删除 5 行。
  - 实现约 68 行：`lock.rs` 约 50、`ctx.rs` 7、`ci/mod.rs` 和 `main.rs` 6、`package.json` 与 `playwright.config.ts` 的注释 5。
  - 单元测试约 53 行。

## 以后 e2e 怎么跑

都持有机器锁，与 `xtask ci` 的 ui、e2e 作业互相排队，不会和别的 worktree 抢焦点：

- 整套，两段跑法：`pnpm test:e2e`（仓库根目录）或 `pnpm --filter @cupper/e2e run test:e2e`。
  - 后面可以跟文件挑选，两段都用：`pnpm --filter @cupper/e2e run test:e2e -- src/interaction.spec.ts`。
- 单个 spec 或单条用例，单 worker：`pnpm --filter @cupper/e2e run test:e2e:one src/interaction.spec.ts -g E2E-question-panel`。
  - 这里不要加 `--`：playwright 会把 `--` 之后的 `-g` 当成文件名。
- 不要再直接运行 `playwright test`。需要别的写法时，套一层 `cargo xtask lock -- <命令>`。
- 跑之前先有构建：`cargo build -p cupper-fake-agent` 和 `pnpm build:e2e`。

## 实际规模

`git diff --numstat`，相对分支起点，不含 docs：

| 部分 | 增 | 删 |
|---|---|---|
| `interaction.spec.ts` 公共部分（打开会话、等第 n 个请求、取回答） | 46 | |
| E2E-approve-enter-esc | 41 | |
| E2E-question-panel（含多选的 `raw` 请求 26 行） | 70 | |
| E2E-persistent-option-native | 35 | |
| E2E-reject-with-message（两条，含 ec1c120 净增的 6 行） | 87 | |
| `project.spec.ts` 文件头 | 12 | |
| E2E-project-add-trust | 48 | |
| E2E-new-session-agent-claude | 37 | |
| **用例小计** | **376** | |
| 缺陷 1、2 的修复（`RequestPanel.tsx`、`kinds.tsx`） | 15 | 5 |
| 缺陷 1、2 的单元测试 | 47 | |
| 缺陷 3（`ci/jobs/e2e.rs`） | 11 | 8 |
| 机器锁（单列，见上） | 121 | 5 |
| **合计** | **570** | 18 |

用例比估计的 545 少，因为 T167 的辅助已经包办了大部分搭建工作。三处缺陷修复都远低于每处 100 行的上限，也没有改接口。
