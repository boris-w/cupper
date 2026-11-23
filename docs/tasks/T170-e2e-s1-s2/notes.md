# T170 笔记

## 开工前的估算（2026-11-23）

照 T171 的写法：两个场景各放进已有的 spec，复用文件里的局部辅助函数。

**E2E-S1**（`new-session.spec.ts` 的 "with Codex too" 一组，用它的两种 persona 配置；`@focus`，因为 ⌘⌥A 经菜单只发给有焦点的窗口）：

1. 侧边栏"添加项目"（对话框替身选 `cupper.repo`），不另建会话。
2. 新建会话页：Claude、新建 worktree，脚本 `write_file claude.txt …; permission k1; say …`；等它的行显示"等待批准"。
3. 回到新建会话页：Codex、新建 worktree，脚本 `write_file codex.txt …; sleep 10000; say …`；它的行显示"运行中"，Claude 的仍是"等待批准"，两行同在项目 repo 下，悬停分别显示 Claude、Codex。
4. ⌘⌥A 跳到 Claude（T153 的 `shortcut`）；点面板上的选项批准（不经 `activatePanel`）。
5. 两个回合结束：两行都是"未读"；点开 Codex 变为已读，Claude 不变（见下面的产品缺陷）。
6. `workspace.ready` 给出两个 worktree 的路径：`claude.txt` 只在 Claude 的，`codex.txt` 只在 Codex 的，主工作区 `git status --porcelain` 为空。

约 85 行（新建会话页的局部辅助函数约 20 行）。

Codex 的"运行中"靠假 agent 脚本里的 `sleep 10000`：这是场景内容，不是测试在等。Claude 先建（停在审批上，不会自己结束），Codex 后建，检查紧跟在 Codex 建好之后，10 秒足够；测试本身只等界面和宿主的状态。

**E2E-S2**（`interaction.spec.ts`，复用它的 `session`、`request`、`outcome`）：

- 前台部分（`@focus`）：脚本 `permission k1; permission k2; question Name?; say done`。
  - 第一个请求：`activatePanel` 后 Enter，记录行"已批准：…"，宿主记的是 allow-once；
  - 第二个：Esc，记录行"已拒绝"；
  - 提问：在输入行打字、点"提交"，agent 说出 `answer:…`，回合结束。
  - 约 30 行。
- 不在前台的部分（不标 `@focus`）：不标 `@focus` 的用例里应用始终不在前台，在场从一开始就是"不在前台"，没有先后竞态。窗口打开这个会话，CLI 新建 `permission k1; say done`，记下 `needs_action` 的意图（带 request_id），CLI `approve` 后回合结束，再记一条 `turn_completed`；通知只记录（e2e 的 `notifier: record`）。约 25 行。
- 两个用例都以 `E2E-S2:` 开头，e2e-repeat 用 `test=E2E-S2` 一次跑两个。

**人工检查表**：在 man-s1.md、man-s2.md 里注明 E2E-S1、E2E-S2 各走了哪几步（文档不计行数）。

**合计**：约 140 行，停止线 300。

## 开工前发现的问题

1. **产品缺陷：桌面界面从不调用 `session/mark_read`**。设计 5.2.9 写"客户端在会话可见且窗口在前台时调用 `session/mark_read`"，宿主有这个方法（T100），但 ui-core、host-client、桌面主进程里都没有调用它的地方。所以在界面上点开一个"未读"的会话，它一直是未读，直到 CLI 去标记。E2E-S1 第 5 步"点开一个变为已读"因此过不了。按任务书先报告，由编排者决定。
2. **T270**：E2E-S2 前台部分用 `activatePanel`，写法与 nightly 里一直通过的 E2E-approve-enter-esc 相同（点侧边栏的行、`focusWindow`，再 Tab）。如果连跑时也报"ten tabs did not reach the request panel"，先告诉编排者，不改 `activatePanel`。E2E-S1 的批准用点选项，不经过它。
3. **T267**：会话行的 agent 现在只在悬停的 tooltip 里。T267 做悬停卡片，如果去掉了这个 tooltip，E2E-S1 里的悬停断言要跟着改。

编排者 2026-11-23 定：缺陷选 B，本任务补上 `mark_read`，接在 T257 的在场判断上；限流方式自定；悬停断言按"悬停后能看到 agent 名"写，不绑定 tooltip 的 DOM。停止线 300 不变。

## 实现记录

- **已读**：`usePresence` 里加 `useReadMark(foreground, sessionId)`，用的就是在场报告里的那两个值。会话来到前台时报一次；之后这条流每来一条通知（快照或事件，host-client 的流先于其他监听者处理，游标已前移）就报，但两次之间至少隔 `READ_MARK_MS`（1 秒），冷却期间来的合并到冷却结束时；离开前台或换页时把冷却期间看到的补报一次。`up_to_seq` 取 host-client 里这条流的游标（新加的 `client.seq(stream)`，快照完成后才有）。单元测试在 presence.test.tsx，没有新编号（同文件原有的用例也没有）。
- **E2E-S1 的审批改由 CLI 回答**：会话在前台时回合结束会被标为已读（这正是刚补上的行为），所以 ⌘⌥A 跳到 Claude 之后先回到新建会话页，再用 CLI 批准，两个回合都在不显示它们的时候结束，才都是"完成未读"。请求面板的按键在 E2E-S2 里走。
- **E2E-S1 的悬停**：数页面上 agent 名的文字在悬停前后的个数，悬停后变多即可，不绑定 tooltip 或悬停卡片的结构。
- **E2E-S2 不在前台的部分**：不标 `@focus` 的用例里应用始终在后面，在场从一开始就是"不在前台"，窗口显示着这个会话也照样记通知意图，没有先后竞态。会话先用 "say ready" 建好、在窗口里打开，再发会提请求的消息，这样请求和回合结束都发生在窗口显示它之后。
- **规模**：+286 −4（`cargo xtask size`）：已读的产品代码约 45 行、单元测试约 75 行，E2E-S1 约 97 行，E2E-S2 约 66 行。
