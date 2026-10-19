# T001 仓库与工程骨架、本机 CI 入口

- 派发日期：2026-10-19
- 执行者：子 agent（工程与 CI 工程师，Rust 与 TypeScript）
- 分支：`build/T001-repo-skeleton`
- 依赖：无（M1a 第 0 波，关键路径上的第一个任务）

## 背景

阶段 ④ 的第一个开发任务。仓库目前只有文档和 `spikes/`。这个任务把 monorepo 的工程骨架和本机 CI 入口搭起来，之后所有任务都在它上面开发，并通过它验收。

仓库暂时没有远端（用户决定 U-P1），CI 由 `cargo xtask ci <作业>` 在本机执行。

## 目标

- 建好 Rust 和 pnpm 两套工作区、工具链锁定、lint 与格式配置；
- `cargo xtask ci` 入口可以运行，自带机器级锁和结构化摘要；
- 之后的任务只需新增 crate 或 package，不用改根清单。

## 范围

**做**（依据：设计 5.1.1–5.1.7，计划 1.5、1.8 和 T001 一行）：

1. **Rust 工作区**
   - 根 `Cargo.toml`：resolver 3，edition 2024，成员用通配 `crates/*`；`[workspace.dependencies]` 和 `[workspace.lints]` 集中管理。
   - `rust-toolchain.toml`：锁定 stable 版本，targets 为 `aarch64-apple-darwin` 和 `aarch64-apple-ios-sim`。
   - `clippy.toml`：
     - `disallowed-methods` 写入设计 5.2.16 的禁用清单（`Instant::now`、`SystemTime::now`、`Instant::elapsed`、`SystemTime::elapsed`、`tokio::time::{sleep, sleep_until, interval, timeout, Instant::now}`、`std::thread::sleep`、`Uuid::now_v7` 等），给出说明文字；
     - 开启 `await_holding_lock`；
     - 准备一个能证明规则生效的负面样例：样例代码违反规则时，clippy 必须失败。可以是 xtask 里的一个测试 fixture，或者一个不进工作区、由测试单独编译的小 crate。
   - `deny.toml`：许可证白名单按 ADR-11（MIT、ISC、BSD-2/3、Apache-2.0、0BSD、Unlicense、CC0、BlueOak），并开启安全公告检查。
   - `rustfmt.toml`。
2. **pnpm 工作区**
   - 根 `package.json`：只放编排脚本，`packageManager` 锁定 pnpm 的版本；
   - `pnpm-workspace.yaml`：`packages/*`、`apps/*`、`tools/*`；
   - `.node-version`；
   - 基础 `tsconfig`、ESLint、Prettier 配置；
   - 保留 pnpm 默认"不运行依赖脚本"的行为，白名单先留空。
   - 本任务不建任何 TS 包，但 `xtask ci ts` 在没有 TS 包时也要能通过。
3. **`crates/xtask` 与 `cargo xtask ci <作业>`**（计划 1.8）
   - 本任务实现以下作业：
     - `rust`：fmt check、clippy `-D warnings`、`cargo test`；
     - `ts`：在有 TS 包的情况下运行 tsc、ESLint、Prettier、Vitest，目前允许为空；
     - `license`：暂时只运行 `cargo deny check`；
     - 组合作业 `pr`。
   - 其他作业名先注册，返回"尚未实现"，这样以后的任务只需补实现。
   - **运行前检查**：git 状态必须干净，否则拒绝运行。
   - **机器级锁**：e2e、`ui`、性能、nfr1 这几类作业取同一把锁，串行运行；锁文件放在每个用户自己的临时目录或缓存目录里，进程异常退出后能自动释放（例如 flock）。本任务先把锁机制和测试做好，这几类作业本身还没有。
   - **结构化摘要**：
     - 字段白名单：提交、工具链版本、macOS 版本、机型泛称、各作业的通过数和失败数、耗时、逐编号的结果清单（编号、套件、结果、提交）、e2e 产物哈希；
     - 写入位置由参数指定，例如 `--record docs/tasks/<ID>-<slug>/ci/`；
     - 写入前自检：发现绝对路径、用户名、主机名一类的内容就拒绝写入；
     - 原始日志写到 `.private/ci-logs/`（已被 gitignore）。
4. **workflow 文件**：`.github/workflows/` 下一个作业一个文件，只调用 `cargo xtask ci <作业>`。按设计 5.1.6 写好通用加固：Action 按 SHA 固定、`permissions: contents: read`、不用 `pull_request_target`。仓库暂时没有远端，所以不启用。
5. **`.gitignore`**：补上构建产物（`target/`、`node_modules/` 等）。

**不做**：
- 其他 crate（`cupper-protocol` 等）和 TS 包；
- `tools/trace-ids`（T002）；
- 其他作业的具体实现。

## 验收标准

- [ ] 在干净的 worktree 里，`cargo xtask ci pr` 通过。
- [ ] `disallowed-methods` 的负面样例会让 clippy 失败，并且有测试断言这一点。
- [ ] 新增 UNIT-ci-summary：摘要只包含白名单字段；字段内容里含路径或用户名时，拒绝写入。
- [ ] 新增 UNIT-ci-lock：两个持锁作业不会同时运行；git 状态不干净时拒绝运行。
- [ ] 遵守 `CLAUDE.md` 的代码约束。`unsafe` 只能出现在 `cupper-sys` 里，本任务不应有任何 `unsafe`。
- [ ] 本任务自己的 CI 运行结果写一份摘要，放在 `docs/tasks/T001-repo-skeleton/ci/`，用的就是本任务实现的 `--record`。

## 注意

- **工具链版本**：本机已有 Rust 1.96（含 iOS 模拟器 target）、cargo-deny 0.20、Node 24、pnpm 12。设计 5.1.4 写的是 pnpm 10.x，并注明"M1a 开工时按当天复核"。请按本机实际情况和稳定性选定版本，写在 `notes.md` 里。
- **不修改全局环境**：不执行 `npm i -g`，不执行 `corepack enable` 这类改全局 shim 的命令，不修改 `~/.cargo/config`。`rustup` 按 `rust-toolchain.toml` 自动下载工具链是可以的。
- **规模**：上限约 1,500 行，不含 lock 文件。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`docs/design/01-repo-build-ci.md`，5.1.1–5.1.7；
- 计划：`docs/03-plan.md` 1.5、1.6、1.8 和 T001 一行；
- `CLAUDE.md`：代码约束、子 agent 守则；
- 时间规则：`docs/design/02-host.md` 5.2.16，`disallowed-methods` 的出处。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。`notes.md` 和 CI 摘要提交在任务分支上。
