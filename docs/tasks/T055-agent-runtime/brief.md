# T055 驱动层 II：AgentRuntime 与 Claude 接入

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T088、T097 的执行者接手，因为本任务要把它写的会话管理器组装进宿主
- 分支：`feat/T055-agent-runtime`
- 依赖：T027、T028、T053、T054、T094（都已合并或合并中）。T047、T063、T068 等依赖本任务；本任务离剩余关键路径有 3 个单位的松弛

## 背景

驱动层的零件都已就位：线路分接与 ACP 连接（T050）、Claude 提取层与能力提取（T053）、CLI 发现与环境变量规则（T054）、登录 shell 与版本检测（T094）、日志与适配器 stderr（T028）、原始帧（T027）。会话这边，会话管理器（T043）、交互代理接线（T088）已经合并，但宿主还没有把它们组装起来：`Host::start` 里没有会话管理器，也没有 `AgentRuntime`。本任务做 `AgentRuntime`，并把会话管理器和它一起组装进宿主（编排者指定，见交接）。

## 范围

**做**（计划 T055 一行；设计 5.6.1、5.6.2、5.6.7、5.6.8、5.8.5）：
1. **`AgentRuntime`**（每种 agent 一个 actor，ADR-09）：
   - 拉起适配器：用 T054 的 `CliLocator`、环境变量规则，T094 的登录 shell 环境和版本分类；拉起前核对（Node 的 cdhash、adapter 的 sha-256，ADR-08）；经 `cupper_sys::spawn` 拉起（`POSIX_SPAWN_SETSID` 等，5.6.1）；拉起后立即写 `adapters` 表；stderr 接 T028 的 `AdapterLog`；
   - 路由：`agent_session_id` → 会话；回复和 `session/update` 投进同一个邮箱以保持线上的顺序（K12）；
   - 各驱动命令（5.6.2 的表，M1 不做 `Steer`、`ListSessions`、`Load`）；用 `PromptTag` 回填 prompt 的结果，用 `AskId` 管理 ACP 的 responder（`AskId`、`PromptTag` 由运行时分配，T053 的提取层不分配）；`PromptContent` 转成 ACP 的内容块（图片读 blob 转 base64，评论按 GT-2 排成文本）；
   - 每个会话建 T053 的 `ClaudeSession::new(cwd, config)`；
   - `CommandsChanged` 按会话去重（UNIT-commands-dedup）；
   - 退出广播 `AdapterExited`；重启退避与 `crashed`（时长接 `test_config.durations`）；`cli_too_old` 的判定（适配器报"需要认证"时，按版本分类改判）；
   - 能力缓存与探测目录（5.6.7；`steering.supported` 从 `initialize` 响应顶层的 `_meta` 读取；`capabilities()` 从 CLI 定位那边拿 `Versions`）；
   - 关闭顺序按 5.6.8：先 `session/close`，drop `AcpConnection` 关 stdin，发信号前枚举全部后代，按各自的进程组发信号（T020 的做法）；
   - 资源采样：记下后代的进程组和 `claude` 进程数（Q43），资源采样接口；
   - `agents/capabilities`、`agents/refresh`、`AgentStatus`（语义码到协议字段的映射）；
   - 测试构建的 agent 注入（`test_config`）。
2. **宿主组装**（编排者指定，T037、T043 都把这一步留给了对方）：
   - 在 `Host::start` 里 `SessionManager::new(SessionDeps { streams, writer, readers, clock, ids, durations, agents, quiescence, audit, index })`：`agents` 是本任务的 `AgentRuntime`；`audit` 用交给 `Gateway::new` 的同一个发送端（`supervisor` 里的 `records`）；`index` 在 T034 之前用 `NoIndex`；
   - 用 `routes.sessions(Arc::new(manager))` 登记，不必改用 `Routes::start`；
   - `supervisor::Part` 的顺序：会话管理器第一，`AgentRuntime` 第二（`stop` 里按 ADR-09 关闭适配器）；
   - `raw.sink()` 在 `Host::start` 里取，用 `RawWriter::sink()` 构造 `Tap`。
3. **假 agent 的路径约定**（交接，T051）：cargo 不支持跨 crate 的 `CARGO_BIN_EXE_*`，由本任务定一个拿 `cupper-fake-agent` 路径的做法（例如测试前先构建、按 target 目录推出路径，或由 `xtask ci` 先构建），写进 `notes.md`，后面的任务照用。

**不做**：Codex（T110 起）；`Steer`（T140）；运行时安装器（T056）；进程级的 K 清单用例（T063）。

## 验收标准

