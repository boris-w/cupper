# T065 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`test/T065-gold-b-runner`（在 T037 的 worktree 里，从 main c75e0d6 开出，未合并 main）
- 完成日期：2026-10-20

B 层金样例运行器做完了。三个剧本的金样例连跑 6 次都稳定，`pnpm test:golden` 已接进 `xtask` 的 `rust` 作业，带记录的 CI 一次通过。规模净增 1,152 行，略高于第二次裁定的上限约 1,150，在硬停止线 1,200 之内；中途越过原硬停止线 950 而没有及时统计，见"规模"一节。

## 做了什么

1. **公开 T067 的比对规则**：把 `cupper_fake_agent::replay::Mapping` 的 `check`、`outgoing` 改为公开，B 层复用同一套比对（id 重映射、`cwd`/`path` 占位、忽略 `_meta`）。
2. **进程内 agent 的入口**，只在 `test-harness` 构建里有：
   - 新增 `test_config.in_process` 字段，不参与反序列化，只有在测试进程里起宿主时才能设置。
   - `Finder` 据此给出 `Program::InProcess`。`launch::start` 遇到它时不起进程：建一对 `MemoryTransport`，把 agent 一端和退出码的发送端交给测试，照常记 `instance_started` 和 `instance_exited` 两条 note。
   - **从源头上不进入发信号、回收进程组的路径**：
     - `Instance.group` 改为 `Option<ProcessGroup>`，进程内实例是 `None`，也不写 `adapters` 行；
     - 运行时里发信号、采样的五处（`Due::Kill`、`term`、`sample`、`Mail::Stop`、`Wire::Exited`）都要先拿到进程组才能进入；
     - 所以 0、1、-1 和本进程的组号都不可能出现。
   - 单元测试 `gold_b_fake_in_process_adapter_has_no_process_to_signal`：`launch::start` 给出的实例没有进程组；不写 `adapters` 行；停止时完整走过"等会话关闭、关 stdin、SIGTERM、SIGKILL"四个截止时间，共 8 s，什么都不发，最后等到 agent 自己退出。
3. **验证 `method not found` 同步的用例**：`gold_b_fake_sdk_answers_an_unknown_request_after_what_came_before`。
   - agent 先发 50 条通知和一个请求的应答，再发一个宿主不认识的请求。
   - 回复到达时，邮箱里正好 50 条通知，应答的回调也已执行。
   - 用例和 `Run::sync` 的注释都写明：这依赖 SDK 3.3.0 的 `incoming_protocol_actor` 逐条分发，换 SDK 时这条用例会先失败。
4. **规范化**，放在 `tests/gold_b/normalize.rs`：
   - 规则依据 `schema::document()` 里的 `x-id`、`x-time`、`x-path` 标记；
   - 同一条目、同一字段的连续 delta 折叠，去掉 `seq` 和 `from`，键排序；
   - 能找出第一个不同的字段路径；
   - 单元测试每类标记都有正反用例，另有扁平联合类型里的 `cwd`、delta 折叠和键排序。
5. **B 层运行器**，放在 `tests/gold_b.rs`：
   - 在测试进程里用 `Host::start` 起宿主，用手动时钟；测试扮演 agent，按录制里的 `t` 经 `test/clock_advance` 推进时钟。
   - 每写一帧都同步一次：先发未知请求往返，再等屏障静止。
   - 宿主到了录制里的时间点还没发出下一帧时，测试扮演用户，发 `create`、`send`、`respond` 或 `interrupt`。每条命令单开一个连接，异步等回复。
   - 最后读出会话流，先严格解码，再规范化、比对；不符时报"第几个事件、哪个字段、实际值、金样例值"。`UPDATE_GOLDEN=1` 时改为写入金样例。
   - 退出时替 agent 应答宿主的 `session/close`，然后强制退出宿主。
6. **首批样例**（GOLD-B-fake）：
   - 三个剧本：普通回合（带思考和工具）、审批后继续、回答出现后中断。
   - 录制在 `testdata/recordings/cupper-fake-agent/0.0.0/fake/`，每份 6–7 KB；金样例在 `testdata/golden/gold-b/` 下的同一路径，每份 6.5–8 KB。
   - 一次性生成程序在 `docs/tasks/T065-gold-b-runner/generate.rs`，是物料，不编译；用法写在 `notes.md`，以后由 T064 的 `--fake` 接手。
