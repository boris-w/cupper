# T172 汇报

- 执行者：子 agent（测试工程师，宿主与桌面端到端）
- 分支：`test/T172-m1b-e2e-nfr`
- 完成日期：2026-11-09
- PR：https://github.com/boris-w/cupper/pull/85
- CI 运行：https://github.com/boris-w/cupper/actions/runs/42589992252 。`324d649a` 上 8 个检查全部通过。
- e2e-repeat（各 10 次、1 个 worker、不加负载）：
  - E2E-NFR2-retry、E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash 三条一起跑：https://github.com/boris-w/cupper/actions/runs/42586743217 。日志汇总为 `30 passed (2.2m)`，提交 `064e3626`。
  - E2E-six-sessions 修正后重跑：https://github.com/boris-w/cupper/actions/runs/42589988651 。日志汇总为 `10 passed (1.0m)`，提交 `324d649a`。
  - E2E-six-sessions 第一次连跑（run 42586753823）10 次全部失败，原因和修正见下文。

## 做了什么

1. **窗口桥的 e2e 钩子 `dropReplyTo`**（`apps/desktop/src/main/bridge/`）：
   - 先登记一个 `command_id`。窗口带着这个 `command_id` 发出请求时，钩子记下请求的 `id`。
   - 宿主对这个 `id` 的回复到达主进程时，钩子清掉还没转给窗口的那一批字节（回复在其中），断开窗口的连接，然后失效。每次登记只拦一次。
   - 调用路径：`bridges.dropReplyTo(webContentsId, command)`。e2e 构建在 `__cupperE2E` 下注册 `dropReplyTo`，与 `disconnectWindow` 写在同一处，同样用 `__CUPPER_FLAVOR__ === "e2e"` 门控。
   - `__cupperE2E` 已列在 `TEST_ENTRY_POINTS` 里，T161 的测试钩子扫描（`flavors.test.ts`、IT-selfuse-build）因此也会覆盖这个钩子。
   - 补了一条单元用例：钩子吞掉一次回复并重连；同一 `command_id` 再发一次时，回复照常到达窗口。
2. **`tools/e2e/src/nfr2-nfr9.spec.ts`** 三条用例：
   - 测试在 App 启动前改写根目录里的 `test_config.json`，让假 agent 带上 `--calls`。共用的夹具没有改。
   - **E2E-NFR2-retry**：
     - 窗口经 `__cupperTest.command` 发 `session/send`，宿主执行了，回复被钩子吞掉。
     - 重连后，窗口的 client 用同一个 `command_id` 重发，账本回 `replayed: true`。
     - 断言只执行了一次：会话里只有两条用户消息，没有重复条目；`--calls` 里 `say once` 的 prompt 只有一次；界面上 `once` 和 `say once` 各出现一次。
   - **E2E-NFR9-adapter-crash** 和 **E2E-NFR9-cli-crash**：断言内容见"两条 NFR9 的实测"。
3. **`tools/e2e/src/six-sessions.spec.ts`**：
   - 6 个 Claude 会话同时跑 `say start-n; flood 1000 32; say end-n`。
   - 断言：6 个回合都 `completed`；每个会话的 `item.delta` 只含自己的 `end-n`；宿主下只有一个假 agent 进程；界面里逐个打开会话，显示的是各自的内容。
4. 四条用例都没加标签，归入 nightly，也都没加 `@focus`。

## 两条 NFR9 的实测（与设计一致，没有要报的产品问题）

- **适配器崩溃**（脚本 `say working; sleep 100; exit 3`）：
  - 回合结束为 `failed`，原因码 `adapter_exited`。
  - 界面的提示框里有"重试"。
  - 下一条消息由新的适配器经 `session/resume` 完成，`--calls` 里 resume 只有 1 次。
  - 历史里三条用户消息，没有重复条目。
- **CLI 崩溃**（脚本 `crash_cli`）：
  - 回合结束为 `failed`，原因码 `process_exited`，提示框里有"重试"。
  - 下一条消息先遇到 "Session not found"，resume 后再发一次（K6）：resume 1 次，`say again` 的 prompt 2 次，`crash_cli` 的 prompt 1 次。
  - 历史没有重复。

## six-sessions 第一次连跑失败的原因和修正

- **原因在用例**：
  - 假 agent 的 `say start-n`、`flood` 的 1000 块和 `say end-n` 属于同一条 agent 消息，界面把它们拼成一个段落 `start-n…end-n`。
  - 用例用 `getByText('end-n', { exact: true })` 去找，永远找不到。
  - 失败停在第 37 行，这一行之前的产品侧断言 10 次都通过了：回合都结束、输出没有串、只有一个适配器。
