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

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,420 行（实现约 1,010、测试约 410），在写代码之前停下汇报，建议拆成两份。

**裁定：采用两分法（计划 v1.3.24）。**
- **T038 收窄为状态与授权**：
  - 提权纯核心：全局一个未决；10 分钟内被拒 2 次后静默 10 分钟；2 分钟过期；按连接授权，最长 15 分钟；断开即撤销。
  - `IndexActor` 接线：三个方法、定时器、`elevation.*` 与 `clients.updated` 的 `elevated`、审计。
  - 网关里连接与提权共享的已授 scope。
  - 时长与审计事件。
  - 验收：新增 UNIT-elevation-core，以及 IT-elevation-grant（进程内：授予、scope 更新、断开与过期撤销）。
  - 上限约 900 行，硬停止线 1,000；推算贴线也先停下。
- **新增 T133（血缘、核实信息与 IT-elevate）**：`cupper-sys` 的三处补充、血缘三信号、核实信息与"是否自带 CLI"（协议加字段）、探针与假 agent 扩展、IT-elevate（由本任务移来），新增 UNIT-lineage-signals。依赖 T038，T046 改为也依赖 T133。`tests/ids.toml` 已在 main 上改好。
- T133 会提前派给另一个执行者，先做不依赖本任务的部分。本任务合并后，由编排者把 main 合进 T133 的分支，再接上本任务的接口、写 IT-elevate。**所以本任务定下的窄接口要尽早稳定**，形状写进 `notes.md`。

**对汇报里几点的答复**：
1. **协议字段**：`bundled_cli` 一类的字段由 T133 加，本任务不改协议。
2. **状态放在 `IndexActor`**：认可。定时器经 `Clock` 和 `TimerTable`，可以照会话 actor 的 `Alarm` 做法；决策写在纯核心里，核心不做 I/O、不 await。
3. **共享的已授 scope**：认可。每次调用在受理时读这个格子；撤销之后的调用按撤销后的 scope 判定，已经受理、正在执行的调用照常完成。把这条写进 `notes.md`。固定令牌随 `Opened` 交给索引。
4. **窄接口在 T133 接上之前的生产实现必须 fail closed**：血缘一律判为"无法排除是适配器的后代"，`auth/elevate` 返回 `forbidden`（`lineage`）。这样 main 上任何时候都没有不经血缘检查就能进入确认的路径。IT-elevation-grant 用一个测试实现，判为"不是后代"。
5. **时钟**：T047 已合并，test-harness 构建里有 `test/clock_advance` 和手动时钟。本任务的进程内用例直接用 `ManualClock`；T133 的进程级用例经 `test/clock_advance` 推进，不靠缩短的真实时长等待。

## 第 2 次补充裁定（2026-10-20，编排者）

执行者做完纯核心（490 行，该模块估 380，多约 29%），推算总量约 1,000 行，碰到硬停止线，停下汇报。

**裁定：不再拆分，放宽上限（计划 v1.3.25）。**
- IT-elevation-grant 是本任务的核心验收，拆走或移给 T133 都会增加两边的协调，得不偿失。
- 规模改为 L。剩余模块按已做部分的实际偏差（约 +30%）推算，上限约 1,150 行，硬停止线 1,250；推算贴线也先停下。
- IT-elevation-grant 尽量复用网关已有的进程内夹具（`Peer` 等），不另起一套。

**窄接口认可**：`Requesters::examine(AuditToken) -> Examined`（`Clear(VerifiedRequester)` 或 `Descendant { signals }`），生产实现 `FailClosed`，经 `IndexDeps.requesters` 注入。先查血缘、再查限频的顺序与 5.8.4 的时序图一致。未决期间第二个申请的 `retry_after` 取未决申请剩余的时间，认可。
