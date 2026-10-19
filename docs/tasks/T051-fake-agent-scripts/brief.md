# T051 假 agent：剧本模式与 persona

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，测试基础设施）
- 分支：`feat/T051-fake-agent-scripts`
- 依赖：T001（已合并）。M1a 第 1 波。T052（回放模式）、T055、T062、T063、T068、T090 等集成测试依赖它。

## 背景

自动化测试一律不登录、不消耗额度，只有兼容矩阵会用到真实的 CLI（设计 5.9）。所以宿主的集成测试和端到端测试都要靠一个能按剧本回应的假 agent：它用 ACP SDK 的 agent 角色实现，按 persona 模拟 Claude 或 Codex 的真实形态。SP-4a 已经有一个简单的假 agent，本任务在它的基础上补全。

## 范围

**做**（计划 T051 一行，设计 5.9.2）：
1. 从 `spikes/SP-4a-host-skeleton/crates/cupper-fake-agent/` 迁移假 agent，放到 `crates/cupper-fake-agent`。
2. **启动配置**：persona（`claude` 或 `codex`）、登录状态、故障注入、进程分段。
   - persona 的能力、configOptions、默认模式、命令列表、权限选项 ID，都按 SP-1 录制里的真实形态来写（`spikes/SP-1-adapter-behavior/recordings/`，已经脱敏）。
   - 启动配置怎么传给假 agent（命令行参数或环境变量），由你定。设计 5.9.9 里的 `test_config.agents` 只负责指定程序路径和参数。
3. **剧本命令**：
   - 沿用 SP-4a 已有的命令；
   - 补上 5.9.2 表格里的全部新命令，包括 `flood_rate`、`terminal`、`diff`、`write_file`、`plan_update`、`plan_confirm`、`form`、`steer_*`、`hang`、`late_tool`、`autonomous`、`crash_cli`、`output_then_crash`、`restartable`、`upstream_retry`、`thread_status`、`echo_env`、`auth_none`。
4. **回显收到的 `_meta` 选项**：IT-trust-settingsources 要用它来确认宿主传了 `settingSources: ["user"]`。
5. **原始行通道**：`raw <json>` 原样写出一行 ACP 消息，不经过 SDK 的类型层。
6. **全局不变式**：同一个会话上收到第二个在途的 prompt 时，立即报错退出。

**不做**：回放模式（T052）；录制格式规格（T052）。

## 验收标准

- [ ] 新增 UNIT-fake-scripts：每条剧本命令至少一个用例，在内存或子进程管道上用一个最小的 ACP 客户端驱动；persona 的关键形态（选项 ID、能力、configOptions）与 SP-1 录制一致；原始行通道；全局不变式。
- [ ] `echo_env` 回显的环境变量里只有变量名，没有值，避免把环境泄露进测试日志。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T051-fake-agent-scripts/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 注意

- ACP SDK 锁精确版本 `=3.3.0`，和宿主一致。
- 假 agent 是测试工具，不进 App 包（5.10.1），但同样遵守 `CLAUDE.md` 的约束：
  - 时间：剧本里的 `sleep`、`late_tool` 等计时用 tokio 的计时器。假 agent 是独立进程，不在宿主的虚拟时钟下。如果 `disallowed-methods` 拦住了，在 `cupper-fake-agent` 里加一个带 reason 的豁免，写明"假 agent 是外部进程，模拟真实适配器的真实时间"。
  - 不写 `unsafe`。
- `write_file` 只能写进会话的工作区（cwd）之内，拒绝 `..` 和绝对路径。
- **规模**：L，上限约 1,500 行。开工前先在 `notes.md` 里写一份按模块的行数估计；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出时，先停下来汇报，并给出拆分建议，例如"persona 与启动配置"和"新增剧本命令"分开。

## 参考资料

- 设计：`09-testing.md` 5.9.2、5.9.9；`06-agent-driver.md` 5.6.9（各 K 条与剧本命令的对应）。
- SP-4a 的假 agent 与报告；SP-1 的录制与报告（persona 的真实形态）；T003 的报告（`steering.supported` 在 `initialize` 响应的顶层 `_meta` 里）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
