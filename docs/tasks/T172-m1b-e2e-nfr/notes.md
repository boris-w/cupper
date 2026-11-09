# T172 notes

## 开工前的估算（2026-11-04）

停止线 450，硬停止线 550。四个 E2E 都写在 `tools/e2e/src/` 下的新文件 `nfr2-nfr9.spec.ts` 里（six-sessions 单独一个文件，见下文），都标 nightly，不加 `@focus`。

| 用例 | 做法 | 行 |
|---|---|---|
| 共用 | `testConfig` 给假 agent 加 `--calls <文件>`；读 `--calls` 的小函数；从 `nfr1.ts` 复用 `session`、`openSession`、`id` | 25 |
| E2E-NFR2-retry | 见下文 | 45（另加钩子约 20） |
| E2E-NFR9-adapter-crash | 见下文 | 45 |
| E2E-NFR9-cli-crash | 见下文 | 50 |
| E2E-six-sessions | 见下文 | 60 |
| IT-NFR9-failpoints 的核对 | 不写代码。在汇报里列出 `-core`、`-git`、`-worktree` 三个子编号在最近 CI 里的结果（trace-ids 的 `status`、PR #71 等的 rust 作业），连同 IT-NFR2-epoch、IT-NFR9-crash-under-load、IT-six-sessions-shared-adapter 的状态 | 0 |
| 合计 | | 约 245（不加钩子约 225） |

### E2E-NFR2-retry

1. 窗口里经 `__cupperTest.command` 发一个写命令，例如 `session/send`。
2. 宿主执行之后、回复到达窗口之前，断开窗口的连接（见问题 1）。
3. 重连后，host-client 把在途的命令用同一个 `command_id` 重发。
4. 断言只执行了一次：
   - 账本回 `replayed`；
   - 会话流里只有一份结果（一个回合、一条用户消息）；
   - 假 agent 的 `--calls` 里只有一次 `session/prompt`；
   - 界面显示的是这一次的结果。

### E2E-NFR9-adapter-crash

1. 会话的回合进行中，脚本 `say working; exit 9` 让假 agent，也就是适配器进程，自己退出。
2. 断言：
   - 回合结束，`turn.completed` 为 `failed`，`error_code` 为 `adapter_exited`（以实测为准，不对就报给你）；
   - 界面给出恢复指引；
   - supervisor 重启适配器后，同一会话再发一条消息能正常完成，`--calls` 里能看到 `session/resume`；
   - 历史完整：两条用户消息、两个回合，条目没有重复或丢失。

### E2E-NFR9-cli-crash

1. 用脚本 `crash_cli`，必要时加 `output_then_crash`：CLI 回 `-32603`，之后一直报 "Session not found"，直到 resume。
2. 断言同上，并覆盖 resume 的路径：
   - 宿主按 K6 resume，并重发一次 prompt；`output_then_crash` 时不重发。
   - `--calls` 里有 `session/resume`；
   - 界面显示的结果与之相符。

### E2E-six-sessions

- 6 个 Claude 会话同时跑 `flood` 类脚本，每个会话的输出各带一个标记。
- 断言：
  - 6 个回合都正常结束；
  - 每个会话只含自己的标记，输出没有串到别的会话；
  - 宿主下只有一个假 agent 进程，与 IT-six-sessions-shared-adapter 一致；
  - 界面里逐个切换会话，显示的是各自的内容。
- 高负载，本机不跑，只在 runner 上跑。
- 单独一个 spec 文件，方便 e2e-repeat 只挑它。

## 要确认的问题

