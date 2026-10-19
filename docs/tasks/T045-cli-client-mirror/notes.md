# T045 工作笔记

## 范围（拆分后）

编排者在 `mirror.rs` 之前采用了下文的方案 B（计划 v1.3.6）：T045 收窄为构建种类与 `--build-info`、按构建种类校验宿主、连接与握手、请求关联、`command_id`，验收编号是 UNIT-cli-host-verify 和新登记的 UNIT-cli-client。Mirror 和 UNIT-mirror 另立为 T049，按本文末尾"Mirror 的设计"做。本文的估计和推算写于拆分之前，保留原样。

## 行数估计（开工前）

### 估算方法

SP-4a 的 `client.rs`（199 行）和 `mirror.rs`（100 行）是起点。本任务在它们之上加：读套接字改走 `recv_closing_fds`、固定令牌、`host/hello` 与版本协商、按构建种类校验宿主、完整的客户端同步状态机（T014 的裁定）。代码按 SP-4a 的密度加 rustfmt 展开估；测试按 T014 的经验（测试比估计多约 70%）单独估，并尽量共用 JSON 构造函数。

### 按模块估计

| 模块 | 内容 | 代码 | 测试 |
|---|---|---|---|
| `Cargo.toml`、`lib.rs` | 包、特性（`dev-auth`、`self-use`、`test-harness`）、`mod` 声明 | 45 | — |
| `build.rs` | 构建种类（编译期常量，测试可注入）、编进来的宿主 cdhash、`--build-info`、默认数据目录与套接字发现 | 110 | 40 |
| `verify.rs` | `HostCheck` 的四个分支（签名要求、cdhash、同目录宿主文件的 cdhash、`harness` 留给 T039）、同 uid、`weak_auth` 规则、固定令牌比对 | 140 | 130（UNIT-cli-host-verify） |
| `client.rs` | 连接与握手（`host/hello`、版本协商、`initialize`、`auth: peer`）、读循环（`recv_closing_fds`、收到 fd 断开、每次读取比对令牌）、请求关联、通知分流、订阅结果与通知保持线上顺序、`command_id` | 260 | 170（UNIT-cli-client，进程内的假宿主） |
| `mirror.rs` | 客户端同步状态机：订阅带游标、快照分页组装与核对、连续性、`ViewError`、`stream/reset`、epoch 改变 | 190 | 260（UNIT-mirror） |
| `main.rs` | bin `cupper`：只有 `--build-info` | 20 | — |
| **合计** | | **约 765** | **约 600** |

**总计约 1,365 行（±10%：1,230–1,500）**，超过计划的 M（400–900 行），在约 1,500 行的上限以内。

**停止线**：每完成一个模块统计一次（`git diff --shortstat main..HEAD -- crates/cupper-cli`）。按已完成模块的实际偏差推算，总数预计超过约 1,500 行时，停下来汇报。到时可以先拿掉的部分：UNIT-cli-client 只留握手和请求关联两例，其余交给 T046 的 IT-cli-commands。

## 实际行数

每完成一个模块统计一次（`git show --shortstat <提交> -- crates/cupper-cli`，不含 `Cargo.lock`）：

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
| `Cargo.toml`、`lib.rs`、`build_kind.rs`、`main.rs`（`86dc1a4`） | 245 | 56 | 约 215 | 245 |
| `verify.rs`（`4a5c179`） | 282 | 142 | 约 270 | 527 |
| `client.rs`（`3254b22`） | 635 / 1 | 224 | 约 430 | 1,161 / 删 1 |
| 拆分后：去掉指向 Mirror 的文档链接（`9778f16`） | 8 / 8 | — | — | 1,161 / 删 1 |
| 拆分后：收到 fd、另一个进程接手宿主那端时断开（`0a4a93b`） | 45 | 45 | — | **1,206 / 删 1** |

- 三个模块合计 1,161 行：代码 739 行、测试 422 行，比这三个模块的估计（约 915 行）多 27%。代码多 28%，测试多 24%。
- 超出最多的是 `client.rs`（多 47%）：rustfmt 把错误枚举、`Options`、读循环展开；测试里的假宿主（`FakeHost`）约 70 行。
- 剩下的 `mirror.rs` 估计 450 行（代码 190、测试 260）。按已完成模块的偏差（+25%）推算约 560 行，**总数约 1,720 行（区间 1,600–1,750）**，超过约 1,500 行的停止线。按守则第 8 条，在写 `mirror.rs` 之前停下来汇报。

**最终（收窄后的 T045）：新增 1,206 行，其中代码 741 行、测试 465 行**（`build_kind.rs` 128 + 56，`verify.rs` 139 + 142，`client.rs` 407 + 267，`Cargo.toml`、`lib.rs`、`main.rs` 67）。比这几个模块开工前的估计（约 915 行）多 32%，在约 1,500 行的上限以内。

### 拆分前提给编排者的两个方案（已选 B）

