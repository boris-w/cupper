# T059 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T059-host-supervisor`（T054 worktree）
- 完成日期：2026-10-20

T059 按裁定后的范围做完了：监护核心、空闲 / 孤立 / 待升级三种自行退出、自检、清理上次运行的残留适配器、流订阅都已接进真实宿主。`cargo xtask ci pr` 全部通过，四个用例编号都在摘要里。分支新增 933 行、删除 33 行，在约 1,300 的上限内。没有碰 `gateway/audit.rs`、审计写入、`store/retention.rs` 和审计接收端的去向，这些归 T138。

## 做了什么

1. **监护核心** `WatchCore`（`supervisor/watch.rs`，纯核心，无 I/O、无 await）
   - 输入：观察结果（连接数、运行中的回合、待处理交互、排队消息、安装器是否忙）、孤立、包已变、空闲定时器到期。
   - 输出：关闭准入（孤立或待升级）、退出（`Idle`、`Orphaned`、`Upgrade`）、定时器请求。
   - 规则：
     - 待升级：回合、待处理请求、排队消息都没有时立即退出，不看连接。
     - 孤立：没有运行中的回合就退出。
     - 空闲：完全空闲满 10 分钟（`supervisor_idle`）退出，期间任一活动都会取消计时。
     - 优先级：待升级 > 孤立 > 空闲；第一次观察之前不判定退出；退出后不再有任何动作。
2. **监护 actor 与宿主接线**
   - 新 actor 种类 `supervisor`：已登记，一个闹钟、一张定时器表，等待点包在 `Participant::idle` 里。
   - 每 `supervisor_observe`（1 s）观察一次各 `Part` 的活动之和与网关连接数；`dev-auth` 构建下同时比较父进程，变了就进入孤立。
   - 准入用共享的 `Admission`：会话管理器对 `session/create`、`session/send` 回 `unavailable`；待升级带 `pending_upgrade`，孤立不带 reason，原因记进 host.log。
   - 待升级时，`host/status` 报真，索引写 `host.updated{pending_upgrade: true}`。
   - 三种退出作为新的 `Exit` 取值交给 `Host::run`。`host/shutdown` 的 reason：空闲 `idle`、待升级 `upgrade`、孤立等其余 `user_quit`。host.log 记一行 `orderly exit`。
   - `Activity` 新增 `queued_messages`，会话句柄多一个 `queued` 计数。
3. **自检**：`SelfCheck` 比较 `current_exe()` 文件的 `file_cdhash` 与 `own_cdhash()`；后者读不到时不做自检。挂在两处：`Gateway::serve` 每次 accept 之后，适配器 `cupper_sys::spawn` 之前。
4. **给 T129 的接入点**：
   - `Watch::bundle_changed()` 用于报告 `Failed(BundleChanged)`。
   - 安装器在 `Host::start` 的 `Watch::new()` 之后启动。
   - 安装器忙闲由 T129 填进 `Observed.installer_busy`，现在固定为 `false`，代码里有注释。
5. **清理残留**（`supervisor/reap.rs`）
   - 读 `ended_at` 为空的 `adapters` 行，取每个适配器自己的进程组和 `groups` 里记下的后代进程组。
   - 先发 SIGTERM，按 `Clock` 每 50 ms 看一次，2 s 后对剩下的发 SIGKILL；只发给仍是记录中那个的组。
   - 清理完把这些行标为 `reaped`。
   - 排在 `maintenance.spawn` 之前；`reap()` 函数 T055 可以直接复用；两个时长已登记。
6. **流订阅**：`routes.streams(streams, readers)` 已接上。

## 关键取舍

