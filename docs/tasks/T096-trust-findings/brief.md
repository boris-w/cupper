# T096 项目信任 findings 解析

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T037、T035 的执行者接手
- 分支：`feat/T096-trust-findings`
- 依赖：T035（已合并）。T068（宿主集成测试 III）依赖本任务

## 背景

T035 开工前拆成两部分（计划 v1.3.15，见 `docs/tasks/T035-trust-scan/brief.md` 末尾的补充裁定）：T035 做了扫描范围、摘要与状态；本任务解析 findings，只用于在原生确认框里展示项目会让 CLI 加载哪些可执行配置（设计 5.2.6"项目信任"的 findings 一行）。

## 范围

**做**：
1. 从 `trust::Scan.files` 的 `path`、`content` 解析 findings（不必再读文件）：hooks、`env`、`apiKeyHelper`、`statusLine`、MCP 服务器、skills、agents、commands，以及 `.codex/` 里的配置项和 hooks。文件范围按执行者在 T035 notes 里列的：`settings*.json`、`.mcp.json`、`skills`、`agents`、`commands`、`.codex/*.toml`。
2. 解析不了的文件记一条 `unparsed` 的 finding，不让扫描失败（摘要照样覆盖它的完整内容）。
3. **`FindingKind` 加 `Unparsed => "unparsed"`**（开放枚举，附加式改动；裁定第 1 条）：改 5.3.11 的那一行，跑 `pnpm gen`，生成物随提交。
4. **TOML 依赖**（裁定第 2 条）：`toml_edit = "=0.25.17"`，`default-features = false, features = ["parse"]`，以 `cargo deny` 通过为准。
5. `Finding.target`、`outside_workspace` 直接取 `ScannedFile` 上的字段；工作区外的目标逐项列出。
6. `detail` 原样输出，不截断（裁定第 8 条；帧上限的问题交给 T034）。

**不做**：确认框本身（桌面任务）；`project/add`、`project/trust_info` 的方法（T034）。

## 验收标准

- [ ] 新增 UNIT-trust-findings：每一类 finding 都有样例；`unparsed`；工作区外的目标；同一个文件里多类条目；`.codex/` 的 TOML。样例里不出现本机的用户名和路径，MCP 服务器名、hook 命令等都用虚构值。
- [ ] `FindingKind::Unparsed` 进了 schema 和 TS 生成物，5.3.11 的那一行已改。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T096 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T096-trust-findings/ci/` 通过，摘要随任务提交。

## 注意

- 只解析内存里的内容，不读本机真实的项目、`~/.claude`、`~/.codex`。
- 代码约束：`cupper-host` 不用 `unsafe`；错误用 thiserror；导出给其他语言的产物不依赖 serde_json 的 map 键顺序。

## 规模

计划估为 M，T035 的执行者估这一部分约 400 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 600 行（包括贴线）时，必须先停下来汇报**。

## 参考资料

- 设计：`02-host.md` 5.2.6"项目信任"；`03-protocol-v1.md` 5.3.11（`FindingKind`）。
- `docs/tasks/T035-trust-scan/`（`brief.md` 的补充裁定、`notes.md`）。
- `crates/cupper-host/src/trust/`、`crates/cupper-protocol/`（`TrustScan`、`Finding`、`FindingKind`）。

## 工作方式

在原来的 T037 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T096-trust-findings main`。T035 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
