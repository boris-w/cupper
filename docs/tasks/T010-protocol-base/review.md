# T010 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并。
- 核对：
  - 在任务的 worktree 里重跑 `cargo xtask ci pr`，通过：rust 41 项，UNIT-digest-jcs、UNIT-proto-base 均为 pass。
  - `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim` 通过。
  - 没有 `unsafe`，没有个人信息。
- 规模：约 2,480 行，明显超出约 1,500 行的上限；执行者做完才发现，没有中途停下来汇报。超出的主要是测试（RFC 8785 的全部样例、性质测试、各类往返）。这是关键路径上的契约 crate，测试的分量有价值，所以接受。连续两个任务都超限，流程上做两处调整：
  - `CLAUDE.md` 子 agent 守则新增一条：每完成一个模块或一个提交，就统计一次改动行数；
  - 计划的规模估计偏低，后续写 brief 时按实际情况上调，契约和测试较重的任务要预留空间。
- 接受的取舍：
  - `SCOPE` 用 `Option<Scope>`；ID 只做宽松校验；
  - 限额按转义后的字节计；
  - serde_json 开启 `float_roundtrip`（工作区统一生效）；
  - JCS 的数字按"偶数优先"处理。
- 转交：给 T011、T012、T015 的事项已记入 `docs/tasks/handoff-notes.md`：
  - `fallback.summary`；
  - `ErrorData` 的具名字段；
  - `Event<B>` 的 schema 命名；
  - 把 iOS 编译检查纳入 CI。
- 设计措辞：由编排者修改。5.3.9 的"字符"改为"转义后的字节"；5.2.15 中 `transport` 的回退目录以 5.2.13 为准。