7. **接入**：
   - 根目录 `package.json` 加 `test:golden`：`cargo test -p cupper-host --features test-harness --locked gold_`，A、B 两层一起跑。
   - `xtask` 的 `rust` 作业在 `pnpm-install` 之后加 `golden` 步骤，运行 `pnpm run test:golden` 并逐个记录用例。
   - 单元测试 `gold_b_fake_runs_in_the_rust_job` 核对三件事：步骤的命令、`package.json` 里的脚本、`run` 里确实调用了这一步。

## 关键取舍

- **每写一帧都同步一次。** 起初只在推进时钟或扮演用户之前同步，结果事件先后不稳定。原因是 `session/new` 的应答和紧随其后的 `available_commands_update`，在会话 actor 里走两个通道。改成逐帧处理之后，同一 `t` 的帧也一帧处理完再发下一帧，金样例才稳定。
- **用户命令从录制推出，不另写一份命令文件。** 只有四种：
  - 还没有会话时发 `session/create`，消息取下一个 `session/prompt` 的文本；
  - 已有会话时发 `session/send`；
  - 对审批的应答，用录制里的 `optionId` 发 `interaction/respond`；
  - `session/cancel` 对应 `session/interrupt`。
- **联合类型怎样选分支。** 标签常量可以写在分支本身（`$ref` 旁边），也可以写在 `$ref` 的目标定义里。最初只看目标，结果选错了分支，工具调用的 `cwd` 没被换掉，本机临时目录进了金样例。已改正并补了用例；`tools/sanitize scan testdata` 结果是 clean。
- **录制在系统时钟上生成，回放时用手动时钟。** 这样录制里的 `t` 是真实的间隔，回放时宿主的定时器按录制的时间触发。

## 规模

只算 `crates/` 的净增，相对开工时的 main。

| 部分 | 增 | 删 |
|---|---|---|
| `replay.rs`（公开比对规则） | 7 | 5 |
| 进程内入口（`launch`、`runtime/mod`、`test_config`、`supervisor`、`Cargo.toml`） | 101 | 12 |
| `runtime/tests.rs`（夹具拆出 `with`，加进程内实例的用例） | 107 | 10 |
| `acp.rs`（SDK 顺序的用例） | 43 | 0 |
| `tests/gold_b/normalize.rs`（实现约 238，测试 91） | 329 | 0 |
| `tests/gold_b.rs`（运行器约 480，测试约 77） | 557 | 0 |
| `xtask/src/ci/jobs/rust.rs` | 36 | 2 |
| **合计** | **1,180** | **29** |

净增 1,151 行，加上 `package.json` 的 1 行共 1,152，略超上限约 1,150，在硬停止线 1,200 之内。

过程中越过了原定的硬停止线 950：规范化做完后我没有统计，到运行器跑稳、汇总时才发现。超出的部分包括裁定新增的两条用例（约 140 行），规范化多出约 90 行，运行器多出约 170 行。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| GOLD-B-fake 首批样例全部通过；改动一个期望事件时用例失败，并指出序号和字段 | `gold_b` 测试目标连跑 6 次，9/9 通过。`gold_b_fake_a_changed_golden_names_the_event_and_field` 把一个事件的 `turn_no` 改掉，报错为 ``event 3: `data.turn_no` is 1, the golden has 9`` |
| 规范化有单元测试，`x-id`、`x-time`、`x-path` 各有正反用例 | `gold_b_fake_normalize_*` 共四条 |
| `pnpm test:golden` 跑 B 层，CI 里能看到 | 本地跑：A 层加库内用例 25 个，B 层 9 个，全部通过。CI 日志里有 `golden ok`，摘要里 GOLD-B-fake 为 pass |
| 测试名以用例编号开头 | 新增测试都以 `gold_b_fake_` 开头，对应 `tests/ids.toml` 的 GOLD-B-fake |
| 干净 worktree 里 `cargo xtask ci pr --record docs/tasks/T065-gold-b-runner/ci/` 通过，摘要随任务提交 | 在提交 9c9c286 上一次通过，摘要已提交（dde54f6），license 没有失败，没有重跑。跑的期间没有写工作区 |

CI 各作业结果：

| 作业 | 结果 |
|---|---|
| rust | 2046 项通过 |
| protocol | 185 项通过 |
| ts | 471 项通过 |
| license | 1 项通过 |
| testdata-scan | 3 项通过 |
| ui | 35 项通过 |
| e2e | 13 项通过 |