- **修正**：
  - 改成按整段匹配：本会话的 `^start-n[a-z]*end-n$` 可见，其他会话以 `end-m` 结尾的段落数为 0。flood 的 filler 只用小写字母（`b'a' + i % 26`），所以 `[a-z]*` 是对的。
  - 先点"展开显示"：项目默认只列 5 个会话（`SHOWN_SESSIONS = 5`），第 6 个会话在它后面，不展开的话循环走到它时会找不到。

## IT-NFR9-failpoints 三个子编号的证据

取自 main 最近一次成功的 CI：run 41818624548，提交 ad61061d。来源是 rust 作业的 summary（`rust-ad61061dc9fc.json`），以及用这份 summary 跑的 `trace-ids status`。

| ID | 结果 | 用例 |
|---|---|---|
| IT-NFR9-failpoints-core（T069） | pass，pr | 6 条 `it_nfr9_failpoints_core_*` 都是 ok |
| IT-NFR9-failpoints-git（T229） | pass，pr | `before_apply`、`after_apply` |
| IT-NFR9-failpoints-worktree（T236） | pass，pr | `before_the_add`、`after_the_add` |
| IT-NFR9-failpoints（合集，T172） | trace-ids 判为 done | 理由为 "all 3 sub-IDs" |
| IT-NFR2-epoch（T062） | pass，pr | — |
| IT-NFR9-crash-under-load（T069） | pass，pr | — |
| IT-six-sessions-shared-adapter（T069） | pass，pr | — |

`trace-ids status` 里唯一的 problem 是 IT-sys-peer-macos15（T021）："ran only in pr, not in its suite nightly"。它与本任务无关，只记在这里。

## 关键取舍

- **NFR2 用钩子保证时机**：只靠 `disconnectWindow` 时，回复可能已经先到了窗口，用例测不到重发。钩子在回复到达主进程的那一刻断开连接，正好落在"宿主执行之后、回复到达界面之前"。
- **`--calls` 由测试自己写进配置**：在 App 启动前改写 `test_config.json`，没有改共用的夹具。
- **six-sessions 分开放**：单独一个文件，方便 e2e-repeat 只挑它；本机不跑。

## 验证方式

- **本机**（都是单份）：
  - `nfr2-nfr9.spec.ts` 用 `test:e2e:one` 在后台跑了一次，3 条通过，没有抢焦点。
  - `six-sessions.spec.ts` 只做了 tsc、eslint、prettier，用例没跑。
  - 窗口桥的 vitest：25 个通过。
  - `cargo xtask ci ts`：546 个通过。
- **对照验收标准**：
  1. **四个 E2E 通过，并在 runner 上各连跑 10 次**：是。NFR 三条 30 次全过；six-sessions 修正后 10 次全过，用的是 runner 的默认负载。
  2. **IT-NFR9-failpoints 三个子编号的证据**：见上表。
  3. **本机没有跑高负载场景，也没有跑抢焦点的用例**：是。
  4. **8 个检查都通过**：是，链接见开头。

## 设计偏差 / 补充

1. **5.9.4 / 5.9.9 的 e2e 钩子**：`__cupperE2E` 下新增 `dropReplyTo(webContentsId, command_id)`，宿主对这个命令的回复到达主进程时断开窗口的连接，只拦一次。它在 `__cupperE2E` 之下，发布构建里没有，测试钩子扫描也会覆盖。
2. **两条 NFR9 的实测与设计一致**：适配器崩溃为 `adapter_exited`，CLI 崩溃为 `process_exited`，两者都有"重试"，下一条消息都经 resume 接着进行（CLI 那条按 K6 重发一次）。

## 提交列表

```
324d649a test(e2e): six-sessions matches each agent's message whole
064e3626 test(e2e): NFR-2's retry, NFR-9's crashes and six sessions at once
f839d4f8 Merge branch 'main' into test/T172-m1b-e2e-nfr
a8a198c2 docs(T172): estimate before starting
```

## 遗留问题 / 范围外发现

- **规模**：+230 加上 six-sessions 修正的少量改动，在停止线 450 以内。
- **IT-sys-peer-macos15（T021）**：在 trace-ids 里只跑在 pr、没跑在 nightly，被判为 problem。与本任务无关，供你参考。
- **编写 e2e 时的一个提醒**：假 agent 同一个回合里的多条 `say` 属于同一条消息，界面会拼成一个段落；用 `getByText(..., { exact: true })` 找其中某一段会落空，要按整段或用正则匹配。

worktree 是否保留由你决定，T172 已经结束。
