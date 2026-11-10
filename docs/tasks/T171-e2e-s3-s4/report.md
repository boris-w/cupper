# T171 汇报

- 执行者：子 agent（前端与 e2e）
- 分支：`test/T171-e2e-s3-s4`
- 完成日期：2026-11-10

**CI 状态（交接时）**：
- PR #107 https://github.com/boris-w/cupper/pull/107 ，运行 https://github.com/boris-w/cupper/actions/runs/42780317605 ：8 个检查全部通过。
- e2e-repeat：
  - E2E-S3 ×10（spec=src/review.spec.ts，test=E2E-S3）：https://github.com/boris-w/cupper/actions/runs/42780308755 ，`10 passed`。
  - E2E-S4 ×10（spec=src/composer.spec.ts，test=E2E-S4）：https://github.com/boris-w/cupper/actions/runs/42776953785 ，`10 passed`。
  - E2E-S3 第一次连跑（10 次全挂）：https://github.com/boris-w/cupper/actions/runs/42776942095 ，原因见"验证方式"。

## 做了什么

1. **E2E-S3**（review.spec.ts，复用 `reviewDocument`）：
   - 准备：测试先在仓库里设好提交者（`user.name`、`user.email`、`commit.gpgsign false`），提交一个 20 行的 `notes.txt`。
   - 假 agent 用 `write_file` 改第 2 行和第 19 行。两处相隔很远，成为两个 hunk。
   - 打开侧边面板，审查文档（默认"未暂存"）显示改动。
   - 在一行上写行内评论，评论作为附件出现在输入框里，随 "say noted" 作为 `review_comment` 块发出：宿主事件里带 `"path":"notes.txt"`，发出的消息上显示"1 条评论"。
   - 等这一轮结束、面板刷新后，先等三个"暂存"按钮都出现（文件一个，每个 hunk 一个），再点第一个 hunk 的。`git diff --cached` 里只有第 2 行。
   - "Git 操作"→"提交…"，等菜单卸载后在弹窗里写信息、点"继续"。`git log`、`git show` 确认提交里只有第 2 行，第 19 行还在未暂存里。
2. **E2E-S4**（composer.spec.ts，复用 `turnRunning`、`steered`）：
   - 等这一轮的输出出现后，Enter 插话，等到 steered 事件；
   - ⌘Enter 排队；
   - 从行菜单编辑排队消息（等菜单卸载）；
   - 第一次 Esc 后出现"再按一次 Esc 停止"，第二次 Esc 后宿主写下 `cancelled` 的 `turn.completed`，暂停的队列里还是编辑后的那条。
3. **notes.md**：开工前的估算，以及两个场景各自等的是什么状态。

## 关键取舍

- **两个场景各放进已有的 spec 文件**，以复用文件里的局部辅助函数（`reviewDocument`、`turnRunning`、`steered`），没有为此去挪动或导出它们。
- **提交者写在仓库配置里**：宿主跑 git 时只有 login_env 的 PATH，提交者要从仓库配置里来，和用户自己的仓库一样。这只是测试准备，没有改产品。
- **只等界面或宿主确认的状态，没有 sleep**：
  - 插话前等这一轮的输出出现；
  - 点完菜单项等菜单卸载；
  - 暂存前等这一轮结束、hunk 的按钮都出现；
  - 暂存和提交的结果用 `expect.poll` 读 git；
  - 中断的结果等宿主的 `turn.completed`。
- **不需要真实输入**：暂存和提交都不经过确认控件，两条都不用 `@focus`。

## 验证方式

- **本机**：eslint、tsc、prettier 通过。e2e 只在 runner 上跑（守则第 10 条）。
- **E2E-S3 第一次 10 次全挂，原因在测试**：
  - 我断言评论发出后，页面上会出现只有 "noted" 的回复。但假 agent 的 `say` 会把整条 prompt 原样说回来，而评论块在 prompt 里被转成了一段文字，所以回复是 "noted" 后面接着 "Review comment on notes.txt, lines 2-2 (old): … name it better"，只有 "noted" 的文字永远不会出现。
  - 改成看发出的消息上显示"1 条评论"，再等宿主的 `turn.completed`。修好后 10 次全过，后面的暂存和提交也都过了。
- **对照验收标准**：
  - [x] E2E-S3、E2E-S4 通过，在 runner 上各连跑 10 次全部通过。
  - [x] 没有为了通过而加 sleep。
  - [x] PR #107 的 8 个检查在 353d384d 上全部通过。
- **规模**：开工前估约 105，实际 +107 −0（`cargo xtask size`），在停止线 300 以内。

## 提交列表

```
353d384d test(e2e): E2E-S3 does not expect the agent to say only noted
7e9a5876 docs(task): T171 notes: the estimate and how the scenarios wait
422b32f2 test(e2e): E2E-S4
94ec8e79 test(e2e): E2E-S3
```

## 遗留问题 / 范围外发现

1. **评论落在旧的一侧**：E2E-S3 hover 的第二个 `[data-line]` 是被删掉的那行，所以评论是 "lines 2-2 (old)"。你说过改成新的一侧更像用户的做法，但只在本来就要重跑时才改；S3 重跑已经通过，所以没有改。以后动这条用例时，可以顺手改成 hover 新增的那一行。
2. **notes.md 没有记那次 S3 修正**：为了不再触发一轮 CI，没有往分支上补提交，这部分记在本汇报里。
3. **E2E-S4 没有覆盖的部分**：人工检查表 man-s4 里的"删掉一条排队消息"（CP-8，P1）、设置页切换（T147）、⌘. 和中断后再发一条，不在本任务范围内，没有写进 E2E-S4。
