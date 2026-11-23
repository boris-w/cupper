# T113 Codex 兼容矩阵与 B 层补录

- 派发日期：2026-11-23
- 执行者：子 agent（CI 与工具；T102 之后各次 codex-drift 审阅的执行者）
- 分支：`test/T113-codex-compat`
- 依赖：T112（已合并）。T273、T157 依赖本任务

## 背景

作者 2026-11-23 同意由子 agent 用本机已登录的真实 Codex 跑本任务，消耗的额度作者同意（`docs/meetings/2026-11-23-manual-acceptance/` D4）。录制和报告入库前由编排者审阅，作者抽查改为可选。

## 范围

**做**（计划 T113 一行；设计 5.9.5、5.9.3）：

1. 用真实 Codex 跑兼容套件：Q8 复核、Q25、Q34、未信任项目拒绝启动（COMPAT-untrusted-project-codex）。
2. 用宿主重录取消场景，补 K3 的 B 层金样例（GOLD-B-codex-s07）。
3. 已验证范围写进清单（MAN-compat-codex 的证据，格式见 `docs/manual/README.md`）。
4. 交接事项里接收任务含 T113 的各行（十几行）逐条处理或说明不处理的原因，重点：
   - `tools/sanitize` 认 Codex 的 MCP 服务器名（T102、T204），Claude 的 MCP 名要不要也按取名处理在本任务定；
   - 剧本 prompt 点名使用的工具、Codex 版的"关闭时有命令正在运行"；
   - compat 真实模式的拉起方式还没真跑过（T203）；`xtask compat --out` 只认绝对路径；
   - codex-drift 积累的影响项（imageGeneration 的 `fileId`、音频 `fileId`、`moderation`、错误类别映射）：本任务只核对和记录，升级适配器不在本任务。

**开工前先估算**：交接事项多，先按"套件运行、金样例重录、sanitize 改动、其余交接"分块估行数和额度，报给 `main`。超过 M 的规模时由 `main` 拆分。

**真实运行的约束**（P-14 及计划第 1 节）：
- 只在临时仓库里运行；不读写 `~/.codex`、`~/.claude`（包括 `auth.json`）；
- 真实 Codex 运行前，`main` 请作者用一条命令备份 `~/.codex/config.toml` 和 `hooks.json`。轮到真实运行时先告诉 `main`，等确认备份好了再跑；
- 运行后报告列出 Codex 配置的差异（由作者核对的部分，`main` 转达），新增的 `[projects]` 信任项单独列出；
- 外发请求不带个人信息；Codex 走作者的自定义网关时会因过载重连，结果"不确定"时重跑（T102）。

**入库前**：录制和报告过 `tools/sanitize` 和本机敏感词清单（`.private/sensitive-words.txt`），结果和清单一起交 `main` 审阅。

**不做**：升级 codex-acp；改 codex-drift 工具。

## 验收标准

- [ ] COMPAT-untrusted-project-codex、GOLD-B-codex-s07 通过；MAN-compat-codex 有证据。
- [ ] 交接事项逐条有结论。
- [ ] 入库内容过敏感词扫描、经 `main` 审阅。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

开工前估算后由 `main` 定停止线。

## 注意

- 守则第 1 条：不执行 merge、rebase。`gh` 只用只读命令。
- 守则第 7 条：外发请求不带作者的邮箱、姓名、账号。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `test/T113-codex-compat`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
