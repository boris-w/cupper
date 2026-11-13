# T260 nightly 在未改动的 main 上不稳定的三项

- 派发日期：2026-11-13
- 执行者：子 agent（前端与 e2e；T145、T147 的执行者）
- 分支：`fix/T260-nightly-flakes`
- 依赖：无。T179 依赖本任务

## 来由

main 从 11-10 起停在 06f5cb87，没有新提交，nightly 却连续失败：

| 夜 | run | 失败 |
|---|---|---|
| 11-11 | 42919847584 | 只有 codex-drift（T259） |
| 11-12 | 43126389561 | E2E-review-comment；helpers 的"主进程未捕获的错误要记日志、不弹框"；UNIT-views-differential 超时（5 s）；codex-drift |
| 11-13 | 43328452471 | E2E-review-comment；codex-drift；security-peer（macos-15）被取消 |

- E2E-review-comment 两晚都在 `review.spec.ts:181`，`review.locator("[data-utility-button]").click()` 超时 30 s。同一提交 11-11 那晚通过。GitHub runner 11-11 换过镜像（20261111.127.1）。
- UNIT-views-differential 是超时，不是视图对不上：随机用例在 runner 上跑了 5 s 以上。T151、T250 给索引视图加了事件，用例可能变多了。

## 范围

**做**（计划 T260 一行）：逐项查清原因再修，原因在测试就改测试，在产品就先报告。

1. **E2E-review-comment**：
   - 下载两晚的 trace 和 error-context，看"+"按钮为什么点不到：没出现、被盖住、在视口外，还是 hover 没生效。
   - 用 e2e-repeat 在 runner 上复现（同时给 `spec` 和 `test`）。
   - T256 改过会话页的布局（全宽、内容居中），审查面板是否受影响也要看。
2. **helpers 未捕获错误**：查清是哪个错误。nightly 里 E2E-SEC-fake-host 也出现过 Electron 内部的 "Object has been destroyed"（T254 汇报），看是不是同一个根源。
3. **UNIT-views-differential 超时**：量一下用例数和单次耗时。给这个测试一个它需要的超时并注明原因，或者控制用例规模，不要只是把上限调大了事。用 test-repeat（`kind=vitest`）在 macos-15 上确认。
4. **security-peer（macos-15）被取消**：看是超时还是 runner 的问题，只记录，不在本任务里修 CI 设置。

**不做**：codex-drift（T259）；无关用例的重构。

## 验收标准

- [ ] 三项都给出原因，修完各自在 runner 上连跑通过：E2E-review-comment ×20，差分测试 ×10，helpers 视原因而定。
- [ ] 没有为了通过而加 sleep，只等界面或宿主确认的状态。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：150；硬停止线：200。

## 参考资料

- 卷宗：T131、T238（审查文档、hunk 按钮、行内评论）、T256（会话页布局）、T200（主进程未捕获的错误）、T254（"Object has been destroyed"）。
- 交接事项：写 e2e 的几行、shuttle 栈那一行（同类"环境变了才暴露"的问题）。

## 注意

- 本机不跑会抢焦点的用例（守则第 10 条），高负载复现放 runner 上。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `fix/T260-nightly-flakes`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