另外：
- clippy 在不带特性、只带 `test-harness`、`--all-features` 三种构建下都没有问题；
- `tools/sanitize scan testdata` 结果是 clean；
- CI 摘要里没有本机路径。

## 提交列表

```
dde54f6 docs(task): T065 CI summary
9c9c286 docs(task): T065 final counts after the golden step
32d093b ci(xtask): run pnpm test:golden in the rust job
2635800 docs(task): T065 progress, counts past the hard stop, and the one-off generator
f9699b1 test(host): GOLD-B runner, normalization and the first fake-agent goldens
a8fc36d test(host): GOLD-B's sync rests on the SDK's in-order dispatch
0db3ba0 feat(host): in-process adapters for test-harness builds
f1f1647 refactor(fake-agent): make the replay's matching rules public
a18e590 docs(task): estimate T065 by module, with a split proposal
```

## 遗留问题 / 范围外发现

### 请补进设计 5.9.3 的规则

1. **信封字段**：去掉 `seq`、`from`；`v` 保留；`ts` 按 `x-time` 处理；`cause.command_id` 按 `x-id` 处理。
2. **`x-id`**：
   - 每个前缀各自按出现顺序编号，换成 `<前缀>_<n>`，例如 `s_1`、`t_1`、`d_1`；
   - 没有 1–4 个小写字母前缀的值（UUID 形式的 `command_id`、blob 哈希）记作 `id_<n>`；
   - 同一个值总是换成同一个占位；
   - 摘要的类型 `Digest` 本身就标了 `x-id`，所以摘要也走这条规则。
3. **`x-time`**：任何非空的值都换成 `"<time>"`，时间戳和时长一视同仁；`null` 保持 `null`。
4. **`x-path`**：按出现顺序换成 `<path_<n>>`，同一个值总是同一个占位。
5. **delta 折叠**：连续的同一条目、同一 `field` 的 `item.delta` 折叠进第一条，保留第一条的信封。
6. **联合类型怎样选分支**：标签常量可以写在分支本身，也可以写在 `$ref` 的目标上；没有标签时，取第一个不是 `null` 的分支。
7. **比对之前先严格解码**：类型已知而数据不合法的事件直接判失败。
8. **金样例文件**：
   - 键排序，一行一个事件；
   - 路径与录制相同，放在 `testdata/golden/gold-b/` 下；
   - 改动后报错的格式是 ``<文件>: event <n>: `<字段路径>` is <实际值>, the golden has <期望值>``。
9. **B 层回放的流程**（也涉及 5.9.2）：
   - **逐帧处理**：同一 `t` 的帧也一帧处理完再发下一帧；
   - **同步**：做法是"发一个宿主不认识的请求，等 `method not found` 回来，再推进 0 ms 等屏障静止"，它依赖 SDK 的按序分发；
   - **扮演用户**：宿主到了录制里的时间点还没发出下一帧时，测试扮演用户，命令从录制推出，只有 `create`、`send`、`respond`、`interrupt` 四种。
10. **假 agent 录制的目录约定**：`testdata/recordings/cupper-fake-agent/0.0.0/fake/`。

### 范围外发现

1. **附加时 `session.capabilities` 会连写两条，事件先后也不确定。**
   - 每次附加都会写两条：第一条的命令列表为空，第二条是 `available_commands_update` 带来的。
   - 附加的应答和紧随其后的第一条更新，在会话 actor 里走两个通道，两者写进会话流的先后取决于时机，真实运行里也是这样。
   - 这只影响事件顺序和条数，不影响结果；和 T062 遗留里"`session.capabilities` 连写两条"是同一现象。B 层靠逐帧处理避开了它。
   - 建议交给负责会话 actor 的任务决定是否合并成一条。
2. **宿主发给 agent 的 JSON-RPC id 是 UUID。** 回放靠 id 重映射照常工作；拿原始帧做比对时，id 不能按字面比较。

### 交接

- 可重复运行的生成器并进 T064 的 `--fake`，用法写在 `notes.md` 里。
- `pnpm test:golden` 会把 B 层用例再跑一遍：它们在 `cargo test --workspace --all-features` 里已经跑过，这次换成只带 `test-harness` 的构建，用时约 6 s。
