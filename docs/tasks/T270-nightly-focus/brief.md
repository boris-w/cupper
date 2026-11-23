# T270 nightly 的 focus 用例：E2E-NFR1-3 与 presence

- 派发日期：2026-11-23
- 执行者：子 agent（前端与 e2e；T144、T266 的执行者）
- 分支：`fix/T270-nightly-focus`
- 依赖：无。T179 依赖本任务

## 来由

main 从 11-16 晚起停在 71c84c72，之后 nightly 的 e2e 每晚失败，都在 `@focus` 用例里（focus 项目只在 nightly 跑，PR 的 e2e 不跑，所以 PR 检查没拦住）：

| run（UTC 开始时间） | 失败 |
|---|---|
| 43997999938（11-17 01:45）、44210665761（11-18 00:47）、44839700115（11-20 23:50）、44959247834（11-21 22:55） | E2E-NFR1-3 的两个用例 |
| 44426069324（11-19 00:04）、45083301404（11-22 23:26） | 上面两个，加 E2E-presence-suppresses-notification |
| 44646045351（11-20 00:27） | E2E-NFR1-3 的两个用例、E2E-presence-suppresses-notification，加 E2E-review-comment |

- 11-16 07:50 在 577c4c42 上的 nightly 通过；之后合并的有 T153、T144、T108、T265、T268、T266。
- **E2E-NFR1-3**（`nfr1.spec.ts:86`、`:122`）每晚必挂，报错 `ten tabs did not reach the request panel`，来自 `tools/e2e/src/input.ts` 的 `activatePanel`：它按 Tab 直到面板激活，上限 10 次。T265 在会话页顶栏加了"打开"和"⋯"，T266 加了会话标题，Tab 路径大概变长了。这是推测，要先确认。
- **E2E-presence-suppresses-notification**（`notification.spec.ts:61`）时有时无，报 `expect(received).toBe(expected)`。
- **E2E-review-comment** 只出现一次，T260 修过同一用例；先看是不是同一个原因，只出现一次就记录，不在本任务里修。

## 范围

**做**（计划 T270 一行）：逐项查清原因再修，原因在测试就改测试，在产品就先报告。

1. **E2E-NFR1-3**：
   - 下载 trace，数清现在从会话页到请求面板要经过哪些可聚焦元素。
   - 判断是测试的上限该变，还是产品的 Tab 顺序该改：请求面板替换了输入框，键盘用户从顶栏要按多少下才能到它，对照 Codex 和设计 5.7.8 的 RP-3 判断。产品要改时先报告给 `main`。
   - 不要只把上限调大了事：如果改测试，就按实际的焦点路径写，让以后再加按钮时报错仍然说得清楚。
2. **E2E-presence-suppresses-notification**：下载失败那三晚的 trace 和 error-context，看是哪一步的值不对，是否也与顶栏或焦点有关。
3. 修完在 runner 上连跑确认：两个用例各 ×20（由 `main` 触发 e2e-repeat，同时给 `spec` 和 `test`）；最后在分支上跑一次 nightly 的 e2e（`job=e2e`）。

**不做**：codex-drift（T269）；无关用例的重构。

## 验收标准

- [ ] 两项都给出原因；修完各自在 runner 上连跑 20 次通过，分支上的 nightly e2e 通过。
- [ ] 没有为了通过而加 sleep，只等界面或宿主确认的状态。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：150；硬停止线：200。查清原因后先估行数报给 `main`，再动手。

## 注意

- focus 用例会抢焦点、发真实按键，本机不跑（守则第 10 条），放到 runner 上。
- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `fix/T270-nightly-focus`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