- [ ] IT-process-hygiene、IT-K13-echo-cwd、IT-K21-reap-group（补"后代在另一个进程组"：假 agent 用 `setsid` 拉起子进程）、IT-K26-client-info。
- [ ] 新增 IT-adapter-restart（用例直接驱动 `AgentRuntime`）、UNIT-commands-dedup（同样的命令列表不再发 `CommandsChanged`）。
- [ ] 宿主组装后，经网关 `session/create` 能在真实宿主进程里用假 agent 跑通一个回合（可以是一条集成用例）。
- [ ] 不睡眠：核心和宿主内的用例用 `ManualClock`；进程级用例经 `test_config.durations` 缩短时长。
- [ ] 测试名以用例编号开头；编号都在 `tests/ids.toml` 的 T055 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T055-agent-runtime/ci/` 通过，摘要随任务提交。

## 交接事项（都在 `handoff-notes.md`，第一列含 T055 的行；开工前逐行读一遍）

- T020：关闭顺序（`with_descendants()` 记下所有进程组 → 逐组 Term → `Clock` 限时等待 → Kill）；持久化存 `pgid` 和 `start.as_micros()`。
- T050：回复和更新投进同一个邮箱（K12）；`RawSink::bounded()` 只建一个；`steering.supported` 的位置；K28 的规避。
- T051：假 agent 的路径约定（本任务定）；配置只经命令行参数；同一会话第二个在途 prompt 时退出码 70。
- T025：`driver/tap/mod.rs` 有一处文档链接指向私有的 `redact`（T028 之后已挪到 `diag`），`RUSTDOCFLAGS=-D warnings cargo doc` 要能通过。
- T040：`PromptTag`、`AskId`、`PromptContent` 的处理。
- T027：`add_ref` 与 `RawWriter::sink()`。
- T042：`AskRequest` 的交法；撤回之后交互代理仍会发一次 `Answer { Cancelled }`，运行时据此放掉应答句柄。
- T037、T043、T088：宿主组装（见范围第 2 项）。
- T054：`Boundaries`、`CliLocator`、`check_config_dir`、`adapter_env` 的调用方式。
- T094：登录 shell 的初始环境、`TestConfig.login_env`、`xcode-select` 与 `--version` 的环境、语义码映射。
- T028：每种 agent 一个 `AdapterLog`；64 KB 尾部放进 `error` 条目。
- T053：`ClaudeSession`、`AskId`/`PromptTag` 由运行时分配、`CommandsChanged` 去重、`capabilities()` 的 `Versions`。

## 注意

- 不运行真实的 Claude Code、Codex CLI，不登录；用例一律用假 agent。
- 代码约束：ACP 回调里只做 channel 发送，耗时的工作另开 task；子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；每条事件流只有一个写者。
- 发信号的用例只能向测试自己拉起的进程组发；用例结束后确认没有残留进程。

## 规模

计划估为 L，上限约 1,500 行。**本任务范围很宽，按今天各任务的经验几乎一定会超线。开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线就开工前停下汇报，并给出拆分建议。** 可以考虑的拆法：
- 第一部分：实例的拉起与关闭、路由与驱动命令、退出广播与重启退避、stderr、宿主组装（让宿主能跑通一个回合）；
- 第二部分：能力缓存与探测目录、`CommandsChanged` 去重、资源采样、`agents/capabilities`、`agents/refresh`、`AgentStatus`。

**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条），推算贴线也要停下。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.1、5.6.2、5.6.6–5.6.10；`08-security.md` 5.8.5；`02-host.md` 5.2.13（启动顺序、`Part`）；`09-testing.md` 5.9.9（`test_config`、agent 注入）。
- ADR-08、ADR-09。
- `docs/tasks/` 下 T050、T051、T053、T054、T094、T028、T037、T043、T088 的 `notes.md` 和 `review.md`。

## 工作方式

在原来的 T043 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T055-agent-runtime main`。T097 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 2,400 行，在写代码之前停下汇报，建议拆成三份。

**裁定：采用方案一（计划 v1.3.18）。**
- **T055 收窄为运行时核心与宿主组装**：actor、拉起、驱动命令、事件与交互请求、退出广播、宿主组装（`SessionManager` 实现 `Part`，`stop(force)` 让 actor 把运行中的回合记为 `interrupted`）、`tap` 的文档链接、假 agent 的路径约定。验收 IT-process-hygiene、IT-K13-echo-cwd、IT-K26-client-info，以及宿主经网关用假 agent 跑通一个回合。上限约 1,500 行，硬停止线 1,600；推算贴线也先停下。
- **新增 T008（适配器的生命周期）**：重启退避与 `crashed`、关闭顺序与后代进程组、资源采样（含 Q43 计数）、假 agent 的 `setsid` 孙进程；IT-K21-reap-group、IT-adapter-restart 移过去。
- **新增 T009（能力与状态）**：能力缓存与探测目录、`CommandsChanged` 去重、`agents/capabilities`、`agents/refresh`、`AgentStatus`；UNIT-commands-dedup 移过去。
- T008、T009 都只依赖 T055，可以并行；T063 改为也依赖两者，T068 改为也依赖 T009。`tests/ids.toml` 已在 main 上改好。

**拉起前核对的范围**：本任务只做按内嵌清单的核对函数（用造的文件测），以及"注入"和"运行时目录"两种拉起分支；真正的运行时目录由 T056 接上；用例一律走 `test_config.agents` 注入的假 agent。认可。

**假 agent 的路径**：测试辅助按 `CARGO_TARGET_DIR`（缺省为工作区的 `target/`）和当前 profile 推出路径，不存在时提示先构建，测试里不调用 cargo；`xtask ci` 的 rust 作业跑测试之前先构建它。认可，写进 `notes.md`，后面的任务照用。
