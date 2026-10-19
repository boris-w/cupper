# T001 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，但要先补一处小改动，补改后合并
- 审阅范围：`git diff main...build/T001-repo-skeleton`，38 个文件，不含 lock 文件约 2,270 行

### 核对

- **本机 CI**：编排者在任务 worktree 里重跑 `cargo xtask ci pr`，全部通过：rust 15 项、ts 3 项、license 1 项，其余作业"尚未实现"。
- **摘要**：只有白名单字段，即提交、工具链版本、`macOS 26.4`、机型泛称、作业统计、逐编号结果。没有路径、用户名、主机名。
- **个人信息**：diff 里唯一的命中是测试夹具中的假路径 `/Users/someone/...`。
- **`unsafe`**：没有。工作区设了 `unsafe_code = deny`。
- **规模**：约 2,070 行，超出上限约 1,500 行，执行者在汇报里主动说明。超出的部分主要是测试和配置，接受；已提醒以后超出预算时先停下来汇报。

### 意见与处理

| # | 问题 | 处理 |
|---|---|---|
| 1 | ADR-11 的许可证白名单缺 `Unicode-3.0`（derive 宏依赖 unicode-ident），以及 `Apache-2.0 WITH LLVM-exception` | 编排者裁定两项都加入白名单（OSI 认可的宽松许可证）；执行者改了 `deny.toml`（`b7b7491`）；ADR-11 由编排者补记 |
| 2 | 设计 5.1.3 说 xtask 只在 macOS 上编译，5.1.6 又把部分作业放在 ubuntu 上，前后矛盾 | 编排者在设计里补注：xtask 目前没有平台专用依赖；以后引入时用 cfg 隔离，或者把这些作业改到 macOS 上跑 |
| 3 | 计划写的日志目录是 `.private/ci/`，任务书写的是 `.private/ci-logs/` | 统一为 `.private/ci-logs/`，编排者修改计划 |
| 4 | pnpm 用 12.14.0（设计写的是 10.x），TypeScript 用 6.0.3 | 接受，编排者更新设计 5.1.4 |
| 5、6 | Vitest 与 `dist` 目录、逐用例的编号结果；nextest | 写进 T015、T070、T002 的 brief |

## 补改（2026-10-19）

- `deny.toml` 加了两项许可证，并附注释（`b7b7491`）。
- 重跑 CI 通过，新摘要为 `ci/pr-ae9caed1ee1c.json`；旧提交的那份摘要已删除（`ae9caed`）。
- 合并前，编排者在最终提交上又跑了一次 `cargo xtask ci pr`。
