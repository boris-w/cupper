# T038 CLI 提权（宿主侧）

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060、T098、T044、T109 的执行者接手
- 分支：`feat/T038-cli-elevate`
- 依赖：T034、T055（都已合并）。T077（原生确认框）、T090（e2e 的 CLI 提权辅助）依赖本任务；本任务离剩余关键路径有 3 个单位的松弛

## 背景

协议这边已经齐了：`auth/elevate`、`auth/elevation_get`、`auth/elevation_respond`（`cupper-protocol` 的 `methods/admin.rs`），索引流的 `elevation.requested`、`elevation.resolved`、`clients.updated` 里的 `elevated`（`index.rs`），`policy/method.rs` 已经只让 CLI 发 `auth/elevate`。宿主还没有实现这些方法，也没有限频、过期和授权绑定。本任务做宿主侧的全部逻辑；原生确认框和横幅是 T077、T081 的事。

## 范围

**做**（计划 T038 一行；设计 5.8.4、5.8.6；5.2.4 的连接与 scope）：
1. **`auth/elevate`**：
   - **血缘检查**（辅助信号，设计 5.8.4 v1.6 的结论）：对端与某个适配器同一个会话、同一个进程组，或者祖先链里有适配器，就返回 `forbidden`（`lineage`），不弹框、记审计。适配器的 pid、pgid、会话从运行中的 `AgentRuntime` 或 `adapters` 表取（选一个并在 notes 里说明理由；要考虑适配器重启后的新实例）。
   - **各信号的期望按 T003 的结论设定**：Claude 的工具命令（`zsh -c` 及其子进程）在 CLI 新建的会话和进程组里，只有祖先链会命中；CLI 本身拉起的 MCP、hooks 留在适配器的进程组里，前两个信号能命中。命令经 nohup、双重 fork 或 disown 脱离父进程后三个信号都失效，这是已知局限，不是缺陷。
   - **限频**（安全 #8）：全局同一时间只允许 1 个未决；10 分钟内被拒 2 次后静默 10 分钟，期间直接返回 `rate_limited`（带 `retry_after_ms`），只计数；2 分钟没有决定就过期。三个时长都接 `test_config.durations`。
   - 通过以上检查后生成申请，记下对端的 pid、可执行文件路径、签名身份、父进程链、有没有控制终端；在索引流写 `elevation.requested`；`auth/elevate` 的回复挂起，等用户决定或过期。
2. **`auth/elevation_get`**（control 连接）：先给宿主核实过的信息（可执行文件路径、签名身份或"未签名"、pid、父进程链；父进程链断在 launchd、或没有控制终端时标"无法确认来源"），以及申请方是否为本 App 自带的命令行工具（路径与 cdhash 都相符）；再给申请方自述、未经核实的 `client_name`、`reason`（各限 200 字符，超长截断；转义显示是界面的事，宿主原样给出截断后的文本）。
3. **`auth/elevation_respond`**（control 连接）：
   - `grant`：授权绑定到申请的那条连接和它的固定令牌，最长 15 分钟；approve 只有在 `scopes` 里明确给出时才授予；回复 `auth/elevate` 为 `granted` 和 `expires_at`；写 `elevation.resolved`、`clients.updated` 的 `elevated`。
   - `deny`：`auth/elevate` 回复 `granted` 为空（协议约定，不是错误），计入"10 分钟内被拒 2 次"。
   - 2 分钟没有决定：`auth/elevate` 返回 `timeout` 错误，写 `elevation.resolved`。
   - 用户批准的范围可以比申请的少，`granted` 照实给出。
   - 连接断开立即失效：挂在 `IndexPort::closed` 上（T033 的交接）；过期同样撤销。
   - **scope 的更新**：`Dispatch` 里的 principal 在 `initialize` 时定下、之后不变（T033 的交接）。授予提权要让这条连接的 scope 能更新，改这里，并保证权限矩阵（`policy/matrix.rs`）按更新后的 scope 判定。
4. **审计**：`elevation.*`（申请、批准、拒绝、过期、因血缘被拒、被限频；申请范围和血缘信号）经现有的审计发送端（与 `Gateway::new` 同一个），落盘是 T059 的事。

**不做**：原生确认框与提权横幅（T077、T081）；审计落盘（T059）；`responsibility_*` 私有 SPI（SP-10）。

## 验收标准

- [ ] **IT-elevate**（进程级，真实宿主加假 agent），覆盖：
  - 适配器的后代被拒：经祖先链识别的工具命令（假 agent 拉起的子进程，自己 setsid）；与适配器同一进程组的 MCP 或 hooks（假 agent 拉起、不 setsid 的子进程）；
  - 非后代的申请进入未决、写 `elevation.requested`，`elevation_get` 给出核实信息，`grant` 后这条连接能调用 write 类方法，另一条连接不能；
  - 全局一个未决；被拒 2 次后静默、返回 `rate_limited` 和 `retry_after_ms`；2 分钟过期；
  - 连接断开即失效；15 分钟后过期失效。
- [ ] 限频、过期的判定写成纯核心，用 `ManualClock` 做单元测试；进程级用例经 `test_config.durations` 缩短时长，不睡眠。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T038 段里（需要新增单元测试编号时补进该段）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T038-cli-elevate/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T038 的行；开工前逐行读一遍）

- T020：`parent_chain` 能经过 root 进程走到 launchd；进程名是内核里的 16 字节短名，要完整路径需另取。
- T033：撤销提权挂在 `IndexPort::closed` 上；`Dispatch` 的 principal 在 `initialize` 时定下，授予提权要让 scope 能更新。

## 注意

- 用例只向测试自己拉起的进程取血缘信息、发信号；结束后确认没有残留进程。
- 不运行真实的 Claude Code、Codex CLI；用例一律用假 agent。
- 代码约束：时间只经 `Clock`；有状态的决策逻辑写成纯核心，核心里不做 I/O、不 await；子进程一律经 `cupper_sys::spawn`；`unsafe` 只在 `cupper-sys`（要新的系统调用，例如取控制终端、`getsid`，就加到 `cupper-sys`）。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 900 行（包括贴线）时，必须先停下来汇报**，并给出拆分建议。

## 参考资料

- 设计：`08-security.md` 5.8.1（固定令牌）、5.8.2、5.8.3、5.8.4、5.8.6；`02-host.md` 5.2.4；`03-protocol-v1.md` 里 `auth/*` 与 `elevation.*` 的部分；`09-testing.md` 5.9.9（`test_config.durations`）。
- `docs/tasks/` 下 T003、T020、T033、T034、T055、T008 的 `notes.md` 和 `review.md`。
- `crates/cupper-host/src/gateway/`、`policy/`、`index/`、`driver/runtime/`。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T038-cli-elevate main`。T109 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
