# T023 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T023-host-core-startup`
- 完成日期：2026-10-19

## 做了什么

启动契约、`test_config`、时长登记、故障点框架、宿主 ID 都已完成，并接进了 `main`。验收里有一项没做到：**读完载荷后把 stdin 指向 `/dev/null`**。原因见"关键取舍"第 1 条和"遗留问题"第 1 条。

改动都在 `crates/cupper-host`，另外改了一处 xtask：

- **退出码**（`src/exit.rs`）：枚举 `HostExit`，按 5.2.13 取值 0、1、3、4、5、6。另有 `Failpoint = 99`，只在 test-harness 构建里存在，避开了上面这些值和 Rust panic 用的 101。
- **命令行**（`src/startup/mod.rs`）：
  - 支持 `--data-dir <dir>`。路径必须是绝对路径；不传时取 `$HOME/Library/Application Support/cupper`。
  - 支持 `--build-info`：输出一行 JSON，`{"debug_assertions","features","name","version"}`，然后以 0 退出。
  - 其他参数（包括 `--test-config`，见 P-9）、参数重复、相对路径，都以 1 退出。
  - `$HOME` 不可用时以 5 退出。
  - `FEATURES` 是编译期常量，列出本构建开启的特性。
- **stdin 启动载荷**（`src/startup/payload.rs`）：
  - **读取方式**：开了 `dev-auth` 或 `test-harness` 的构建，由辅助线程读一行（以换行或 EOF 结束，最长 1 MiB）。等待用的是 `Clock` 上的定时器，`TIMEOUT` 为 2 s。超时、没有内容、超长、格式错误时，都返回 `PayloadError`，退出码 1。
  - **编译期门控**：`dev_key` 只在 `dev-auth` 下解析；`harness_key` 和 `test_config` 只在 `test-harness` 下解析。在不读的构建里，这些字段用 `IgnoredAny` 跳过，内容不解析。顶层不认识的键报错；载荷必须是 JSON 对象。
  - **发布构建**：编译进去的是另一版 `load`，完全不碰 stdin。
  - **密钥**：类型是 `SecretKey`，32 字节，线上格式是 64 位十六进制。它的 `Debug` 输出和解析报错都不带密钥内容。
- **`test_config`**（`src/startup/test_config.rs`，只在 test-harness 下存在）：
  - 包含 5.9.9 的全部注入点：`clock`（system 或 manual）、`durations`、`failpoints`、`boot_session_uuid`、`agents.{claude,codex}`（`program` 必须是绝对路径，另有 `args`）、`runtime.registry_url`、`login_env`、`notifier`（system 或 record）、`rss_samples.{claude,codex}`。
  - 每一层都拒绝不认识的键。缺省的字段保持生产行为。
- **时长登记**（`src/durations.rs`）：
  - 一个时长由 `DurationKey` 表示（名字加设计值），全部登记在 `ALL` 里；定时器通过 `Durations::get(key)` 取值。
  - test-harness 构建从 `{"<name>_ms": n}` 读覆盖值，名字没登记就报错。发布构建里没有覆盖机制。
- **故障点**（`src/failpoint.rs`）：
  - 用法是 `failpoint!(NAME)`。test-harness 构建里，被测试打开的故障点第 n 次执行到时（默认第 1 次），用 `process::exit(99)` 退出；没打开的故障点没有任何效果。
  - 故障点的名字统一登记在 `ALL` 里，命名是 `<模块>.<位置>`。`test_config.failpoints` 里出现未登记的名字、同一个故障点出现两次、`hit` 为 0，都会报错。
  - 不带 test-harness 时，宏展开为空，模块里也不存在其他内容。
- **宿主 ID**：`startup::new_host_uuid()` 用 getrandom 取 16 字节，再用 `uuid::Builder::from_random_bytes` 生成 UUIDv4，不需要打开 uuid 的 `v4` 特性。
- **`main`**：按顺序解析参数、处理 `--build-info`、建一个 current-thread runtime（开 time）、读载荷、在 test-harness 构建里打开故障点。之后的启动步骤还没组装，所以打印"not assembled yet"并以 1 退出。
- **CI**（`crates/xtask/src/ci/jobs/rust.rs`，在 `crates/cupper-host` 之外）：
  - `rust` 作业原来只跑 `--all-features`。现在对 cupper-host 另外三种特性组合（无特性、只开 `dev-auth`、只开 `test-harness`）各跑一次 clippy 和 test。
  - 依据：计划 5.7 写明 rust 作业由"T023（无特性运行）"扩充。只开 `dev-auth` 就是自用构建的配置，只开 `test-harness` 就是集成测试的配置，两种都必须测。

