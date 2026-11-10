# T171 notes

## 开工前的估算（2026-11-10）

| 场景 | 估算（新增行） |
|---|---|
| E2E-S3：仓库里先提交一个 20 行的文件；假 agent 用 `write_file` 改其中相隔很远的两行（两个 hunk）；打开审查面板；行内评论随下一条消息发出；按 hunk 暂存第一处；经"Git 操作"的提交弹窗提交，提交里只有暂存的那一处 | 约 70 |
| E2E-S4：运行中 Enter 插话；⌘Enter 排队；行菜单编辑排队消息；两次 Esc 中断，排队消息还在 | 约 35 |
| 合计 | 约 105 |

在停止线 300 以内。S3 放进 review.spec.ts，复用 `reviewDocument`；S4 放进 composer.spec.ts，复用 `turnRunning`、`steered`。

## 写法

- **E2E-S3**：
  - 测试先在仓库里设好提交者（`user.name`、`user.email`，`commit.gpgsign false`），提交一个 20 行的 `notes.txt`。宿主跑的 git 只有 login_env 的 PATH，提交者要从仓库配置里来，和用户自己的仓库一样。
  - 假 agent 用 `write_file` 改第 2 行和第 19 行，相隔足够远，成为两个 hunk。
  - 审查面板默认是"未暂存"，直接能看到这两处。
  - 行内评论随下一条消息发出：等宿主写下带评论的事件，再等这一轮结束，面板刷新后再暂存，免得点到刷新前的按钮。
  - 按 hunk 暂存：审查文档里先是文件自己的"暂存"，然后每个 hunk 下一个。先等三个按钮都出现（hunk 的按钮要等 `git/file_diff` 回来），再点第二个。用 `git diff --cached` 确认只暂存了第一处。
  - 提交："Git 操作"菜单的"提交…"，先等菜单卸载，再在弹窗里写信息、点"继续"。用 `git log`、`git show` 确认提交里只有那一处，第 19 行留在未暂存里。
  - 不需要真实输入：暂存和提交都不经过确认控件。
- **E2E-S4**：
  - 复用 `turnRunning`（等这一轮的输出出现再插话）和 `steered`。
  - 编辑排队消息先等菜单卸载。
  - 第一次 Esc 之后等"再按一次 Esc 停止"出现，第二次 Esc 之后等宿主写下 `cancelled` 的 `turn.completed`，再看暂停的队列里还是编辑后的那条。

两条都没有 sleep，只等界面或宿主确认的状态。

## 实际规模

`cargo xtask size`：+107 −0。