- **方案 A：在 T045 内做完。** `mirror.rs` 和 UNIT-mirror 约 450–560 行，总数约 1,600–1,750 行，超上限 7%–17%，超出主要在测试。好处是不增加任务，T033、T039、T046 的依赖不变。
- **方案 B：拆分。** T045 收窄为"构建种类与 `--build-info`、按构建种类校验宿主、连接与握手、请求关联、`command_id`"，已经完成（1,161 行，还要跑 `cargo xtask ci pr`）。Mirror（客户端同步状态机）和 UNIT-mirror 拆成新任务（S，约 450–560 行）。T039 只依赖 T045 的连接部分；T046 和要用 Mirror 的宿主集成测试改为依赖新任务。
- 两种方案下 Mirror 的设计相同，见下文"Mirror 的设计"。

## 实现时的取舍

**构建种类**（`build_kind.rs`；特性与优先级、`CUPPER_HOST_CDHASH`、发布构建签名要求为空、e2e 分支的处理已由编排者认可，写进设计 5.2.14 末尾"T045 实现时的裁定"）

- CLI 的特性：`dev-auth`（开发构建）、`self-use`（自用构建）、`test-harness`（e2e 构建）；都不开是发布构建。同时开时 `test-harness` 优先于 `self-use`，`self-use` 优先于 `dev-auth`。`dev-auth`、`test-harness` 与宿主同名，`--build-info` 的 `features` 也就和宿主一样，发布流水线可以用同一条断言。
- 自用构建的宿主 cdhash 来自编译期环境变量 `CUPPER_HOST_CDHASH`（40 位小写十六进制，与 `Cdhash` 的打印形式一致）。只有 `self-use` 开、`test-harness` 不开时才读它，缺失或格式不对都编译失败，不会悄悄退成开发构建的弱校验。T005 的构建命令是 `CUPPER_HOST_CDHASH=<宿主的 cdhash> cargo build -p cupper-cli --release --features self-use`。CI 的 `--all-features` 由 `test-harness` 优先，不需要这个变量。
- `BuildKind::SelfUse { host_cdhash }` 带着 cdhash，测试直接构造任意值，这就是"测试里可以注入"。
- 默认数据目录：发布 `cupper`、自用 `cupper-local`、开发 `cupper-dev`，都在 `~/Library/Application Support/` 下（5.7.9）；e2e 没有默认值，每个用例自己传。套接字路径用 `cupper_protocol::transport::socket_path` 和宿主同一个算法。
- `--build-info` 输出一行 JSON：`name`、`version`、`features`、`debug_assertions`（同宿主），加上 `build`（`release`、`self_use`、`dev`、`e2e`）、`host_check`（`signature`、`cdhash`、`host_file_cdhash`、`harness_proof`），自用构建另带 `host_cdhash`，供 T005、T160 核对"CLI 内嵌的 cdhash 等于宿主的 cdhash"。
- bin 现在只认 `--build-info`，其余参数打印用法、退出码 2。子命令归 T046。

**校验宿主**（`verify.rs`）

- 四个分支都先要求对端的 euid 等于本用户，再按构建种类：
  - 发布：签名要求加动态状态字（`AuditToken::verify`）。签名要求要带 Team ID，由 T180 设置；在那之前 `HOST_REQUIREMENT` 是 `None`，发布版 CLI 不接受任何宿主。做法同 T030 的 `ui_requirement`。
  - 自用：对端运行中的 cdhash（`AuditToken::cdhash`）等于编进来的 cdhash。
  - 开发：对端运行中的 cdhash 等于同目录宿主文件的 cdhash。CLI 的路径先 `canonicalize`（"安装命令行工具"建的是符号链接），再在同目录依次找 `CupperHost.app`（App 的 `Helpers/`）和 `cupper-host`（cargo 的 target 目录）。找不到就拒绝连接。
  - e2e：`harness` 证明归 T039；在那之前只查同用户和固定令牌，输出同样标"宿主未经签名校验"（`HostCheck::is_weak`）。
- 校验在连接后、发送任何数据之前做，通过后固定令牌；读循环每次读到数据都比对，不一致或对端已无法识别都断开。
- `weak_auth`：只有发布构建（签名要求分支）拒绝，其他构建的宿主本来就是退化认证。

**连接**（`client.rs`）