## 关键取舍

1. **stdin 重定向到 `/dev/null` 没有做。**
   - 原因：标准库没有安全的 `dup2`，`unsafe` 又只能出现在 `cupper-sys`。main 上已经合并的 `cupper-sys`（T020）也没有这个函数，而且我的分支从 ee1496c 拉出、不含它，按规定不能 merge 或 rebase。
   - 我没有改用 rustix 这类库。那样会让系统调用绕开 `cupper-sys`，等于做了一个架构决定，需要你来定。
   - 接法见"遗留问题"第 1 条。
2. **超时用"读线程 + `Clock` 定时器 + oneshot"实现**，不用 `spawn_blocking`。tokio 的 runtime 在 drop 时会等阻塞任务结束，读线程卡在 stdin 上时退出流程就会挂住。超时之后读线程留在原地，进程随即以 1 退出。
3. **严格解析**：所有对象都用 `deny_unknown_fields`。serde 会把 JSON 数组按位置当成结构体接受，所以顶层另外检查必须是对象。嵌套的对象仍然接受数组形式（例如 `"agents": []` 会得到默认值），我认为这个风险可以接受。
4. **两份登记表起始为空（只有测试用的条目）**：
   - 时长：设计里涉及约 30 个时长，有退避序列、清理周期这类有语义细节的，命名和拆分交给各定时器的负责任务更稳妥。计划 1.4 也写明，新增定时器的任务在同一任务里登记。
   - 故障点：由 T024、T025、T043、T069、T121、T124 在加调用点时登记。
5. **故障点自己写，不用 `fail` crate**：
   - `fail` 没有"退出进程"这个动作，需要的话只能用回调；
   - 它从环境变量 `FAILPOINTS` 读配置，而我们的配置只从 stdin 载荷来；
   - 它的名字是自由字符串，要做登记校验还得再包一层。
   - 自己写的核心代码约 80 行。退出用 `process::exit`，不用 `abort`：`exit` 同样不运行析构、不刷新宿主的缓冲，效果接近崩溃，而 `abort` 会让 macOS 的 ReportCrash 生成崩溃报告。
6. **`harness_key` 的格式定为与 `dev_key` 相同（64 位十六进制）。** 设计里这里只写了 `"..."`。按 brief，三个字段都是可选的。
7. **宿主 ID 只返回 `Uuid`。** 加前缀交给 main 上已有的 `cupper_protocol::HostId::from_uuid`，避免重复定义类型。

## 验证方式

- 在干净的 worktree 里，提交 5c46b43 上跑 `cargo xtask ci pr --record docs/tasks/T023-host-core-startup/ci/`，通过：rust 163 项、ts 26 项、license 1 项。摘要是 `pr-5c46b4300072.json`，已提交，内容里没有路径和用户名。UNIT-startup-payload、UNIT-failpoint、UNIT-release-features 三个编号都是 pass。
- 对照验收标准：
  - [x] **UNIT-startup-payload**：
    - 格式正确：用真实管道传入，换行即结束，不等 EOF；EOF 结尾的行也接受。
    - 格式错误时退出码为 1：解析层 13 到 17 种坏输入，外加空输入和超长输入；进程层用真实二进制验证了三种坏载荷。
    - 2 秒超时：用 `ManualClock`，runtime 不开 time driver。1,999 ms 时仍在等待，2,000 ms 时报 Timeout；截止前 1 ms 到达的行照常接受。
    - 不同特性组合下忽略对应的字段：只开 `dev-auth` 时，坏的 `harness_key` 和 `test_config` 被忽略；只开 `test-harness` 时，坏的 `dev_key` 被忽略。另外验证了 `--test-config` 开关不存在，以及报错和 `Debug` 输出里不出现密钥。
  - [ ] **读完后 stdin 指向 `/dev/null`：没做到**，见上文。
  - [x] **UNIT-failpoint**：
    - 子进程用例：打开 `test.probe`（`hit: 2`）后，执行三次未打开的故障点、执行一次 probe，进程都继续运行；第二次执行 probe 时以 99 退出，stderr 里注明是哪个故障点。另外验证了重复打开会报错。
    - 父进程里执行未打开的故障点后，登记表连初始化都没有发生。
    - 登记表的名字唯一、格式正确；解析时拒绝未知名字、重复、`hit` 为 0。
  - [x] **UNIT-release-features**（无特性构建）：
    - 真实二进制的 `--build-info` 输出里没有 `dev-auth`、`test-harness`，features 为空。
    - `load` 没有读输入（用记录读取的 reader 验证），也没有建定时器，返回的 `Payload {}` 没有任何字段。
    - 给真实二进制传一个所有读载荷的构建都会拒绝的载荷，它照样不读 stdin。
    - `failpoint!` 宏展开为空，未登记的名字也能编译通过。
  - [x] CI 摘要已随任务提交。
  - [x] 带编号的测试名以编号的 snake_case 开头。
