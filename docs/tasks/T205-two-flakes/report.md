# T205 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T205-two-flakes`（T052 worktree）
- 完成日期：2026-10-28
- PR：https://github.com/boris-w/cupper/pull/21 。CI 运行：https://github.com/boris-w/cupper/actions/runs/40649145948 。第 2 次 attempt 上 8 个检查全部通过；第 1 次 attempt 的 rust 作业失败在一个无关用例上，见"遗留问题"。

**状态**：两处都查明了根因并修好。runner 上修复前都复现出了失败，修复后各连跑 600 次（任务书要求 200 次），没有失败。

## 做了什么

**1. `it_runtime_first_prompt_waits_for_the_install`：根因在测试，已修测试**
- 回合结束写在会话流里，安装失败的原因写在索引流的 `agents.updated` 里。这两条流由不同的写者写入，先后没有保证。
- 安装器先给索引 actor 发邮件，再唤醒等待中的启动 task。tokio 的 LIFO 槽往往让启动 task 先跑，所以"启动失败 → 回合 failed"常常先进库。
- 用例一看到 `turn.completed` 就去读失败原因，读到的还是 `installing` 那一条，所以是 `"-"`。索引稍后会写上 `hash_mismatch`，最终状态没错。
- 修法：先等索引里 runtime 到 `ready` 或 `failed`（用已有的 `until`，不用 sleep），再读 `failure`。结束状态和失败原因在同一条事件里，等到前者就能读到后者。

**2. `gold_b_claude_approval`：根因在宿主，已修宿主，并补了稳定复现的用例**
- `Control::exit` 先给 supervisor 发 `Exit::Requested`，再返回应答，网关的应答 task 随后才把应答推进发送队列。
- supervisor 如果被别的 worker 拿去先跑完有序退出，执行 `close_all` 之后发送队列进入 Finishing，后到的应答被直接丢掉，客户端读到 EOF。
- 这违反了设计 5.2.13 的裁定："`host/exit` 的回复与 `host/shutdown` 都在连接关闭之前进入发送队列"。
- 修法：
  - `Call` 新增 `reply_queued`（oneshot）。网关的应答 task 把应答推进发送队列之后触发它；推不进去、或者发送端被丢弃，它也会结束，所以等它的一方不会挂住。
  - `Control::exit` 记下结果后另开一个小 task，等 `reply_queued` 结束，再给 supervisor 发 `Exit::Requested`。
  - 只改了 `host/exit`，别的请求的行为不变。
- 新用例 `it_lifecycle_host_exit_starts_once_its_answer_is_queued`，在 `gateway/dispatch/tests.rs`，属于 IT-lifecycle，不新增编号：
  - 测试扮演网关：应答 future 已经结束、但还没入队时，supervisor 必须什么都没收到；触发之后才收到 `Exit::Requested { force: true }`；
  - 另一半核对：`reply_queued` 结束时，应答已经在发送队列里。

## 关键取舍

- **第一处不改宿主**：两条流之间本来就不保证先后，界面也是分别订阅两条流的，所以不算产品问题。
- **第二处只针对 `host/exit`**：考虑过"关闭连接时等所有在路上的应答（以关闭宽限期为上限）"。没有采用，原因有两个：
  - 它会改变所有请求在退出时的行为；
  - 手动时钟下宽限期不会自己到期，某个应答不来，退出就会挂住。

  设计只对 `host/exit` 有裁定；别的命令可以带同一个 `command_id` 重试，拿到账本里记下的结果。
- **也没采用**：
  - 靠 future 的 drop 时机来触发退出：太隐晦；
  - 让 Control 自己推应答：`Call` 里没有请求 id，而且网关会再推一次。

## 验证方式

**根因的证据**（临时改动，都没有提交）
- 第一处：在索引处理 `Mail::Runtime` 的地方阻塞 300 ms，单跑一次。失败信息和 runner 上的一模一样：`left: ["-"]`，`right: ["hash_mismatch"]`。改好的用例带着这个延迟能通过，去掉延迟也通过。
- 第二处：在 `Control::exit` 发出退出之后、返回应答之前阻塞 300 ms。GOLD-B 每次都失败，信息和 runner 上的一模一样：`gold_b.rs:394:20`，`Closed(Some(Eof))`。修复后在同一位置阻塞 300 ms，通过。
- 新用例的修前修后对照：把 Control 临时改回"先发退出、再返回应答"，它每次都在 `exits.try_recv().is_err()` 上失败，不靠时序；改回修复后的代码，通过。

