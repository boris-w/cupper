# T187 兼容套件 II：真实模式与剧本补全

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师）。T064 的执行者接着做（T043、T055、T008、T116、T046、T090、T167、T091、T092、T185、T063、T064）
- 分支：`feat/T187-compat-real-mode`
- 依赖：T064（已合并）。T066 依赖本任务

## 背景

T064 做了兼容套件的框架和 `--fake`：编排、"基本"组 10 个剧本、执行者、报告与 schema、录制入库、`ci compat-framework`。真实模式在 T064 里一律报错，计划 v1.3.55 把它和其余剧本拆给本任务。T066 由维护者用真实 Claude 跑兼容套件，在那之前真实模式的代码路径要靠单元测试和 `--dry-run` 验证到位。

## 范围

**做**（计划 T187 一行；设计 5.9.5）：
1. **真实模式的自我约束**（UNIT-compat-guards）：
   - 每个剧本显式设权限模式：Claude 用 `default`，Codex 用 `read-only` 或 `workspace-write`；
   - 拒绝 privileged 选项和放宽档位，除非剧本声明了、而且只作用于临时仓库；
   - 只选 `allow_once`、`reject_once`；
   - 临时仓库只建在每个用户自己的临时目录下。
2. **真实配置的前后哈希对比**（U-R4、安全 #8）：
   - 运行前后对 `~/.claude/settings.json`、`~/.codex/config.toml` 等真实配置做哈希，只读不改，写进报告的 `config_files`，有变化就判失败；
   - 测试里用临时目录代替 HOME。
3. **真实模式的运行**：
   - 宿主以真实配置启动，agent 由宿主的发现找到；
   - 每个剧本的真实提示词与准备文件；
   - 报告的 `cli_sessions` 列出需要清理的 CLI 会话；
   - `--write` 在真实模式下把报告写进 `testdata/compat/reports/`，文件名由你定（T188 校验这个目录下的全部报告）。
4. **`--dry-run`**：列出要跑的剧本、每个剧本的权限模式与准备文件、要做哈希的配置文件，以及自我约束的检查结果，不启动任何 CLI。
5. **其余 11 个剧本**（5.9.5），`--fake` 能跑的都要跑：
   - CLI 或 app-server 被杀后恢复；宿主重启后 resume；
   - 图片输入：先上传 blob，再发带图的 prompt。协议里这条路径还没通的话，记"不适用"并写明缺什么（T064 第五点确认）；
   - Steer：记"不适用（M1b）"并写明原因；
   - Q8：两个会话在同一个适配器上并发 prompt，通过条件见 5.9.5；
   - 6 个会话并发（COMPAT-claude-six-sessions）；
   - 关闭时有命令正在运行（COMPAT-claude-close-running，K21）：关闭之后命令及其子进程都已退出；
   - 多次 resume 同一个会话后的进程数（COMPAT-claude-resume-procs，Q43）；
   - 未信任项目（COMPAT-untrusted-project-claude，安全确认 ③）：项目的 hooks 不运行、`.mcp.json` 里的服务器不启动、项目的 `env` 和 `apiKeyHelper` 不生效；
   - Q28：回合中改写 `.claude/settings.json` 的 hooks，看当前回合内是否生效；
   - Q34：在上级目录和 git 根目录之外各放一份带 hook 的配置，看哪些会生效。
   - Q28、Q34 是观察型剧本：报告里记下观察到的结果，不按通过或失败判；假 agent 模拟不了 CLI 读配置的行为的，`--fake` 下记"只在真实模式运行"。
6. **`lsof` 断言**（安全建议 #19）：每个剧本跑完，适配器的后代都不持有宿主的管道和套接字，也没有遗留的进程。
7. **报告的结果取值**：要加"不适用"或"观察"一类的取值时，以附加方式改 schema 并重新生成 `testdata/compat/report.schema.json`。

**不做**：用真实 CLI 跑（T066，维护者手动）；testdata-scan 的报告校验、已验证范围重算、`rec-shape-diff`、`trace-ids --compat`（T188）。

## 验收标准

- [ ] UNIT-compat-guards：上面第 1 条的每一项都有用例，含拒绝的情况。
- [ ] 前后哈希对比有用例：配置不变时通过、被改时判失败；用临时目录代替 HOME。
- [ ] `--dry-run` 能列出完整的计划，不启动任何 CLI。
- [ ] `cargo xtask compat --fake` 跑全部 21 个剧本，结果都是通过、不适用或"只在真实模式运行"，没有失败。
- [ ] 三份 B 层录制用 `--fake --write` 重新产出；与入库的差异（显式权限模式带来的 `config` 变化等）在汇报里逐条说明。
- [ ] `cargo xtask ci compat-framework` 通过。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T187 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T187-compat-real-mode/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T187 的行；开工前逐行读一遍）

