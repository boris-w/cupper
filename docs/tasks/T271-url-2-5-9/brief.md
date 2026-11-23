# T271 升级 url 到 2.5.9（RUSTSEC-2026-0592）

- 派发日期：2026-11-23
- 执行者：子 agent（CI 与工具；T269 的执行者）
- 分支：`fix/T271-url-2-5-9`
- 依赖：无。所有 PR 的 `license` 检查都被它挡住，优先做

## 背景

2026-11-23 起，PR 上的 `license` 作业里 `cargo-deny` 报 RUSTSEC-2026-0592：

- `url` 2.5.8 及更早的版本，`Url::origin()` 处理嵌套的 `blob:` URL 时会递归调用自己，可能栈溢出，整个进程中止；
- 修法是升到 2.5.9 以上。

`url` 2.5.8 是间接依赖：宿主经 `reqwest 0.12.28`、`tower-http 0.6.11` 用到它。宿主不对不可信输入调用 `Url::origin()`，暴露面很小；但 `license` 是 main 的保护规则要求通过的检查，不修所有 PR 都合并不了（PR #128–#131 都挂在这里，例如 job 137502418280）。

## 范围

**做**：

1. `cargo update -p url --precise 2.5.9`（或更新的补丁版），只改 `Cargo.lock`。先确认 `url` 不是任何 crate 的直接依赖；如果连带升级了别的包，逐个列在汇报里，说明为什么不能避免。
2. `cargo deny check` 在本机通过（advisories、bans、licenses、sources）。
3. 宿主测试和 clippy 通过（单份，守则第 10 条）。

**不做**：升级 reqwest 或其他依赖；改 `deny.toml`（不加忽略项）。

## 验收标准

- [ ] `cargo deny check` 通过，`Cargo.lock` 里 `url` ≥ 2.5.9。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

只改 lock 文件，代码改动应为 0。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令；要推送时告诉 `main`。
- T269 的 PR #128 也被这条公告挡住，等本任务合并后再收尾；你在两个 worktree 之间不要混用。

## 工作方式

worktree 是 `.claude/worktrees/T054`，编排者已切到新分支 `fix/T271-url-2-5-9`，从 main 开出（含本任务书）。T269 的 worktree `.claude/worktrees/T052` 保持不动。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
