# T172 M1b 端到端：NFR-2、NFR-9 与容量

- 派发日期：2026-11-04
- 执行者：子 agent（测试工程师，宿主与桌面端到端）
- 分支：`test/T172-m1b-e2e-nfr`
- 依赖：T111、T121、T124、T236（都已合并）。T179 依赖本任务

## 背景

M1b 的退出标准第 ② ③ 条（12-milestones）要求 NFR-2、NFR-9 的用例通过；NFR-11 要求同时运行至少 6 个会话。实现都已在 M1a、M1b 前几波完成，本任务补上桌面端到端的验收用例，并核对 NFR-9 故障点的子编号全部通过。

## 范围

**做**（计划 T172 一行；设计 5.9.4、NFR-2、NFR-9、NFR-11）：

1. **E2E-NFR2-retry**：
   - 在桌面 App 里发一个写命令，宿主执行之后、回复到达界面之前断开连接（e2e 的断连钩子见交接事项 T078 一行）；
   - 重连后用同一个 `command_id` 重发；
   - 断言只执行了一次：事件流里只有一份结果，假 agent 的 `--calls` 里只有一次调用；
   - 界面最终显示的是这一次的结果。
2. **E2E-NFR9-adapter-crash**：回合进行中适配器进程退出。断言以下几点：
   - 回合按设计结束，结果是 `interrupted` 或 `failed`，原因码正确；
   - 界面给出恢复指引；
   - 适配器按 supervisor 的规则重启后，同一会话能继续发消息；
   - 历史完整，没有重复或丢失的条目。
3. **E2E-NFR9-cli-crash**：CLI 进程崩溃，用假 agent 的 `crash_cli` 或 `output_then_crash`。断言同上，并覆盖 resume 的路径。
4. **E2E-six-sessions**：6 个会话同时运行回合，用假 agent 的 flood 类脚本。断言以下几点：
   - 每个会话的回合都正常结束，输出不串会话；
   - 同一种 agent 共用适配器（与 IT-six-sessions-shared-adapter 一致）；
   - 界面切换会话时显示正确。
   - **这是高负载场景，只在 runner 上跑，本机不跑**（守则第 10 条）。
5. **IT-NFR9-failpoints**：是 core、git、worktree 三个子编号的合集。核对这三个子编号在最近的 CI 结果里都通过（trace-ids 的 `status`），汇报里列出证据。缺哪个子编号的覆盖，先报给 `main`，不要顺手补。
6. 用例都标 nightly（已登记），能后台跑的不加 `@focus`。

**不做**：
- IT-NFR2-epoch（T062）、IT-NFR9-crash-under-load 和 IT-six-sessions-shared-adapter（T069）：在 M1a 已完成。核对时一并列出它们的状态即可。
- 修产品缺陷：用例发现的问题先报给 `main`，由 `main` 决定在哪个任务里修。

## 验收标准

- [ ] 四个 E2E 通过，并在 runner 上各连跑 10 次（由 `main` 触发 e2e-repeat；E2E-six-sessions 用 runner 的默认负载）。
- [ ] IT-NFR9-failpoints 三个子编号的证据列在汇报里。
- [ ] 本机没有跑高负载场景，也没有跑抢焦点的用例。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：450；硬停止线：550。开工前在 `notes.md` 里按用例估行数，估完报给 `main`。

## 参考资料

- 设计：5.9.4（端到端测试与 NFR-1 验收）、5.9.8、5.2.8（`command_id` 与命令账本）、5.2.12（supervisor）、5.6.9；12-milestones 的 M1b 退出标准。
- 交接事项：`docs/tasks/handoff-notes.md` 里与 e2e 断连钩子、e2e 构建目录、`click` 辅助相关的几行。
- 卷宗：T078（断连钩子）、T090（e2e 框架）、T236（worktree 故障点）、T111（Codex 回收）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑，不跑高负载和抢焦点的用例（守则第 10 条）。
- e2e 共用的 `press(cupper, "Enter")` 不发 `char`，激活不了原生按钮。要点按钮，用 T132 加的 `click`（T132 合并后才在 main 上，到时 merge main）。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
