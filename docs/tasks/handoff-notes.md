# 交接事项汇总

编排者维护。各任务的汇报和审阅中留给后续任务的提醒，按接收任务汇总在这里，写 brief 时逐条带进去；带进去之后在行末标"已写入 brief"。

| 接收任务 | 事项 | 来源 |
|---|---|---|
| T002 | 逐编号结果目前解析的是 libtest 的输出（`crates/xtask/src/ci/ctx.rs` 的 `parse_libtest`）。改用 nextest 的 JUnit 报告时，替换这个函数 | T001 |
| T002 | ts 作业已经接上 Vitest 的 JSON reporter（`parse_vitest`）。按编号匹配时，JS 标题要以编号原文开头（`ids::owner`） | T070 |
| T006 | `lightningcss`（MPL-2.0，vite 的依赖）只在开发和测试时使用，不进 App。许可证门禁要按生产依赖闭包计算，否则会误报 | T070 |
| T015、T071 及以后的 TS 包 | 惯例：包的 `exports` 直接指向 `src/*.ts`；类型检查走 project references；Vitest 只收 `src/**/*.test.ts`；包的 `test` 脚本必须运行 Vitest，作业会在末尾补上 reporter 参数，不用 Vitest 的另起脚本名 | T070 |
| 写不可见字符测试的任务（T083 等） | 文件写入工具会把 `\uXXXX` 解码成真实字符。测试里需要双向控制符、零宽字符时，用脚本生成或用 `String.fromCodePoint`，提交前扫描确认没有混入真实字符 | T070 |
| T023 | 宿主 ID 是 `h_` 加 UUIDv4（5.4.4），不归 `IdGen` 管，`cupper-host` 的 `uuid` 也没开 `v4`。需要自己取随机源 | T022 |
| T023 及以后的 `cupper-host` 任务 | `tests/` 下用到 `ManualClock`、`CoreDriver`、`IdGen::seeded` 的集成测试，要声明 `required-features = ["test-harness"]`。不带特性的 `cargo test -p cupper-host` 会跳过它们 | T022 |
| T024 | 写线程"只在 `SystemClock` 下等 2 ms"。`Clock` 没有"是否是虚拟时钟"的查询，从启动配置（`test_config.clock`）判断，或者给 trait 加方法 | T022（已写入 brief，T024 按启动配置判断） |
| T043（第一个 actor 任务） | 适配循环写在 `pure/mod.rs` 的文档里。给 `TimerTable::next_deadline` 维持一个 `Clock` 定时器的小助手还没写，加在 `pure` 里 | T022 |
| T045 | CLI 在客户端生成 UUIDv7 的 `command_id`（5.3、5.7），而工作区禁用了 `now_v7`。需要一个带 reason 的豁免，或者用自己的生成器 | T022 |
| T047 | 屏障：actor 在 poll 自己的等待点之前先标记为忙，只有等待的 future 返回 Pending 时才算空闲（例如给 `Participant` 加一个 `idle(fut)` 包装）。统计在途的消息和已触发、尚未被消费的定时器。逐个到期时间推进用 `ManualClock::next_deadline` 加 `advance_to`。存储写线程和 `spawn_blocking` 也作为参与者登记 | T022 |
| tokio、uuid、getrandom | 目前写在 `cupper-host` 自己的 `Cargo.toml` 里。出现第二个使用方时，提到 `[workspace.dependencies]` | T022 |
| `pathForFile`、`saveFile` | ShellAPI 冻结以后，以附加方式加入（M1b 的 T142 等） | T070 |
| T074、T154 | i18n 已有 `command`、`update` 两个命名空间。措辞按 Codex 的中文译文核对 | T070 |
| T021 | `PROC_PIDTBSDINFO` 读不了其他用户的进程；`PROC_PIDT_SHORTBSDINFO` 和 `PROC_PIDUNIQIDENTIFIERINFO`（flavor 17，56 字节）对 root 进程也能读（实测 pid 1 可以）。libc 0.2.191 没有定义 `POSIX_SPAWN_SETSID` 和 `proc_bsdinfowithuniqid`，需要自己声明 | T020 |
| T037、T055 | 关闭顺序：先用 `child.group().with_descendants()` 记下所有进程组，再逐组 `signal(Term)`，用 `Clock` 限时等待并用 `is_alive()` 核对，最后对还在的组 `signal(Kill)`。持久化时存 `pgid` 和 `start.as_micros()`。适配器自己先退出时，后代已无从枚举，只能靠资源采样时记下的组 | T020 |
| T038 | `parent_chain` 能经过 root 进程走到 launchd；进程名是内核里的 16 字节短名，要完整路径需另取 | T020 |
| T054 | 直接用 `Env::from_allowed` 加 `set`。`CUPPER_*` 已在 `cupper-sys` 里拒绝，这里不用再处理 | T020 |
| 所有拉起子进程的任务 | `cupper_sys::spawn` 不搜索 `PATH`，调用方要传入解析好的绝对路径。`Child` 在 drop 时不会自动回收，必须 `wait`，否则会留下僵尸进程 | T020 |
| T011 | `Fallback{kind, summary}`（ADR-04）里的 `summary` 是界面文字，和"宿主只下发语义码"的原则冲突。定下 `summary` 的填法：例如只放语义码和参数，或者只作为最后兜底的英文短句，并写明理由 | T010 |
| T011 | `ErrorData.resolved_by` 目前放在 `details` 里，T011 定义好相关类型后，以附加方式改成具名字段 | T010 |
| T012 | `ErrorData.supported` 目前放在 `details` 里，依赖版本区间类型，T012 以附加方式改成具名字段 | T010（已写入 brief） |
| T015 | `Event<B>` 在泛型下由 schemars 自动生成 schema 名，导出时可能要加 `rename`。另外，把 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim` 纳入 `xtask ci`（设计 5.1.6；T010 只手动跑过）| T010 |
| T011–T015 | `cupper-protocol` 的惯例：`lib.rs` 只放 `mod` 声明；ID 用 `ids` 里的类型；时间用 `TimestampMs`、`DurationMs`，路径用 `WirePath`，这样 `x-id`、`x-time`、`x-path` 注解会自动带上；可合并的事件实现 `Mergeable`，并在变体上标 `x-mergeable`；协议里不用超过 2^53−1 的整数 | T010 |
| T075、T090 | e2e 构建的目录布局由 T074 定下：`CUPPER_E2E_DIR` 下的 `data/` 存放 userData，即其中的 `ui/`；`logs/` 存放日志。宿主的数据目录和 harness 密钥文件沿用这个布局。`CUPPER_E2E_DIR` 只编进 e2e 构建 | T074 |
| T076 | prefs 还没接到 ShellAPI（`init` 的快照、`set`、`onChange`）；preload 目前是空模块；菜单命令走 `shell:command` 通道（`apps/desktop/src/shared/ipc.ts`）。ShellInfo 的 `flavor` 没有自用构建的 `local`，冻结前以附加方式加上，或者映射为 release，写明选了哪种 | T074 |
| T080（第一个渲染进程任务） | 渲染进程和 preload 的 ESLint 安全规则（5.7.8：禁止 `innerHTML`、`dangerouslySetInnerHTML`、`eval` 等，以及 Trusted Types 策略只能在 `policies.ts` 里创建）还没有加，和 ui-core 一起加 | T074 |
| T090 | Chromium 会往 `~/Library/Preferences/dev.cupper.app.e2e.plist` 写系统默认值，这个文件不在 `CUPPER_E2E_DIR` 下，每轮测试后清理一次 | T074 |
| T150 或做深链接的任务 | 各构建的 URL scheme（`cupper-e2e://` 等）还没有注册，和深链接的处理函数一起加 | T074 |
| T152 | 窗口外观：菜单缩放后，交通灯的 y 坐标不会按 `(44 × 缩放 − 14) / 2` 重新计算；vibrancy 还没设 | T074 |
| T153、T154 | 菜单顶层标题和角色项（撤销、复制、退出等）是 Electron 自带的英文，需要给 `@cupper/i18n` 加 `menu` 命名空间；菜单分组是 T074 暂定的，按 Codex 定稿 | T074 |
| T160、T180 | Electron 自带的 Info.plist 有 5 条用途说明（相机、麦克风、两条蓝牙、音频采集），electron-builder 的 `extendInfo` 删不掉。自用构建和发布构建要在打包后删除，例如用 `afterPack` 钩子（设计 5.10.1、5.10.2） | T074 |
| T161 | Electron 44 有第 9 个 fuse，@electron/fuses 1.8.0 不认识，读出来是 `undefined`，逐项断言时要处理 | T074 |
| T012 | `stream/events`、`stream/snapshot`、`history/page` 的参数若整体用派生的 `Deserialize`，一个残缺事件就会让整条通知解析失败。参数类型要让客户端能逐个事件调用 `Event::decode`、`EventBody::decode`（宽容模式） | T011（已写入 brief） |
| T015 | `Event<EventBody>` 的 schema 名是 `Event`；`UnknownMember` 的 schema 是任意对象，后处理时用 `.loose()` | T011 |
| T017 | 在 `EventBody` 里加会话流的变体，`stream_kind()` 是穷尽 `match`，会强制给新变体分类；`item.delta` 变体上标 `#[schemars(extend("x-mergeable" = true))]`，并为 `EventBody` 实现 `Mergeable`；条目联合用 `open_union!` 的 `in tag "kind"` 形式；测试复用 `testutil::{roundtrip, open_values}` | T011 |
| 第一个写事件的宿主任务（T026 或 T027） | 事件的版本常量 `EVENT_VERSION`（`v`）还没定义，按需加上（5.4.6 的事件升级器） | T011（已写入 T026 的 brief） |
| T037 | 在 `main` 里接入 stdio 重定向：调用 `cupper_sys::redirect_stdio_to_null()`（T021 补充，把 0、1、2 都指向 `/dev/null`）。读载荷的构建读完再调用，发布构建一开始就调用。另外，`main` 目前只为等载荷建了一个 current-thread runtime，可以按 CFRunLoop 的方案重组；数据目录的 0700 和属主检查、把 `test_config.clock` 和 `Durations` 分发给各模块，也都归 T037 | T023 |
| T025 | 宿主 ID 用 `HostId::from_uuid(new_host_uuid().as_u128())` | T023（已写入 brief） |
| T030 | 密钥取 `payload.dev_key`、`payload.harness_key` 的 `SecretKey::expose()`；`dev_key` 只接受拉起宿主的进程（5.8.1，`p_puniqueid`，见 T021） | T023（已写入 brief） |
| T045、T075 | 启动载荷是一行 JSON 加换行，写完立即关闭 stdin；两种密钥都是 64 位十六进制；出现未知的键时宿主以退出码 1 退出。CLI 的 `--harness-key-fd` 用同样的密钥格式 | T023 |
| T056 | `--self-test` 还没加，现在会被当作未知参数，以退出码 1 退出 | T023 |
| T161 | `--build-info` 输出一行 JSON：`{"debug_assertions","features","name","version"}` | T023 |
| 新增定时器或故障点的任务 | 定时器的时长登记到 `crates/cupper-host/src/durations.rs`（`DurationKey` 加入 `ALL`）；故障点用 `failpoint!(NAME)`，名字登记到 `failpoint.rs` 的 `ALL`，格式为 `<模块>.<位置>`；故障点的用例断言退出码为 99 | T023 |
| T030、T045、T073 | 读 cupper 套接字一律用 `cupper_sys::recv_closing_fds`：普通读取会装入 `SCM_RIGHTS` 传来的 fd。网关的用法是 `stream.try_io(Interest::READABLE, \|\| recv_closing_fds(&stream, buf))`，`closed_fds > 0` 时断开连接并记审计（设计 5.8.5 v1.8） | T021（已写入 T030 的 brief） |
| T030、T073 | `LOCAL_PEERPID` 在最后一次做 I/O 的进程退出后，仍返回它的旧 pid，这个 pid 可能已经被复用，所以 `dev_key` 证明里的 `pid_c` 取固定令牌的 `pid()`。固定令牌只反映最后一次 I/O，比对要跟在每一次 read 之后。判断"启动时父进程已退出"用 `process_info(自己).ppid == 1`，`p_puniqueid` 只用来比对对端 | T021（已写入 T030 的 brief） |
| T002 | 缺口编号要登记：IT-sys-peer-macos14、IT-sys-peer-macos15，状态写"缺口：本机替代不了，里程碑退出时由用户确认"；本机跑出的结果只记作 IT-sys-peer-current | T021 |
| T014 | `apply` 规则：`ItemPatch` 里出现的字段整体替换，没出现的保持不变。`late_by_ref` 的键在线上是 `ref.item_id`，在 Rust 里是 `LateToolOutput.of.item_id`。工具调用的用时取自 `item.completed.duration_ms`。快照里的长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta` | T017 |
| T012 | `interaction/respond.outcome` 复用 `InteractionOutcome`；`interaction/get` 的状态复用 `InteractionStatus`（含 `pending`）。宿主核对摘要用 `request_digest`，它返回 `Result`，只在出现超过 2^53 的整数时失败 | T017（已写入 brief） |
| T013 | `git/diff` 复用 `DiffStat`、`ExcludedPath`、`SnapshotStatus` | T017（已写入 brief） |
| T015 | `ItemStarted` 的 schema 是外壳属性加 9 个成员的 `anyOf`；`ItemUpdated` 展开 `ItemPatch` 后共 21 个属性；`AnswerValue` 是不带标签的联合（`anyOf`）；`Item`、`InteractionRequest`、`InteractionOutcome` 的兜底成员是 `UnknownMember`，后处理时用 `.loose()` | T017 |
| T053（提取层） | `ToolCall::missing_required(ended)` 不为空时，把 `extraction` 降为 `partial` | T017 |
| T053 | 封闭枚举的原值用 `tap::k19::original` 读取，路径是 `_meta.cupper.original.<字段名>`，覆盖 `stopReason`、选项 `kind` 和工具调用的 `status`。`initialize` 只声明了 elicitation、`terminal_output`、`plan`，比 SP-1 的客户端少了 `session.{configOptions.boolean, notices, compaction}` 和 `auth.terminal`。不声明 `session.compaction` 时适配器还推不推 compaction，要用录制或兼容剧本确认，必要时在设计 5.6.1 里补上 | T050 |
| T055 | 回复和 `session/update` 投进同一个邮箱，以保持线上的顺序（K12 依赖这一点）。drop `AcpConnection` 会关闭适配器的 stdin，这是关闭顺序的第 2 步。`RawSink::bounded()` 只建一个，各个实例共用。`steering.supported` 从 `open` 返回的 `InitializeResponse` 顶层的 `_meta` 读取。K28 的规避方式是不对外暴露 `SentRequest`，请求发出时就把回调交给 SDK | T050 |
| T028 | 凭据的识别规则目前在 `driver/tap/redact.rs`（`sk-`、`ghp_`、JWT、`Bearer`、URL 中的 userinfo 和查询参数，只匹配词首）。可以挪到 `diag` 共用，避免两处各自改出差异 | T050 |
| T025、T047 | 原始帧的方向 `Dir` 目前有 `send`、`recv`、`recv_nonjson`；`stderr` 和 `note` 由写入它们的任务补上 | T050（已写入 T025 的 brief） |
| 全工作区 | serde_json 的 `preserve_order` 由 SDK 打开，在工作区内是统一生效的，所以 `cupper-protocol` 在工作区构建时也会带上它。导出物一律不能依赖 map 的键顺序（`CLAUDE.md`） | T050 |
| T052 | 回放模式落地时，加一项自动比对：假 agent 的 persona（能力、configOptions、默认模式、命令列表、权限选项 ID）与 SP-1 录制的形态一致。T051 里这一项只用测试里的常量逐条核对 | T051 |
| T037、T043 及以后的 actor 任务 | 任何 `posix_spawn`（包括带 `CLOEXEC_DEFAULT` 的 `cupper_sys::spawn`）都会在内核里短暂复制整张 fd 表。别的线程正在拉起子进程时，刚关掉的 fd 可能还被持有片刻。凡是"关掉 fd 后立刻判断对端已关闭"的逻辑都要容忍这个窗口；测试里用 `cupper_sys` 的 `testutil::alone` 隔离这类断言 | T021 |
| T025 | 用 `store::open_writer(path, Database::Host)`（或 `Raw`）打开连接，迁移在这个连接上跑完，再交给 `Writer::spawn`：迁移需要自己的事务，`VACUUM INTO` 也不能在事务里执行，所以不能作为写线程的操作。`Readers::open` 放在写连接打开之后 | T024（已写入 brief） |
| T037 | 启动第 5 步：`epoch::decide(LastExit::from_meta(..), Boot::since(meta.boot_session_uuid, current_boot_session(&payload)?))`，然后写 `last_start_at` 和 `boot.to_string()`。`Writer::spawn` 之前先 `quiescence.register(store::WRITER)`，写线程配置用 `WriterConfig::for_startup(&payload, &durations)`。有序退出时，把写 `last_clean_exit_at` 作为 `stop()` 之前的最后一个操作提交 | T024 |
| T037 | 写线程上的操作 panic 时，写线程随之结束：在途回执都是 `Stopped`，`WriterThread::stop()` 返回 `Err`。宿主把这种情况当作致命错误，有序退出，不能带着死掉的写线程继续运行 | T024 审阅 |
| T026、T032、T121 | 写线程的回执在提交之后才返回，可以直接作为"追加三步"的第二步。写 `pending_external` 的操作用 `Durability::Full`。操作不能自己结束事务，否则整批作废。需要 `prepare_cached` 时给 rusqlite 打开 `cache` 特性（会引入 foldhash，Zlib 已在 allowlist 里） | T024（已写入 T026 的 brief） |
| T027 | 写线程目前只有 `host.db` 的连接。原始帧（`raw.db`，有界队列 4,096）需要在 `Batcher` 里加第二个连接和队列（按 5.2.8，每个数据库一个事务），或者另开线程；`Database::Raw` 的 pragma 已经备好 | T024 |
| T047 | 写线程现在只登记"运行中"。实现忙或空闲时，阻塞在 `inbox.recv()` 并且手里没有批次才算空闲。ManualClock 下 `linger` 为 0，不会有真实时间的等待 | T024 |
| T069 | 故障点 `store.before_commit`、`store.after_commit` 的位置：`Batcher::commit` 里 COMMIT 之前；`Batcher::flush` 里提交之后、回复回执之前 | T024 |
| T012 | `stream/subscribe` 的结果复用 `sync::SubscribeMode`、`sync::ResyncReason`，不要另定义 | T014（已写入 brief） |
| T018 | 5.3.11 末尾"T012 开工前的裁定"列出的取值，包括 `findings`、`diff_from_trusted`、`fallback_reason`、`blockers`、补充的错误码，以及 `queue_edit` 的 scope | T012 裁定（已写入 brief） |
| T026 | 每个 `turn.completed` 之后，在同一个事务里先存 `turn_ops(&turn_id, &OpLimits::V1)` 作为回合快照，再调用 `trim(&Window::V1)`。检查点直接存视图的 JSON。`history/page` 用 `paginate(回合 ops, max_bytes)`，`op_index` 是页的起点 | T014（已写入 brief） |
| T026、T043 | 宿主要守的不变式见 5.3.11 末尾"T014 实现时的裁定"：引用某个回合的事件都写在它的 `turn.completed` 之前；交互请求一律属于某个回合，没有进行中的回合时按 K9 开 `autonomous` 回合；回合在它的请求全部解决之前不结束 | T014 审阅（已写入 T026 的 brief） |
| T033 | 订阅时调用 `sync::subscribe_mode(resume, &LogExtent)`。快照分页用 `paginate(snapshot_ops(), 帧负载减去信封的余量)` | T014 |
| T045 | 客户端同步状态机归本任务（T014 没做）。遇到 `snapshot`，不论原因（包括未知的 mode），都丢弃视图和游标；核对 `at_seq`、`epoch` 和页序号后组装，之后的事件经 `StreamCursor` 再 `apply`。遇到 `ViewError` 时不带游标重新订阅。`stream/reset` 保留游标 | T014 |
| T015 | 把 `IndexView`、`SessionView` 导出进 `v1.json`，测试 #16 要比对两端的视图 JSON | T014 |
| T072 | TS reducer 照 5.3.11 末尾"T014 实现时的裁定"的规则和视图 JSON 实现。JSON 对象的键顺序没有含义，比较时按深度相等 | T014 |
| T030 | `RawMessage::classify` 会把缺省的 `params` 变成 `null`，而 `ping` 这类参数为 `{}` 的方法，按结构体解析时会失败。分发时把 `null` 当作 `{}` | T012（已写入 brief） |
| T035 | `project/trust_info.diff_from_trusted` 按文件列出差异，所以信任时要另存每个文件的哈希清单，按 P-4 加进 `0001` 的 `projects` 表 | T012 裁定 |
| T052 | 假 agent 的 `--segment-file <路径>` 每次启动段号加 1，重启后的实例接着演下一段，用来回放多实例录制。persona 与 SP-1 录制的 jq 摘取命令在 `docs/tasks/T051-fake-agent-scripts/notes.md` 里，自动比对可以直接用 | T051 |
| T055、T062、T090 | 假 agent 是独立的二进制，cargo 不支持跨 crate 的 `CARGO_BIN_EXE_*`。T055 定一个拿路径的做法（例如测试前先 `cargo build -p cupper-fake-agent`，再按 target 目录推出路径，或由 `xtask ci` 先构建），后面的任务照用。配置只经命令行参数（`--persona`、`--auth`、`--fault`、`--segment`、`--segment-file`），因为宿主的环境变量规则会过滤掉我们自己的变量。同一会话收到第二个在途 prompt 时，假 agent 以退出码 70 退出 | T051 |
| T037 | host.db 的启动顺序：`open_writer(host.db, Database::Host)` → `store::migrate(&mut conn, MIGRATIONS, &data_dir.join("backups"), &*clock)` → 用 `meta::get` 读出 epoch 判定的输入，用 `meta::set` 写 `LastStartAt`、`BootSessionUuid`、`DataEpoch` → `Writer::spawn`。raw.db：`open_writer(raw.db, Database::Raw)` → `prepare_raw(&mut conn, &RAW_SCHEMA)`。退出码：`TooNew` → 4；`Failed` → 6，界面显示 `backup`；`Backup`、`Unreadable`、`BadVersion` 建议用 5 | T025 |
| T026 | 写事件时 `v` 取 `Upcasters::host().current(type)`，可以替代 T011 交接里的 `EVENT_VERSION`；读事件时调用 `upcast(type, v, data)`。检查点：写 `view_v`，读时调用 `checkpoint_fate(view_v)`；`VIEW_VERSION` 从 `store::upcast` 挪到 `cupper-protocol::view`，和视图类型放在一起（编排者裁定）。`turn_snapshots.v` 是整个快照一个版本，而事件的 `v` 是按类型分的，ops 里的事件体也不带 `v`，请定下这一列的含义，例如让每个 op 带自己的 `v` | T025（已写入 brief） |
| T027 | `backups/` 只保留最近 3 份（5.4.5），迁移成功后回收。备份文件名 `host-v<版本>-<YYYYMMDDTHHMMSSZ>.db`，按名字排序就是按时间排序；忽略 `host.partial` | T025 |
| T055 | `driver/tap/mod.rs` 有一处文档链接指向私有的 `redact`，`RUSTDOCFLAGS=-D warnings cargo doc -p cupper-host --all-features` 会失败。改 `driver/` 时顺手修 | T025 |
| T160 | 迁移脚本在 `foreign_keys=ON` 下执行。按 SQLite 的 12 步法重建表的脚本，需要在事务外关闭外键，迁移框架（`store::migrate`）目前没有处理 | T025 |
| T018、T013 | 接入方法的步骤（详见 `docs/tasks/T012-protocol-methods-core/notes.md`"给 T018、T013 的说明"）：新建组文件，用 `method!` 声明并提供 `register`；`methods/mod.rs` 加一行 `mod`，`catalog()` 加一行；在组文件的测试模块里写 `samples()`，加进 `catalog.rs` 的覆盖测试；按 5.8.3 往 `MATRIX` 加行；参数必须是具名结构体。类型按完整的方法名命名（`session/send` 是 `SessionSend`） | T012（已写入 brief） |
| T018、T013 | 用例编号的归属（编排者裁定）：`catalog.rs` 里的跨组测试（往返、矩阵、`command_id`、整数）属于 T012 的 UNIT-proto-methods-core-stream，不改名，它们会自动检查新加的方法。各任务另写以自己编号开头的测试（T018 是 `unit_proto_methods_core_session_`，T013 是 `unit_proto_methods_ext_`），用共享的测试辅助函数检查自己这组的样例和矩阵行，让自己的编号有独立通过的用例 | T012 审阅（已写入 brief） |
| T018 | 权限矩阵要新增一种格子："scope 允许，但按 SEC-9 由策略拒绝"，用在 `project/add` 等行的 remote_device 一列。T012 用不到，所以没加 | T012（已写入 brief） |
| T030、T045 | 可以直接用 `methods::handshake::PROTOCOL`、`VersionRange::negotiate`（取双方交集中的最大版本）、`StreamSubscribeParams::resume()`（转成 T014 的 `Resume`）。`nonce`、`proof` 在协议里是字符串，格式校验和转成字节归 T030 | T012（已写入 T030 的 brief） |
| T015 | 方法目录用 `methods::catalog::catalog(&mut generator)` 导出，之后再 `take_definitions(true)`。`Raw<Event<EventBody>>` 的 schema 名就是 `Event` | T012 |
| T048 | 重置 T040 分支时，两个时长的登记随 `e03a930` 一起移走了，要在 `durations.rs` 里重新登记：`turn_prepare` 3 s，`interrupt_fallback` 取 `limits::V1`。`Action` 的 large_enum_variant 目前用带 reason 的 `#[expect]` 放过 | T040 |
| T043、T048 | T018 定义了会话方法的参数类型之后，把 `QueueOp`、`QueueEditError`、`QueueFull` 映射到协议的错误码。队列满对应 `too_large/queue_full`（5.3.11 末尾，编排者裁定） | T040 |
| T055 | 实现 `driver::api`：用 `PromptTag` 回填 prompt 的结果；用 `AskId` 管理 ACP 的 responder；把 `PromptContent` 转成 ACP 的内容块，图片读 blob 转成 base64，评论按 GT-2 排成文本 | T040 |
| T043 | 会话检查点因 `view_v` 不符而作废时，存储层按 5.4.6 只用回合快照重建视图，会话的配置、能力、队列、`tail`、`detached` 都会缺失。会话 actor 恢复会话时，要重发 `session.configured`、`session.capabilities`，并按数据库里的队列发 `queue.updated` | T026 |
| T034 | 索引流的检查点作废、日志又已经裁剪时，索引视图会不完整。IndexActor 启动时按 `projects`、`sessions` 两张表重发相应事件 | T026 |
| T033 | 补发 `after_seq+1..=head_seq` 时，读到的事件可能刚被裁剪掉（补发与裁剪并发）。发现缺口时按 `cursor_expired` 改发快照，或者发 `stream/reset`，由 T033 定。`store::log::read_events` 是公开的，补发用它 | T026 |
| T030、T037、T043 及以后的 actor 任务 | 关掉 fd 后要判断对端是否已关闭，以读到 EOF 为信号，不要关闭后立刻查 `getsockopt`：别的线程拉子进程时，内核会短暂复制整张 fd 表。另外，std 在 macOS 上建管道和 socketpair 时是先建、后设 close-on-exec，`std::process` 和 `tokio::process` 拉起的子进程可能把它们一直带到自己退出。这再次说明"拉子进程一律经 `cupper_sys::spawn`"的必要 | T021 修复第 2 轮（已写入 T030 的 brief） |
| T051 以后改 `cupper-fake-agent` 测试的任务 | `unit_fake_scripts_probe` 只要求继承的 fd 以 `0,1,2` 开头，不要改成严格等于 `0,1,2`：并发拉起时别的 fd 可能被带走，接在后面，严格比较会偶发失败 | T021 修复第 2 轮 |
| T031 | 策略层要实现以下三点：`MATRIX` 里每个 `Sec9` 格子（scope 允许、按 SEC-9 拒绝，见 5.3.11 末尾"T018 实现时的取值"）；`InputEvidence` 的 `key` 与 `digit` 互相一致；`digit` 与所选选项的序号相符（特权选项和效果未知的选项也占序号） | T018 |
| T035 | `trust_digest` 在协议里是 `Digest` 类型（`d_` 加 64 位十六进制），信任扫描的摘要要按这个形式输出 | T018 |
| 升级 claude-agent-acp 到 0.93 之前 | `/remote-control`（Q42）：先经安全评审，并由宿主拦截 | T003 |
| T063、T066 | 观察多次 resume 之后的 `claude` 进程数（Q43，上游 #1011） | T003 |
| 以后建远端、公开之前 | git 历史里有 SP-1 录制补充脱敏之前的内容，两份会议记录里也有带用户名的路径。要对全部历史做个人信息扫描并清理（计划 1.8、R12），由用户决定 | SP-1 补充脱敏 |
