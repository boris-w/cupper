# T043 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 过程：
  - 分支开出之后，main 合进了 T087、T072、T019。合并时 `gateway/dispatch/tests.rs` 有冲突：本分支把 `Peer` 挪进了 `gateway/testkit.rs`，T019 改了 `Peer`（`Dispatch::new` 多了 `Auditor`，outbox 的项改为 `Item`）。保留挪动，把 T019 的两处改动搬到 testkit 的 `Peer` 里，并删掉一个用不到的 `mpsc` 导入；
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1130、protocol 185、ts 314、license 1、testdata-scan 2，摘要是 `ci/pr-40de51a33a80.json`。执行者合并前那次的 `pr-8b4e96723435.json` 一并保留。
- 核对：没有新 crate，只给 tokio 加了 `macros` 特性，`Cargo.lock` 不变。没有个人信息。
- 审阅：
  - **一步一事务**：每个输入是核心的一步，这一步的事件、改动的行和命令的账本记录在同一批里提交（`Batch::with`），提交成功之后才回复、发驱动命令、起 attach。提交失败时不回成功、不发命令，符合 5.2.3 规则 3。
  - **行的 `seq`**：`Rows::of` 用 `writer.head() + 1` 预先算出首个 `seq`。会话流只有这一个写者（actor），所以预算的值就是实际的值。
  - **视图拒收事件**：测试和 test-harness 构建里 panic，其他构建写 stderr。T037 之后发布构建的 stderr 指向 `/dev/null`，这条要随 T028 改写进 host.log，已记入交接。
  - **创建**：检查顺序（worktree、agent、项目、信任门控、档位）和"一个事务写行、事件、账本，提交之后才登记句柄、启动 actor"都与裁定一致。
  - **不睡眠**：用例全部用 `ManualClock`，等待靠订阅流或替身收到的命令。
- 范围外发现（编排者复现）：T037 报告的 T058 性质测试反例，在本分支同样能稳定复现（`prop_autonomous_dequeue_guard_queue_waits_while_the_agent_works`，种子见 T037 的汇报）。这个问题在 main 上本来就有，不是本任务引入的；本任务把 `Session` 原样提成了 `SessionCore`，所以修复要落在 `orchestrator/session/core.rs` 或裁判本身。另开修复任务，在 T088 接线之前或同时完成。
- 规模：新增 2,022 行（`crates/`），删除 256 行，在放宽后的 2,100 上限以内。
- 转交事项已写进 `handoff-notes.md`：T088、T089、T044、T061、T028，以及宿主组装（T037 和本任务都把接线留给了对方，由编排者另行指定）。
