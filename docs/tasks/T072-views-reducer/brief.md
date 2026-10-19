# T072 host-client II：视图 reducer 与交叉验证

- 派发日期：2026-10-19
- 执行者：子 agent（TypeScript 工程师）。与 T071、T087 是同一位执行者。
- 分支：`feat/T072-views-reducer`
- 依赖：T087（已合并）。T080 等界面任务依赖它。

## 背景

宿主和客户端用同一套视图规则（设计 5.3.3、5.7.6）：
- Rust 端是 `cupper-protocol::view`（T014）里的 `IndexView`、`SessionView`；
- TS 端由本任务实现，作为 T087 的 `StreamSink` 的视图。

两端要用 `protocol/fixtures/` 交叉验证，再加差分测试（5.9.1，测试 #16）。

## 范围

**做**（计划 T072 一行；设计 5.7.6、5.9.1；5.3.11 末尾"T014 实现时的裁定"）：
1. TS 的两个视图 reducer：`IndexView`、`SessionView`。
   - 规则与 Rust 一致：
     - 窗口：最近 20 个已结束回合，条目合计不超过 2,000，只数 item；
     - 进行中的回合和 `tail` 永远保留；
     - 最新的一个已结束回合总是保留；
     - `detached`；
     - 历史插入按 `turn_no` 排序；
     - `late_by_ref`；
     - `ItemPatch` 只设置本类条目有的字段。
   - 视图 JSON 与 Rust 端深度相等，键的顺序不算。
2. 按条目订阅的版本号：界面按条目订阅变化，避免整页重渲染（5.7.6）。
3. 实现 T087 的 `StreamSink`：
   - `reset`；
   - `apply(body, event?)`；
   - `live`：视图在这一刻才完整；
   - `failed`。
   - `apply` 抛出异常时，T087 会不带游标重新订阅。
4. 与 Rust 视图交叉验证：两端分别对同一批样例事件 fold，比较得到的视图 JSON。接入 `xtask ci protocol`。
5. 差分测试：随机事件序列在两端产生相同的视图。接入 `xtask ci nightly`。

**不做**：界面组件（T080 起）。

## 验收标准

- [ ] 新增 UNIT-views-parity：用 `protocol/fixtures/` 的样例，两端视图 JSON 深度相等。
- [ ] 新增 UNIT-views-differential：随机事件序列下两端相等，接入 nightly 作业。
- [ ] 以上两个编号已在 `tests/ids.toml` 的 T072 段登记；如果有出入，就改自己那一段。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T072-views-reducer/ci/` 通过，摘要随任务提交。

## 交接事项

- **规则来源**（T014）：TS reducer 照 5.3.11 末尾"T014 实现时的裁定"的规则和视图 JSON 实现。JSON 对象的键顺序没有含义，比较时按深度相等。
- **生成的类型**（T015）：`zIndexView`、`zSessionView` 及对应类型已经生成。`Item` 这类嵌套的开放联合在 TS 里无法按标签收窄；需要时生成类似 `isKnownEvent` 的守卫，也可以留给 T082。
- **`StreamSink` 的约定**（T087）：
  - 快照 op 经 `apply(body)` 送来，不带 event；实时事件经 `apply(body, event)` 送来。
  - 调用 `live()` 之后，视图才算完整。
  - 类型已知、数据残缺的事件也会送来，表现为未知成员（`isKnownEvent` 为 false），视图要为它显示占位。
- **差分测试的 Rust 侧**：可以写一个小的 Rust 测试或 bin，读同一批事件、输出视图 JSON。按 5.9.1 选最简单的接法，写进 `notes.md`。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

计划估为 M，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块统计一次**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报**。

## 参考资料

- 设计：`07-desktop.md` 5.7.6；`03-protocol-v1.md` 5.3.3、5.3.11；`09-testing.md` 5.9.1。
- `crates/cupper-protocol/src/view/`（T014）、`packages/host-client`（T071、T087）、`packages/protocol`（T015）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