1. **E2E-NFR2-retry 的时机怎么做到确定**："宿主执行之后、回复到达界面之前"断开，现有的 `disconnectWindow` 只能在某个时刻断开。命令执行很快，回复可能已经到了窗口，用例就测不到重发。
   - **A（推荐）**：在窗口桥里加一个只在 e2e 构建里有的钩子。桥本来就按 `command_id` 记着在途的命令（`#awaiting`）。钩子的作用是"宿主对某个 `command_id` 的回复到达主进程时，不转给窗口，直接断开这个窗口"。时机就是确定的。产品代码里约 20 行，用 `__CUPPER_FLAVOR__ === "e2e"` 门控，与 T078 的 `disconnectWindow` 同一种做法。
   - **B**：不改产品代码。发命令后立即 `disconnectWindow`，靠竞态。断言"只执行一次"仍然成立，但不一定真的走到了重发的路径，用例偏弱。
2. **E2E-NFR9 的两条**：结束状态和原因码以实测为准，与设计（5.6.9、5.2.12）不符时先报给你，不顺手修。
3. **本机怎么跑**：只单份跑 NFR2、NFR9 三条，不跑 six-sessions。四条都不加 `@focus`，在后台窗口里跑。

## 编排者的决定（2026-11-04，2026-11-09 开工）

1. **E2E-NFR2-retry 取 A**：在窗口桥里加一个只在 e2e 构建里有的钩子，编进构建的方式照 T078 的 `disconnectWindow`。发布构建里不能有它（T161 会扫描测试钩子的标记）。钩子只按 `command_id` 拦一次，拦完即失效。
2. **两条 NFR9 的结束状态和原因码以实测为准**：跟设计不符时先报给 `main`，不顺手修。
3. **本机运行**：只单份跑 NFR2 和两条 NFR9；six-sessions 只在 runner 上跑；都不加 `@focus`。

## 实现与本机实测（2026-11-09）

- **窗口桥的钩子**：
  - `WindowBridge::dropReplyTo(command)` 先记下这个 `command_id`。窗口带着它发出请求时，记下请求的 `id`。宿主对这个 `id` 的回复到达主进程时，清掉还没转给窗口的那一批字节（回复在其中），然后断开连接；钩子同时失效。
  - 经 `bridges.dropReplyTo(webContentsId, command)` 和 e2e 构建专有的 `__cupperE2E.dropReplyTo` 调用，与 `disconnectWindow` 在同一处，发布构建里没有这个全局。
  - 单元用例：钩子吞掉一次回复并重连，同一 `command_id` 再发一次时，回复照常到达窗口。
- **E2E-NFR2-retry**：
  - 窗口经 `__cupperTest.command` 发 `session/send`，宿主的回复被钩子吞掉。重连后窗口的 client 用同一 `command_id` 重发，账本回 `replayed: true`。
  - 会话里只有两条用户消息，没有重复的条目；`--calls` 里 `say once` 的 prompt 只有一次；界面上 `once` 和 `say once` 各出现一次。
- **E2E-NFR9-adapter-crash**：`say working; sleep 100; exit 3`。实测与设计一致：
  - 回合为 `failed`，原因码 `adapter_exited`；
  - 界面的提示框有"重试"；
  - 下一条消息由新的适配器经 `session/resume` 完成，`--calls` 里的 resume 只有一次；
  - 历史里三条用户消息，没有重复。
- **E2E-NFR9-cli-crash**：`crash_cli`。实测与设计一致：
  - 回合为 `failed`，原因码 `process_exited`，提示框有"重试"；
  - 下一条消息先报 "Session not found"，经 resume 后再发一次：resume 1 次，`say again` 的 prompt 2 次，`crash_cli` 的 prompt 1 次（K6）；
  - 历史没有重复。
- **本机运行**：只单份跑了 `nfr2-nfr9.spec.ts`（`test:e2e:one`，后台，不抢焦点），3 条通过。`six-sessions.spec.ts` 没在本机跑，只做了类型检查。
- **假 agent 记录调用**：测试在 App 启动前改写根目录里的 `test_config.json`，给假 agent 加上 `--calls`。没有改共用的夹具。
- **规模**：`cargo xtask size` 为 +230，在停止线 450 以内。
