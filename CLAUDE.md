# cupper

> 技术栈、目录结构、常用命令等在技术设计（阶段 ②）确认后补充到本文件。

## 角色分工

- **主会话（编排者）**：把控流程和阶段闸门；与用户沟通并撰写 PRD、技术设计、任务拆分；派发任务给子 agent；review 并合并分支。
- **子 agent（执行者）**：每次只领一个任务，在独立 worktree 的独立分支上完成开发、测试和提交。

流程和当前进度见 `docs/00-process.md`。

## Git 协作约定

### 分支与 worktree

- `main` 始终保持可用。代码改动不直接提交到 `main`，统一走"任务分支 → review → 合并"。
- 阶段文档（PRD / 设计 / 计划 / ADR）由编排者直接提交到 `main`。
- 一个任务一条分支、一个 worktree。分支名：`<type>/<任务ID>-<简短英文描述>`，例如 `feat/T03-login-api`。
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

## 子 agent 工作守则

1. 只在分配给你的 worktree 和分支里工作。不切换到其他分支，不修改 `main`，不执行 merge、rebase 或 push。
2. 开始前先读 `docs/01-prd.md`、`docs/02-design.md`、`docs/03-plan.md` 中与本任务相关的部分。
3. 只做任务范围内的事。发现范围外的问题时记录下来，在汇报中提出，不要顺手改。
4. 按任务的验收标准自测（构建、测试、lint 都通过）后再提交。
5. 结束时汇报：分支名、提交列表（`git log --oneline main..HEAD`）、怎么验证的、遗留问题。
