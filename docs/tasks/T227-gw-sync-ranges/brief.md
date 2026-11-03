# T227 网关同步用例的实时 range 又偶发失败

- 派发日期：2026-11-03
- 执行者：子 agent（Rust 工程师，宿主网关）
- 分支：`fix/T227-gw-sync-ranges`
- 依赖：T199（复发）

## 背景

`it_gw_sync_reconnect_replays_without_gaps`（`crates/cupper-host/tests/gateway_sync.rs`）最后一段断言：200 ms 合并窗口里的实时流出现 range 事件。T199 查过它（"no range events live"）：发送端的 200 ms 窗口从回合的第一条事件开始计时，agent 的第一段输出要是来得晚，就凑不成 range。T199 的修法是让整个回合都落在窗口里，由用例在回合结束后推进时钟（`advance(&writer.client, 200)`）来结束窗口。

T222 的 PR 运行 https://github.com/boris-w/cupper/actions/runs/41603412092 （rust 作业）里，它在第 373 行又以 "no range events live" 失败了一次。T222 改的是 git 服务，和网关无关。

## 范围

**做**：
1. 查清这次的原因，给出证据，例如本机注入延迟后确定性复现，或者从构件日志里找到时间线。可能的方向：
   - 推进时钟与窗口登记的先后（和 T224 的第二个原因同类：听到事件就推进时钟，运行时那一步还没做完）；
   - 窗口实际是不是按用例假定的方式开始和结束；
   - 也可能是别的原因。
2. 修用例；如果是产品问题，先报给 `main`，不要动手。
3. 在 runner 上连跑：`test-repeat`，kind=cargo、package=cupper-host、args=`--features test-harness --test gateway_sync`、filter=`it_gw_sync_reconnect`，修前（main）、修后（分支）各一次，次数和并行度由你提。要触发时把参数和提交发给 `main`。

**不做**：改合并窗口的设计。

## 验收标准

- [ ] 原因有证据。
- [ ] 修后 runner 上连跑全部通过，附运行链接。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：80；硬停止线：120

## 参考资料

- 卷宗：T199（report.md，第 1 处）、T224（`ManualClock` 上"听到事件就推进时钟"的问题）、T033（订阅与合并）。
- 设计：5.2.4（网关）、5.3（合并与 range）、5.9.1。

## 注意

- 不运行真实的 Claude Code、Codex CLI；本机只单份跑（守则第 10 条），连跑放到 runner 上。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
