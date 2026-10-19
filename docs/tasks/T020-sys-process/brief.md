# T020 cupper-sys I：进程与系统信息

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 系统工程师，macOS）
- 分支：`feat/T020-sys-process`
- 依赖：T001（已合并）。M1a 第 1 波。

## 背景

`cupper-sys` 集中存放全部 `unsafe` 和 macOS 系统接口（ADR-02 修订、设计 5.1.3）。宿主、CLI、原生扩展拉起任何子进程，都必须经 `cupper-sys::spawn`（`CLAUDE.md` 的代码约束）。本任务实现进程相关的部分；对端校验和代码签名由 T021 负责。

## 范围

**做**（计划 T020 一行）：从 SP-4a 迁移 `sys.rs`，内容如下。
- **`spawn`**：
  - 可选 `POSIX_SPAWN_SETSID`；
  - 使用 `POSIX_SPAWN_CLOEXEC_DEFAULT`；
  - 显式指定 stdio，信号处置恢复默认；
  - 环境变量白名单。
- **进程组清理**。T003 的结论是：Claude Code 拉起的工具命令会建自己的会话和进程组，所以清理时要能枚举全部后代，再按它们各自的进程组发信号（设计 v1.6 的 5.6.8、5.2.13）。
- **libproc**：进程启动时间、父进程链、会话 ID 与进程组。
- 读取 `kern.bootsessionuuid`。
- 取得 `DARWIN_USER_TEMP_DIR`。

**不做**：
- IOPM 与 UserNotifications（以后的任务）；
- 对端校验与签名（T021）。

## 验收标准

- [ ] 新增 UNIT-sys-spawn：
  - 子进程只继承 fd 0–2；
  - 变异检查：去掉 `CLOEXEC_DEFAULT` 后，测试应当失败；
  - SETSID 生效；
  - 环境变量白名单生效。
- [ ] 新增 UNIT-sys-reap-group：
  - 能清理整个进程组；
  - 后代进入另一个进程组时，也能被枚举并清理到。这是 T003 观察到的情况。
- [ ] `unsafe` 只出现在 `cupper-sys` 里，每处都写明安全前提；工作区的 `unsafe_code = deny` 只对这个 crate 放开，并写明原因。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T020-sys-process/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（计划 1.6）。

## 注意

- 测试拉起的子进程只能是 `sleep`、`sh -c` 这类无害命令；测试结束时必须清理干净，不留残留进程。
- 不运行真实的 Claude 或 Codex CLI。
- 规模上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`01-repo-build-ci.md` 5.1.3；`08-security.md` 5.8.5；`02-host.md` 5.2.13；`06-agent-driver.md` 5.6.8（v1.6）。
- T003 报告中 SP-4b 遗留第 7 条的进程树（`docs/tasks/T003-tech-validation/report.md`）。
- SP-4a 的 `sys.rs`，SP-4b 的报告。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
