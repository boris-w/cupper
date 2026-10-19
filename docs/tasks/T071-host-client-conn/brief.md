# T071 host-client I：连接与续传，ShellAPI 的协议部分

- 派发日期：2026-10-19
- 执行者：子 agent（TypeScript 工程师，桌面与协议客户端）
- 分支：`feat/T071-host-client-conn`
- 依赖：T015、T070（均已合并）。**零松弛**：桌面链 T015 → T071 → T075 → T076 → T077 → T090。T072、T075、T084 等依赖它。

## 背景

桌面端经 host-client 连接宿主（设计 5.7.6）：
- 主进程负责连接、握手和续传；
- 渲染进程经 ShellAPI 的 `HostPort` 拿到消息流。

T015 已经从 Rust 生成了 `@cupper/protocol`，内容包括 TS 类型、zod、`isKnownEvent` 和两端共用的样例。T070 做好了 ShellAPI 里与协议无关的部分，以及 TS 包的惯例。

本任务做 `packages/host-client` 的第一部分，以及 ShellAPI 里依赖协议的部分。视图归 T072。

## 范围

**做**（计划 T071 一行；设计 5.7.6、5.3.3、5.7.3）：
1. **`packages/host-client`**：
   - 连接状态机；
   - 切帧：`u32 长度 | u8 类型 | 负载`，包括 `0x02` blob 分片，单帧不超过 64,000 字节；
   - 请求关联；
   - 断线后，命令用同一个 `command_id` 重发；
   - 订阅与游标，包括 `data_epoch`；
   - 快照重置；
   - 连续性检查和三种计数，按 5.7.6。
   - 客户端同步规则与 Rust 的 Mirror（T049）一致：
     - 不论原因，收到快照就丢弃视图和游标；
     - 有缺口时带游标重新订阅；
     - `ViewError` 或信封解码失败时，不带游标重新订阅；
     - `stream/reset` 保留游标。
2. **ShellAPI 的协议部分**（5.7.3）：
   - `host.*`；
   - `HostPort`；
   - `Handshake`；
   - `PrivilegedAction` 和 `PrivilegedResult`。
3. **测试替身**：
   - 帧级的 TS mock 宿主：用 `protocol/fixtures/methods.json`；
   - 浏览器 mock 的 HostPort；
   - HostPort 合同测试套件：真实实现和 mock 都要通过同一套测试。

**不做**：
- 视图和 reducer（T072）；
- 原生扩展的宿主校验（T073）；
- 界面（T075 起）。

## 验收标准

- [ ] 新增 UNIT-hc-conn：
  - 连接状态机；
  - 切帧，包括 `0x02` 和帧上限；
  - 请求关联；
  - 断线后同一 `command_id` 重发；
  - 握手失败的各种情况。
- [ ] 新增 UNIT-hc-sync：
  - 订阅与游标；
  - `data_epoch` 改变时整体重置；
  - 快照分页与重置；
  - 连续性缺口；
  - 三种计数；
  - 一条事件残缺时只影响它自己，见交接事项。
- [ ] 新增 UNIT-hostport-contract-mock：mock 的 HostPort 通过合同测试套件。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T071-host-client-conn/ci/` 通过，摘要随任务提交。
- [ ] 测试标题以编号原文开头，例如 `describe("UNIT-hc-conn")`，CI 按 `ids::owner` 匹配。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **用生成的协议包**（T015）：
  - 从 `@cupper/protocol` 用 `zEvent`、`zEventBody`、`isKnownEvent`、`KnownEvent`；`isKnownEvent` 为 false 时记日志，并显示占位。
  - 生成的 `zStreamEventsParams`、`zStreamSnapshotParams`、`zHistoryPageResult` 会对数组里的每个事件做完整校验。只要有一个事件没有合法的外壳（例如缺 `seq`），整条消息就会失败；而 Rust 的 `Raw<T>` 只让那一条失败。所以要先把 `events`、`ops` 当作 `unknown[]` 取出来，再逐个 `zEvent.safeParse`。
  - `protocol/fixtures/methods.json` 可以直接给 mock 宿主用。
- **TS 包的惯例**（T070）：
  - `exports` 直接指向 `src/*.ts`；
  - 类型检查走 project references；
  - Vitest 只收 `src/**/*.test.ts`；
  - 包的 `test` 脚本必须运行 Vitest，作业会在末尾补上 reporter 参数，不要另起脚本名。
- **同步规则**（T014、T049）：
  - 规则见 5.3.11 末尾"T014 实现时的裁定"；
  - Rust 端的实现可以参照 `crates/cupper-cli/src/mirror.rs`，两端行为要一致。
- **宿主的握手**（T030、T039）：
  - 先是 `host/hello`，带 nonce；
  - `initialize` 只接受一次；
  - 桌面端在开发、自用、e2e 构建里都用 `dev_key`。证明的格式见 5.8.1，T039 正在实现 Rust 侧。TS 侧的证明计算放在主进程，不进渲染进程（5.7.8）。
  - 证明的格式以 5.8.1 为准；T039 合并后，用它的测试向量交叉核对。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 注意

- 代码约束（`CLAUDE.md` 的 TypeScript 部分）：`host-client` 不 import `electron`；渲染进程能用到的部分，不 import `node:*`。哪部分只能在主进程里运行，按 5.7.3、5.7.6 划分。
- 界面文案一律走 `@cupper/i18n`；宿主只下发语义码，不下发文案。
- npm 依赖锁精确版本，许可证要在 ADR-11 的 allowlist 内。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就统计一次实际行数**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `07-desktop.md` 5.7.3（ShellAPI）、5.7.6（host-client）、5.7.8；
  - `03-protocol-v1.md` 5.3.1、5.3.2、5.3.3、5.3.9、5.3.11；
  - `08-security.md` 5.8.1。
- `packages/protocol`（T015）、`packages/shell-api`（T070）、`crates/cupper-cli/src/mirror.rs`（T049）；SP-4a 的报告。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-19，编排者）

执行者开工前估计约 1,850 行，测试约占 745 行，于是按守则第 8 条在写代码之前停下汇报。

**裁定：不拆分。** 上限放宽到约 1,850 行，硬停止线 1,900 行。理由有两点：
- 计划要求状态机、切帧、续传共用一个连接对象，mock 宿主和合同测试也围绕同一套 HostPort 语义；
- T075 还要等 T073 和 T005，T071 稍长一点不会卡住零松弛的链。

执行者提出的 5 个接口点全部认可，已写进 `07-desktop.md` 5.7.6 末尾的"T071 开工前的裁定"。其中，用 T039 的测试向量交叉核对证明这一项改归 T073。