**修复前在 runner 上的失败率**（main 1dae188）

| 用例 | 运行 | 次数 | 失败 | 失败信息 |
|---|---|---|---|---|
| 第一处 | run 40647386890（100 次 × 2 份并行） | 200 | 2（1%） | 和 CI 上的相同 |
| GOLD-B | run 40647390457（200 次 × 3 份并行） | 600 | 3（0.5%） | 都是 `gold_b.rs:394:20 Closed(Some(Eof))` |

另外，第一处在 CI 上还失败过两次：run 40639964185，以及 run 40643620891 的第 2 次 attempt。

**修复后的连跑**（提交 2a3ba4d，条件和修复前相同，次数多于验收要求的 200）
- 第一处 300 次 × 2 份并行：run 40649143139，下载 `results.txt` 核对，600 次 ok，0 次失败。
- GOLD-B 200 次 × 3 份并行：run 40649225638，核对 `results.txt`，600 次 ok，0 次失败。

**本机检查**（都只跑一份）
- 通过：fmt；clippy 的四组（无特性、dev-auth、test-harness、all-features），`-D warnings` 无告警；库测试 gateway、supervisor、index（test-harness，65 个）；不带特性的 gateway::dispatch；gold_b（12 个）；lifecycle（19 个）；migrations。

**验收标准**
- [x] 两处的根因都有证据。
- [x] runner 上修复后各连跑 600 次，全部通过。
- [x] PR #21 的 8 个检查在第 2 次 attempt 上都通过。

## 提交列表

```
2a3ba4d docs(task): T205 notes on both flakes' causes, evidence and fixes
b3de035 fix(host): host/exit's exit starts once its answer is queued
ca8b4e0 test(host): IT-runtime-first-prompt-waits waits for the index's runtime end
```

规模（`cargo xtask size`）：新增 76、删除 8，停止线 150、硬停止线 200。行数估计、根因、证据和取舍都写在 `docs/tasks/T205-two-flakes/notes.md`，随分支提交。

## 遗留问题 / 范围外发现

- **退出时别的请求的应答仍可能丢**：`host/exit` 以外、和退出撞上的请求，应答照旧可能被丢弃，客户端看到连接断开。它们都能按 `command_id` 重试拿到结果，所以这次没改。要不要让关闭连接时等所有在路上的应答（以宽限期为上限），需要设计上定，并考虑手动时钟下宽限期不会自己到期的问题。
- **独立进程的宿主照样读不到应答**：`it_lifecycle_host_exit_and_sigterm_exit_in_order` 容忍 `host/exit` 得到 `Closed`，因为宿主进程退出之后，客户端校验令牌会拒收后面的读。所以对独立进程的宿主，即便应答已经入队，客户端也可能读不到。这次没有改，属于另一个问题（T196 的说明里提到过）。
- **`it_nfr9_crash_under_load_kill9_again_and_again` 仍会偶发失败，和本任务的改动无关**：PR #21 第 1 次 attempt 的 rust 作业失败在 `crash_concurrency.rs:344:13`，"no load in round 3"，只重跑 rust 作业后通过。这个用例不发 host/exit，delta 经订阅推送，不走请求的应答路径；失败形态就是 T199 第 3 处的老问题（PR #4，run 40322065826）。每轮要在 30 s 内等到 200 条新 delta，coalescer 按 40 ms 的节奏写，至少要 8 s，runner 负载高时余量仍然不够。建议另开任务。
- **两条流之间不保证先后**：设计里没有明说。用例读跨流的状态时都要等对象本身，不能借另一条流的事件当时序。别的用例里可能还有类似的写法，这次没有排查。
- **修复后的连跑结果没有写进 `notes.md`**：只写在本汇报里，由编排者存档时一并记下。
