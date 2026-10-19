# T072 工作笔记

## 设计（开工前）

**TS 视图**（`packages/host-client/src/views/`）

- `StreamView<T>`：两个视图的基类，实现 T087 的 `StreamSink`。
  - `reset` 清空视图，`complete = false`。
  - `apply` 只处理 `isKnownEvent` 为 true 的事件体，其余（未知类型、类型已知但数据残缺）记进 `unknown` 列表，供界面显示占位。这个列表不进视图 JSON：Rust 视图对 `Unknown` 不做任何改变，两端 JSON 才能相等。
  - `live` 置 `complete = true`；`failed` 记下错误。
  - 规则不符时抛 `ViewError`，T087 收到后不带游标重新订阅。抛错之前不改动视图，与 Rust 一致。
- `IndexView`、`SessionView`：状态就是生成的 `IndexView`、`SessionView` 类型的 JSON 形状，`toJSON()` 直接返回。规则逐条照 Rust（`crates/cupper-protocol/src/view/`）：
  - 回合按 `turn_no` 插入；比所有回合都新的回合收走 `tail`。
  - 没有 `turn_id` 的条目并入进行中的回合，否则进 `tail`。
  - 请求所属的回合不在视图里时进入 `detached`；同一请求再次到来、它的回合也在视图里时放回原位。
  - `late_by_ref`。
  - `ItemPatch` 按条目的类型只设置该类有的字段，未知类型合并全部字段。
  - `snapshot.recorded` 每个 phase 只留一个。
  - `ItemEnd` 的 `may_still_run` 为 false 时省略。
  - 客户端不做 `trim`，也不生成快照 op：这两样是宿主的事。
- `Versions`：按条目的版本号，供 T080 的 `useSyncExternalStore` 使用。
  - 键的形式：`item:<id>`、`request:<id>`、`turn:<id>`、`turns`、`tail`、`detached`、`session`；索引流是 `project:<id>`、`session:<id>`、`elevation:<id>`、`projects`、`sessions`、`elevations`、`host`、`agents`、`clients`。另有 `view`，整个视图的任何变化都会改它。
  - 一次改动可能牵动别的条目：`late_by_ref` 建立反向索引时，被引用的那个工具调用（`item:<ref>`）也要重新渲染。
  - `flush()` 通知自上次以来变过的键，由 T080 每个动画帧调用一次。

**交叉验证**（UNIT-views-parity，pr）

- 新增 `protocol/fixtures/views.json`：`{name, view: "index" | "session", ops, expected}`。
  - 由 Rust 测试 `view::parity` 生成并核对：Rust 对 `ops` fold 得到的视图 JSON 必须等于 `expected`。按 5.9.3 的惯例，`UPDATE_GOLDEN=1` 时改写文件。
  - TS 读同一个文件：解析每个 op 后交给视图，`toJSON()` 与 `expected` 深度相等。
  - 样例的来源有两种。第一种是 `events.json` 的全部样例（已知、未知、残缺），按 ID 改写后串成序列，覆盖每类事件、每种条目，以及每种条目上的补丁。第二种是情景样例：`tail` 被下一个回合收走、历史页把较早的回合插到前面、`detached` 的请求被历史页放回、`late_by_ref`、工作区状态。情景样例用 `view::fixtures` 的辅助函数写。
  - 接入 `xtask ci protocol`：Rust 步骤的过滤加上 `unit_views_parity`，另加一个只跑 host-client 里 parity 用例的 TS 步骤。
- 文件由 Rust 写出，加进 `.prettierignore`，与 `protocol/schema/` 同样处理。

**差分测试**（UNIT-views-differential，nightly）

- 按 5.9.1，"Rust 用 proptest 生成事件序列，交给 Node 进程里的 TS reducer"。
  - Rust 侧是 `view::props` 里一个 `#[ignore]` 的测试，用已有的宿主模型 `steps()` 和 `script()` 随机生成会话流，写出三种样例：整条日志及其视图；按随机窗口裁剪后的快照 op 及宿主视图；快照加上从新到旧的历史页及整条日志的视图。写到 `CUPPER_VIEW_CASES` 指定的文件。
  - TS 侧的用例读这个文件。没有设这个环境变量时跳过，PR 里不运行。
- 新增作业 `xtask ci differential`，只列在 nightly 里：先跑 Rust 生成，再跑 TS 用例。这是最简单的接法：不需要 Rust 拉起 Node，也不需要 Node 直接加载 TS 源码。

## 行数估计（开工前）

| 模块 | 代码 | 测试 |
|---|---|---|
| `views/versions.ts`、`views/view.ts`（基类、`ViewError`、占位列表） | ~120 | — |
| `views/index-view.ts` | ~70 | — |
| `views/session-view.ts` | ~220 | — |
| 导出 | ~10 | — |
| TS 单元测试：版本号、sink 的四个入口、占位、错误 | — | ~90 |
| Rust `view::parity`（生成并核对 `views.json`）、`tests/fixtures.rs` 的文件清单 | — | ~155 |
| Rust 差分样例的写出（`props.rs`） | — | ~50 |
| TS 的 parity 和差分用例、共用的 fold 辅助 | — | ~90 |
| xtask：`protocol` 作业加两步，新作业 `differential` | ~65 | — |
| 合计 | **~485** | **~385** |

