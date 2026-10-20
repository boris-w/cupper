# T034 索引 actor 与关注度

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T035、T096 的执行者接手，因为网关的连接登记表（T037）和信任扫描（T035、T096）都是它做的
- 分支：`feat/T034-index-actor`
- 依赖：T033、T035、T037、T098（都已合并或合并中）。**本任务在剩余关键路径上，没有松弛**：T034 → T047 → T064 → T066 → T099

## 背景

索引流是界面左栏、项目列表、待处理提示和通知的数据来源（设计 5.2.9、5.3.5 的索引流）。`IndexActor` 是索引流和 `projects` 表的唯一写者，汇总项目、会话状态、在场、客户端、agent 状态和宿主状态。目前宿主里只有占位：会话报告待处理请求用的是 `NoIndex`（T088），网关的索引端口还没登记（T033、T037）。

## 范围

**做**（计划 T034 一行；设计 5.2.9、5.3.5 的索引流、5.5.10）：
1. **`IndexActor`**：索引流与 `projects` 表的唯一写者（代码约束：每条事件流只有一个写者）；实现 `IndexPort`（`deliver`、`opened(Opened)`、`closed(&ConnId)`；`ClientInfo.auth` 由 `Opened` 里的 principal 种类和 `weak_auth` 推出）（交接，T033）；用 `routes.index(..)` 登记，作为宿主的第三个 `Part`（`stop` 里写索引流的检查点）；启动第 7 步（加载索引流）放在绑定套接字之前（交接，T037）；检查点作废、日志又已裁剪时，按 `projects`、`sessions` 两张表重发相应事件（交接，T026）。
2. **项目与信任**：`project/add`（用 T098 的仓库档取 git common dir 和 git 根目录：`tool.repo_tier(仓库).run(..., RepoOp::RevParse 加 --show-toplevel ...)`）、`project/trust`、`project/trust_info`。信任扫描放进阻塞线程池；错误映射 `ScanError` → `unavailable/scan_failed`、`DigestMismatch` → `conflict/digest_mismatch`；写库用 `Manifest::to_json`、`TrustedBy::as_str`；`TrustScan.findings` 用 `scan.findings()`（交接，T035、T096）。**findings 的 `detail` 不截断、`.codex` 按叶子配置项列，结果可能超过 64,000 字节的帧上限：在本任务里处理**（例如分页或只给摘要加按需取详情），方案写进 `notes.md`。
3. **状态计算**：会话的状态（含 v1.4 的 `pending_kind`）与 100 ms 合并；实现 `IndexNotes`，换掉 `NoIndex`（交接，T088）。
4. **已读与在场**：`session/mark_read`（只前移不后退）；在场只采信 `desktop_ui`（安全建议 #16）。
5. **通知意图**：会话进入 `needs_action`、回合结束、出错时，此刻没有前台客户端在看，就写 `notification.intent` 事件（系统通知本身不做）。
6. **`clients`、`agents`、`host`**：`clients.updated` 用 `Gateway::clients()`（交接，T037）；`agents` 状态的接口留给 T009 的 `AgentStatus`（T009 之前为空）；`host`（版本、弱认证、待升级、防休眠）。

**不做**：系统通知（notifier）、`project/remove`；通知意图的进程级用例（T062）。

## 验收标准

- [ ] UNIT-index-status（含 `pending_kind`）；新增 UNIT-index-intent（直接向 IndexActor 输入状态变化）、IT-project-add-trust（真实宿主进程经网关：添加项目、扫描、信任、`stale`、`trust_info` 的差异，以及大量 findings 时不超过帧上限）。
- [ ] 不睡眠：核心和宿主内的用例用 `ManualClock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T034 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T034-index-actor/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T034 的行，开工前逐行读一遍）

T026、T033、T037、T088、T035、T096，以及 T098 的仓库档调用方式（`docs/tasks/T098-git-repo-tier/notes.md`）。

## 注意

- 用例只在临时目录里建项目和仓库，不对本机真实的仓库运行 git，不读 `~/.claude`、`~/.codex`。
- 代码约束：每条事件流只有一个写者；有状态的决策逻辑写成纯核心；时间只经 `Clock`。

## 规模

计划估为 L，上限约 1,500 行。**本任务范围很宽，开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线（包括贴线）就开工前停下汇报，并给出拆分建议**，例如：
- 第一部分：`IndexActor` 骨架与索引端口、项目与信任（含帧上限的处理）、`clients`、`host`、宿主组装（T047 依赖它）；
- 第二部分：状态计算与 100 ms 合并、`IndexNotes`、已读与在场、通知意图、`agents` 的接口。

**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.9（含"待处理请求的类别""已读""在场""通知意图"）、5.2.13（启动第 7 步、`Part`）；`03-protocol-v1.md` 5.3.5 的索引流；`05-flows.md` 5.5.10；`04-data-model.md`（`projects`、`sessions`）。
- `docs/tasks/` 下 T026、T033、T035、T037、T088、T096、T098 的 `notes.md`。

## 工作方式

在原来的 T037 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T034-index-actor main`。T096 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
