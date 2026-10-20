# T065 工作笔记

- 执行者：子 agent（Rust 工程师，宿主核心）

## 开工前读到的现状

- **A 层**（T053）：`driver/claude/gold_a.rs`，是 `cupper-host` 库里的单元测试（`gold_a_claude_*`），跟着 `cargo test` 跑；`UPDATE_GOLDEN=1` 时写金样例，`xtask ci` 遇到 `UPDATE_GOLDEN` 就拒绝运行。根目录 `package.json` 里还**没有** `test:golden` 脚本，`xtask` 的 `rust` 作业也没有单独的金样例步骤（模块文档写着"A later task adds the golden tests"）。
- **录制格式**（T052）：`cupper_protocol::recording`，有 `Recording::parse`、`segments()`。宿主的 `diag/recording_export`（T047）把会话的原始帧导成录制，头行 `source: host`、`usage: [gold-b, replay]`，`scenario` 先填会话 ID；导出函数 `recording(host, raw, session)` 是公开的。
- **假 agent 的回放**（T067）：`cupper_fake_agent::replay::play` 按真实时间（压到 1/100）逐帧回放，比对规则（id 重映射、`cwd`/`path` 占位、忽略 `_meta`）在私有的 `Mapping` 里。
- **内存传输**：`driver::transport::MemoryTransport` 已有，但运行时只会经 `cupper_sys::spawn` 起进程（`launch::start`），没有接入内存传输的入口。
- **进程内宿主**：`Host::start` 可以在测试进程里调用（`it_clock_advance_the_host_registers_every_kind_of_actor` 就这样做）；`test/clock_advance` 会逐个截止时间推进 `ManualClock`，并在每一步后等待静止屏障。
- **同步的难点**：静止屏障不覆盖 SDK 内部的读写任务，按设计 5.2.16 "等外部进程的输出算空闲"。B 层在进程内扮演 agent，写入一行之后要先确认宿主已经分发了它，才能推进时钟或发出用户命令。办法是往返一次：发一个宿主不认识的请求，SDK 会按顺序回 `method not found`（K27）。收到这个回复时，之前的消息都已经交到宿主的邮箱里；再推进 0 ms 等屏障静止。
- **规范化的依据**：`cupper_protocol::schema::document()` 在内存里给出整份 schema（不做 I/O）。`x-id`、`x-time`、`x-path` 标在 `$defs` 里的新类型（各种 ID、`Digest`、`TimestampMs`、`DurationMs`、`WirePath`）上，个别字段自带标记；从 `EventBody` 起按 `$ref`、`anyOf`（按 `type` 常量选分支）、`properties`、`items`、`additionalProperties` 走一遍即可。

## 方案

1. **进程内 agent 的入口**（产品代码，只在 `test-harness` 构建里有）：
   - `test_config` 增加一个不参与反序列化的 `in_process` 字段，类型是一条通道；
   - `Finder` 据此给出 `Program::InProcess`；
   - `launch::start` 遇到 `InProcess` 时不起进程，而是建一对 `MemoryTransport`，把 agent 一端连同"退出码"的发送端交给测试；
   - 退出时照常记 `instance_exited` 的 note，并调用 `exited`；
   - 进程组用一个不存在的组号，保证任何信号都发不到别的进程。
2. **B 层运行器**（`crates/cupper-host/tests/gold_b.rs`）：
   - 在临时目录里用 `Host::start` 起进程内宿主，用手动时钟；
   - 测试本身扮演 agent，复用 T067 的 `Mapping` 做比对（为此把它公开）；
   - 按录制的 `t` 经 `test/clock_advance` 推进时钟，每次推进前用上面的往返加屏障静止来同步；
   - 宿主到了录制里的时间点还没发出下一帧时，运行器扮演用户，按这一帧推出要发的命令：
     - `initialize` 之前还没有会话：发 `session/create`，消息取录制里第一个 `session/prompt` 的文本；
     - 下一帧是 `session/prompt`：发 `session/send`；
     - 下一帧是对审批的应答：发 `interaction/respond`，选项取录制里的 `optionId`；
     - 下一帧是 `session/cancel`：发 `session/interrupt`；
     - 推不出命令：失败，并给出帧号。
   - 最后读出会话流的事件，严格解码（类型已知但数据残缺即失败），规范化后与 `testdata/golden/gold-b/` 下同路径的金样例逐行比对。不符时报"第 n 个事件、`字段路径`、实际值、金样例值"，风格照 T067；`UPDATE_GOLDEN=1` 时改为写入。
3. **规范化**（同一测试目标里的模块）：
   - 同一条目、同一 `field` 的连续 delta 折叠成一条；
   - 去掉 `seq`、`from`；
   - `x-id` 的值按前缀和出现顺序换成占位（`s_1`、`t_1`、`d_1`……）；
   - `x-time` 的值换成固定的占位，只保留"有没有"；
   - `x-path` 的值按出现顺序换成占位；
   - 键排序，一行一个事件。
