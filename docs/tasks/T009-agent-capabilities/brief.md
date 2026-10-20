# T009 驱动层 II-c：能力与状态

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T052、T053、T095、T076 的执行者接手，因为能力提取（T053）是它做的
- 分支：`feat/T009-agent-capabilities`
- 依赖：T055（合并中）。T063、T068 依赖本任务

## 背景

T055 开工前拆成三份（计划 v1.3.18）：T055 做了运行时核心与宿主组装；本任务做能力与状态；T008（生命周期）与本任务并行，由另一位执行者做。

## 范围

**做**（设计 5.6.2、5.6.7；T055 的交接）：
1. **能力缓存与探测目录**（5.6.7）：attach 返回的能力里命令列表现在是空的，补上；`Probe` 命令现在被丢弃，接上（能力缓存为空时使用）；写 `agent_capabilities` 表；探测目录每次探测前删掉重建为空目录（0700），视为已信任（5.2.6）；`steering.supported` 从 `initialize` 响应顶层的 `_meta` 读取。
2. **`CommandsChanged` 按会话去重**（5.6.2）：按会话记住上一份命令列表的摘要，内容没变就不发事件；会话流里的 `session.capabilities` 也只在内容变化时写。
3. **`AuthStatus`**：现在被忽略，接上，进 `AgentStatus`。
4. **`agents/capabilities`、`agents/refresh`、`AgentStatus`**：`agents/refresh` 重新跑发现（T055 的 `supervisor::discover`）和版本检查；`AgentStatus` 汇总 CLI 路径与来源、版本分类、登录状态、运行时、资源占用；语义码到协议字段的映射；经接口交给索引（T034 的 `IndexActor`，T034 合并之前用空实现）。

**不做**：生命周期、资源采样本身（T008）；索引里 `agents` 状态的写入（T034、T100）。

## 验收标准

- [ ] UNIT-commands-dedup（同样的命令列表不再发 `CommandsChanged`）。
- [ ] 能力缓存（写表、读表、探测目录重建）、`agents/refresh`、`agents/capabilities` 的用例。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T009 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T009-agent-capabilities/ci/` 通过，摘要随任务提交。

## 注意

- 不运行真实的 Claude Code CLI，不登录；用例一律用假 agent（按 T055 定下的路径约定）。
- 代码约束：ACP 回调里只做 channel 发送；时间只经 `Clock`。

## 规模

计划估为 M，T055 的执行者估约 550 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 750 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.2、5.6.6、5.6.7；`02-host.md` 5.2.6（能力探测目录）、5.2.9（索引的 `agents`）。
- `docs/tasks/T055-agent-runtime/notes.md`（给 T009 的交接）、`docs/tasks/T053-claude-extraction/`。

## 工作方式

T055 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T009-agent-capabilities main`。T076 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
