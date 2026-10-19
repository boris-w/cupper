# T022 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T022-host-core-clock`
- 完成日期：2026-10-19

## 做了什么

在 `crates/cupper-host` 建好了 crate，内容如下：

- **crate 骨架**：lib 加 bin `cupper-host`，特性 `dev-auth` 和 `test-harness` 只建了开关，背后没有代码。`lib.rs` 只放 `mod` 声明。启动契约归 T023，所以 bin 目前只打印"startup contract is not implemented yet"，然后以退出码 1 退出。
- **`clock` 模块**（`src/clock/`）：
  - `MonoTime`：自有的单调时间类型，记录距时钟原点的时长。`Now { mono, wall_ms }` 是 `step` 收到的两个读数。
  - `Clock` trait：`now_mono()`、`now_wall_ms()`、`now()`、`timer_at(deadline) -> Timer`。`Timer` 是 future，drop 就等于取消。
  - `SystemClock`（`system.rs`）：整个宿主里唯一读取真实时间的地方，文件顶部有 `#![expect(clippy::disallowed_methods, reason = …)]`。
  - `ManualClock`（只在 `cfg(test)` 和 `test-harness` 下编译）：
    - 只能用 `advance`、`advance_to` 推进时间，时间不能往回走，往回调会 panic；
    - 墙钟跟着单调时钟走，毫秒对齐；
    - 推进时先把时间设到目标值，再在锁外按到期时间唤醒（同一时刻到期的按创建顺序）；
    - `next_deadline()` 加 `advance_to()` 可以逐个到期时间推进，给 T047 用。
  - `IdGen`（`id.rs`）：
    - `new_id()` 从 `Clock` 的墙钟取时间戳，`new_id_at(wall_ms)` 给纯核心用；
    - 时间戳之外的 74 位是计数器：换毫秒时取随机值（最高位清零，所以不会溢出），同一毫秒内加 1，因此 ID 严格递增；
    - 随机部分用 SplitMix64。`new(clock)` 从系统随机源取一次种子；`seeded(clock, seed)` 只在测试和 `test-harness` 下存在；
    - 返回 `uuid::Uuid`，加前缀的协议 ID 类型由 T010 负责。
- **纯核心框架**（`src/pure/`）：
  - `Core` trait：关联类型 `Input`、`Action`、`TimerKey`，方法 `step(&mut self, input, now: Now) -> Vec<Action>`、`timer_request(&Action) -> Option<TimerRequest<K>>`、`timer_expired(K) -> Input`。不用定时器的核心把 `TimerKey` 设成 `Infallible`。
  - `TimerTable<K>` 在生产代码里也用：按 (到期时间, 设置顺序) 排序，同名定时器重新设置就是改期，取消的不再触发。
  - 模块文档写明了 actor 适配层的循环，也就是 TurnMachine、QueueCore 等以后怎样接入。
  - `CoreDriver` 和 `Step`（只在测试和 `test-harness` 下）：用 `ManualClock` 驱动核心，提供 `input`、`advance`、`run(script)`。`run` 返回带时间戳的动作记录，定时器在各自的到期时刻依次触发。
- **静止屏障的参与者登记**（`src/quiescence.rs`）：
  - `ActorKind`；`Quiescence` 提供 `register`、`kinds`、`running`、`enter -> Participant`，`Participant` 在 drop 时计数减一。
  - 没登记就 `enter`，或者同一种类登记两次，debug 构建断言失败。
  - 登记表是实例，不是全局变量，所以并行的测试互不干扰。屏障本身留给 T047。

**根配置的改动**（brief 要求列出）：

- `clippy.toml`：`uuid::Uuid::now_v7` 这一条加了 `allow-invalid = true`，附两行注释。原因：`cupper-host` 构建 `uuid` 时没开 `v7`，`now_v7` 根本不存在，clippy 每次运行都警告"路径不可达"。xtask 的负面样例开了 `v7`，仍然在验证这一条会报错，所以这样改不会掩盖写错的路径。
- `Cargo.lock`：新增依赖带进来的条目。根 `Cargo.toml` 没有改。

## 关键取舍

