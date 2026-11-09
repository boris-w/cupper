# T140 评审

- 评审人：编排者
- 日期：2026-11-04
- 结论：**通过，合并**（PR #82）

## 核对

- **规模**：+862 −73。开工前估约 760，拆出 T243 后停止线定为 650、硬停止线 720。执行者做到 +730 时越线，停下来报告；余下的 CLI 和三个进程级用例约 140 行，是验收标准的一部分，所以不再拆，硬停止线放到 880。
- **CI**：PR #82 https://github.com/boris-w/cupper/pull/82 。5350069f 上 8 个检查全过（run 41809862898）；merge main 之后的 d7a9685a 上也是 8 个全过。
- **连跑**：三组各 10 次 × 4 份，日志里都是 40 passed：
  - 三个进程级用例，run 41809879467；
  - PROP-steer-only-prompting，run 41809913125；
  - IT-cli-commands，run 41810044357。
- **协议**：只附加了 `FallbackReason::steer_pending`，schema-compat 为 additive。
- **合并**：T243 先合并，两边在 `tests/driver_k.rs` 的 import 上有冲突。执行者 merge main 时两边都保留，合并后在本机跑过 fmt、clippy、host lib、`--test driver_k`（25 个，含双方的用例）和 `cargo xtask ci protocol`。
- **个人信息**：改动的文件过了敏感词清单，干净。

## 逐项意见

1. **前提判定和七个回退原因都在纯核心里**：
   - 不做 I/O，时间经 `Clock`；
   - `Preparing`、`Cancelling`、`Autonomous`、`Ending` 都不算 `Prompting`，K11 的规避因此成立；
   - `mode: queue` 的路径没有改动，原有的属性测试照常通过。
2. **时序按开工前定的方案落实**：
   - `turn.completed` 等在途插话的结果，消息只会出现在一个地方；
   - 晚到的应答会把还在队列里的那一条取出，避免同一条消息发给 agent 两次。
   - PROP 把这两条规则都写成了性质，执行者分别改坏试过，属性测试都能抓到。
3. **驱动守住了 K28**：在途请求不丢弃；回调里只做 channel 发送。结果以驱动事件交回，和 `Prompt` 一致。
4. **IT-K11 取 `Cancelling` 做"不在 Prompting"**：结果确定，比用自发回合稳定，这个取舍对。
5. **CLI 从会话视图取 `expected_turn_id`**：`session/get` 不带回合 ID，事先已同意。

## 记下的点

- **写进交接事项**：
  - `started_new_turn` 之后，如果原回合的 prompt 一直不返回（K1 那类挂起），这次 `steer` 的回复会一直等着。概率很低，T242 的界面要能承受迟迟没有回复的情形。
  - 在原回合的 prompt 响应到达之前，新回合的早期输出会记在原回合里。界面不用为此特殊处理。
  - T149 的 `queue_edit` 插话复用这里的判定和回退原因。
- **设计已在合并时改**：
  - 5.2.6"排队、Steer、中断"的 `mode: steer` 一行；
  - 5.6.2 的 `Steer`；
  - 5.2.14 的 `cupper send --steer`；
  - 5.3 的 `fallback_reason` 清单。
