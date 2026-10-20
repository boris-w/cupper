# T062 宿主集成测试 I：网关同步、历史与通知意图

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T035、T096、T034、T047、T100、T138、T163、T068 的执行者接着做（做过索引 actor、测试接口，刚把进程级夹具整理进 `tests/common/`）
- 分支：`test/T062-it-gateway-history`
- 依赖：T019、T029、T044、T047、T049、T059、T089、T100（都已合并）。T099 依赖本任务

## 背景

宿主集成测试按主题拆成四份，T068（交互、信任与档位）刚完成。本任务是第一份：网关同步、历史与通知意图。内容包括：
- 把 SP-4a 原型里的端到端测试 1–7 迁到真实宿主上；
- 数据代次（epoch）的进程级用例；
- 历史与通知意图；
- 新增的"拉起宿主的父进程在回合中退出"。

## 范围

**做**（计划 T062 一行；设计 5.9.1、5.9.8；测试 #2）：
1. **IT-gw-sync**：迁移 SP-4a 端到端测试 1–7，跑在真实宿主进程、claude 形态的假 agent 和 CLI 库上。覆盖：
   - 多客户端一致；
   - 先到先得；
   - 续传；
   - 慢客户端；
   - `kill -9` 恢复；
   - 命令幂等；
   - 帧上限；
   - 适配器隔离。
   
   以 `spikes/SP-4a-host-skeleton/` 的测试为蓝本，**只参考、不依赖**原型代码；有设计后来改过的地方，以设计为准，并在 notes 里逐条对照。
2. **IT-NFR2-epoch**：设计 5.9.8 的数据代次用例。
3. **IT-history-from-host**：历史从宿主取（`history/page`），不依赖客户端缓存。
4. **IT-notification-intent**：通知意图的产生与去重。
5. **IT-host-orphaned**（新增）：回合进行中，拉起宿主的父进程退出。宿主不再开始新回合，等回合结束后有序退出（T059 的孤立规则）。
6. **缺陷就地修**：按计划 1.2 的上限，单处修复不超过 100 行、不改接口；超出的另开 fix 任务，相关用例先标为待修，在汇报里提出。
7. **文件**：用例放在新文件 `crates/cupper-host/tests/gateway_sync.rs`，共用夹具用 T068 整理出的 `tests/common/`。可以按需扩充它，但不要改坏 `lifecycle.rs`、`interaction_trust.rs`。现有的 `tests/gateway.rs` 是网关层的用例，不要往里塞进程级用例。

**不做**：驱动与回合的 K 清单（T063）；并发、故障点与崩溃一致性（T069）。

## 验收标准

- [ ] 五个编号全部通过，连跑 10 次全绿，跑完没有残留进程（`tests/common` 的 `Running` 守卫已改为先 SIGTERM）。
- [ ] SP-4a 测试 1–7 与新用例的逐条对照写在 `notes.md`。
- [ ] 就地修的每处缺陷在汇报里单列：现象、原因、修法、行数。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T062 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T062-it-gateway-history/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T062 的行）

- T019：`TestGateway::start` 和 `cupper-gateway-fixture` 用的是 `Routes::default()`，要订阅时加 `Routes::streams`。
- T055、T051：拿假 agent 路径的做法；假 agent 的配置只经命令行参数。
- T059：孤立规则与有序退出。
- T089：宿主重启后会话按需载入、补写，队列一律暂停（`kill -9` 恢复的用例要照这个断言）。
- T029：blob 分片（帧上限的用例可能用到）。
- T068：`tests/common/` 的夹具；`Running` 守卫。

## 注意

- 不运行真实的 Claude Code、Codex CLI；agent 一律用假 agent。
- 进程级用例结束时宿主、假 agent、CLI 都要被回收；`kill -9` 只对本用例拉起、并且核实过的进程。
- 产品代码不依赖 `spikes/`。

## 规模

L。**开工前先在 `notes.md` 里按用例写行数估计，就地修单列；推算超过约 1,300 行（包括贴线）就先停下汇报，并给出拆分建议**；硬停止线 1,450。每完成几个用例统计一次。

## 工作方式

T068 合并之后，在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b test/T062-it-gateway-history main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