- 握手：先校验宿主，再读第一条消息，必须是 `host/hello`；本地先用 `PROTOCOL.negotiate` 判断有没有共同版本，没有就不发 `initialize`；结果里的 `protocol` 必须等于协商出的版本；最后按构建种类检查 `weak_auth`。
- 读循环：`readable()` 加 `try_io(recv_closing_fds)`；带 fd 的读取、EOF、令牌变化、分帧错误、不是 JSON-RPC 的消息、宿主发来请求，都结束读循环并记下原因（`CloseReason`），等待中的请求随之失败。
- **`stream/subscribe` 的结果不走 oneshot**，而是作为 `Incoming::Subscribed` 按线上顺序进通知流。否则响应和通知分两条通道，客户端分不清一条 `stream/events` 是旧订阅还在路上的，还是新订阅的。其他请求照常用 oneshot。
- `command_id()`：`Uuid::now_v7()`，用 `#[expect(clippy::disallowed_methods, reason = ...)]` 豁免，理由是"CLI 是独立进程，按真实时间给命令编号"。
- tokio 开了没有用到的 `time` 特性：没有它，单独对本 crate 跑 clippy 时，`clippy.toml` 里 tokio 定时器的禁用条目解析不到，`-D warnings` 下报错。
- SP-4a 里没有迁移的：`pause_reading`、`max_frame_seen`（慢客户端和帧上限的集成测试用）、blob 分片的转交（CLI 不调 `blob/get`）、`next_timeout`（时间规则）。由需要它们的任务加。

**拒绝路径的用例**（拆分后补齐）

- 收到 `SCM_RIGHTS` 传来的 fd 时断开：用 `cupper-sys` 的 `send_with_fds`，只在 dev-dependencies 里打开 `test-util`，产品构建不带它。
- 校验之后另一个进程接手宿主那一端：照 T030 的 IT-pinned-token 的做法，让 `/bin/cat` 往宿主那端写一帧。`cat` 在客户端读到之前不退出，令牌指向它，客户端以 `TokenChanged` 断开。测试里用 `std::process::Command` 拉 `cat`，同宿主的测试；`cupper_sys::spawn` 只支持 null 和管道，交不了套接字。

## 给 T039 的接入点

- `client::ClientAuth`：加 `#[cfg(feature = "test-harness")] Harness(key)`，密钥从 `--harness-key-fd` 读，格式同启动载荷（64 位十六进制，T023）。
- `ClientAuth::credentials(&hello, &host)`：已经拿到 `host/hello` 的 `nonce` 和固定下来的宿主令牌（证明里的 `pid_h` 取 `host.pid()`），T039 在这里算 `c_nonce` 和 `proof`。
- `Client::handshake`：在 `check.accept(&init.host)` 旁边校验 `init.host_proof`，不符就断开。
- `verify::HostCheck::Harness`：证明落地后，`is_weak` 对它改为 false。

## Mirror 的设计（留给 T049）

`Mirror<V: View>` 是纯核心：不做 I/O，不 await。构造只有 `Mirror::index()` 和 `Mirror::session(id)`，视图类型和流不会配错。

- **状态**：`Waiting`（还没订阅，或在等 `stream/subscribe` 的结果）、`Loading { at_seq, epoch, next_page, view }`、`Live`。另存最近一份完整的视图和游标（`epoch` 加 `StreamCursor`）。
- **`subscribe()`**：进入 `Waiting`，返回带游标的 `StreamSubscribeParams`（没有游标就都不带）。调用方用 `Client::subscribe` 发出。
- **`handle(&Incoming)`**：只看本流的消息，返回 `Ok(Some(params))` 表示要用 `params` 重新订阅；订阅本身出错（`RpcError`）交还调用方。
  - `Subscribed`：`replay`，并且 `epoch` 和 `at_seq` 与游标一致，就进入 `Live`；`replay` 却对不上游标，整体重置并不带游标重新订阅；其他 mode（`snapshot`，以及不认识的 mode），不论原因都丢弃视图和游标，进入 `Loading`。
  - `Snapshot`：只在 `Loading` 时接收，其余状态下是旧订阅的，忽略。`at_seq`、`epoch`、页序号三项都核对，不符就整体重置、不带游标重新订阅。逐个宽容解码后 `apply`；`ViewError` 同样处理。最后一页到达后换上新视图，游标设为 `at_seq`，进入 `Live`。
  - `Events`：`Waiting` 时忽略（旧订阅的）。`Loading` 时收到属于协议错误，整体重置。`Live` 时逐个经 `StreamCursor::accept` 检查连续性：有缺口就带游标重新订阅；`apply` 报 `ViewError`，或者信封解码失败（拿不到 seq），就不带游标重新订阅。
  - `Reset`：`Waiting` 时忽略；否则保留游标和视图，带游标重新订阅。
- **epoch 改变**：宿主对不同 epoch 的游标回 `snapshot`（`epoch_changed`），由上面的规则整体重置；之后 seq 可以重用，不报重复。
- **UNIT-mirror**：快照分页组装（按页组装的视图等于直接应用全部 ops）；页序号、`at_seq`、`epoch` 不符时重置；快照不论原因（含未知 mode）丢弃旧状态；连续性缺口带游标重新订阅（含带 `from` 的区间事件）；`ViewError` 不带游标重新订阅；`stream/reset` 保留游标；epoch 改变时整体重置、seq 可重用；`Waiting` 时旧订阅的数据被忽略。
