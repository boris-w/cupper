# T069 宿主集成测试 IV：并发、故障点、崩溃一致性与外发

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T047、T100、T138、T163、T068、T062、T065 的执行者接着做
- 分支：`test/T069-it-concurrency-failpoints`
- 依赖：T044、T047、T089、T169（都已合并）。T099 依赖本任务

## 背景

宿主集成测试按主题拆成四份，T068（交互与信任）、T062（网关同步与历史）已完成，T063（驱动与回合）同时在做。本任务是最后一份，覆盖三块：
- 多会话共用适配器；
- 故障点与崩溃一致性（NFR-9 的核心部分）；
- 宿主的外发检查。

## 范围

**做**（计划 T069 一行；设计 5.9.1、5.9.8 的故障点与崩溃一致性）：
1. **IT-six-sessions-shared-adapter**：6 个会话共用同一个假适配器进程，同时 prompt，断言互不串扰、各自的流完整（NFR-11）。
2. **故障点夹具与断言库**：
   - 断言库：完整性检查（`PRAGMA integrity_check`）、seq 连续、历史是前缀、命令账本的语义、第一条消息不丢；
   - 故障点：M1a 已有的点（例如 `store.before_commit`、`store.after_commit`，见 T024 的交接），按设计 5.9.8 列出的 M1a 范围逐个登记；
   - git 写操作与 worktree 的故障点不做，T121、T124 以后只登记新点、复用本夹具。
3. **IT-NFR9-failpoints-core**：在每个 M1a 故障点上让宿主停下（`kill -9` 或故障注入），重启后用断言库检查。
4. **IT-NFR9-crash-under-load**：高负载写入中反复 `kill -9` 宿主，每次重启做完整性检查与断言库检查。
5. **IT-host-egress-seatbelt**（新增）：宿主放进拒绝出站的 Seatbelt profile 里跑一个普通会话，断言能正常完成、没有出站被拒的记录（SEC-6）。运行时安装照 T178 的做法指向本机。
6. **补一条 T068 留下的场景**：第一个回合停在准备阶段时（`turn.started` 已写出、发送前的重扫还没做完），项目配置变了；信任恢复后，这个回合能正常发出。
7. **缺陷就地修**：按计划 1.2 的上限，单处不超过 100 行、不改接口，单独提交；超出的停下汇报。

**不做**：git 写操作与 worktree 的故障点（T121、T124）；驱动与回合的 K 清单（T063）。

## 验收标准

- [ ] 四个编号全部通过，连跑 10 次全绿，跑完没有残留进程。
- [ ] 故障点清单与每个点上的断言结果写进 `notes.md`。
- [ ] 就地修的每处缺陷在汇报里单列：现象、原因、修法、行数。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T069 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T069-it-concurrency-failpoints/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T069 的行；开工前逐行读一遍）

- T024：故障点 `store.before_commit`、`store.after_commit` 的位置。
- T068、T062：`tests/common/` 的夹具；`Running` 守卫；宿主不是测试子进程时按 pid 加启动时间核对。
- T089、T169、T175：重启后的载入、补写与重建；空闲退出。
- T062：孤立修法后的微秒级窗口；`it_gw_blob_put_and_get_through_the_gateway` 在高负载下失败过一次；附加时 `session.capabilities` 连写两条，不要断言精确个数。
- T177：actor 在同一步里 await 写 blob，32 MiB 的输出可能阻塞会话；故障测试里出问题可改成后台写。
- T178：e2e 夹具把运行时安装指向本机；宿主的 Seatbelt 用例照做。

## 注意

- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。
- `kill -9` 只对本用例拉起、并且核实过的进程。
- 代码约束：时间只经 `Clock`；纯核心里不做 I/O；不在 tokio 工作线程上同步调用慢的系统接口。

## 规模

- 停止线：1300；硬停止线：1450

<!-- 执行者开工前在 `notes.md` 里按用例写行数估计，就地修单列；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报，并给出拆分建议。 -->

## 工作方式

在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b test/T069-it-concurrency-failpoints main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
