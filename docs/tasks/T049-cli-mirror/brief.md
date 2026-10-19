# T049 cupper-cli I-b：Mirror 与客户端同步状态机

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，CLI）。与 T045 是同一位执行者，Mirror 的设计也由这位执行者写成。
- 分支：`feat/T049-cli-mirror`
- 依赖：T045（已合并）。T046、T062 依赖它。本任务由 T045 拆分而来（计划 v1.3.6）。

## 背景

CLI 订阅宿主的流，在本地维护一份 Mirror：用 T014 的纯视图和同步规则，按客户端同步状态机处理快照、事件、缺口和重置。T045 已经做好连接、握手和请求关联；订阅结果按线上顺序进入通知流，类型是 `Incoming::Subscribed`。设计写在 `docs/tasks/T045-cli-client-mirror/notes.md` 的"Mirror 的设计"一节。

## 范围

**做**（计划 T049 一行；设计 5.2.14、5.3.3、5.3.11 末尾 T014 的裁定）：
- 纯核心 `Mirror<V: View>`，只能经 `Mirror::index()`、`Mirror::session(id)` 构造。
- 三个状态：`Waiting`、`Loading`、`Live`。
- 状态规则：
  - 只有游标对得上的 `replay` 才保留视图和游标。其余情况一律丢弃，包括客户端不认识的 mode；
  - 快照页要核对 `at_seq`、`epoch` 和页序号；
  - 连续性出现缺口时，带游标重新订阅；
  - `ViewError` 或信封解码失败时，不带游标重新订阅；
  - 收到 `stream/reset` 时保留游标；
  - 处于 `Waiting` 时，旧订阅的数据一律忽略。
- 与 T045 的 `Client` 接起来：订阅、处理 `Incoming`、重新订阅。

**不做**：子命令（T046）。

## 验收标准

- [ ] 新增 UNIT-mirror，覆盖：
  - 快照分页组装；
  - 不论原因收到快照，都丢弃旧状态；
  - 连续性缺口触发重新订阅；
  - `ViewError` 时不带游标重新订阅；
  - `stream/reset` 保留游标；
  - epoch 改变时整体重置；
  - `Waiting` 时忽略旧订阅的数据。
  - 用 T014 的视图和样例事件驱动，不需要真实宿主；也可以用 `protocol/fixtures/`。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T049-cli-mirror/ci/` 通过，摘要随任务提交。
- [ ] 测试名以 `unit_mirror_` 开头。

## 规模

S，估计约 450–560 行，上限约 900 行。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- `docs/tasks/T045-cli-client-mirror/notes.md` 的"Mirror 的设计"一节，以及 T045 的代码。
- `03-protocol-v1.md` 5.3.3、5.3.11 末尾"T014 实现时的裁定"；T014 的 `view/`、`sync.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
