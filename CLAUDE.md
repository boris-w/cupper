# cupper

> 技术栈、目录结构、常用命令等在技术设计（阶段 ②）确认后补充到本文件。

## 角色分工

- **主会话（编排者）**：把控流程和阶段闸门；与用户沟通并撰写 PRD、技术设计、任务拆分；派发任务给子 agent；review 并合并分支。
- **子 agent（执行者）**：每次只领一个任务，在独立 worktree 的独立分支上完成开发、测试和提交。

流程、当前进度和留痕体系见 `docs/00-process.md`。

## 留痕原则

所有工作都要留痕并提交：与用户的讨论、与子 agent 的讨论会、同步会写成会议记录（`docs/meetings/`）；每次派发都有任务卷宗（`docs/tasks/<ID>-<slug>/`：brief、report、review 及物料）；重要技术决策写 ADR（`docs/decisions/`）。模板在 `docs/templates/`。

## Git 协作约定

### 分支与 worktree

- `main` 始终保持可用。代码改动不直接提交到 `main`，统一走"任务分支 → review → 合并"。
- 阶段文档（PRD / 设计 / 计划 / ADR）由编排者直接提交到 `main`。
- 一个任务一条分支、一个 worktree。分支名：`<type>/<任务ID>-<简短英文描述>`，例如 `feat/T03-login-api`；调研任务用 `research/R01-codex-ui-teardown` 这种形式；原型实测用 `spike/SP-5-git-snapshot` 这种形式。
- 原型实测的代码放在 `spikes/<ID>-<slug>/`，只用于验证，不是产品代码，后续开发不直接依赖它。提交前要去掉个人信息（本机路径、用户名、账号、主机名等），因为仓库会开源。
- 合并统一用 `git merge --no-ff`，让每个任务在历史里保留成一个完整的合并节点。合并后删除 worktree 和分支。
- 每个阶段闸门通过后打 annotated tag：`gate/prd`、`gate/design`、`gate/plan`、`release/vX.Y.Z`。

### 提交信息

采用 Conventional Commits：

```
<type>(<scope>): <一句话说明做了什么>

<为什么这么做 / 关键取舍，可选>

Refs: T03
Co-Authored-By: ...
```

- type：`feat` `fix` `refactor` `test` `docs` `chore` `perf` `build` `ci`
- 小步提交：每个提交只做一件事，并且能独立通过构建和测试。
- 提交信息中不出现任何 claude.ai/code 会话链接。

## 工程原则

- 不为兼容旧系统写任何兼容分支、版本判断或补丁代码。需要新版本系统才有的 API 时，直接提高最低支持版本。目前最低支持 macOS 14。
- 不留技术债：临时写法、被注释掉的代码、没人用的代码，不能进入 main。

## 子 agent 工作守则

1. 只在分配给你的 worktree 和分支里工作。不切换到其他分支，不修改 `main`，不执行 merge、rebase 或 push。
2. 开始前先读本任务的 `docs/tasks/<ID>-<slug>/brief.md`，以及 `docs/01-prd.md`、`docs/02-design.md`、`docs/03-plan.md` 中的相关部分。
3. 只做任务范围内的事。发现范围外的问题时记录下来，在汇报中提出，不要顺手改。
4. 按任务的验收标准自测（构建、测试、lint 都通过）后再提交。
5. 过程中的思路、试错和取舍可以记在同目录的 `notes.md`；调研数据、截图、基准结果等物料也放在该目录。
6. 结束时按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`，由编排者存档并提交。`notes.md`、资料摘录等其他物料仍由你在分支上提交。
7. 调研或测试时，任何外发请求（HTTP 头如 User-Agent、查询参数、表单、提交到外部服务的内容）都不能包含用户的邮箱、姓名、账号等个人信息。API 要求填联系方式时，用项目占位信息，或者换一个不需要联系方式的来源。
