# T201 运行时装好之前启动适配器

- 派发日期：2026-10-28
- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T201-runtime-install-wait`
- 依赖：无（T066 发现）。T099 依赖本任务

## 背景

T066 用真实 Claude 冒烟时发现，宿主在运行时装好之前就启动适配器：
- 宿主启动后在后台安装运行时：node、esbuild-wasm、claude-agent-acp，约 9.4 s，从 npm 下载。
- 执行者在宿主启动后约 0.2 s 发出 `session/create`，第一回合 37 ms 就以 `failed`、`error_code: runtime_failed` 结束，会话进入 error。
- 9 s 后安装完成（`agents.updated` 里 runtime 变成 `ready`），但已经晚了。

原因（T066 的执行者查明）：
- `crates/cupper-host/src/driver/runtime/launch.rs` 的 `start` 遇到 `Program::Runtime` 时，直接核对 node 和打包文件的哈希，文件还没装好就核对失败，返回 `RuntimeFailed`；
- `crates/cupper-host/src/supervisor/mod.rs` 在后台起安装器（T129），但启动适配器之前不等它；
- 假 agent 不走 `Program::Runtime`，所以以前的用例都没暴露。

设计 5.2.12 的"时机"写的是："某个 agent 需要启动适配器而组件还没就绪时，等待安装完成"。产品与设计不符。用户第一次用 Claude、App 升级换了适配器版本之后，安装完成之前发出的第一个 prompt 都会直接失败。

## 范围

**做**（计划 T201 一行；设计 5.2.12、5.6.2）：
1. 适配器要启动、而它需要的运行时组件还在安装时，等安装结束：
   - 装好了就照常启动；
   - 安装失败时，回合按原来的 `runtime_failed` 结束，错误信息里带上安装失败的原因；
   - 等待期间不占住驱动层的其他工作，不阻塞别的 agent 和别的会话（ACP 回调里只做 channel 发送，耗时的工作另开 task，见 `CLAUDE.md` 的代码约束）。
2. 等待怎样被观察到：沿用现有的状态，`agents` 里 runtime 的 `installing`，不新增协议成员。契约已冻结，真要加只能附加，先汇报。
3. 新增 IT-runtime-first-prompt-waits（进程级），登记进 `tests/ids.toml`：
   - 用本地 registry 放慢下载，在安装完成之前发 prompt，回合在安装完成后正常运行；
   - 安装失败时，回合以 `runtime_failed` 结束；
   - 时间经 `Clock` 和故障点控制，不用 sleep 赌时序（设计 5.2.16）。
4. 宿主运行期间不读 App 包内的文件这一条不变：唯一的例外仍是安装器按内嵌清单核对哈希后的复制。

**不做**：改安装器本身的下载、复制、自检（T056、T129）；改兼容套件。T066 在套件里先等运行时就绪再建会话，那一步留着。

## 验收标准

- [ ] IT-runtime-first-prompt-waits 通过，有修前修后对照：修复之前，安装完成前发 prompt 的用例失败。
- [ ] IT-runtime-startup、IT-runtime-install、IT-runtime-download 照旧通过。
- [ ] 本机单份跑 `cargo xtask ci rust` 里与改动相关的测试；不要循环、并行重跑（守则第 10 条）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：200；硬停止线：260

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.2.12（运行时安装的时机）；5.2.13；5.2.16（时间与可测性）；5.6.2（AgentRuntime）；ADR-08。
- 卷宗：T055（AgentRuntime）、T056、T115、T129（安装器和它接进宿主启动的方式，IT-runtime-startup 的夹具）；T066 的 `notes.md`（现象和复现）。
- `docs/03-plan.md`：T201 一行。

## 注意

- 不运行真实的 Claude Code、Codex CLI；用假 agent 和本地 registry。
- 不读、不改用户的 `~/.claude`、`~/.codex`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
