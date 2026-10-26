# cupper

macOS 桌面 agent 客户端：复刻 Codex 桌面版的界面，经 ACP 驱动本机的 Claude Code 和 Codex CLI；M3 加 iOS 端（端到端加密）。PRD 见 `docs/01-prd.md`，技术设计见 `docs/02-design.md`（第 5 节索引 `docs/design/`），决策见 `docs/decisions/`。

## 技术栈

| 层 | 选型 | 依据 |
|---|---|---|
| 会话宿主 | Rust（tokio、rusqlite、`agent-client-protocol =3.3.0`），独立进程，打包为 `Cupper.app/Contents/Helpers/CupperHost.app` | ADR-01、ADR-02 |
| 宿主协议 | 自有 JSON-RPC，分帧 `u32 长度 \| u8 类型 \| 负载`，单帧 ≤ 64,000 字节；schema 以 Rust 类型为源，生成 TS 类型和 zod | ADR-04 |
| 本地认证 | Unix 套接字 + `LOCAL_PEERTOKEN` + 代码签名校验 + 固定令牌 | ADR-05 |
| 事件与存储 | 宿主自己的事件日志（SQLite WAL）是唯一的历史来源 | ADR-06 |
| Agent 接入 | 宿主经 ACP 驱动 claude-agent-acp 和 codex-acp，每种 agent 一个适配器进程；Node 运行时随 App 带、用 cupper 身份重签 | ADR-07、08、09 |
| Git | 只用 git 命令行；自动调用分快照、仓库、用户三档 | ADR-10 |
| 桌面 UI | Electron 44 + React 19 + TypeScript；electron-builder + electron-updater | ADR-03 |
| 许可证 | Apache-2.0；Anthropic 的 SDK 和 Claude Code 二进制绝不进入仓库和产物 | ADR-11 |

## 目录结构与常用命令

产品代码从 M1a 起按 `docs/design/01-repo-build-ci.md` 5.1.2 建立：`crates/`（Rust：protocol、sys、host、cli、fake-agent、desktop-native、xtask）、`apps/desktop/`（Electron）、`packages/`（TS：protocol、host-client、shell-api、ui-core、i18n）、`protocol/`（schema 与样例）、`runtime/`、`testdata/`、`tools/`。`spikes/` 只是原型实测代码，产品代码不依赖它。

常用命令（工程建立后可用，完整列表见 5.1.5）：`pnpm i`、`pnpm dev`、`pnpm gen`（导出 schema 并生成 TS）、`pnpm lint`、`pnpm test`、`pnpm test:golden`、`pnpm test:e2e`、`cargo xtask runtime-pack`。`cargo xtask compat` 会用真实 CLI 并消耗额度，只由维护者手动运行。

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
- 远端 `origin` 是 GitHub 上的公开仓库 `boris-w/cupper`（2026-10-26 起，见 `docs/meetings/2026-10-26-github-remote/`）。只有编排者 push，子 agent 不 push。
- 每个任务一个 PR：编排者推任务分支、开 PR，GitHub CI 通过后以合并提交合入 `main`（与 `--no-ff` 同形），再删掉远端分支。编排者直接提交到 `main` 的文档随后推送。合并以 PR 上 GitHub CI 的 8 个检查（`ci / <作业> (<runner>)`）为准，main 的保护规则要求它们通过；本机只跑与改动相关的检查，不再提交本机 CI 的摘要（2026-10-26，T197 起）。
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

## 代码约束

来源：ADR-02、SP-4a 的实测结论、详细设计 5.1.7。违反时 review 不通过。

**Rust**
- 运行时用 tokio，模块之间用 channel 通信；ACP 回调里只做 channel 发送，耗时的工作另开 task。
- `unsafe` 只能出现在 `cupper-sys`；错误类型用 thiserror；共享的 crate（`cupper-protocol` 的库部分）不做 I/O。
- 依赖 SDK 时锁定精确版本；导出给其他语言的产物不依赖 serde_json 的 map 键顺序。
- 协议里的整数不超过 2^53−1。每个协议方法必须声明 `SCOPE` 和 `IDEMPOTENCY`。
- 每条事件流只有一个写者，其他模块只能发消息请写者追加事件。
- 时间只经 `Clock` 获取，定时只经 `Clock` 设置，UUIDv7 只经 `IdGen` 生成（clippy `disallowed-methods` 强制）。有状态的决策逻辑写成纯核心，核心里不做 I/O，也不 await。
- 宿主运行期间不读取 App 包内的文件；唯一的例外是运行时安装器按内嵌清单核对哈希后的复制。
- 宿主、CLI、原生扩展拉起任何子进程，都经 `cupper-sys::spawn`。

**TypeScript / Electron**
- `ui-core` 不 import `electron`、`node:*`；渲染进程不用 `innerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`eval`、`new Function`。
- 只有 `ui-core/src/security/policies.ts` 可以创建 Trusted Types 策略，禁止名为 `default` 的策略；agent 给出的字符串不能进入 `<style>` 文本、`insertRule` 或 CSS 自定义属性。
- 主进程在调用时才取 `dialog.showMessageBox` 这类函数，不在模块加载时解构保存。
- 界面文案一律走 `@cupper/i18n`；宿主只下发语义码，不下发文案。

## 子 agent 工作守则

1. 只在分配给你的 worktree 和分支里工作。不切换到其他分支，不修改 `main`，不执行 merge、rebase 或 push。
2. 开始前先读本任务的 `docs/tasks/<ID>-<slug>/brief.md`，以及 `docs/01-prd.md`、`docs/02-design.md`、`docs/03-plan.md` 中的相关部分。
3. 只做任务范围内的事。发现范围外的问题时记录下来，在汇报中提出，不要顺手改。
4. 按任务的验收标准自测（构建、测试、lint 都通过）后再提交。
5. 过程中的思路、试错和取舍可以记在同目录的 `notes.md`；调研数据、截图、基准结果等物料也放在该目录。
6. 结束时按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`，由编排者存档并提交。`notes.md`、资料摘录等其他物料仍由你在分支上提交。
7. 调研或测试时，任何外发请求（HTTP 头如 User-Agent、查询参数、表单、提交到外部服务的内容）都不能包含用户的邮箱、姓名、账号等个人信息。API 要求填联系方式时，用项目占位信息，或者换一个不需要联系方式的来源。
8. 每完成一个模块或一个提交，就统计一次改动行数（含测试，不含 lock 文件和生成物）。一旦预计会超出任务书写的规模上限，立即停下来汇报，由编排者决定是否拆分；不要做完了才发现超限。
9. 要在 GitHub 上跑 CI 时，先在本机提交，再用 SendMessage 发给 `main`："请推送 `<分支>`，提交 `<sha>`"，由编排者推送并开 PR。`gh` 只用只读命令（`gh pr checks`、`gh run view --log-failed`、`gh run download` 等），不 push、不开或合并 PR、不改仓库设置。汇报里写上 PR 和运行的链接。
