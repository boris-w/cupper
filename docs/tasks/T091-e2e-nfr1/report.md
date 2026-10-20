# T091 汇报

- 执行者：子 agent（测试工程师）
- 分支：`test/T091-e2e-nfr1`（编排者已把含 T089 的 main 合入，合并提交 3ecf0b0）
- 完成日期：2026-10-20

## 做了什么

NFR-1 的五个编号都按设计 5.9.4 写成了 e2e 用例，共 12 条，全部通过。T082 交接的"插入历史后视口不跳"也在真实窗口里确认了。过程中查出三处产品缺陷，都按计划 1.2 就地修复、单独提交。

**支撑代码**（两处都只在测试构建里有）
- **UI 的视图导出**：在渲染进程入口的 e2e 分支里加了 `__cupperTest.views()`，按流给出 `{seq, complete, view}`。
  - 做法：包一层 `client.subscribe`，记下界面的 store，并从实时事件和快照记下 seq。
  - 为了赶在界面订阅之前拿到 client，`HostProvider` 改为创建 client 时就调 `onClient`。
  - `flavors.test.ts` 仍确认它不进非 e2e 构建。
- **CLI 的视图导出**：`cupper view index|<会话>`，只在 test-harness 构建里有。它跟随这条流，每处理完一批事件就输出一行 `{stream, seq, resubscribed, view}`；`resubscribed` 计数 Mirror 因缺口或视图应用失败而重新订阅的次数。
- **用例共用的 `nfr1.ts`**：等两端都到达宿主最后的 seq（取自 `host.db`）后取两边视图；建会话；在界面里打开会话；把带变量的界面文案转成匹配模式。

**用例**（`nfr1.spec.ts`、`nfr1-restart.spec.ts`）

| 编号 | 用例 | 标签 |
|---|---|---|
| NFR1-1 | 索引和一个会话：两端到达同一个 seq 后，视图完全相同 | `@smoke` |
| NFR1-2 | CLI 依次发送、排队、中断、批准、回答，界面依次出现：新回合；`[data-queue-id]` 里的排队消息；"你在 … 后停止了"，之后队列暂停；面板收起；记录行"已批准：…"、"已询问 1 个问题" | `@smoke` |
| NFR1-2 | 真实用户路径：`cli` 身份在 `repl` 里提权，界面的原生确认框批准后发送 | `@focus` |
| NFR1-3 | 界面批准，CLI 看到 `resolved` | `@focus` |
| NFR1-3 | CLI 批准，界面的面板收起，并提示"已在 cupper 处理" | `@smoke` |
| NFR1-3 | 两端同时回答：宿主只写一次 `interaction.resolved`，输的一方得到 `conflict/already_resolved`，不预设谁赢 | `@focus` |
| NFR1-4 | 断开 CLI，新 CLI 从快照补齐 | `@smoke` |
| NFR1-4 | 重载渲染进程，走快照 | `@smoke` |
| NFR1-4 | 只断开窗口的数据连接，按游标续传 | `@smoke` |
| NFR1-4 | 重载后加载更早的历史，屏幕上那一行不跟着动 | `@smoke` |
| NFR1-5 | 回合运行中 `kill -9`：回合补写为 interrupted，界面显示"已中断" | `@smoke` |
| NFR1-5 | 等待审批时 `kill -9`：请求补写为 expired（`host_restarted`），面板收起，记录行"未回答" | `@smoke` |

- **NFR1-4 的三种断开**：断开期间都用 `flood_rate 40 5 30` 持续产生事件，其中有合并成区间的 delta。结束时两端视图相同，CLI 的 `resubscribed` 为 0，界面 host-client 的缺口、重置、重复计数都为 0。
- **NFR1-5 的共同步骤**：杀宿主的时机以事件为屏障；界面拉起新宿主；被杀宿主的适配器消失；下一条消息成功。

## 就地修复（行数单列，不计入估计）

1. **`fix(cli)`**（新增 5 行，删除 3 行）：`approve`、`reject`、`answer` 遇到视图里已经处理过的请求时，原来在本地报 "no pending request"。现在照常发给宿主，由宿主回 `conflict/already_resolved`，与界面一致。由 NFR1-3 的同时回答查出。
2. **`fix(ui-core)` `data-queue-id`**（1 行）：设计 5.9.4 规定的这个 DOM 标记，排队列表原来没有设置。
3. **`fix(ui-core)` 加载历史时的锚点**（新增 5 行，删除 1 行）：
   - 缺陷：`loadEarlier` 锚住的是第一个进入视口的行，在列表顶部它就是"加载更早的历史"这个控件本身。控件在还有历史时一直在第 0 行，历史载完就消失，所以视口从没被放回去，每次都往下跳整整一页（一个回合 158 px）。
   - 修法：改为锚住控件之后屏幕上的第一行。