1. **单调时间用自有的 `MonoTime`，不用 `Instant`**：这样虚拟时间可以从 0 开始，测试结果确定、断言好写。`SystemClock` 内部再换算成 tokio 的 `Instant`。
2. **`ManualClock`、`CoreDriver`、`IdGen::seeded` 用 `cfg(any(test, feature = "test-harness"))` 门控**：依据是 5.9.9 的"注入点只在 test-harness 存在"，这样发布构建里没有它们。代价是 `tests/` 下的集成测试如果用到它们，要声明 `required-features = ["test-harness"]`。
3. **`ManualClock` 推进时先设好时间、再按顺序唤醒，而不是边推进边唤醒**：在多线程 runtime 上，被唤醒的任务可能在推进的过程中就开始运行。先设时间，它们读到的时间就是一致的。"每个到期时间处理完再推进到下一个"的严格语义，由 T047 用 `next_deadline` 加 `advance_to` 再加屏障来实现。
4. **定时器放进 `Core` trait（`TimerKey`、`timer_request`、`timer_expired`），而不是框架层再包一层动作**：这样保持了设计规定的 `step -> Vec<Action>` 形式，驱动器和 actor 也能用同一套逻辑执行定时器。
5. **`IdGen` 只取一次系统种子，再用 SplitMix64，不是每次取系统随机数**：生产和测试走同一条代码路径，只有种子不同。我核对了 5.4.4 和安全设计，ID 没有不可预测的要求。`new_id_at` 让纯核心不必读时钟，铸出的 ID 时间又和 `step` 的 `now` 一致。
6. **登记表在所有构建里都编译**：actor 在任何构建里都会调用 `enter`，代价是每启动一个 actor 加一次锁。`Participant` 是 T047 加忙闲跟踪的扩展点。
7. **依赖写在 crate 自己的 `Cargo.toml`，没放进 `[workspace.dependencies]`**：用的是 tokio 1.53.3（只开 `time`）、uuid 1.28.0（只开 `std`）、getrandom 0.4.3，目的是避免和并行的任务冲突。合并时编排者可以再提到工作区。

## 验证方式

`cargo xtask ci pr --record docs/tasks/T022-host-core-clock/ci/` 在提交 055abb6 的干净 worktree 里通过：rust 31 项、ts 3 项、license 1 项，其余作业"尚未实现"。摘要是 `ci/pr-055abb62d192.json`，已经提交，里面没有路径和用户名。逐编号的结果是 UNIT-clock-manual、UNIT-idgen、UNIT-quiescence-registry 都为 pass。

对照验收标准逐条说明：

- [x] **UNIT-clock-manual**，5 个用例：
  - 用记录唤醒顺序的 waker 断言按到期时间顺序触发，同时覆盖同一时刻到期、drop 等于取消、`next_deadline`；
  - 已经到期的定时器立即完成；
  - 按亚毫秒的步长推进时，墙钟和单调时钟一致；
  - 时间不能往回走；
  - 在一个**没有启用 time driver** 的 tokio runtime 里等待 1 小时的定时器，推进后任务按顺序完成。这证明没有用到真实的 sleep 或 tokio 计时器：要是用了，会 panic 或者卡住。
- [x] **UNIT-idgen**，3 个用例：
  - 时间戳由 `uuid` crate 独立解码，等于 `Clock` 的墙钟；版本是 7，变体是 RFC；同时覆盖 `new_id_at`；
  - 同一个种子加 `ManualClock` 产出的 ID 完全相同，换一个种子就不同；
  - 4,000 个 ID 在同一毫秒内和跨毫秒都严格递增。
- [x] **UNIT-quiescence-registry**，3 个用例：
  - 登记的种类能按登记顺序枚举，运行中的计数随 `enter` 和 drop 增减；
  - 没登记就启动时 debug 断言失败（`cfg(debug_assertions)` 加 `should_panic`）；
  - 重复登记时断言失败。
- [x] **`disallowed-methods` 在 `cupper-host` 上生效，只有 `clock` 例外**：
  - 如果 lint 没有生效，`system.rs` 里的 `#![expect]` 会触发 unfulfilled 警告，在 `-D warnings` 下失败；
  - 我临时在 `lib.rs` 里加了一处 `Instant::now()`，clippy 报"use of a disallowed method"，之后已还原；
  - 用例 `clock_module_is_the_only_disallowed_methods_exemption` 扫描 `src/` 和 `tests/`，`src/clock` 之外出现 `disallowed_methods` 就失败；它还禁止 crate 内放 `clippy.toml`。