- T064：真实提示词、权限模式、准备文件、自我约束都留给本任务；`session/create` 带显式权限模式后三份 B 层录制会变；cancel 剧本在真实 CLI 上是"允许后等 1 秒再中断"，`sleep 60` 没有输出，可靠性要在 T066 实际跑时确认。
- T063：多次 resume 后宿主侧不泄漏进程的做法（假 agent）。
- T003：Q8 的通过条件与 2 个会话的测法。
- T116、T029：录制导出、blob 分片。
- T188 同时在做：它会在 `tests/ids.toml` 里给 COMPAT 编号登记剧本与 CLI 的对应。你新增剧本的编号（`id`）定下来后不要再改；与它的改动在合并时由编排者处理。

## 注意

- **本任务里不运行真实的 Claude Code、Codex CLI**，只用 `--fake` 和 `--dry-run`；真实模式的代码路径靠单元测试验证。
- 不读、不改用户的 `~/.claude`、`~/.codex`；哈希对比的代码在测试里用临时目录代替。
- 入库的录制与报告不含个人信息。
- 代码约束：时间只经 `Clock`；拉起子进程经 `cupper-sys::spawn`（宿主、CLI 一侧）。

## 规模

- 停止线：850；硬停止线：950

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报，并给出拆分建议。 -->

## 工作方式

在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T187-compat-real-mode main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-26，编排者）

执行者开工前估计约 1,200 行（区间 1,050–1,300），超过硬停止线 950，在写代码之前停下，建议拆分。

**裁定：方案一（计划 v1.3.60）。**
- **T187 收窄为"真实模式与安全约束"**，约 660 行，UNIT-compat-guards 留下。内容：
  - 自我约束、配置哈希（用临时目录代替 HOME）、`--real` 与 `--dry-run`、真实宿主的启动；
  - 基本组 10 个剧本的真实提示词与准备文件；
  - 报告的新结果取值与 schema、`cli_sessions`；
  - 真实报告写进 `testdata/compat/reports/`，录制入库；
  - `lsof` 断言与遗留进程检查；
  - Steer 剧本记"不适用（M1b）"。
- **新增 T192（剧本补全）**，约 540 行：其余 10 个剧本（`crash-resume`、`host-restart`、`image`、`concurrent-two`、`six-sessions`、`close-running`、`resume-procs`、`untrusted-project`、`hooks-reload`、`config-scope`），以及它们需要的步骤和执行者能力。T192 在 T187 合并后由你接着做。
- 验收标准里"`--fake` 跑全部 21 个剧本"改为"跑基本组 10 个剧本和 Steer"。
- **剧本编号**以 `notes.md` 里定下的 21 个为准，之后不改。

**四点确认**：
1. **真实模式用哪个宿主**：用 `cargo xtask host-bundle --kind e2e --debug` 产出的 `CupperHost.app`，执行者经 `spawn_from` 启动，环境取自登录 shell（这样 PATH 里才找得到 `claude`）。认可。宿主自己的环境过滤照常生效，不要为兼容套件放宽。
2. **进入真实模式要显式写 `--real`**。只写 `--fake`、`--real` 之一才运行，两个都不写或都写，就报错并说明用法；`--dry-run` 两种模式都能用。这样多一道防误跑的保护，免得不小心消耗额度。
3. **要算哈希的文件**照 T003 的 `spikes/T003-tech-validation/config-hash.ts`：
   - `~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.codex/config.toml` 按整文件算；
   - `~/.claude.json` 是 CLI 平时就会改写的状态文件，不按整文件算，只对和权限有关、会拉起程序的字段逐个算：顶层的 `mcpServers`、`bypassPermissionsModeAccepted`、名字像权限的键，以及每个项目的 `allowedTools`、`hasTrustDialogAccepted`、各类 MCP 服务器开关；
   - 已有的值变了，判失败；新出现的项目条目只属于本次运行的临时仓库时，列进报告里需要清理的项（与 `cli_sessions` 放在一起），不判失败；其他新增，判失败；
   - 报告里只写文件名、字段名和是否变化，不写值，也不写本机路径。凭据文件不算哈希。
4. **Q8 检查"没有串话"要看文本**：每个 prompt 让 agent 原样回一个由执行者生成的标记，检查每个会话的回答里只有自己的标记。认可，这是"只断言结构"的唯一例外，标记由执行者生成、不含 agent 自己的话。Q8 归 T192。

**规模**：

- 停止线：720；硬停止线：800

**补充（2026-10-26，来自 T188 的发现）**：真实模式加 `--write` 时只接受全量运行，带 `--only` 就报错。原因是 T188 计算已验证范围时，计入"所有剧本都通过"的真实报告；部分运行的报告入库会把范围算错。这条的单元测试算在 UNIT-compat-guards 里。
