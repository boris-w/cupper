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