4. **首批样例**：
   - 先用进程级宿主和真的假 agent（系统时钟），跑普通回合、审批、取消三个剧本，经 `diag/recording_export` 导出；
   - 头行改写为剧本名，`usage: [gold-b]`；路径换成占位后放进 `testdata/recordings/claude-agent-acp/0.91.0/fake/`；
   - 再用 `UPDATE_GOLDEN=1` 生成金样例并评审。
5. **`pnpm test:golden`**：
   - 根目录加脚本 `cargo test -p cupper-host --features test-harness --locked gold_`，A、B 两层一起跑；
   - `xtask` 的 `rust` 作业加一个 `golden` 步骤调用它。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，按净增计，只算 `crates/`。数据文件不计入行数。

### 实现

| 模块 | 内容 | 估计 |
|---|---|---|
| 进程内 agent 的入口 | `test_config.in_process`、`Finder` 分支、`Program::InProcess`、`launch::start` 的内存分支（传输对、交出 agent 端、退出码、`instance_exited`、`adapters` 行、无害的组号） | 85 |
| 假 agent 的比对规则公开 | `Mapping`、`check`、`outgoing`、`Diff` 改为 `pub`，补文档 | 10 |
| 规范化 | 沿 schema 走事件（`$ref`、`anyOf`、`properties`、`items`、`additionalProperties`）、三类占位、delta 折叠、键排序输出、找第一个不同的字段路径 | 150 |
| B 层运行器 | 起进程内宿主与客户端；逐帧回放（等宿主的帧、按 `t` 推进、写出 agent 的帧、段末退出）；往返加静止的同步；从录制推出用户命令并发出；读事件、严格解码、比对与写金样例；用例登记 | 330 |
| 首批录制的生成器 | 进程级宿主加假 agent，三个剧本的命令，导出、改头行、换路径、写入 `testdata/`；`#[ignore]`，手动跑 | 80 |
| `pnpm test:golden` 与 `xtask` | `package.json` 脚本；`rust` 作业的 `golden` 步骤 | 15 |
| **实现小计** | | **670** |

### 测试

| 用例 | 内容 | 估计 |
|---|---|---|
| 规范化的单元测试 | `x-id`（同值同占位、不同前缀分开计数、非 `x-id` 字段不动）、`x-time`（时间与时长都换掉、非时间的整数不动）、`x-path`（路径换掉、普通字符串不动）、delta 折叠、键排序 | 90 |
| GOLD-B-fake 三个剧本 | 宏登记 | 10 |
| 改动期望时报错 | 把金样例里的一个事件改掉，断言失败信息带事件序号和字段路径 | 25 |
| 录制与金样例一一对应 | 每份 `gold-b` 录制都有用例和金样例，反之亦然 | 20 |
| **测试小计** | | **145** |

### 数据（不计行数）

三份录制，每份约 20–40 行、几 KB；三份金样例，每份约 15–30 行、几 KB。

### 合计

约 815 行；按以往偏差乘 1.1，约 **900 行**，超过约 850 的停止线（硬停止线 950）。按任务书，开工前停下汇报，未写代码。

## 拆分建议

| 方案 | 内容 | 估计 |
|---|---|---|
| **A. 拆出生成器** | T065 不提交生成器：首批录制用一次性程序产出，程序作为物料放在本目录（不编译、不进 `crates/`），步骤写进 notes；以后要重新录时另开小任务，把生成器做成可重复运行的工具（或并进 T064 的兼容套件，录制导出那条路它已经在走） | T065 约 735，乘 1.1 约 **810**；生成器 S，约 100 |
| **B. 拆出命令推导** | T065 的运行器按每份录制旁边的一份"用户命令"小文件驱动，不从录制推导；推导留给 T066，因为真实录制里的命令更杂 | 运行器少约 30 行，但多一份文件格式和读入，净省有限，不推荐 |
| **C. 不拆，放宽上限** | 上限提到约 950，硬停止线 1,000 | 约 900 |

推荐 A：B 层的接口、运行器、规范化和 CI 接入一次做完，验收不变；生成器是给日后重录用的工具，和 T064 的录制导出、兼容套件放在一起更合适。

风险：靠往返同步是本方案的前提。已读过 SDK 3.3.0 的 `incoming_protocol_actor`：它在一个循环里逐条取消息、逐个等处理器返回，所以 `method not found` 的回复一定晚于之前的通知交到邮箱；对宿主请求的应答，`AcpConnection::request` 的文档也写明 SDK 会等回调返回后才处理下一条。开工后仍会先用一个用例验证这一点；如果不成立，要改为在内存传输的入站一侧加计数，大约多 40 行。