## 关键取舍

- **规模**：新增 532 行、删除 11 行（不含 lock 和文档）。去掉修复，T091 本身新增约 521 行，估计约 585，上限约 850。
- **视图比较的口径**：两端都从快照订阅，界面也没有加载历史，所以拿到的是同一个会话窗口，比的是完整视图。`notification.intent` 本来就不进视图。
- **NFR1-5 按现状断言**：新宿主只在会话收到第一条命令时才载入它（T089 的按需载入），窗口的订阅不触发载入。所以用例先发下一条消息，再断言补写的事件和界面显示。
- **`@smoke` 的范围**：不需要焦点的 NFR-1 用例都进了 `@smoke`，`ci pr` 一共跑 10 条（含夹具自检），两个 worker 约 20 s，整个 e2e 作业约 1 分钟，远低于 3 分钟。五个编号都留在 `pr`，不需要改成 `nightly`。三条 `@focus` 用例在 `ci e2e` 和 `ci nightly` 里跑。

## 验证方式

- 五个用例全部通过，靠真实输入的三条打了 `@focus`。
- 按 T167 的两段跑法（18 条用两个 worker，7 条 `@focus` 用一个 worker）连跑 10 次，全绿，每次约 47–51 s。每次跑完本 worktree 都没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在。
- 测试名以编号开头，`ids.toml` 里五个编号都是 `pr`。
- 在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T091-e2e-nfr1/ci/` 通过，运行期间没有写工作区。摘要 `pr-9a4463417f8a.json` 已提交，五个编号均为 pass。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,909 通过 |
  | protocol | 185 通过 |
  | ts | 471 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 35 通过 |
  | e2e | 13 通过（3 项检查加 10 条 `@smoke`） |

## 提交列表

```
f487916 docs(task): T091 CI summary
9a44634 docs(task): T091 notes: sizes, cases, fixes, findings
aed100e test(e2e): earlier history after a reload; NFR-1 cases in the smoke set
d8179a5 test(e2e): E2E-NFR1-5
7786228 fix(ui-core): hold the first row on screen, not the history control
3ecf0b0 Merge branch 'main' into test/T091-e2e-nfr1
41aa102 test(e2e): E2E-NFR1-1 to E2E-NFR1-4
1e16411 fix(ui-core): mark queued messages with data-queue-id
b144e0e feat(desktop): export the window's views in e2e builds
3c1e27a fix(cli): send an answer to a request another client answered first
321c076 feat(cli): view in test builds
35bf47a docs(task): T091 size estimate
```

## 遗留问题 / 范围外发现

1. **需要你决定：宿主重启后窗口显示过时的状态**。新宿主载入会话之前，窗口里被杀的回合仍显示为运行中，请求面板也还在，直到这个会话收到下一条命令。窗口的订阅要不要也算作"第一次调用"、触发载入，请你决定，可能归 T089 或 T069 的后续。
2. **插入历史后的残余偏差**：锚点修好后多数时候是 0 px，但大约每 10 次有 1 次下移 26 px。原因是视口按估计行高（44 px）放回去，新插入的三行实测为 40、79、39 px，之后的测量差额没有被补上。我试过改用 `virtualizer.scrollToOffset` 恢复，没有消除，已撤回。用例只断言移动小于 40 px，守住整页跳动的修复，注释里写明了这个偏差。建议交给 T082 的后续：等新插入的行测量完再恢复，或者测量后再按差额补一次。
3. **"经 `session/resume`" 没有直接断言**：假 agent 不对外暴露它收到的是 `session/resume` 还是 `session/new`，用例只确认下一条消息在新宿主上正常跑完。要直接断言的话，需要假 agent 或线路分接层提供一个可观察的记录。
4. **T167 夹具里一处注释写错了**：`fixture.ts` 的 `processes()` 说宿主不是 App 的子进程。实测宿主是由 Electron 主进程直接拉起的，只是 App 退出后它会留下来。代码本身同时收了两棵进程树，行为没有问题。我没有在 CI 摘要之后再加提交，留待下次改到夹具时一并修正。
