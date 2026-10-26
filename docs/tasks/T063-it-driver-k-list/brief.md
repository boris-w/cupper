# T063 宿主集成测试 II：驱动与回合（K 清单）

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师）。T043、T088、T097、T055、T008、T116、T046、T090、T167、T091、T092、T185 的执行者接手（驱动、适配器运行时与假 agent 都是它写的）
- 分支：`test/T063-it-driver-k-list`
- 依赖：T008、T009、T037、T044、T055（都已合并）。T099 依赖本任务

## 背景

K 清单是驱动层要规避的已知问题（设计 5.6.9）。进程内已经有一部分用例。本任务在真实宿主进程上，用 claude 形态的假 agent 把 K 清单的关键条目跑一遍，并观察多次 resume 同一会话后宿主侧有没有累积进程（上游 #1011，Q43）。

## 范围

**做**（计划 T063 一行；设计 5.9.1、5.6.9）。十个编号已在 `tests/ids.toml` 的 T063 段里：
- IT-K1-close-resume：先 close 再 resume；
- IT-K5-withdraw：撤回；
- IT-K6-crash-resume：CLI 崩溃后 resume，含多次 resume 后断言宿主侧的进程数不累积；
- IT-K6-output-then-crash：先产生输出再崩溃，不重发；
- IT-K9-autonomous：prompt 返回后的自发更新；
- IT-K14-old-cli-auth：旧 CLI 的认证；
- IT-auth-none：没有认证；
- IT-K19-unknown-kind-privileged：不认识的选项 kind 按特权处理；
- IT-thought-claude：思考流；
- IT-plan-entries：计划条目。

每条照设计 5.6.9 与 5.9.8 的断言写。假 agent 已有的剧本见设计 5.9.2，缺的在假 agent 里补。

**顺带的交接**：
1. **假 agent 暴露 resume 的可观察记录**（T091 的交接）：让 K1、K6 能直接断言收到的是 `session/resume` 还是 `session/new`。可以用一个只在测试里读的记录文件，或者在 `echo_env` 类命令里带出来。
2. **T116 交接的缺陷**：`AdapterLog` 打开失败时，launch 不读适配器的 stderr、直接关掉管道，适配器再写 stderr 会收到 SIGPIPE，stderr 原始帧也丢了。修法：没有日志时也起一个只写原始帧的读取任务。按计划 1.2 就地修，带用例。
3. **缺陷就地修**：单处不超过 100 行、不改接口，单独提交；超出的停下汇报。

**不做**：codex 形态（T112）；并发与故障点（T069）。

## 验收标准

- [ ] 十个编号全部通过，连跑 10 次全绿，跑完没有残留进程。
- [ ] 多次 resume 后的进程数，写进 `notes.md`（Q43 的观察）。
- [ ] 就地修的每处缺陷在汇报里单列：现象、原因、修法、行数。
- [ ] 测试名以用例编号开头。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T063-it-driver-k-list/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T063 的行；开工前逐行读一遍）

- T068、T062：`tests/common/` 的夹具与 `Running` 守卫；用例放在新文件 `crates/cupper-host/tests/driver_k.rs`，共用夹具按需扩充，不要改坏别的测试目标。
- T091：假 agent 不暴露 resume 与 new 的区别。
- T116：`AdapterLog` 打开失败时的 SIGPIPE。
- T003、T066：多次 resume 之后的 `claude` 进程数（Q43）。
- T089：重启后的按需载入与恢复。

## 注意

- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。
- `kill -9` 只对本用例拉起、并且核实过的进程。

## 规模

- 停止线：850；硬停止线：950

<!-- 执行者开工前在 `notes.md` 里按用例写行数估计，就地修单列；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b test/T063-it-driver-k-list main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