- **排队消息只算会运行的**：队列暂停（等用户恢复）时记为 0。否则一条被中断后暂停的消息会让空闲退出和待升级退出永远等下去；这些消息在库里，退出不会丢。
- **孤立时不等待处理请求**：孤立只看运行中的回合，待升级还要等待处理请求。所以"孤立 + 待升级 + 只剩待处理请求"时按孤立退出。
- **监护 actor 分两阶段建**：`Watch::new()` 先给出句柄（自检和 T129 要用），等网关建好后再用 `Watching::start` 启动 actor；在此之前发来的消息排在信箱里。

## 验证方式

- **UNIT-orphan-policy**（4 个表驱动用例）：
  - 空闲 10 分钟退出，连接、回合、请求、排队消息、安装器任一出现都会重新计时；
  - 孤立时拒绝新会话和新回合，等回合结束才退出，连接不影响；
  - 待升级时立即退出，或依次等回合、请求、队列清空后退出；
  - 几种状态同时出现时的优先级，以及退出后不再有动作。
- **IT-idle-exit**：有连接时宿主不退出（期间订阅了一次索引流，验证流订阅可用）；断开后退出码 0，`last_clean_exit_at` 已写入，host.log 里恰好有一行以 `idle` 为原因的有序退出。
- **IT-self-check**：宿主从测试自己复制出的可执行文件运行，期间用假 agent 的 `wait_cancel` 让一个回合一直运行。删掉复制件后：
  - 下一个新连接触发待升级；
  - `session/create` 被拒（`pending_upgrade`）；
  - `host/status` 和索引流的 `host.updated` 都报待升级；
  - 中断回合后，客户端收到 `host/shutdown{reason: upgrade}`，宿主退出码 0。
- **清理残留**：并进 IT-lifecycle 的启动用例（计划里清理残留本来属于 IT-lifecycle）。测试自己用 `setsid` 拉起两个进程组：当适配器的那个死于 SIGTERM，忽略 SIGTERM 的后代死于 SIGKILL，`adapters` 行标为 `reaped`。
- **clippy 与测试**：不开 feature 和 `--features test-harness,dev-auth` 两种情况下 clippy 都无告警，`cargo test -p cupper-host --features test-harness` 全部通过。
- **CI**：在干净的工作区里 `cargo xtask ci pr --record docs/tasks/T059-host-supervisor/ci/` 通过：rust 1841、protocol 185、ts 403、license、testdata-scan 全部通过。跳过了 sensitive-words（本机没有词表）和截图回归（没有改界面相关的路径）。摘要 `pr-815c94c59b3c.json` 里有 UNIT-orphan-policy、IT-self-check、IT-idle-exit、IT-lifecycle 四个编号。

## 提交列表

```
fa4bc4d docs(task): T059 CI summary
815c94c docs(task): T059 notes after the ruling
613eb66 feat(host): end what a previous run's adapters left before retention starts
8f3fa17 feat(host): the supervisor's watch: idle exit, orphaned, pending upgrade
8eaba31 docs(task): T059 size estimate and split proposal
```

规模（相对分支起点，不含 docs）：新增 933、删除 33，其中测试约 330。

## 遗留问题 / 范围外发现

- **残留进程（已清理）**：开发过程中，失败的进程级用例留下了一些宿主和假 agent 进程。我逐个确认它们都来自本 worktree 的 target 或测试临时目录，然后用 SIGTERM 结束，现在没有残留。原因有两个：
  - `cargo test -p cupper-host` 不会构建 `cupper-fake-agent`，也不会构建 `cupper-cli` 的 `cupper-elevate-probe`，旧的或缺失的二进制会让无关用例失败；
  - 进程级用例失败时不会结束宿主进程。
  这个做法问题不归本任务，留给维护者决定是否加保护。
- **中断回合的回复可能丢失**：中断回合的命令回复和它引起的待升级退出会赛跑，回复可能来不及发出。命令结果在账本里，客户端重连后重试同一个 `command_id` 能拿到结果，所以用例不要求一定收到回复。
- **给 T138**：`Host::start` 里审计接收端的去向我没有动。本任务没有新增任何审计事件。