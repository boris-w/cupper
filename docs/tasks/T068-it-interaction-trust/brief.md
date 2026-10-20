# T068 宿主集成测试 III：交互、信任与档位

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T035、T096、T034、T047、T100、T138、T163 的执行者接手（做过信任扫描、信任发现、索引 actor 和测试接口）
- 分支：`test/T068-it-interaction-trust`
- 依赖：T009、T034、T044、T055、T096、T109（都已合并）。T099 依赖本任务

## 背景

宿主集成测试按主题拆成四份（计划 1 节"T062、T063、T068、T069"一条）：
- T062：网关同步与历史；
- T063：驱动与回合；
- T068（本任务）：交互、信任与档位；
- T069：并发、故障点与崩溃一致性。

进程内的用例已经有了：IT-orch-trust-gating（T109）、IT-orch-gating（T044）、交互代理（T088）。本任务在**真实宿主进程**上把这些路径跑一遍：claude 形态的假 agent，加 CLI 库或测试接口（T047）。

## 范围

**做**（计划 T068 一行；设计 5.9.1、5.9.8 的项目信任与配置档位）：
1. **七个进程级用例**，编号已在 `tests/ids.toml` 的 T068 段里：
   - IT-form：提问（表单）请求的往返；
   - IT-plan-confirm：计划确认；
   - IT-interaction-expire：交互失效；
   - IT-config-unconfirmed：档位越权与 `config_unconfirmed`、`session/confirm_config`；
   - IT-trust-stale：项目信任变旧；
   - IT-trust-rescan-before-prompt：发 prompt 之前重扫；
   - IT-trust-settingsources：`settingSources` 与项目配置的加载。
   
   每个用例照设计 5.9.8 对应条目的断言写。信任部分可以照 IT-orch-trust-gating 的 6 个场景挑选（T109 的交接）。
2. **缺陷就地修**：按计划 1.2 的上限，单处修复不超过 100 行、不改接口；超出的另开 fix 任务，相关用例先标为待修，在汇报里提出。已知一处必须修：
   - `orchestrator/session/actor.rs` 的 `rescan` 用 `tokio::spawn` 另起任务，没有持有静止屏障的 `Hold`；
   - 任务里有数据库读取、git 子进程和 `spawn_blocking`；
   - 进程级的信任用例里，`test/clock_advance` 可能在重扫完成之前就返回（T100 合并时发现，违反 T047 的规则）。
3. **文件**：用例放在新文件 `crates/cupper-host/tests/interaction_trust.rs`。T062、T063、T069 以后各用自己的文件，共用的夹具如果要从 `lifecycle.rs` 提出来，放进 `tests/common/`，在汇报里说明。

**不做**：网关同步与历史（T062）；驱动与回合的 K 清单（T063）；并发与故障点（T069）。

## 验收标准

- [ ] 七个用例全部通过，连跑 10 次全绿，跑完没有残留进程（照 T008 修复 1 的守卫做法）。
- [ ] `rescan` 持有 `Hold` 的修复，带一条能复现的用例。
- [ ] 就地修的每处缺陷在汇报里单独列出：现象、原因、修法、行数。
- [ ] 测试名以用例编号开头。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T068-it-interaction-trust/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T068 的行；开工前逐行读一遍）

- T109：IT-orch-trust-gating 的 6 个场景；会话夹具的项目是临时目录里真实存在的 `app/`；测试构建里有 `GitTool::with_test_env`。
- T100：`rescan` 没有持有 `Hold` 的缺陷。
- T047：静止屏障与 `test/clock_advance`；另起的 task 要持有 `Hold` 直到交回结果。
- T046、T133：CLI 的退出码与 `--json` 的输出（如果用 CLI 驱动）。
- T044：档位核对、`config_unconfirmed`、`session/confirm_config`。
- T088：交互代理、`interaction/respond`、`interaction/get`。
- T163：审计里的 `project.trust.*`、`config.confirmed` 可以顺带断言。

## 注意

- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。
- 不读、不改用户的 `~/.claude`、`~/.codex`；所有数据目录、项目目录都在临时目录里。
- 进程级用例结束时宿主、假 agent、CLI 都要被回收。

## 规模

M（测试为主）。**开工前先在 `notes.md` 里按用例写行数估计，就地修单列；推算超过约 850 行（包括贴线）就先停下汇报**；硬停止线 950。每完成几个用例就用 `git diff --numstat` 统计一次（守则第 8 条）。

## 工作方式

T163 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b test/T068-it-interaction-trust main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
