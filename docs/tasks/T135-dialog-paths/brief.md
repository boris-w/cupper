# T135 原生确认框补项目路径与"自带 CLI"提示

- 派发日期：2026-10-20
- 执行者：子 agent（Electron 工程师）。T077、T134、T136 的执行者接手
- 分支：`feat/T135-dialog-paths`
- 依赖：T133、T134（都已合并）。T016（契约冻结）依赖本任务

## 背景

T134 的评审发现：信任项目、新建会话的原生确认框里看不出是哪个项目。`project/trust_info` 和 `policy/describe` 的 `details` 都只给 `project_id`，不给路径。

原生确认框的威胁模型是"渲染进程可能被攻破"：被攻破的渲染进程可以请求信任另一个项目，确认框只列 findings，用户分不出是哪个项目。

另外，T133 已给 `VerifiedRequester` 加了 `bundled_cli`，提权框要用上（T134 的交接）。

## 范围

**做**（计划 T135 一行；设计 5.7.8 的"原生确认框"、5.8.4、5.2.6）：
1. **信任项目的确认框显示项目路径**：给 `project/trust_info` 的结果加 `path`（宿主解析后的绝对路径），协议改动，重新生成 TS 类型、zod 和 schema。`add_project` 走的也是这个确认框，一并显示。
2. **新建会话的确认框显示项目路径**，内容必须来自宿主。两种做法选一种，理由写进 `notes.md`：
   - (a) `policy/describe` 对 `session/create` 的 `details` 加 `project_path`：`describe` 现在是不查库的纯函数，要给它一个按 `project_id` 查路径的入口；
   - (b) 主进程照 T134 处理 `confirm_config` 的做法，在 control 连接上订阅一次索引流，取该项目的 `path` 后退订。
3. **提权确认框**：`bundled_cli` 为假时，醒目标出"申请方不是本 App 自带的命令行工具"，文案放 `privileged` 命名空间。
4. 路径逐字显示，不可见字符用 `escapeInvisible` 转义。路径太长时，确认框里截断的方式要保证看得出是哪个目录，例如保留开头和结尾，在 notes 里说明。

**不做**：渲染进程界面；其他 action 的内容（已由 T134 完成）。

## 验收标准

- [ ] UNIT-privileged-paths：
  - 信任项目（含 `add_project`）和新建会话的确认框里有宿主给出的项目路径；
  - 路径里的不可见字符被转义；超长路径的显示方式；
  - `bundled_cli` 为假时提权框有醒目提示，为真时没有。
- [ ] 协议改动后 `pnpm gen` 的生成物一致，协议的 Rust 与 TS 测试通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T135 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T135-dialog-paths/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T135 的行）

- T134：`dialogs.ts` 的 `elevationBox`；`trust_project`、`create_session` 的确认框现状。
- T133：`bundled_cli` 在开发和测试构建里恒为否（ad-hoc 签名）。签名构建出来后实测一次为真的情况；本任务只用单元测试覆盖两种取值。

## 注意

- 中文文案的字要在截图夹具的字体子集里（T118 的 `fonts.test.ts` 会检查全部 locale 文件；T134 曾因 `⚠` 失败）。
- 生产代码里不加测试分支；`dialog.*` 在调用时才取。

## 规模

S，上限约 250 行（不含生成物）。超出就停下汇报。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T135-dialog-paths main`。T136 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
