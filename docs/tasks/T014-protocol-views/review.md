# T014 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，合并。
- 核对：
  - 分支上 `crates/cupper-protocol` 以外的文件，main 后来都没有改过。合并后在 main 上重跑 `cargo xtask ci pr`；
  - 没有 `unsafe`，没有新依赖，没有个人信息；
  - 视图代码里没有 `unwrap`、`expect`、`panic!`，也没有数值强转（测试除外）；不做 I/O，不读时钟。
- 代码审阅：
  - `trim` 的规则：从新到旧数已结束的回合，第一个放不下的就停；进行中的回合和 `tail` 保留；被淘汰回合里仍待处理的请求进 `detached`；`late_by_ref` 随淘汰清理。
  - `start_turn` 的规则：按 `turn_no` 插入；只有比已有回合都新的回合才收进 `tail`，所以历史页送来的旧回合不会把 `tail` 收走；重复的 `turn_id` 或 `turn_no` 报 `Duplicate`。
  - 两条性质测试（合并后应用等于逐条应用；窗口快照加全部历史页等于从头重放），再加执行者做的 5 种变异检查，足以支撑这些规则。
- **规模**：1,657 行，超出上限约 10%，接受。执行者开工前定了停止线，第二个模块后的推算超过停止线时，按预案收窄了范围；超出的部分在测试里。
- **范围调整**：客户端同步状态机（快照分页组装、`stream/reset` 处理）交给 T045，接受。计划里 T045 本来就包括"订阅与连续性、epoch 重置"。T014 交付的是规则本身：`subscribe_mode`、`SubscribeMode`、`ResyncReason`，加上已有的 `StreamCursor`。
- **测试编号**：`IndexView`、订阅规则、schema 检查暂时归在 UNIT-view-window 和 PROP-view-coalesce 下（对照表在 `notes.md`）。暂不新增编号；T002 登记时如果觉得需要更细，再提出来。
- **写进设计的规则**：执行者定下的视图规则，以及编排者对"回合外的交互请求"的裁定，已写进 `docs/design/03-protocol-v1.md` 5.3.11 末尾的"T014 实现时的裁定"。
- **转交事项**：T012、T026、T033、T043、T045、T015、T072 各条，已写进 `docs/tasks/handoff-notes.md`。
