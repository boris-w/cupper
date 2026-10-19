# T049 汇报

- 执行者：子 agent（Rust 工程师，CLI）
- 分支：`feat/T049-cli-mirror`
- 完成日期：2026-10-19

T049 已完成：客户端同步状态机 Mirror 及其测试 UNIT-mirror 都写好了，`cargo xtask ci pr` 全部通过。新增 650 行，删除 3 行，在约 900 行的上限以内。

## 做了什么

1. **纯核心 `Mirror<V: View>`**（新文件 `crates/cupper-cli/src/mirror.rs`），按 T045 notes.md 里"Mirror 的设计"一节实现：
   - 只能用 `Mirror::index()` 或 `Mirror::session(id)` 构造，视图类型和流配不错。
   - 三个状态：`Waiting`、`Loading`、`Live`。
   - 对外的方法：
     - `subscribe()`：进入 `Waiting`，返回订阅参数，有游标时带上游标；
     - `handle(&Incoming)`：处理一条消息，返回 `Ok(Some(params))` 表示要用这组参数重新订阅；订阅失败时原样返回 `RpcError`；
     - `view()`、`is_live()`、`resume()`、`stream()`。
   - 状态规则：
     - 只有 `epoch` 和 `at_seq` 都对得上游标的 `replay`，才保留视图和游标。其余情况一律丢弃，包括不论原因的快照、不认识的 mode、对不上游标的 replay。
     - 快照页要核对 `at_seq`、`epoch` 和页序号。
     - 连续性出现缺口时，带游标重新订阅。
     - `ViewError`、信封解码失败、在 `Loading` 时收到事件，都不带游标重新订阅。
     - 收到 `stream/reset` 时保留游标。
     - 处于 `Waiting` 时，旧订阅的数据一律忽略。
2. **接到 T045 的 `Client` 上**：新增两个异步函数，`mirror::subscribe(client, mirror)` 和 `mirror::fold(client, mirror, incoming)`。
   - `fold` 在 Mirror 要求重新订阅时，经 `Client::subscribe` 发出。
   - 没有改 `client.rs`，避免和 T039 冲突。
3. **`lib.rs`**：加了 `pub mod mirror;`。另外改了 3 行模块文档：原文是"mirror 由 T049 提供"，改成指向 `mirror` 的链接。这超出了"只加 mod 行"的要求，但只动了文档。

## 关键取舍

- **纯核心和异步胶水分开**：Mirror 不做 I/O、不 await，符合纯核心约束，测试可以直接用 JSON 构造的 `Incoming` 驱动。经过 `Client` 的部分只有两个很小的异步函数。
- **两种不认识的输入，处理不同**：
  - 快照页里连 `type` 都解不出的 op 直接跳过，和不认识的类型一样不改变视图；
  - 事件的信封解不出时（拿不到 seq），不带游标重新订阅。这样新快照绕过了这条事件，不会每次重新订阅都要回同一条坏事件、陷入循环。
- **两类重订阅，对视图的处理不同**：
  - 整体重置时（`ViewError`、对不上的 replay）立刻清空视图，新快照到齐之前 `view()` 为空，不展示出错的半成品。
  - 缺口和 `stream/reset` 保留视图，因为视图仍然对应游标。
- **`Live` 时收到快照页直接忽略**：这在协议里不该出现，按"只在 `Loading` 时接收"的规则处理。

## 验证方式

- 在干净的 worktree 里跑了 `cargo xtask ci pr --record docs/tasks/T049-cli-mirror/ci/`，提交 `af89f1e` 上全部通过，摘要文件是 `pr-af89f1efd33d.json`：
  - rust：807 通过；
  - protocol：176 通过；
  - ts：232 通过；
  - license：通过。
- UNIT-mirror、UNIT-cli-client、UNIT-cli-host-verify 都在摘要里，结果都是 pass。
- 本 crate 单独检查：
  - `cargo test -p cupper-cli`：19 例通过，`--all-features` 下同样通过；连跑 10 次没有出现偶发失败。
  - `clippy -D warnings`：无特性和 `--all-features` 下都通过。
  - `cargo doc`：没有警告。
- 对照验收标准：
  - [x] UNIT-mirror 已覆盖以下各项：
    - 快照分页组装（`unit_mirror_snapshot_pages_assemble_the_view`）：
      - 按页组装的视图等于直接应用全部 op 的结果；
      - 最后一页到达之前视图为空；
      - `at_seq`、`epoch`、页序号任一不符就重来。
    - 不论原因收到快照，都丢弃旧状态（`unit_mirror_any_snapshot_drops_the_view_and_cursor`）：覆盖无原因、`cursor_expired`、`cursor_ahead`、不认识的 mode，以及对不上游标的 replay。
    - 连续性缺口触发重新订阅（`unit_mirror_gap_resubscribes_with_the_cursor`）：
      - 带 `from` 的区间事件能通过检查；
      - seq 跳号和 seq 重复都带游标重新订阅，视图保留；
      - 之后的 replay 能接着往下走。
    - `ViewError` 时不带游标重新订阅（`unit_mirror_view_error_resubscribes_without_the_cursor`）：覆盖对不存在的条目发 delta、信封缺 seq、快照里的 `ViewError`、`Loading` 时收到事件。
    - `stream/reset` 保留游标（`unit_mirror_reset_keeps_the_cursor`）。
    - epoch 改变时整体重置（`unit_mirror_epoch_change_resets_and_reuses_seqs`）：新 epoch 里 seq 可以重用，不报重复。
    - `Waiting` 时忽略旧订阅的数据（`unit_mirror_waiting_ignores_the_old_subscription`）：另外覆盖了其他流的消息被忽略、订阅失败交还调用方。
    - 经过 `Client` 跟随宿主（`unit_mirror_follows_the_host_over_a_client`）：用进程内的假宿主，依次经过快照、缺口、带游标的 replay，并断言两次订阅的参数。
  - [x] 测试名都以 `unit_mirror_` 开头。
  - [x] CI 摘要已随任务提交。
- **规模**：650 行，其中代码 255、测试 395，比估计的上端（560 行）多 16%。每个模块的统计写在 `docs/tasks/T049-cli-mirror/notes.md`。

## 提交列表

```
4bab090 docs(task): record the T049 pr CI summary
af89f1e docs(task): record the T049 line counts and the mirror's choices
c5d8f9c test(cli): follow a host's stream through the client with a mirror
a00031f feat(cli): mirror a stream with the client-side sync rules
```

## 遗留问题 / 范围外发现

- **宿主写完就关闭连接时，客户端会丢掉最后读到的数据**（T045 的 `client.rs`）。
  - 原因：读循环每次读到数据都先查对端令牌。对端已经关闭时，`LOCAL_PEERTOKEN` 返回 `ENOTCONN`，这批数据被丢弃，关闭原因记为 `Refused(PeerExited)`，而不是 `Eof`。
  - 从安全角度看这样做是对的：认不出是谁写的数据就不处理。
  - 实际影响很小，因为宿主发 `host/shutdown` 之后有 2 秒的关闭宽限。
  - 需要注意的地方：T046 要报告"宿主已退出"时，应把 `Refused(PeerExited)` 和 `Eof` 当作同一种情况。本任务经过 `Client` 的用例也因此让假宿主等客户端先挂断。
- **两份重复的测试辅助**：`mirror.rs` 的测试里有一个很小的进程内假宿主，用途和 `client.rs` 测试里的 `FakeHost` 相同。为了不和 T039 冲突，这次没有合并成共用的测试辅助；T039 合并后可以合并。
