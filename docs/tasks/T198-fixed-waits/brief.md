# T198 TS 单测里的固定等待

- 派发日期：2026-10-27
- 执行者：子 agent（工程师）。T064、T187、T192 的执行者接着做（e2e 夹具 T090、E2E-SEC-real-input T092 也是它写的）
- 分支：`fix/T198-fixed-waits`
- 依赖：无。T099 依赖本任务

## 背景

M1a 的退出验收（T099）要求在同一个提交上连续 20 次全绿，不允许重试。`apps/desktop/src/main/bridge/testing.ts` 的 `settle()` 只等固定 10 ms：

```ts
export const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));
```

GitHub 的 ubuntu runner 稍慢，回包还没到就开始断言。10-26 在 CI 上挂了 5 次，每次挂在 `apps/desktop/src/main/bridge/contract.test.ts` 的不同用例上，例如：
- `UNIT-hostport-contract-bridge > a request reaches the host re-serialized, and its response comes back`：`expected [] to deeply equal [ { jsonrpc: '2.0', id: 7, … } ]`；
- `… > initialize is answered with forbidden/already_initialized and stays local`：`expected [] to match object [{ id: 9, error: … }]`。

## 范围

**做**：
1. 去掉 `settle()` 这种固定时长的等待，改成等事件本身发生：等到所需的回包或调用出现为止，带一个足够宽的上限（例如几秒），超时时报出在等什么。
2. 所有用到 `settle()` 的测试一并改：`contract.test.ts`、`window-bridge.test.ts`、`bridges.test.ts` 等。
3. 同类排查：`apps/`、`packages/`、`tools/` 的 TS 测试里，凡是"等固定时长再断言"的写法都列出来，能改的一并改，不能改的说明原因。断言"在一段时间内什么都没发生"的用例，要说明它为什么不能改成等事件，以及它的时长为什么够。
4. 在高负载下复现修前的失败，并证明修后不再失败。负载的造法照 T196：后台跑几份整套测试，记下 pid，结束时用 SIGTERM 收掉。

**不做**：改产品代码。发现产品的问题，停下汇报。

## 验收标准

- [ ] 修前在负载下的失败率、修后同样负载下连跑 200 次无失败，数据写进本目录的 `notes.md`。
- [ ] 同类排查的清单与结论写进 `notes.md`。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 交接事项

- T196：在高负载下复现偶发失败的做法（`docs/tasks/T196-flaky-under-load/notes.md`）。
- T189、T196：只放宽等待时，要说明为什么放宽是对的，而不是在掩盖问题；不要用"失败就重试"。

## 规模

- 停止线：200；硬停止线：240

<!-- 执行者开工前在 `notes.md` 里写估计；每写完一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b fix/T198-fixed-waits main`。要在 GitHub 上跑 CI 时照守则第 9 条请编排者推送。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
