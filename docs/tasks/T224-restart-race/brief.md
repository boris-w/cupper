# T224 重启用例里 `restart` 与 prompt 的先后

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`fix/T224-restart-race`
- 依赖：T111（发现）

## 背景

T111 给 `driver::runtime::tests::it_adapter_restart_backs_off_then_crashed` 加了 `agents/restart` 一段：调 `f.runtime.restart()` 之后立刻 `f.prompt(&mut session, "say lost")`，期望这个 prompt 以 `SessionNotFound` 失败（运行时在重启期间拒绝 prompt，会话 resume 后重发，K6）。

T220 的 PR 运行 https://github.com/boris-w/cupper/actions/runs/41585641594 （rust 作业，合并了含 T111 的 main）里它失败了一次：

```
panicked at crates/cupper-host/src/driver/runtime/tests.rs:823:5:
AdapterExited { instance: …, status: ExitStatus(unix_wait_status(0)), stderr: "", reclaimed: false }
```

也就是先到的是 `AdapterExited`，不是 `PromptFailed(SessionNotFound)`。推测：prompt 先于 `Mail::Restart` 到了运行时，被旧实例接下；重启按关闭顺序停掉旧实例，prompt 被切断，按 T218 的规则不转发失败，回合由 `AdapterExited` 结束。两种结局对会话都是正确的，只是用例假定了两条消息的先后。T220 改的只是 e2e，和这个用例无关。

## 范围

**做**：
1. 查清 `restart()` 和会话的命令走的是不是同一条信道、actor 怎么取，为什么两者的先后没有保证。给出证据，例如本机加延迟确定性复现。
2. 改用例：
   - 两种结局都接受，并分别断言后续正确：重发后 resume 成功；
   - 或者先等重启生效（例如等实例开始停止的可观察信号），再发 prompt，确定性地走 `SessionNotFound` 那条路。
   - 两条路径最好都有确定性的覆盖。
3. 如果查下来发现产品行为有问题（例如某种先后下 prompt 会丢、回合挂住），先报给 `main`，不要动手。
4. 在 runner 上连跑验证：`test-repeat`，修前（main）、修后（分支）各一次。参数由你提，例如 kind=lib、package=cupper-host、filter=`it_adapter_restart`、times=200。要触发时把参数和提交发给 `main`。

**不做**：改 `agents/restart` 的产品语义。

## 验收标准

- [ ] 原因有证据。
- [ ] 修后 runner 上连跑全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：80；硬停止线：120

## 参考资料

- 设计：5.6.8（回收、手动重启）、5.2.6 崩溃表、06 的 `DriverEvent` 表（T218）。
- 卷宗：T111（report.md、review.md）、T218（`AdapterExited` 的规则）、T216（偶发失败的排查方法）。

## 注意

- 不运行真实的 Claude Code、Codex CLI。
- 本机只单份跑（守则第 10 条）；连跑放到 runner 上。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