预计约 870 行。按前三个任务的经验，实际多 20%–45%，即 1,050–1,250 行，在约 1,500 行的上限以内。`views.json` 是 Rust 生成的数据，不计。

## 实际行数

每完成一个模块统计一次（相对分支起点，不含 `pnpm-lock.yaml`、`docs/` 和生成的 `views.json`）。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 累计 |
|---|---|---|---|
| TS 视图、版本号、sink，及其单元测试（`6c30923`） | 595 | 131 | 595 |
| Rust `view::parity`、`views.json`（不计）、TS parity 用例、`testing.ts` 的 fold（`f4e9be4`） | 296 / 7 | 288 | 891 |
| 差分测试的两半、`xtask` 的 `protocol` 两步和 `differential` 作业（`223542e`） | 82 / 6 | 37 | **971** |

- 合计 971 行：代码约 520 行，测试约 450 行。比估计（约 870 行）多 12%，在约 1,500 行的上限以内。
- `session-view.ts`（262 行）比估计多约 40 行：`ItemPatch` 每类条目可设的字段表；`#setTurn`、`#place` 各自按 `item` 和 `request` 两种 entry 分支。
- `views.json` 由 Rust 写出，约 128 KB，不计入行数。

## 实现时的取舍

- **视图 JSON 的相等**：
  - TS 视图存的是 zod 解析后的事件数据。zod 解析 Rust 写出的 JSON 时原样保留，因为生成的 schema 不严格，只去掉不认识的键；Rust 反序列化时同样丢掉不认识的字段。所以两端拿到同样的事件，得到的 JSON 也一样。
  - 比较时，TS 侧先做一次 `JSON.parse(JSON.stringify(view))`，再用 `toStrictEqual` 与 Rust 的 JSON 比：比的就是线上的 JSON，`undefined` 不会被忽略。
- **补丁里的 `null`**：Rust 的 `Option` 把 `null` 读成 `None`，不设置这个字段；TS 同样跳过 `null`。宿主写出的数据里没有 `null`，这条只是保证两端遇到时一致。
- **未知事件**：Rust 视图对 `Unknown` 不做任何改变。TS 把它们放进 `unknown`，供界面显示占位，不进 `toJSON()`。占位应该显示在消息流的什么位置，由界面任务（T082）决定；现在只按到达顺序记下事件体和事件（实时事件带 `seq` 和 `fallback`）。
- **抛错之前不改动视图**：每个分支都先查找、核对，最后才改动，与 Rust 的 `apply` 相同。单元测试检查了五种不相符的事件体：抛错之后，视图 JSON 不变。
- **版本号**：
  - 每次改动都让 `view` 键加一。
  - 只有回合真的收走 `tail` 里的条目时，`tail` 键才变。
  - `flush()` 返回本次通知的键集合，供调试用。
  - `reset` 让所有出现过、或者被订阅过的键都变化。
- **parity 样例从哪里来**：
  - 第一类用 `events.json` 的全部样例（已知、未知、残缺）。每个 `item.started`、`interaction.requested` 改用自己的 ID；关于条目或请求的样例分发给此前的每一个条目或请求。Rust 视图不接受的组合直接跳过，不写进样例。这样每一类条目都会碰到每一种补丁，未知类型（`canvas`）的条目会合并全部字段。
  - 第二类是情景样例，覆盖 `tail` 被收走、历史插入、`detached` 放回、`late_by_ref`。
  - 第三类是一条确定性的随机宿主流，取第一条跑完四个回合的流，截取前 70 步，用三种方式各 fold 一次：整条日志；按窗口（1 个回合、6 个条目）裁剪后的快照；快照加历史页。
- **差分测试的接法**：
  - 按 5.9.1，Rust 用 proptest 生成。Rust 半边是 `view::parity` 里一个 `#[ignore]` 的测试，读 `CUPPER_VIEW_CASES` 指定的文件路径，写出 300 条随机流，每条三种方式，窗口随序号变化。
  - TS 半边没有这个文件时整组跳过；Vitest 的 JSON 报告不计跳过的用例。
  - 新作业 `differential` 只列在 nightly 里，样例文件写在这次运行的日志目录里。
  - 设计 5.1.6 的作业表里还没有这个作业，汇报里提出。
- **手动验证**：差分测试在本机跑了 4 次，每次 900 个样例，都相等。parity 做过变异检查：去掉 notice 的可设字段、`may_still_run` 总写出来、不建立 `late_by_ref`、新回合不收走 `tail`、未知回合的请求不进 `detached` 而进 `tail`。五种改法各有 1–4 个样例失败。
