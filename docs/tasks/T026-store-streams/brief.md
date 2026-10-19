# T026 存储 III：流、检查点、回合快照与历史

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主存储）
- 分支：`feat/T026-store-streams`
- 依赖：T014、T025（均已合并）。M1a 第 4 波；T033（订阅协议，关键路径）依赖它。T012 拆分后，本任务的松弛为 1。

## 背景

宿主自己的事件日志是唯一的历史来源（ADR-06）。前面的任务已经做好的部分：
- T024：写线程和读池；
- T025：表结构、迁移和事件升级器；
- T014：`cupper-protocol::view` 里的纯视图、`apply`、窗口、快照 ops 和分页。

本任务把它们接起来，做成 `store::streams`：每条流由唯一的写者追加事件，事件落盘后应用到内存视图、推给订阅者；回合结束时写检查点和回合快照；`history/page` 从回合快照读。订阅协议本身（`stream/subscribe` 的方法处理、快照分页下发）归 T033。

## 范围

**做**（计划 T026 一行；设计 5.2.3、5.2.8、5.3.3、5.4.5）：
1. **`store::streams`**，在 SP-4a 的 `hub` 基础上重构（5.2.3 流注册表，5.2.8；迁移对照见 `02-host.md` 约第 546 行）：
   - **追加的三步**（5.2.8 第 2 条）：
     1. 写者分配 seq，组装成批；
     2. 交给写线程，等待提交确认，这一步不持任何锁；
     3. 拿流锁，应用到视图、推进 head、推给订阅者。
   - 订阅在流锁内读取 head 和视图，订阅和追加之间不会漏也不会重（SP-4a 的不变式）。
   - **懒加载**：流状态由检查点加其后的事件得到；
   - **淘汰**：会话流状态在没有订阅者、也没有 actor 10 分钟后从内存移除，时长登记在 `durations.rs`；
   - 同步原语经 `cfg(shuttle)` 门面，正常构建用 std 或 tokio 的原语，shuttle 测试换成 shuttle 的。
2. **扇出接口**：订阅者拿到的是什么、慢订阅者怎么办，按 5.2.3 和 5.3.3。帧级的下发归 T033。
3. **回合结束时，在同一个事务里**：
   - 写检查点（视图 JSON 加 `view_v`）；
   - 写不可变的回合快照（`turn_ops`）；
   - 然后对内存视图调用 `trim`。
4. **`history/page` 的读取**：从回合快照读，用 `paginate` 分页，`op_index` 是页的起点。
5. **events 裁剪**：按 5.4.5 的保留规则裁剪事件，裁剪之后从检查点重建的结果不变。

**不做**：
- 订阅协议和方法处理（T033）；
- blob、原始帧和其他保留清理（T027）；
- 会话 actor（T043）。

## 验收标准

- [ ] 新增 PROP-stream-seq（shuttle）：穷举追加与订阅的线程调度，任何订阅者看到的 seq 都连续、不重复（5.9.8）。
- [ ] 新增 PROP-stream-replay，包括 5.9.8 里流不变式的其余几条：
  - 检查点加其后的事件，等于从头重放；
  - 按保留规则裁剪后，重建结果不变；
  - 窗口快照加全部 `history/page`，等于从头重放。这一条走真实的存储路径；T014 已经在纯视图层验证过同一性质。
- [ ] 在第二步之后、第三步之前崩溃，重启后从数据库加载，状态一致。可以用故障点（在 `failpoint.rs` 登记）在子进程里验证，也可以用其他确定性的办法。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T026-store-streams/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **视图与快照**（T014）：
  - 每个 `turn.completed` 之后，在同一个事务里先存 `turn_ops(&turn_id, &OpLimits::V1)` 作为回合快照，再调用 `trim(&Window::V1)`；
  - 检查点直接存视图的 JSON；
  - `history/page` 用 `paginate(回合 ops, max_bytes)`。
- **宿主要守的不变式**（T014 审阅，见 `03-protocol-v1.md` 5.3.11 末尾）：引用某个回合的事件都写在这个回合的 `turn.completed` 之前。存储层至少要保证顺序不被打乱。
- **事件版本与检查点**（T025、T011）：
  - 写事件时，`v` 取 `Upcasters::host().current(type)`，这就是 T011 交接里说的 `EVENT_VERSION`；
  - 读事件时调用 `upcast(type, v, data)`；
  - 检查点写 `view_v`，读的时候调用 `checkpoint_fate(view_v)`，结果是 `Discard` 时，用最近 20 个回合快照加其后的事件重建视图（5.4.6）。
  - **编排者裁定**：`VIEW_VERSION` 从 `store::upcast` 挪到 `cupper-protocol::view`，和视图类型放在一起。
  - **请你定**：`turn_snapshots.v` 的含义。整个快照只有一个版本，而事件的 `v` 是按类型分的，ops 里的事件体也不带 `v`。T025 的建议是每个 op 带自己的 `v`。在 `notes.md` 里写清楚，在汇报里说明；如果要改表结构，按 P-4 直接改 `0001`。
- **写线程**（T024）：
  - 回执在提交之后才返回，正好用作追加的第二步；
  - 写 `pending_external` 的操作用 `Durability::Full`；
  - 操作不能自己结束事务，否则整批作废；
  - 需要 `prepare_cached` 时，给 rusqlite 打开 `cache` 特性。它会引入 foldhash，Zlib 已在 allowlist 里。
- **惯例**：
  - 时间只经 `Clock`，定时只经 `Clock`；
  - 每条事件流只有一个写者；
  - SQLite 只在写线程或 `spawn_blocking` 里运行，不在 async 工作线程上；
  - 用到 `ManualClock` 等设施的集成测试，声明 `required-features = ["test-harness"]`；
  - `lib.rs` 只放 `mod` 声明。
- **新依赖**：shuttle 以 dev-dependency 加进 `cupper-host`，锁精确版本。许可证要在 ADR-11 的 allowlist 内，汇报里写明。

## 并行注意

T027（blob、原始帧、保留清理）之后会在同一个 `store/` 模块里进行。本任务不要碰 blob 和原始帧；`store/mod.rs` 只做附加。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试和测试辅助单独列一项**（计划 1.1 的执行经验）；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出时，先停下来汇报，并给出拆分建议，例如"流与扇出"和"检查点、回合快照与历史"分开。

## 参考资料

- 设计：
  - `02-host.md` 5.2.3（并发模型、流注册表）、5.2.8（事件存储）、迁移对照表（约第 546 行）；
  - `03-protocol-v1.md` 5.3.3、5.3.11 末尾的 T014 裁定；
  - `04-data-model.md` 5.4.2、5.4.5、5.4.6；
  - `09-testing.md` 5.9.8（流的不变式）。
- SP-4a 的 `spikes/SP-4a-host-skeleton/crates/cupper-host/src/hub.rs` 和它的报告。
- T014、T024、T025 的代码和汇报。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料（`notes.md` 等）由执行者在任务分支上提交。