- 其他检查：
  - 四种特性组合下 clippy（`-D warnings`）和 rustdoc（`-D warnings`）都通过。
  - 全特性连跑 25 次、无特性连跑 15 次，全部通过。
  - 变异检查都被测出来了：把故障点的计数条件改坏，`unit_failpoint_*` 失败；把截止时间多加 1 ms，超时用例失败。
- **规模**：`crates/` 下新增约 1,565 行，其中实现加文档约 800 行，测试约 760 行，略超约 1,500 行的上限。不含 Cargo.lock 和 CI 摘要。

## 提交列表

```
c3d2b29 docs(task): record the T023 pr CI summary
5c46b43 test(host): say what the payload test checks with the trailing line
ff3da09 feat(host): mint the random UUID behind the host ID
d10489c feat(host): read the startup payload and test_config from stdin
6a324e3 docs(host): keep the failpoint exit code out of rustdoc links
98967fc feat(host): add the failpoint framework
997ae0a feat(host): add the registry of timer durations
eaad83e feat(host): parse the command line and print --build-info
f7c4dc0 ci(xtask): run the host's clippy and tests per feature set
a3d376c feat(host): define the host exit codes
```

## 遗留问题 / 范围外发现

1. **stdio 重定向需要你决定由谁做。** 建议在 `cupper-sys` 加 `redirect_stdio_to_null()`：打开 `/dev/null`，用 `dup2` 指到 0、1、2。5.8.1 和 5.2.13 要求三个都指过去，不只是 stdin。`main` 里加一行调用：读载荷的构建在读完之后调，发布构建在一开始就调。测试可以放在单独的集成测试二进制里，进程内调用后比对 `/dev/fd/0` 与 `/dev/null` 的 rdev。量很小，可以作为后续小任务，或并入 T037。
2. **主仓库根配置和 crate 外的改动**：
   - 改了 `crates/xtask/src/ci/jobs/rust.rs`（特性组合矩阵）。
   - `Cargo.lock` 新增了 thiserror 和 thiserror-impl，会和 main 冲突，重新生成即可。
   - 根 `Cargo.toml` 没有改。serde（1.0.229，开 derive）和 thiserror（2.0.21）写在 cupper-host 自己的 `Cargo.toml` 里，版本与 main 的工作区依赖一致，合并时可以改成 `.workspace = true`。tokio 改为常规依赖，开 `rt`、`sync`、`time`，dev-dependencies 里的 tokio 已删除。
3. **交接事项**：
   - **T037**：`main` 目前只为等载荷建了一个 current-thread runtime，可按 CFRunLoop 的方案重组。数据目录的 0700 和属主检查、把 `test_config.clock` 和 `Durations` 分发给各模块，都归 T037。
   - **T056**：`--self-test` 还没加，目前会被当作未知参数，以 1 退出。
   - **T025**：宿主 ID 用 `HostId::from_uuid(new_host_uuid().as_u128())`。
   - **T030**：密钥通过 `payload.dev_key` 和 `payload.harness_key` 的 `SecretKey::expose()` 取得。
   - **T074 和 T045**：载荷是一行 JSON 加换行，写完立即关闭 stdin；两个密钥都是 64 位十六进制；出现未知的键时宿主以 1 退出。CLI 的 `--harness-key-fd` 应使用同样的密钥格式。
   - **T161**：`--build-info` 的格式见上文。
   - **新增定时器和故障点的任务**：分别登记到 `durations.rs`、`failpoint.rs` 的登记表里；故障点的用例断言退出码 99。
4. **故障点退出与 `kill -9` 的差别**：`process::exit` 仍会运行 libc 的 atexit 处理函数。宿主自己不注册 atexit 处理函数，影响可以忽略。
5. 我的分支从 ee1496c 拉出，之后 main 已经合并了 T010 和 T020，合并时预计只有 `Cargo.lock` 冲突。
6. 本任务没有写 `notes.md`，思路和取舍都在本汇报里。
