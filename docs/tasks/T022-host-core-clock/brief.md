# T022 宿主核心骨架 I：时钟、纯核心框架与静止屏障登记

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T022-host-core-clock`
- 依赖：T001（已合并）。M1a 第 1 波。宿主的所有任务都依赖它。

## 背景

设计评审的阻断项（测试 #1）要求：宿主里的时间一律经 `Clock` 获取，有状态的决策写成纯核心，这样才能用虚拟时间做确定性测试（设计 5.2.16）。本任务建立 `cupper-host` crate，以及这套时间与纯核心的基础设施。

## 范围

**做**（计划 T022 一行）：
- 建立 `cupper-host` crate，含 lib 和 bin，特性有 `dev-auth`、`test-harness`。特性的含义见 5.1.3；本任务只建特性开关，不实现背后的功能。
- **时钟**：
  - `Clock` trait：提供单调时钟、墙钟，以及按截止时间等待的计时器；
  - `SystemClock`：生产实现；
  - `ManualClock`：测试实现，能推进时间，到期的计时器按截止时间的顺序触发。
- **`IdGen`**：UUIDv7 的时间部分取自同一个 `Clock`。
- **纯核心框架**：`step(&mut self, input, now) -> Vec<Action>`，加上测试驱动器，即按输入序列和虚拟时间驱动纯核心，并断言产出的 Action。
- **静止屏障的参与者登记**：每种 actor 要先登记，没登记就启动时，debug 构建断言失败。屏障本身由 T047 实现。
- `clock` 模块是唯一允许调用被禁方法的地方，用 `#[expect(clippy::disallowed_methods, reason = …)]`。

**不做**：
- 启动契约、注入点、故障点（T023）；
- `test/clock_advance` 方法（T047）；
- 任何具体的 actor。

## 验收标准

- [ ] 新增 UNIT-clock-manual：
  - 推进时间后，到期的计时器按截止时间顺序触发；
  - 墙钟和单调时钟一致；
  - 不依赖真实的 sleep。
- [ ] 新增 UNIT-idgen：UUIDv7 的时间戳等于 `Clock` 的墙钟；在 ManualClock 下结果确定。
- [ ] 新增 UNIT-quiescence-registry：没登记的 actor 启动时，debug 断言失败；登记过的能被枚举。
- [ ] clippy 的 `disallowed-methods` 在 `cupper-host` 上生效，只有 `clock` 模块例外。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T022-host-core-clock/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（计划 1.6）。

## 注意

- 写纯核心框架时，想清楚以后 TurnMachine、QueueCore 等怎样接入（5.2.16），把接口设计好；本任务不写这些核心。
- 规模上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`02-host.md` 5.2.3、5.2.16；`01-repo-build-ci.md` 5.1.3、5.1.7。
- `CLAUDE.md` 的 Rust 约束；设计评审纪要 D12（`docs/meetings/2026-10-16-design-review/minutes.md`）。
- SP-4a 的 `normalize.rs`：它已经把 `now` 作为参数传入，可以作为参考。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
