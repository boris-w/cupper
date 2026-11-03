# T220 PERF-bridge-gate 的压力用例在 runner 上点不开会话

- 派发日期：2026-11-03
- 执行者：子 agent（e2e 与桌面工程师）
- 分支：`fix/T220-bridge-gate-stress`
- 依赖：T219（发现）。`gate/m1a` 等本任务

## 背景

T219 让 nightly 在托管 runner 上也跑 e2e 的 `@perf` 用例，只记录。T219 合并后，`gate/m1a` 的候选 5da6f1c 上：
- push 触发的 `ci`（41445057052）通过；
- nightly（https://github.com/boris-w/cupper/actions/runs/41445179537 ）的 e2e 有一个用例失败，`perf` 和 `differential` 被矩阵的 fail-fast 连带取消，一步都没执行。

失败的用例：
- `src/gate.spec.ts:246` "PERF-bridge-gate: six streams of 200 fragments a second, a stress report"（`@perf`）；
- 在 `nfr1.ts:71` 的 `openSession` 处报 `locator.click: Timeout 30000ms exceeded`：侧边栏里 "say g6;" 的按钮找得到，但一直没等到"可见、可用、稳定"；
- 这一步在 `gate.spec.ts:162`，6 路流式输出已经开始之后、打开最后 4 个会话时。
- 同一个用例在 T219 分支的 nightly（41437271713）里通过了。

失败附件（main.log、host.log、adapter-claude.log、error-context.md）在这次运行的 `e2e-macos-15-playwright` 构件里，T212 之后放在 `rest/` 下。

## 范围

**做**：
1. 查清原因：按钮为什么一直不"稳定"，是侧边栏在流式期间持续重排或动画、按钮被遮挡，还是 runner 负载下渲染跟不上。用附件、页面快照给出证据。
2. 对症修：
   - 是测试的问题（比如在流式开始后才去点侧边栏），就改测试，例如先打开会话再开始流式输出，或者改用不依赖"稳定"的打开方式。修的同时不能削弱用例要测的东西。
   - 是产品的问题（比如侧边栏在流式期间不停重排，用户也点不中），就修产品，有用例守着。
   - 是产品问题的话，先把原因和方案报给 `main`，再动手。
3. 在 runner 上连跑对照：`e2e-repeat`，`spec=src/gate.spec.ts`，`test=PERF-bridge-gate`，`times` 至少 30，修前（main）、修后（分支）各一次。要触发时把参数和提交发给 `main`。

**不做**：改 PERF 的"只记录"规则（M1a 同步会 D1）；在作者机器上跑压测。

## 验收标准

- [ ] 原因有证据。
- [ ] 修后 runner 上连跑全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：80；硬停止线：120

## 参考资料

- 卷宗：T219（report.md、review.md）、T078（测量闸门，PERF-bridge-gate 的来源）、T212（e2e 附件分两轮存放）、T208、T217（e2e 偶发失败的排查方法）。
- 设计：5.7.1（测量闸门）、5.9.6、5.9.9。

## 注意

- 用户决定不在自己的电脑上跑压测：这个用例本机不要跑，复现和验证都放到 runner 上。本机只做普通检查。
- 不运行真实的 Claude Code、Codex CLI。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