- [x] 在干净 worktree 里跑 CI 通过，摘要已随任务提交。
- [x] 新用例的测试名以用例编号的 snake_case 开头，摘要里的逐编号清单已经识别出来。

另外还做了这些检查：

- 不带特性的 `cargo clippy -p cupper-host --all-targets -- -D warnings` 通过，只编 lib 的 clippy 也通过。
- `cargo test --release` 14 项通过，两个只在 debug 下的 `should_panic` 用例在 release 下不编译。
- 带特性和不带特性的 `cargo doc` 都没有警告。
- `cupper-host` 的全部测试连跑 20 次全绿；clock 用例单独连跑 50 次全绿。

**规模**：`crates/cupper-host` 加 `clippy.toml` 共新增约 1,425 行，不含 `Cargo.lock` 和 CI 摘要。其中实现和文档注释约 880 行，测试约 500 行。超出了计划估的 M（400–900），但在 1,500 的上限以内，超出的部分主要是测试和文档。

## 提交列表

```
c419f23 docs(task): record the T022 PR CI summary
055abb6 docs(host): keep rustdoc links to test-only items out of default builds
b12630f feat(host): register actor kinds as quiescence barrier participants
120b297 feat(host): add the pure core framework and its test driver
bdc16a5 feat(host): add IdGen for UUIDv7s on the host clock
cda1321 build(clippy): stop the unreachable-path warning for the now_v7 ban
69c9332 feat(host): add Clock with SystemClock and ManualClock
3b9b1f6 build(host): add the cupper-host crate with its feature flags
```

## 遗留问题 / 范围外发现

1. **给 T047 的建议（屏障）**：定时器触发后，actor 开始处理之前，有一小段时间屏障会看到"空闲"。建议：
   - actor 在 poll 自己的等待点之前先标记为忙，只在等待的 future 返回 Pending 时才算空闲，例如给 `Participant` 加一个 `idle(fut)` 包装；
   - 再统计在途的消息和已触发、还没被消费的定时器；
   - 逐个到期时间推进用 `ManualClock::next_deadline` 加 `advance_to`；
   - 存储写线程和 `spawn_blocking` 里的工作也要作为参与者登记，`ActorKind` 不限于 actor。
2. **给 T043 等第一个 actor 任务**：适配循环已经写在 `pure/mod.rs` 的文档里。actor 侧"给 `TimerTable::next_deadline` 维持一个 `Clock` 定时器"的小助手还没写，因为现在还没有使用方，建议由第一个 actor 任务加在 `pure` 里。
3. **T024**：写线程"只在 `SystemClock` 下等 2 ms"。`Clock` 没有"是否虚拟时钟"的查询，建议从启动配置（`test_config.clock`）判断，或者到时再给 trait 加方法。
4. **集成测试的门控**：`tests/` 下用到 `ManualClock`、`CoreDriver`、`IdGen::seeded` 的集成测试要声明 `required-features = ["test-harness"]`；5.1.5 那次不带特性的 `cargo test -p cupper-host` 会跳过它们。
5. **合并时要处理的冲突**：`Cargo.lock` 会和 T010、T020 冲突，合并时重新生成即可。tokio、uuid、getrandom 是否提到 `[workspace.dependencies]`，由编排者决定。
6. **范围外 1：宿主 ID**：宿主 ID 是 `h_` 加 UUIDv4（5.4.4），不归 `IdGen` 管，而且 `cupper-host` 构建 `uuid` 时没开 `v4`。实现宿主 ID 的任务需要自己取随机源。
7. **范围外 2：CLI 的 command_id**：`cupper-cli` 要在客户端生成 UUIDv7 的 `command_id`（5.3、5.7），而工作区的 `now_v7` 禁令对所有 crate 生效。T045 需要一个带 reason 的豁免，或者用自己的生成器。建议在 T045 的 brief 里写明。
8. 本任务没有写 `notes.md`，思路和取舍都在本汇报里。
