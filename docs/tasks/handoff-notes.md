# 交接事项汇总

编排者维护。各任务的汇报和审阅中留给后续任务的提醒，按接收任务汇总在这里，写 brief 时逐条带进去；带进去之后在行末标"已写入 brief"。

| 接收任务 | 事项 | 来源 |
|---|---|---|
| T002 | 逐编号结果目前解析的是 libtest 的输出（`crates/xtask/src/ci/ctx.rs` 的 `parse_libtest`）。改用 nextest 的 JUnit 报告时，替换这个函数 | T001（已写入 brief） |
| T002 | ts 作业已经接上 Vitest 的 JSON reporter（`parse_vitest`）。按编号匹配时，JS 标题要以编号原文开头（`ids::owner`） | T070（已写入 brief） |
| T006 | `lightningcss`（MPL-2.0，vite 的依赖）只在开发和测试时使用，不进 App。许可证门禁要按生产依赖闭包计算，否则会误报 | T070 |
| T015、T071 及以后的 TS 包 | 惯例：包的 `exports` 直接指向 `src/*.ts`；类型检查走 project references；Vitest 只收 `src/**/*.test.ts`；包的 `test` 脚本必须运行 Vitest，作业会在末尾补上 reporter 参数，不用 Vitest 的另起脚本名 | T070（已写入 T015 的 brief） |
| 写不可见字符测试的任务（T083 等） | 文件写入工具会把 `\uXXXX` 解码成真实字符。测试里需要双向控制符、零宽字符时，用脚本生成或用 `String.fromCodePoint`，提交前扫描确认没有混入真实字符 | T070 |
| T023 | 宿主 ID 是 `h_` 加 UUIDv4（5.4.4），不归 `IdGen` 管，`cupper-host` 的 `uuid` 也没开 `v4`。需要自己取随机源 | T022 |
| T023 及以后的 `cupper-host` 任务 | `tests/` 下用到 `ManualClock`、`CoreDriver`、`IdGen::seeded` 的集成测试，要声明 `required-features = ["test-harness"]`。不带特性的 `cargo test -p cupper-host` 会跳过它们 | T022 |
| T024 | 写线程"只在 `SystemClock` 下等 2 ms"。`Clock` 没有"是否是虚拟时钟"的查询，从启动配置（`test_config.clock`）判断，或者给 trait 加方法 | T022（已写入 brief，T024 按启动配置判断） |
| T043（第一个 actor 任务） | 适配循环写在 `pure/mod.rs` 的文档里。给 `TimerTable::next_deadline` 维持一个 `Clock` 定时器的小助手还没写，加在 `pure` 里 | T022（已写入 T043 的 brief） |
| T045 | CLI 在客户端生成 UUIDv7 的 `command_id`（5.3、5.7），而工作区禁用了 `now_v7`。需要一个带 reason 的豁免，或者用自己的生成器 | T022（已写入 brief） |
| T047 | 屏障：actor 在 poll 自己的等待点之前先标记为忙，只有等待的 future 返回 Pending 时才算空闲（例如给 `Participant` 加一个 `idle(fut)` 包装）。统计在途的消息和已触发、尚未被消费的定时器。逐个到期时间推进用 `ManualClock::next_deadline` 加 `advance_to`。存储写线程和 `spawn_blocking` 也作为参与者登记 | T022 |
| tokio、uuid、getrandom | 目前写在 `cupper-host` 自己的 `Cargo.toml` 里。出现第二个使用方时，提到 `[workspace.dependencies]` | T022 |
| `pathForFile`、`saveFile` | ShellAPI 冻结以后，以附加方式加入（M1b 的 T142 等） | T070 |
| T074、T154 | i18n 已有 `command`、`update` 两个命名空间。措辞按 Codex 的中文译文核对 | T070 |
| T021 | `PROC_PIDTBSDINFO` 读不了其他用户的进程；`PROC_PIDT_SHORTBSDINFO` 和 `PROC_PIDUNIQIDENTIFIERINFO`（flavor 17，56 字节）对 root 进程也能读（实测 pid 1 可以）。libc 0.2.191 没有定义 `POSIX_SPAWN_SETSID` 和 `proc_bsdinfowithuniqid`，需要自己声明 | T020 |
| T037、T055 | 关闭顺序：先用 `child.group().with_descendants()` 记下所有进程组，再逐组 `signal(Term)`，用 `Clock` 限时等待并用 `is_alive()` 核对，最后对还在的组 `signal(Kill)`。持久化时存 `pgid` 和 `start.as_micros()`。适配器自己先退出时，后代已无从枚举，只能靠资源采样时记下的组 | T020（已写入 T037 的 brief） |
| T038 | `parent_chain` 能经过 root 进程走到 launchd；进程名是内核里的 16 字节短名，要完整路径需另取 | T020 |
| T054 | 直接用 `Env::from_allowed` 加 `set`。`CUPPER_*` 已在 `cupper-sys` 里拒绝，这里不用再处理 | T020 |
| 所有拉起子进程的任务 | `cupper_sys::spawn` 不搜索 `PATH`，调用方要传入解析好的绝对路径。`Child` 在 drop 时不会自动回收，必须 `wait`，否则会留下僵尸进程 | T020 |
| T011 | `Fallback{kind, summary}`（ADR-04）里的 `summary` 是界面文字，和"宿主只下发语义码"的原则冲突。定下 `summary` 的填法：例如只放语义码和参数，或者只作为最后兜底的英文短句，并写明理由 | T010 |
| T011 | `ErrorData.resolved_by` 目前放在 `details` 里，T011 定义好相关类型后，以附加方式改成具名字段 | T010 |
| T012 | `ErrorData.supported` 目前放在 `details` 里，依赖版本区间类型，T012 以附加方式改成具名字段 | T010（已写入 brief） |
| T015 | `Event<B>` 在泛型下由 schemars 自动生成 schema 名，导出时可能要加 `rename`。另外，把 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim` 纳入 `xtask ci`（设计 5.1.6；T010 只手动跑过）| T010（已写入 brief） |
| T011–T015 | `cupper-protocol` 的惯例：`lib.rs` 只放 `mod` 声明；ID 用 `ids` 里的类型；时间用 `TimestampMs`、`DurationMs`，路径用 `WirePath`，这样 `x-id`、`x-time`、`x-path` 注解会自动带上；可合并的事件实现 `Mergeable`，并在变体上标 `x-mergeable`；协议里不用超过 2^53−1 的整数 | T010（已写入 T015 的 brief） |
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
| T015 | `Event<EventBody>` 的 schema 名是 `Event`；`UnknownMember` 的 schema 是任意对象，后处理时用 `.loose()` | T011（已写入 brief） |
| T017 | 在 `EventBody` 里加会话流的变体，`stream_kind()` 是穷尽 `match`，会强制给新变体分类；`item.delta` 变体上标 `#[schemars(extend("x-mergeable" = true))]`，并为 `EventBody` 实现 `Mergeable`；条目联合用 `open_union!` 的 `in tag "kind"` 形式；测试复用 `testutil::{roundtrip, open_values}` | T011 |
| 第一个写事件的宿主任务（T026 或 T027） | 事件的版本常量 `EVENT_VERSION`（`v`）还没定义，按需加上（5.4.6 的事件升级器） | T011（已写入 T026 的 brief） |
| T037 | 在 `main` 里接入 stdio 重定向：调用 `cupper_sys::redirect_stdio_to_null()`（T021 补充，把 0、1、2 都指向 `/dev/null`）。读载荷的构建读完再调用，发布构建一开始就调用。另外，`main` 目前只为等载荷建了一个 current-thread runtime，可以按 CFRunLoop 的方案重组；数据目录的 0700 和属主检查、把 `test_config.clock` 和 `Durations` 分发给各模块，也都归 T037 | T023（已写入 brief） |
| T025 | 宿主 ID 用 `HostId::from_uuid(new_host_uuid().as_u128())` | T023（已写入 brief） |
| T030 | 密钥取 `payload.dev_key`、`payload.harness_key` 的 `SecretKey::expose()`；`dev_key` 只接受拉起宿主的进程（5.8.1，`p_puniqueid`，见 T021） | T023（已写入 brief） |
| T045、T075 | 启动载荷是一行 JSON 加换行，写完立即关闭 stdin；两种密钥都是 64 位十六进制；出现未知的键时宿主以退出码 1 退出。CLI 的 `--harness-key-fd` 用同样的密钥格式 | T023（已写入 T045 的 brief） |
| T056 | `--self-test` 还没加，现在会被当作未知参数，以退出码 1 退出 | T023（已由 T056 实现） |
| T161 | `--build-info` 输出一行 JSON：`{"debug_assertions","features","name","version"}` | T023 |
| 新增定时器或故障点的任务 | 定时器的时长登记到 `crates/cupper-host/src/durations.rs`（`DurationKey` 加入 `ALL`）；故障点用 `failpoint!(NAME)`，名字登记到 `failpoint.rs` 的 `ALL`，格式为 `<模块>.<位置>`；故障点的用例断言退出码为 99 | T023 |
| T030、T045、T073 | 读 cupper 套接字一律用 `cupper_sys::recv_closing_fds`：普通读取会装入 `SCM_RIGHTS` 传来的 fd。网关的用法是 `stream.try_io(Interest::READABLE, \|\| recv_closing_fds(&stream, buf))`，`closed_fds > 0` 时断开连接并记审计（设计 5.8.5 v1.8） | T021（已写入 T030 的 brief）（已写入 T045 的 brief） |
| T030、T073 | `LOCAL_PEERPID` 在最后一次做 I/O 的进程退出后，仍返回它的旧 pid，这个 pid 可能已经被复用，所以 `dev_key` 证明里的 `pid_c` 取固定令牌的 `pid()`。固定令牌只反映最后一次 I/O，比对要跟在每一次 read 之后。判断"启动时父进程已退出"用 `process_info(自己).ppid == 1`，`p_puniqueid` 只用来比对对端 | T021（已写入 T030 的 brief） |
| T002 | 缺口编号要登记：IT-sys-peer-macos14、IT-sys-peer-macos15，状态写"缺口：本机替代不了，里程碑退出时由用户确认"；本机跑出的结果只记作 IT-sys-peer-current | T021（已写入 brief） |
| T014 | `apply` 规则：`ItemPatch` 里出现的字段整体替换，没出现的保持不变。`late_by_ref` 的键在线上是 `ref.item_id`，在 Rust 里是 `LateToolOutput.of.item_id`。工具调用的用时取自 `item.completed.duration_ms`。快照里的长文本按 `MAX_DELTA_ESCAPED` 拆成 `item.delta` | T017 |
| T012 | `interaction/respond.outcome` 复用 `InteractionOutcome`；`interaction/get` 的状态复用 `InteractionStatus`（含 `pending`）。宿主核对摘要用 `request_digest`，它返回 `Result`，只在出现超过 2^53 的整数时失败 | T017（已写入 brief） |
| T013 | `git/diff` 复用 `DiffStat`、`ExcludedPath`、`SnapshotStatus` | T017（已写入 brief） |
| T015 | `ItemStarted` 的 schema 是外壳属性加 9 个成员的 `anyOf`；`ItemUpdated` 展开 `ItemPatch` 后共 21 个属性；`AnswerValue` 是不带标签的联合（`anyOf`）；`Item`、`InteractionRequest`、`InteractionOutcome` 的兜底成员是 `UnknownMember`，后处理时用 `.loose()` | T017（已写入 brief） |
| T053（提取层） | `ToolCall::missing_required(ended)` 不为空时，把 `extraction` 降为 `partial` | T017 |
| T053 | 封闭枚举的原值用 `tap::k19::original` 读取，路径是 `_meta.cupper.original.<字段名>`，覆盖 `stopReason`、选项 `kind` 和工具调用的 `status`。`initialize` 只声明了 elicitation、`terminal_output`、`plan`，比 SP-1 的客户端少了 `session.{configOptions.boolean, notices, compaction}` 和 `auth.terminal`。不声明 `session.compaction` 时适配器还推不推 compaction，要用录制或兼容剧本确认，必要时在设计 5.6.1 里补上 | T050 |
| T055 | 回复和 `session/update` 投进同一个邮箱，以保持线上的顺序（K12 依赖这一点）。drop `AcpConnection` 会关闭适配器的 stdin，这是关闭顺序的第 2 步。`RawSink::bounded()` 只建一个，各个实例共用。`steering.supported` 从 `open` 返回的 `InitializeResponse` 顶层的 `_meta` 读取。K28 的规避方式是不对外暴露 `SentRequest`，请求发出时就把回调交给 SDK | T050 |
| T028 | 凭据的识别规则目前在 `driver/tap/redact.rs`（`sk-`、`ghp_`、JWT、`Bearer`、URL 中的 userinfo 和查询参数，只匹配词首）。可以挪到 `diag` 共用，避免两处各自改出差异 | T050 |
| T025、T047 | 原始帧的方向 `Dir` 目前有 `send`、`recv`、`recv_nonjson`；`stderr` 和 `note` 由写入它们的任务补上 | T050（已写入 T025 的 brief） |
| 全工作区 | serde_json 的 `preserve_order` 由 SDK 打开，在工作区内是统一生效的，所以 `cupper-protocol` 在工作区构建时也会带上它。导出物一律不能依赖 map 的键顺序（`CLAUDE.md`） | T050 |
| T067（v1.3.12 由 T052 拆出） | 回放模式落地时，加一项自动比对：假 agent 的 persona（能力、configOptions、默认模式、命令列表、权限选项 ID）与 SP-1 录制的形态一致。T051 里这一项只用测试里的常量逐条核对 | T051 |
| T037、T043 及以后的 actor 任务 | 任何 `posix_spawn`（包括带 `CLOEXEC_DEFAULT` 的 `cupper_sys::spawn`）都会在内核里短暂复制整张 fd 表。别的线程正在拉起子进程时，刚关掉的 fd 可能还被持有片刻。凡是"关掉 fd 后立刻判断对端已关闭"的逻辑都要容忍这个窗口；测试里用 `cupper_sys` 的 `testutil::alone` 隔离这类断言 | T021（已写入 T037 的 brief）（已写入 T043 的 brief） |
| T025 | 用 `store::open_writer(path, Database::Host)`（或 `Raw`）打开连接，迁移在这个连接上跑完，再交给 `Writer::spawn`：迁移需要自己的事务，`VACUUM INTO` 也不能在事务里执行，所以不能作为写线程的操作。`Readers::open` 放在写连接打开之后 | T024（已写入 brief） |
| T037 | 启动第 5 步：`epoch::decide(LastExit::from_meta(..), Boot::since(meta.boot_session_uuid, current_boot_session(&payload)?))`，然后写 `last_start_at` 和 `boot.to_string()`。`Writer::spawn` 之前先 `quiescence.register(store::WRITER)`，写线程配置用 `WriterConfig::for_startup(&payload, &durations)`。有序退出时，把写 `last_clean_exit_at` 作为 `stop()` 之前的最后一个操作提交 | T024（已写入 brief） |
| T037 | 写线程上的操作 panic 时，写线程随之结束：在途回执都是 `Stopped`，`WriterThread::stop()` 返回 `Err`。宿主把这种情况当作致命错误，有序退出，不能带着死掉的写线程继续运行 | T024 审阅（已写入 brief） |
| T026、T032、T121 | 写线程的回执在提交之后才返回，可以直接作为"追加三步"的第二步。写 `pending_external` 的操作用 `Durability::Full`。操作不能自己结束事务，否则整批作废。需要 `prepare_cached` 时给 rusqlite 打开 `cache` 特性（会引入 foldhash，Zlib 已在 allowlist 里） | T024（已写入 T026 的 brief） |
| T027 | 写线程目前只有 `host.db` 的连接。原始帧（`raw.db`，有界队列 4,096）需要在 `Batcher` 里加第二个连接和队列（按 5.2.8，每个数据库一个事务），或者另开线程；`Database::Raw` 的 pragma 已经备好 | T024（已写入 brief） |
| T047 | 写线程现在只登记"运行中"。实现忙或空闲时，阻塞在 `inbox.recv()` 并且手里没有批次才算空闲。ManualClock 下 `linger` 为 0，不会有真实时间的等待 | T024 |
| T069 | 故障点 `store.before_commit`、`store.after_commit` 的位置：`Batcher::commit` 里 COMMIT 之前；`Batcher::flush` 里提交之后、回复回执之前 | T024 |
| T012 | `stream/subscribe` 的结果复用 `sync::SubscribeMode`、`sync::ResyncReason`，不要另定义 | T014（已写入 brief） |
| T018 | 5.3.11 末尾"T012 开工前的裁定"列出的取值，包括 `findings`、`diff_from_trusted`、`fallback_reason`、`blockers`、补充的错误码，以及 `queue_edit` 的 scope | T012 裁定（已写入 brief） |
| T026 | 每个 `turn.completed` 之后，在同一个事务里先存 `turn_ops(&turn_id, &OpLimits::V1)` 作为回合快照，再调用 `trim(&Window::V1)`。检查点直接存视图的 JSON。`history/page` 用 `paginate(回合 ops, max_bytes)`，`op_index` 是页的起点 | T014（已写入 brief） |
| T026、T043 | 宿主要守的不变式见 5.3.11 末尾"T014 实现时的裁定"：引用某个回合的事件都写在它的 `turn.completed` 之前；交互请求一律属于某个回合，没有进行中的回合时按 K9 开 `autonomous` 回合；回合在它的请求全部解决之前不结束 | T014 审阅（已写入 T026 的 brief）（已写入 T043 的 brief） |
| T033 | 订阅时调用 `sync::subscribe_mode(resume, &LogExtent)`。快照分页用 `paginate(snapshot_ops(), 帧负载减去信封的余量)` | T014（已写入 brief） |
| T045 | 客户端同步状态机归本任务（T014 没做）。遇到 `snapshot`，不论原因（包括未知的 mode），都丢弃视图和游标；核对 `at_seq`、`epoch` 和页序号后组装，之后的事件经 `StreamCursor` 再 `apply`。遇到 `ViewError` 时不带游标重新订阅。`stream/reset` 保留游标 | T014（已写入 brief） |
| T015 | 把 `IndexView`、`SessionView` 导出进 `v1.json`，测试 #16 要比对两端的视图 JSON | T014（已写入 brief） |
| T072 | TS reducer 照 5.3.11 末尾"T014 实现时的裁定"的规则和视图 JSON 实现。JSON 对象的键顺序没有含义，比较时按深度相等 | T014（已写入 brief） |
| T030 | `RawMessage::classify` 会把缺省的 `params` 变成 `null`，而 `ping` 这类参数为 `{}` 的方法，按结构体解析时会失败。分发时把 `null` 当作 `{}` | T012（已写入 brief） |
| T035 | `project/trust_info.diff_from_trusted` 按文件列出差异，所以信任时要另存每个文件的哈希清单，按 P-4 加进 `0001` 的 `projects` 表 | T012 裁定 |
| T067（v1.3.12 由 T052 拆出） | 假 agent 的 `--segment-file <路径>` 每次启动段号加 1，重启后的实例接着演下一段，用来回放多实例录制。persona 与 SP-1 录制的 jq 摘取命令在 `docs/tasks/T051-fake-agent-scripts/notes.md` 里，自动比对可以直接用 | T051 |
| T055、T062、T090 | 假 agent 是独立的二进制，cargo 不支持跨 crate 的 `CARGO_BIN_EXE_*`。T055 定一个拿路径的做法（例如测试前先 `cargo build -p cupper-fake-agent`，再按 target 目录推出路径，或由 `xtask ci` 先构建），后面的任务照用。配置只经命令行参数（`--persona`、`--auth`、`--fault`、`--segment`、`--segment-file`），因为宿主的环境变量规则会过滤掉我们自己的变量。同一会话收到第二个在途 prompt 时，假 agent 以退出码 70 退出 | T051 |
| T037 | host.db 的启动顺序：`open_writer(host.db, Database::Host)` → `store::migrate(&mut conn, MIGRATIONS, &data_dir.join("backups"), &*clock)` → 用 `meta::get` 读出 epoch 判定的输入，用 `meta::set` 写 `LastStartAt`、`BootSessionUuid`、`DataEpoch` → `Writer::spawn`。raw.db：`open_writer(raw.db, Database::Raw)` → `prepare_raw(&mut conn, &RAW_SCHEMA)`。退出码：`TooNew` → 4；`Failed` → 6，界面显示 `backup`；`Backup`、`Unreadable`、`BadVersion` 建议用 5 | T025（已写入 brief） |
| T026 | 写事件时 `v` 取 `Upcasters::host().current(type)`，可以替代 T011 交接里的 `EVENT_VERSION`；读事件时调用 `upcast(type, v, data)`。检查点：写 `view_v`，读时调用 `checkpoint_fate(view_v)`；`VIEW_VERSION` 从 `store::upcast` 挪到 `cupper-protocol::view`，和视图类型放在一起（编排者裁定）。`turn_snapshots.v` 是整个快照一个版本，而事件的 `v` 是按类型分的，ops 里的事件体也不带 `v`，请定下这一列的含义，例如让每个 op 带自己的 `v` | T025（已写入 brief） |
| T027 | `backups/` 只保留最近 3 份（5.4.5），迁移成功后回收。备份文件名 `host-v<版本>-<YYYYMMDDTHHMMSSZ>.db`，按名字排序就是按时间排序；忽略 `host.partial` | T025（已写入 brief） |
| T055 | `driver/tap/mod.rs` 有一处文档链接指向私有的 `redact`，`RUSTDOCFLAGS=-D warnings cargo doc -p cupper-host --all-features` 会失败。改 `driver/` 时顺手修 | T025 |
| T160 | 迁移脚本在 `foreign_keys=ON` 下执行。按 SQLite 的 12 步法重建表的脚本，需要在事务外关闭外键，迁移框架（`store::migrate`）目前没有处理 | T025 |
| T018、T013 | 接入方法的步骤（详见 `docs/tasks/T012-protocol-methods-core/notes.md`"给 T018、T013 的说明"）：新建组文件，用 `method!` 声明并提供 `register`；`methods/mod.rs` 加一行 `mod`，`catalog()` 加一行；在组文件的测试模块里写 `samples()`，加进 `catalog.rs` 的覆盖测试；按 5.8.3 往 `MATRIX` 加行；参数必须是具名结构体。类型按完整的方法名命名（`session/send` 是 `SessionSend`） | T012（已写入 brief） |
| T018、T013 | 用例编号的归属（编排者裁定）：`catalog.rs` 里的跨组测试（往返、矩阵、`command_id`、整数）属于 T012 的 UNIT-proto-methods-core-stream，不改名，它们会自动检查新加的方法。各任务另写以自己编号开头的测试（T018 是 `unit_proto_methods_core_session_`，T013 是 `unit_proto_methods_ext_`），用共享的测试辅助函数检查自己这组的样例和矩阵行，让自己的编号有独立通过的用例 | T012 审阅（已写入 brief） |
| T018 | 权限矩阵要新增一种格子："scope 允许，但按 SEC-9 由策略拒绝"，用在 `project/add` 等行的 remote_device 一列。T012 用不到，所以没加 | T012（已写入 brief） |
| T030、T045 | 可以直接用 `methods::handshake::PROTOCOL`、`VersionRange::negotiate`（取双方交集中的最大版本）、`StreamSubscribeParams::resume()`（转成 T014 的 `Resume`）。`nonce`、`proof` 在协议里是字符串，格式校验和转成字节归 T030 | T012（已写入 T030 的 brief） |
| T015 | 方法目录用 `methods::catalog::catalog(&mut generator)` 导出，之后再 `take_definitions(true)`。`Raw<Event<EventBody>>` 的 schema 名就是 `Event` | T012（已写入 brief） |
| T048 | 重置 T040 分支时，两个时长的登记随 `e03a930` 一起移走了，要在 `durations.rs` 里重新登记：`turn_prepare` 3 s，`interrupt_fallback` 取 `limits::V1`。`Action` 的 large_enum_variant 目前用带 reason 的 `#[expect]` 放过 | T040 |
| T043、T048 | T018 定义了会话方法的参数类型之后，把 `QueueOp`、`QueueEditError`、`QueueFull` 映射到协议的错误码。队列满对应 `too_large/queue_full`（5.3.11 末尾，编排者裁定） | T040（已写入 T043 的 brief） |
| T055 | 实现 `driver::api`：用 `PromptTag` 回填 prompt 的结果；用 `AskId` 管理 ACP 的 responder；把 `PromptContent` 转成 ACP 的内容块，图片读 blob 转成 base64，评论按 GT-2 排成文本 | T040 |
| T043 | 会话检查点因 `view_v` 不符而作废时，存储层按 5.4.6 只用回合快照重建视图，会话的配置、能力、队列、`tail`、`detached` 都会缺失。会话 actor 恢复会话时，要重发 `session.configured`、`session.capabilities`，并按数据库里的队列发 `queue.updated` | T026（已写入 T043 的 brief） |
| T034 | 索引流的检查点作废、日志又已经裁剪时，索引视图会不完整。IndexActor 启动时按 `projects`、`sessions` 两张表重发相应事件 | T026 |
| T033 | 补发 `after_seq+1..=head_seq` 时，读到的事件可能刚被裁剪掉（补发与裁剪并发）。发现缺口时按 `cursor_expired` 改发快照，或者发 `stream/reset`，由 T033 定。`store::log::read_events` 是公开的，补发用它 | T026（已写入 brief） |
| T030、T037、T043 及以后的 actor 任务 | 关掉 fd 后要判断对端是否已关闭，以读到 EOF 为信号，不要关闭后立刻查 `getsockopt`：别的线程拉子进程时，内核会短暂复制整张 fd 表。另外，std 在 macOS 上建管道和 socketpair 时是先建、后设 close-on-exec，`std::process` 和 `tokio::process` 拉起的子进程可能把它们一直带到自己退出。这再次说明"拉子进程一律经 `cupper_sys::spawn`"的必要 | T021 修复第 2 轮（已写入 T030 的 brief）（已写入 T043 的 brief） |
| T051 以后改 `cupper-fake-agent` 测试的任务 | `unit_fake_scripts_probe` 只要求继承的 fd 以 `0,1,2` 开头，不要改成严格等于 `0,1,2`：并发拉起时别的 fd 可能被带走，接在后面，严格比较会偶发失败 | T021 修复第 2 轮 |
| T031 | 策略层要实现以下三点：`MATRIX` 里每个 `Sec9` 格子（scope 允许、按 SEC-9 拒绝，见 5.3.11 末尾"T018 实现时的取值"）；`InputEvidence` 的 `key` 与 `digit` 互相一致；`digit` 与所选选项的序号相符（特权选项和效果未知的选项也占序号） | T018（已写入 brief） |
| T035 | `trust_digest` 在协议里是 `Digest` 类型（`d_` 加 64 位十六进制），信任扫描的摘要要按这个形式输出 | T018 |
| T042 | 按 `Action::Settle { end, asks }` 的 `asks` 结束回合里挂着的请求：`adapter_exited` 时状态为 `expired`，`turn_cancelled` 和 `turn_ended` 时状态为 `cancelled`。中断时 `TurnMachine` 发 `CancelAsks`。Cancelling 期间新到的请求直接取消 | T048（已写入 brief） |
| T043 | `Settle` 之后补齐被放弃条目的 `item.completed`（`abandoned`，Codex 还要加 `may_still_run`）；`Complete` 时补上 `final_item_ids`，写 `turn.completed`；执行 `Action::Attach` 时补上工作区、配置和信任状态；`QueueFull` 映射为 `too_large/queue_full`；从数据库恢复回合序号、队列和 agent 会话（目前只有 `TurnMachine::new`） | T048（已写入 T043 的 brief） |
| T041 | 自发回合的进入点和退出点是 `Input::AutonomousStarted`、`AutonomousEnded`。Ending 期间到达的更新归属哪个回合，由本任务处理 | T048（已写入 brief） |
| T031 | T013 新增的策略点：git 写操作和 `settings/update` 的 `Sec9` 格子；`auth/elevate` 的 `CliOnly`（`cli` 以外的 principal 一律拒绝）；`git/revert` 的 `input` 证据（`Input` 格子）；`settings/update` 修改安全相关的键时要 privileged；`policy/describe` 每种 `action` 的 `details` 键 | T013（已写入 brief） |
| T030、T033 | 带 `x-builds` 的方法只在相应特性的构建里注册。UNIT-unsupported-set 要把 M2 的 `diag/export` 算进"返回 `unsupported`"的集合 | T013（已写入 T033 的 brief） |
| T015 | 方法条目可能带 `x-builds`，导出时要保留。两处 schema 不够精确：`Settings` 的 `cli_path` 实际总会出现（值可为 `null`），但 schema 里不是必填；`SettingsPatch` 的非空键在 schema 里也允许 `null`。都在客户端→宿主方向，宿主会拒绝，生成 TS 时可以考虑收紧 | T013（已写入 brief） |
| T036、T126 | `apps.terminal`、`apps.editor` 可选的内置应用清单没有设计，界面怎样拿到这个列表，需要补上 | T013 |
| T047 | `test/*` 的参数形状由 T013 补上：`clock_advance {ms}`、`run_maintenance {task}`、`seed {profile}`，实现时可以细化 | T013 |
| T002 | `xtask ci rust` 缺了 `cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim`（5.1.6 要求每个 PR 都跑），T001 时漏掉了，请补上 | T013（改由 T015 做，已写入它的 brief） |
| T033 | 网关的发送队列实现 `store::streams` 的 `Sink`：两个方法都在流锁内调用，只能排队，不能回调流；`push` 返回 `Full` 时流会摘掉这个订阅者并调用 `reset`（`slow_consumer`）。订阅时，把响应、快照页或补发标记放进 `subscribe` 的 `start` 回调。补发用 `store::log::read_events(conn, stream, after, head, ..)`。`history/page` 用 `store::history::page`；不带游标时，用 `StreamHandle::read` 取窗口里最早的 `turn_no`；`max_bytes` 自己截到 48,000 | T026（已写入 brief） |
| T032、T043 | 命令结果和它产生的事件放进同一个 `Batch`（`Batch::with`），同一事务提交。`Appended::rejected` 不为空，说明写者违反了视图规则，是 bug：要记日志；test-harness 构建里建议直接让用例失败 | T026（已写入 T032 的 brief）（已写入 T043 的 brief） |
| T037 | 组装流：`Streams::new(writer, readers, clock, StreamsConfig::new(epoch, &durations))` | T026（已写入 brief） |
| T039 | 接入点：`handshake::authenticate` 的 match、`GatewayConfig` 里的密钥、传给 `initialize` 的固定令牌、`host_proof` 的返回值、`principal` 里 `pr_test` 那一行（详见 `docs/tasks/T030-gateway-handshake/notes.md`"给 T039 的接入点"）。另外实现握手期限：`host/hello` 之后 10 s 内没有 `initialize` 就断开，审计原因 `handshake_timeout`（5.3.11 末尾，编排者裁定） | T030（已写入 brief） |
| T047 | 网关的连接任务还没有登记进静止屏障：它们由 I/O 驱动，只有关闭宽限（和 T039 的握手期限）用到 `Clock` 定时器 | T030 |
| T006 | `xtask ci license` 跑 `cargo deny check` 时要从 GitHub 拉 advisory 数据库，网络出错时整个作业失败（2026-10-19 在 main 上遇到一次，重跑通过）。考虑拉取失败时用已缓存的数据库继续，并在摘要里标记为过期，或者重试 | 编排者 |
| T033 | 每个请求和客户端通知都先调用 `policy::method(p, M::NAME, M::SCOPE.as_ref())`；再对相应方法调用只看参数的规则 `settings_update`、`set_config`、`git_revert`；`policy/describe` 的处理器按 `Confirmable` 的种类查好 `Facts` 再调用 `describe`。`Denial`、`DescribeError` 都实现了 `Into<RpcError>` | T031（已写入 brief） |
| T042 | 在检查完 pending、摘要和 `actionable_by` 之后调用 `policy::respond`；收到请求时用 `Clock` 记下 `entered`（`MonoTime`），并传入会话当前的档位。另外请核对：5.3.6 写"所有选项都是 privileged 时 `actionable_by` 只有 `desktop_ui`"，会把 test_harness 排除在外，而权限矩阵里 test_harness 对特权选项是 ✓。请提出方案，由编排者裁定 | T031（已写入 brief） |
| T043、T044 | `session/create`、`set_config` 的档位：用能力里的 `risk` 换算成 `Tier` 后调用 `policy::tier_change`，映射里没有的模式传 `FullAccess`；`config_unconfirmed` 的判定用 `policy::ceiling`；Attach 之前调用 `policy::attach`（信任门控） | T031（已写入 T043 的 brief） |
| T077 | 原生确认框按 5.3.11 末尾"T031 实现时的裁定"里各 action 的 `details` 键取内容，文案由界面按语义码组织 | T031 |
| T071 | 从 `@cupper/protocol` 用 `zEvent`、`zEventBody`、`isKnownEvent`、`KnownEvent`；`isKnownEvent` 为 false 时记日志并显示占位。生成的 `zStreamEventsParams`、`zStreamSnapshotParams`、`zHistoryPageResult` 对数组里每个事件做完整校验，一个没有合法外壳的事件（例如缺 `seq`）会让整条消息失败，而 Rust 的 `Raw<T>` 只让那一条失败：先把 `events`、`ops` 当作 `unknown[]` 取出，再逐个 `zEvent.safeParse`。`protocol/fixtures/methods.json` 可以直接给 mock 宿主用 | T015（已写入 brief） |
| T072 | `zIndexView`、`zSessionView` 及类型已生成；两端视图按深度相等比较。`Item` 等嵌套开放联合在 TS 里无法按标签收窄（`UnknownMember` 带索引签名），需要时生成类似 `isKnownEvent` 的守卫（也可在 T082 定） | T015（已写入 brief） |
| T016 | 冻结前要定：可选字段在 TS 里是 `T \| null`（zod `.nullish()`），宿主从不发 `null`，是否统一改成只是缺省；`Settings.cli_path` 标必填、`SettingsPatch` 的非空键不接受 `null`，在 Rust 里改，两端一起变；`v1.json` 是加了缩进的 JCS，方法目录在 `x-methods` 的三张表里；`protocol` 作业还没有对导出文件单独做整数安全检查（5.1.6），导出时超界会直接失败 | T015 |
| T006 | npm 依赖闭包里有 `argparse@2.0.1`（Python-2.0，不在 allowlist），经 hey-api → `js-yaml` 和 electron-builder → `js-yaml` 引入，只在开发时使用。许可证门禁按生产依赖闭包计算时不会算进去；如果要连开发依赖一起检查，需要把它列为例外 | T015 |
| 所有改协议类型的任务 | 改了 `cupper-protocol` 的类型就要跑 `pnpm gen` 并提交生成物（`protocol/schema/v1.json`、`packages/protocol/src/gen/`），`xtask ci protocol` 会卡住漏提交的情况 | T015 |
| T005 | 自用构建的 CLI：`CUPPER_HOST_CDHASH=<宿主 cdhash，40 位小写十六进制> cargo build -p cupper-cli --release --features self-use`。变量缺失或格式不对会让编译失败 | T045 |
| T002 | 新增用例编号 UNIT-cli-client（T045，握手与请求关联），计划 v1.3.6 已登记；登记 `tests/ids.toml` 时一并收入 | T045（已写入 brief） |
| T039 | CLI 侧的接入点：`ClientAuth` 加 `Harness` 变体；`ClientAuth::credentials(&hello, &host)` 已经拿到 nonce 和固定令牌；在 `Client::handshake` 里 `check.accept` 的旁边校验 `host_proof`（详见 `docs/tasks/T045-cli-client-mirror/notes.md` 末尾）。CLI 收到 fd 时断开的用例已由 T045 补上（`unit_cli_client_passed_fds_or_another_writer_end_the_connection`） | T045（已写入 brief） |
| T180 | 设置宿主与 UI 的签名要求，都要带 Team ID：CLI 的 `cupper_cli::verify::HOST_REQUIREMENT`，以及网关的 `ui_requirement`（T030）。在那之前，发布版 CLI 不接受任何宿主，宿主的 `codesign` 认证也全部失败 | T045、T030 |
| T005 | 产物核对：CLI 的 `cupper --build-info` 输出里的 `host_cdhash` 要等于同一产物里宿主的 cdhash | T045 |
| T046 | 客户端读循环每次读到数据都先查对端令牌；宿主写完就关闭连接时，这批数据会被丢弃，关闭原因是 `Refused(PeerExited)` 而不是 `Eof`（安全上正确）。报告"宿主已退出"时，把两者当作同一种情况 | T049 |
| T039 合并之后改 `cupper-cli` 测试的任务 | `mirror.rs` 测试里的进程内假宿主，与 `client.rs` 测试里的 `FakeHost` 用途相同，可以合并成一个共用的测试辅助 | T049 |
| T033 | 账本接线：启动时在 host.db 连接上调用 `Ledger::load`，并 `spawn(ledger.clone().sweeper())`。读循环对命令类请求调用 `begin`，传入原始 params 的 `Value`：`Run(ticket)` 时把 ticket 随命令交给执行者；`Replay` 时直接回复；`Wait(w)` 时另起 task，等 `w.outcome()` 后回复。`LedgerError` 可以直接转成 `RpcError`。`test/run_maintenance` 调用 `Ledger::sweep()` | T032（已写入 brief） |
| T043、git 执行者（T061 等） | 受理后调用 `ticket.complete(Ok(result))`，把 `Box::new(completion.row())` 放进 `Batch::with`；`append` 成功后调用 `completion.committed()`，失败时丢弃 `completion`。改动用户仓库的 git 命令，在执行 git 之前先 `mark_external().await`。前置条件不满足时用 `reject(err)` | T032（已写入 T043 的 brief） |
| T016 | 账本回放的结果带 `replayed: true`（5.2.5），协议 schema 里还没有声明。请在冻结前定下声明方式 | T032 审阅 |
| T037 | 组装存储：在静止屏障里登记 `store::RAW_WRITER`、`store::retention::MAINTENANCE`。启动顺序：`open_writer(raw.db, Database::Raw)` → `prepare_raw` → `RawWriter::spawn(conn, &quiescence)`，再把 `raw.sink()` 交给 T055；`BlobStore::new(data_dir, writer.clone(), clock)`；`Maintenance::new(writer, raw, blobs, data_dir.join("backups"), clock, durations).spawn(&quiescence)`，放在"按 `adapters` 表清理残留"之后。有序退出时先停清理任务，再 `RawWriterThread::stop()`。清理失败目前直接吞掉，有了 host.log 之后要记下；`RawSink::dropped()` 还没有地方展示 | T027（已写入 brief） |
| T033 | blob 传输：`BlobStore::put(data, media_type, Some(hash))`，`Mismatch` 对应 `conflict`，`TooLarge` 对应 `too_large`；客户端声明的 `size` 由 T033 自己和 `data.len()` 比对。`get` 每次都读整个文件并核对哈希，`offset`/`length` 由 T033 截取。图片的 10 MiB 限制（按 agent）不在 `BlobStore` 里 | T027（已写入 brief） |
| T043、T055 | 事件引用 blob 时，在同一个写操作里调用 `store::blobs::add_ref`，返回 `false` 时让客户端重新上传。T055 用 `RawWriter::sink()` 构造 `Tap` | T027（已写入 T043 的 brief） |
| T047 | `test/run_maintenance` 的 `raw_frames`、`audit`、`orphan_blobs` 对应 `Maintenance::run(Cleanup::…)`，`ledger` 对应 T032 的 `Ledger::sweep()`。静止屏障要把 `store_raw_writer`、`store_maintenance` 两类参与者算进去 | T027 |
| T043 | 宿主重启后，用 `InteractionBroker` 的 `Input::Ended` 载入 `interactions` 表里本会话已经结束的请求；写 `interaction.requested` 之前先刷出 normalizer 缓冲的文本，并通知 `IndexActor`；`Respond.tier` 传会话当前的档位；CLI、test_harness 完成的回答要写审计 `interaction.responded`（也涉及 T037） | T042（已写入 T043 的 brief） |
| T053、T055 | 按 `AskRequest` 交出 agent 的请求：权限请求用 `Classified`，form 模式的 elicitation 用 `Form`（原样带 JSON schema）。`DriverEvent::Ask` 还没有 `toolCallId`，actor 填 `item_id` 需要它，由提取层补上。撤回之后交互代理仍会发一次 `Answer { Cancelled }`，运行时据此放掉应答句柄 | T042 |
| T110 | Codex 用 `_meta.codex.isOther` 和 `<id>_note` 表达"其他"和备注，目前按 JSON Schema 类型映射成两个字段；要不要合成一题，由本任务决定 | T042 |
| T016 | `interaction/respond` 的 `x-errors` 里没有 `not_found`，会话里不存在的请求 ID 目前回 `invalid_params`，而 `interaction/get` 回 `not_found`。冻结前补上，并跑 `pnpm gen` | T042 审阅 |
| T073 | 网关夹具 `cupper-gateway-fixture` 要用 `--features dev-auth,test-harness` 构建，必须给 `--data-dir`，stdin 读启动载荷；监听后 stdout 输出 `{"socket": ...}`，审计按 JSON 行写到 stderr。`dev_key` 只接受拉起夹具的那个进程（Node 测试进程）；`host_proof` 按夹具的 pid 校验。证明的已知答案见 UNIT-proof-binding（`cupper-protocol::proof`） | T039（已写入 brief） |
| T046 | `--harness-key-fd` 的参数解析接 `cupper_cli::harness::HarnessKey::read_fd`；输出里"宿主未经签名校验"的标记用 `HostCheck::is_weak(&auth)`（e2e 构建里带了 harness 密钥就不算弱） | T039 |
| T037 | `main` 里 `startup::launcher()` 返回的拉起方 ID，要放进 `GatewayConfig::dev_key`（`DevKey { key, launcher }`）。新增关闭原因 `HandshakeTimeout`，审计记为 `handshake_timeout`。夹具的审计行目前是 `Debug` 文本，正式的审计格式归本任务 | T039（已写入 brief） |
| T047 | 每条还没 `initialize` 的连接持有一个握手期限计时器（`gateway_handshake`，10 s），静止屏障要把它算进去 | T039 |
| T043 | 合并器只写内联文本；工具输出超过内联上限时转 blob（`BlobStore`、`add_ref`）、标 `truncated`、每秒更新尾部，由 actor 做（5.3.11 末尾"T041 实现时的裁定"）。收到回合机的动作后，用 `TurnSignal::of(&Action)` 转发给合并器和裁判 | T041（已写入 T043 的 brief） |
| T053 | `NormUpdate::ToolStart` 要给出 `cwd`：没有它，合并器按 `missing_required` 把 `execute` 类工具的 `extraction` 定为 `partial` | T041 |
| T076 | windowBridge 的白名单要按协议常量生成，但生成的 `@cupper/protocol` 目前没有方法目录（方法名 → scope、幂等）：扩展 `tools/gen-protocol-ts` 输出它（`v1.json` 的 `x-methods` 里有）。本地拒绝和重连顺序按 5.7.6 末尾"T071 开工前的裁定"，状态和数据走同一条有序通道，并通过 HostPort 合同测试 | T071 |
| T075 | `ConnectionState` 的 `failed.reason` 是开放枚举，宿主拉起失败（退出码 4–6、宿主校验失败）的取值由本任务追加 | T071 |
| T080 | 5.7.6 的"生产构建只校验非 delta 事件"还没做，T071 目前校验全部事件 | T071 |
| T058 | 合并器已经能暂存 Ending 期间的更新，也能为被放弃的工具建晚到条目。组合测试环境里：回合机的每个动作先转 `TurnSignal` 给合并器，再给裁判；裁判的 `Start`、`End`、`Hold` 在回合机当前这一步之后，作为输入回送给回合机 | T041（已写入 brief） |
| T043 | 用 `TurnSignal::of` 把回合机的动作转给合并器；`turn.completed` 由合并器写出，actor 收到 `Action::Complete` 时自己不写。交互代理写 `interaction.requested` 之前，先给合并器送 `Input::Interject`（刷出缓冲的文本并结束活动组）。构造合并器时传入能力里的 `interrupt_stops_commands` | T041（已写入 T043 的 brief） |
| T075 | host-client 的握手没有内置超时：调用方传 `AbortSignal.timeout(...)`，套接字关闭时也要同样 abort。mock 宿主的 `conn.port()` 是不经检查的 HostPort，可以直接给 control 连接用 | T071 |
| T043 | 实现 `SessionPort`：`deliver(Call)` 按 `params.session_id` 投进会话邮箱，只排队不等待，返回回复的 future；用 `Call.ticket` 结束命令（T032 的账本接口）。实现 `facts(&Confirmable)`，给出 `mode_tier`、`option`、`running_turns`。登记 M1a 的会话方法时用 `Routes::sessions(port)`，接上后从 UNIT-unsupported-set 的允许表里删掉相应行（详见 `docs/tasks/T033-gateway-dispatch/notes.md`"给其他任务的接入点"） | T033（已写入 T043 的 brief） |
| T044 | 登记 `session/confirm_config`，并删掉 UNIT-unsupported-set 允许表里标为 T044 的那一行 | T033 |
| T034 | 实现 `IndexPort`（`deliver`、`opened(Opened)`、`closed(&ConnId)`）；`ClientInfo.auth` 由 `Opened` 里的 principal 种类和 `weak_auth` 推出。客户端通知目前不计入在途、每条另起一个 task，没有上限；加入代价大的通知时要定上限 | T033 |
| T036 | 登记 `settings/*`（`Routes` 的 `add::<M>`），`settings_update` 规则网关已经执行过；接上后从 UNIT-unsupported-set 的允许表里删掉相应行 | T033 |
| T037 | 组装：`Routes::start(conn, writer, clock, durations, sessions, index)` → `(routes, sweeper)`，再 `Gateway::new(config, clock, durations, routes)`；有序退出时 abort 清理任务。登记 `host/status`、`host/exit` | T033（已写入 brief） |
| T038 | 撤销提权挂在 `IndexPort::closed` 上。`Dispatch` 里的 principal 在 `initialize` 时定下、之后不变；授予提权需要让这条连接的 scope 能更新，要改这里 | T033 |
| T047 | `test/run_maintenance` 要用账本：给 `Routes` 加一个公开的账本访问函数（现在是 `pub(super)`）。账本清理任务、各连接等待回复的 task 还没有登记进静止屏障 | T033 |
| T019、T029 | 接入点见 `docs/tasks/T033-gateway-dispatch/notes.md`"给 T019、T029 的接入点"：`stream/*`、`history/page` 登记为 `Target::Streams`，`blob/*` 为 `Target::Blobs`；订阅表和上传缓冲放进每条连接的 `Dispatch`，断开时自动清理；`HOST_FEATURES` 目前为空，由它们补上 | T033（已写入 T019 的 brief） |
| 所有新增或调整用例编号的任务 | 编号登记在 `tests/ids.toml`（T002）：每个任务只在自己那一段里追加，写明里程碑、套件、状态；`xtask ci testdata-scan` 会把文档里出现但没登记的编号报出来。`node tools/trace-ids/src/main.ts status --summary <摘要>` 可以看哪些编号已完成、待完成 | T002 |
| T188 | 给 `tools/trace-ids` 加读兼容报告的入口（`--compat <报告>`）；在那之前 COMPAT 编号一直是待完成（原交 T064，v1.3.55 拆给 T188） | T002（已写入 brief） |
| T173 | MAN 证据的格式先定为 `tests/ids.toml` 里的 `[[evidence]]`（id、date、record、versions），可以在此基础上改 | T002 |
| 改用 cargo-nextest 的任务 | 替换 `crates/xtask/src/ci/ctx.rs` 的 `parse_libtest`，并考虑在摘要里记"skip"；摘要格式不变时 trace-ids 不用改（它的 JUnit 读取是按格式手写的样例测试的，接上时用真实报告再核对一次） | T002 |
| T053、T110 | "其余录制"的 GOLD-A 编号，由各自在 `tests/ids.toml` 里追加 | T002 |
| T087 | 接入点（详见 `docs/tasks/T071-host-client-conn/notes.md`"给 T087 的接入点"）：所有 `stream/*` 通知按顺序经 `onNotification` 到达；`stream/subscribe` 必须用 `client.call(method, params, done)`，`done` 同步执行，排在同一数据块里随后的通知之前（用 `request()` 的 Promise 的话，后面的事件已经分发出去了）；`onState` 的 `reconnecting`、`failed`、`closed` 表示"停下并保留游标"，`ready` 带新的握手结果（含 `data_epoch`），这时重新订阅。三种计数放在订阅层 | T071（已写入 brief） |
| T076 | 对 windowBridge 运行 `hostPortContract`，setup 要提供 `shell`、`host`（一个 `MockHost`）和 `settle`；合同里的请求用例用 `host/status`（scope `read`），不依赖 `ping` 在不在白名单里；握手结果来自 host-client 的 `handshake()`，要去掉 `host_proof`；状态和数据走同一条有序通道 | T071 |
| T08x 浏览器截图测试 | `MockHost` 的夹具由构造函数传入：用 Vite 导入 `protocol/fixtures/methods.json` 再传进去 | T071 |
| T080、T145 | `HostClient` 还没有 `notify`（例如 `client/presence`）和 `sendBlob`，需要时加到 `client.ts` | T071 |
| T043 | 照 `orchestrator/session_props.rs` 的组合核心 `Session` 接线：驱动事件依次交给裁判、回合机，如果是更新再交给合并器；回合机的 `TurnSignal` 立即交给合并器，这一步结束后再交给裁判，裁判的 `Start`、`End`、`Hold` 作为回合机的输入回送；`turn.completed` 由合并器写。请求先送裁判（`Asked`），再送合并器（`Interject`），每次解决之后送 `Resolved`。收到 `Asked` 而开出的自发回合，要在交互代理写出请求之前开好，请求才带上 `turn_id`。回合收尾期间到达的请求，放进随后开的自发回合，裁判会等它解决后才结束这个回合，接线时请确认 | T058（已写入 T043 的 brief） |
| T110 | 裁判把 thread 状态当作"最近一次报告"：只有 Codex 会报告，Claude 一直是未知，只按静默规则判断。Codex 提取层提供 `threadStatus` 时，要有 idle。工具结束之后又来不带结束状态的更新时，裁判会把它重新记为运行中，回合不会因静默而结束，录制金样例时留意 | T058 |
| T075 | 原生扩展与 host-client 的胶水：`auth: (hello) => conn.auth(hello.nonce)`（返回协议 `Auth` 的 `dev_key` 分支）；`verifyHostProof: (proof) => conn.verifyHostProof(proof)`，信道由扩展记着；`host_proof` 缺失时 host-client 不调用 `verifyHostProof`、直接报 `host_unverified`，这条路径扩展不会自己断开，T075 要在握手失败时调用 `conn.close()`。`connectHost` 抛出的 `code`（`peer_exited`、`peer_execed`、`code_changed`、`unverifiable`、`not_the_host`、`other_user`、`no_requirement`、`connect_failed`）并入 `ConnectionState` 的 `failed.reason`。宿主被杀后重新拉起时复用同一个 `HostKey`。用 `buildInfo()` 核对扩展的构建种类与 App 的 flavor 一致。宿主写完立刻退出时令牌读不到，那批数据会丢弃并记为 `token_changed`（例如最后一条 `host/shutdown`），UI 不要把 `token_changed` 一律当作攻击提示 | T073 |
| T076 | 原生扩展用 socketpair 转发，每条连接两个线程加一次拷贝；macOS 上 socketpair 和本机套接字的默认缓冲区只有 8 KB，转发会分成很多小块。测量闸门不达标时，在 `cupper-sys` 里加 `setsockopt(SO_SNDBUF/SO_RCVBUF)` | T073 |
| T161 | `release-config` 可以断言扩展发布构建的 `buildInfo().features` 为空 | T073 |
| 下一个改 Rust 依赖的任务 | `getrandom 0.4.3` 已有三个使用方（cupper-host、cupper-cli、cupper-desktop-native），按惯例提到 `[workspace.dependencies]` | T073 |
| T080 | `client.counters` 是全部流的计数之和，E2E 的 `window.__cupperTest` 可以直接导出。`StreamSink` 的约定见 T072 的任务书。"生产构建只校验非 delta 事件"仍未做，目前每个事件都用 zod 解析 | T087 |
| T080 | 视图：每帧调用一次 `versions.flush()`；组件用 `versions.subscribe(key, cb)` 和 `versions.of(key)` 订阅（键如 `item:<id>`、`turn:<id>`、`tail`、`session`，`view` 随每次变化）；`complete` 用来显示"加载中"，`error` 显示订阅失败 | T072 |
| T082 | 视图的 `unknown` 列表（键 `unknown`）是要显示成占位的事件体，实时到达的带着 event 和 `fallback`；占位在消息流里的位置由本任务定 | T072 |
| 改动视图规则或协议类型的任务 | 视图 JSON 变了就重新生成交叉验证的样例：`UPDATE_GOLDEN=1 cargo test -p cupper-protocol --lib view::parity`，在 PR 里审阅 `protocol/fixtures/views.json` 的差异；TS 侧的 reducer 要同步改 | T072 |
| T037 | 组装时在 `Routes::start(..)` 之后调用 `routes.streams(streams, readers)`，否则 `stream/subscribe`、`stream/unsubscribe`、`history/page` 仍回 `unsupported`。`AuditEvent::Subscribed`（Q16）只写 host.log，不写 `audit` 表 | T019 |
| T062 | `TestGateway::start` 和 `cupper-gateway-fixture` 用的是 `Routes::default()`，不提供订阅和历史；进程级用例需要订阅时，在这两个夹具上加 `Routes::streams` | T019 |
| T029 | outbox 的项现在是 `Item`（带订阅的 feed 编号），blob 分片仍走 `push_frame`；`Target::Streams` 已删除，`Target::Blobs` 还在，由本任务接上 | T019 |
| 用大合并窗口的客户端（CLI、iOS） | 合并窗口内，同一批里的响应也要等窗口结束才发（与 SP-4a 相同，最多 1 s）。桌面默认 `coalesce_ms = 0`，不受影响；要用大窗口时，改成先发窗口前的响应帧（`gateway/send.rs`） | T019 |
| 以后排查宿主内存时 | 对不存在的会话订阅或取历史，会把一条空流载入内存，直到空闲淘汰（10 分钟） | T019 |
| T058 的修复（T088 之前或同时） | 组合核心的性质测试有稳定的反例：`orchestrator::session_props::prop_autonomous_dequeue_guard_queue_waits_while_the_agent_works`，断言"Guard: after a turn ended on silence, the queue waits for 30 s of quiet"失败。种子：`cc 7390a05ebe580707816e5bab38bbf67b0501e1d444018d9994dbaf6d6edc9900 # shrinks to ops = [Send, Send, Interrupt, Text(0), Advance(25000), Advance(25000), Thread(true)]`。main 上和 T043 之后的 `SessionCore` 都能复现，先判断是裁判、组合接线还是性质本身写错了。CI 在修好之前可能随机撞上。`proptest-regressions/` 定为入库（proptest 的推荐做法），修复时把这个种子文件一并提交 | T037、T043（编排者复现）（已修复，`fix/T058-dequeue-guard`） |
| T088 | agent 发来的请求（`DriverEvent::Ask`）现在没人应答，actor 也不处理 `CancelAsks`、`Settle`；接线点是 `SessionCore` 的请求计数（`Input::Asked`、`Input::Resolved`）和 actor 的 `Action::Machine(_)` 分支。`send` 的 `reply_to` 还没写进 `SendRequest`，`queue_entries.reply_to` 列和行的写法已经有了。还要补 `interactions` 的行和 `facts.option` | T043 |
| T089 | 不在内存里的会话，`deliver` 和重启后的 `get` 现在都回 `not_found`，按需载入接在 `SessionManager::deliver` 里；排队消息到命令的对照（`command_id`）要从 `queue_entries` 恢复。载入时补写 T037 启动清理留下的 `turn.completed{interrupted}` 和交互的 `expired` 事件，并决定是否暂停队列 | T043、T037 |
| T044 | 信任状态只在 `session/create` 时判定一次，attach 和每次 prompt 之前的重扫由本任务做；`facts` 对 `session/set_config` 还没有 `mode_tier` | T043 |
| T061 | 快照端口是 `orchestrator/session/actor.rs` 里的 `skipped()`，换成真正的快照任务即可；`turns.snapshot_status` 只有一列，最后记的是结束快照的状态 | T043 |
| T028 | 发布构建的 stdio 指向 `/dev/null`（T037）之后，有两处输出要改写进 host.log：启动失败的原因；会话 actor 在视图拒收事件时写的 stderr（测试构建里是 panic） | T037、T043 |
| T059 | 在 `Host::start` 里调用 `routes.streams(streams, readers)`，删掉 `supervisor/mod.rs` 里 `streams` 上的 `#[expect(dead_code)]`（编排者指定，T019、T037 都留了这一步）。另外：空闲、孤立、待升级作为新的 `Exit` 取值接进 `Host::run`；`host/shutdown` 的 `reason` 按 `Exit` 取值（空闲 `idle`、待升级 `upgrade`）；把 `_audit` 接到写线程，`HostExiting` 记为 `host_exiting`；清理残留排在 `maintenance.spawn` 之前；`pending_upgrade` 改取自检结果 | T037、T019 |
| T055 | 会话管理器的组装（编排者指定，T037、T043 都把这一步留给了对方）：在 `Host::start` 里 `SessionManager::new(SessionDeps { streams, writer, readers, clock, ids, durations, agents, quiescence })`，`agents` 是本任务的 `AgentRuntime`；用 `routes.sessions(Arc::new(manager))` 登记，不必改用 `Routes::start`。`supervisor::Part` 的顺序：会话管理器第一，`AgentRuntime` 第二（`stop` 里按 ADR-09 关闭适配器）。`raw.sink()` 在 `Host::start` 里取。拉起适配器时用 T054 的 `CliLocator` 和环境变量规则，并把发现结果接进宿主启动和 `agents/refresh` | T037、T043、T004 |
| T054 | 适配器的环境变量规则里，禁用清单直接取 `runtime/manifest.json` 的 `env.deny`（设计 5.6.6），不另抄一份（已写入 brief） | T004 |
| T034 | 用 `routes.index(..)` 登记索引端口，作为第三个 `Part`（`stop` 里写索引流的检查点）；`clients.updated` 用 `Gateway::clients()`；启动第 7 步（加载索引流）放在绑定套接字之前 | T037 |
| T047 | `test/clock_advance` 要用的 `Quiescence` 和 `ManualClock` 现在只是 `Host::start` 里的局部值，要留在 `Host` 上 | T037 |
| T005 | `cargo xtask runtime-pack` 的暂存目录是 `target/runtime-pack/Contents/`，布局与 5.10.1 一致（`Helpers/node`、`Resources/runtime/…`），可以直接拷进 `CupperHost.app/Contents`。esbuild-wasm 整包随附，含用不到的浏览器版文件，每个文件都记了哈希；要精简由本任务定 | T004 |
| T056 | 拉起 claude-agent-acp 的打包脚本时，`PATH` 只放随包 Node 所在的目录，其余环境变量清空：esbuild-wasm 按 `node` 这个名字从 PATH 拉起服务进程，否则会找不到或用到别的 node | T004 |
| T006（license-gate） | codex-acp 内联依赖的许可证声明，按上游 v2.2.2 标签的 `package-lock.json` 生成；SP-2 的锁文件是按版本范围自己解析的，与上游实际内联的版本不同（例如 `@agentclientprotocol/sdk` 1.5.0 对 1.8.0），明细见 `docs/tasks/T004-runtime-pack/notes.md` | T004 |
| T161、T180 | 5.10.7 第 2 步的 Node GPG 核对还是手工做的，自动化需要 `gpgv` 和锁定的 Node 发布公钥环。清单里 Node 的 sha-256 和 cdhash 是 ad-hoc 签名的值，Developer ID 签名后都会变：发布流水线要按发布身份重新生成并内嵌清单，设计里要写明每种构建内嵌哪一份 | T004 |
| T188、T066 | 清单里还没有"已验证 CLI 范围"（5.10.3，由兼容报告生成）。T066 升级适配器时，重新评估静态扫描对 SDK 动态读取环境变量的覆盖（5.6.6 的已知局限） | T004 |
| T110 | 出队保护放开之后，空闲时 Codex 报 thread active 不会挡住出队：`queue_resume` 或排队消息会把 prompt 发给 thread 仍在运行的 Codex 会话。这符合 5.2.6（threadStatus 只用于显示运行状态和界定自发回合）和 T058 的决定；Claude 再工作一定有输出，输出会开自发回合挡住出队，所以只有 Codex 有这个空隙。接入 Codex 时评估是否要让空闲时的 thread active 也挡住出队 | T058 修复第 1 轮 |
| 组装 `Cupper.app` 的任务、T161 | 5.10.2 要求两个 Info.plist 都不含用途说明：`host-bundle` 只产出宿主的那一份（CLI 没有 Info.plist），`Cupper.app` 的那一份由 electron-builder 产出，要由组装它的任务断言，T161 的 `release-config` 每晚复核 | T005 |
| T075、T180 | 宿主的 cdhash 随 Info.plist、运行时文件和二进制变化，自用构建的 CLI 必须与同一次 `cargo xtask host-bundle` 产出的 `CupperHost.app` 配套分发。Developer ID 签名之后 Node 和宿主都按发布身份重签，清单里 Node 的 sha-256 和 cdhash 要重算（另见 T004 的交接）。产物在 `target/host-bundle/<kind>/`，编译中间目录是 `target/host-bundle/build/`（每个 worktree 约 1.2 GB） | T005 |
| 改 5.7.9 构建身份的任务 | IT-host-bundle-layout 直接读设计 5.7.9 的"宿主的 bundle id""CLI 的 identifier"两行与 `crates/xtask/src/host_bundle.rs` 的 `KINDS` 比对，改一边要同步另一边 | T005 |
| T055 | 接 T094 的 `driver/probe`：登录 shell 要保留的名字传内嵌清单的 `env.allow` 加放行名单；初始环境建议取宿主自己的 `HOME`、`USER`、`LOGNAME`、`SHELL`、`TMPDIR`、`LANG`、`LC_*`、`PATH`；`TestConfig.login_env` 有值时直接构造 `LoginEnv { vars, failure: None }`，不运行 shell；`SHELL` 为空时用 `LoginEnv::fallback(Failure::ShellMissing)`；`xcode-select -p` 和 `--version` 用同一份白名单环境；版本范围从内嵌清单取版本号字符串；语义码到协议字段的映射归 T055、T034 | T094 |
| 接日志的任务（T028 及以后） | `LoginEnv.vars` 的值（登录 shell 取到的环境）不能写进 host.log 或诊断包；`Debug` 已经只打印变量名，不要另外手动打印这些值 | T094 |
| T053、T067 | 录制格式库在 `cupper-protocol::recording`：读文件后用 `Recording::parse(&str)`，分段用 `segments()`（按 `instance` 切分，从 0 起，与假 agent 的 `--segment` 编号一致）；判断 note 事件用 `Body::event()`，常量 `INSTANCE_STARTED`、`INSTANCE_EXITED`。SP-1 的 48 份录制在 `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/`（18 份）、`codex-acp/2.2.2/0.160.1/`（16 份）、`codex-acp/2.2.2/0.159.2/`（14 份），`usage` 只有 `gold-a`，头行的 `scenario` 取文件名 | T052 |
| T047 | `diag/recording_export` 逐行写出用 `Header::to_line`、`Frame::to_line`（`cupper-protocol::recording`），导出前过 `tools/sanitize` | T052 |
| T055 | 接 T054 的 `driver/locate`：`Boundaries::new(各项目和工作区根目录加 worktree 根目录, system_temp_dirs($TMPDIR))`；`CliLocator::new(boundaries, 登录 shell 的 PATH, 家目录).locate(&resolve::CLAUDE, cli_path)`；`check_config_dir(值, cupper_sys::current_uid(), &boundaries)`；`adapter_env(env_rules::CLAUDE, 宿主环境, 登录 shell 的变量表, binary, config_dir, 放行名单)`；已验证范围从 `manifest::embedded().cli.claude` 取，交给 T094 的分类函数；git、git-lfs 的绝对路径来自 `CliLocator::tool`，交给 T094 的检测函数 | T054 |
| T060 | git 子进程的 PATH 直接用 `CliLocator::trusted_path()`；git 的绝对路径用 `CliLocator::tool("git")` | T054 |
| T111 | Codex 的安装布局加一个 `resolve::Layout`；`env_rules::CODEX` 的变量名已有。本机 codex 的 npm 布局已经和 SP-2 记录的不同（原生二进制在平台包 `codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex`），见 `docs/tasks/T054-cli-discovery/notes.md` | T054 |
| T066 | T054 有两处有意保守的取舍要用真实环境确认：常见位置的 `~/.npm-global/bin` 来自 npm 文档，不是实测；nvm 只看默认的 `~/.nvm`，不读 `NVM_DIR`。原生安装器的布局也没在本机核实 | T054 |
| T055 | `SessionDeps` 多了 `audit` 和 `index`：`audit` 用交给 `Gateway::new` 的同一个发送端（`supervisor` 里的 `records`）；`index` 在 T034 之前用 `NoIndex` | T088 |
| T034 | 实现 `IndexNotes`（会话在请求写出或解决之后报告待处理的个数和最早一个的类别），换掉 `NoIndex` | T088 |
| T059 | 落盘新的审计记录 `InteractionResponded`，记为 `interaction.responded`。T088 之后 `Gateway::new` 接收宿主建好的审计发送端（`supervisor` 里的 `records`），网关和会话共用；T059 把它的接收端接到写线程 | T088 |
| T089 | 宿主重启后用 `interactions.request_json`（整个 `interaction.requested`）和状态，经 `Input::Ended` 把已结束的请求交给交互代理；`interaction/get` 对淘汰出视图的请求可以改从表里读 | T088 |
| T053 | `DriverEvent::Ask` 带上 `toolCallId` 之后，在 `SessionCore::write_held` 里填上 `item_id`（T088 已合并；T053 的分支开得早，做不了的话合并时由编排者处理或交给 T095） | T088 |
| T034 | 信任扫描的 findings 由 T096 解析；T096 合并之前 `project/add`、`project/trust` 的 findings 先留空列表。findings 的 `detail` 原样输出不截断，文件多时 `project/add` 的结果可能超过 64,000 字节的帧上限，要在本任务里处理（例如分页或只给摘要） | T035 |
| T059 | T028 已合并：审计记录除了写 `audit` 表，还要按 5.8.6 同时写 host.log（`tracing` 事件，字段按 `diag` 的约定）；T059 任务书里"T028 合并之前先只写表"的过渡写法不再需要 | T028 |
| T055 | 每种 agent 开一个 `diag::stderr::AdapterLog`（`AdapterLog::open(目录, agent, clock, redactor)`），每个实例调用一次 `read(instance_id, 管道)`；每个实例在内存里保留最后 64 KB（整行、已脱敏），可直接放进 `error` 条目的错误详情。实例 ID 作为行前缀，不受长串规则约束 | T028 |
| 以后在宿主里加日志的任务 | 宿主自己的 ID 和哈希作为字段记录（字段名带 `id`、`hash`、`digest`、`blob`），不要拼进消息正文，否则 40 字符以上的会被当成长串替换；prompt、正文、命令输出、文件内容一律经 `diag::Content`（`info` 级别只记种类和长度） | T028 |
| T095 | `driver/claude/ask.rs` 的 `option()` 是换成按工具区分的对照表的位置；换表后 s04、s04b、s05 等的金样例用 `UPDATE_GOLDEN=1` 重新生成并评审。`capabilities.rs` 里 `permission_mode` 每个取值都标 `full_access` 是档位表之前的过渡写法。`SessionCore` 对 `DriverEvent::Ask` 的匹配用 `..` 暂时忽略了 `tool_call_id`（T053 合并时），填 `item_id` 由本任务接上（已写入 brief） | T053、T088 |
| T055 | 接 T053 的提取层：每个会话建 `ClaudeSession::new(cwd, config)`；`AskId`、`PromptTag` 由运行时分配；`CommandsChanged` 按 5.6.2 去重（金样例里每次 `available_commands_update` 都是完整列表）；`capabilities()` 要从 CLI 定位那边拿到 `Versions`（已写入 brief） | T053 |
| T066 | 宿主不声明 `session.compaction` 时适配器还推不推 `compaction_update`，录制回答不了（SP-1 的客户端声明了它），用兼容剧本核对；提取层照常处理这类更新 | T053 |
| T098 | 带防护的 `RepoTier::run` 放在 `git/` 下：读配置用 `GitCmd::repo(RepoOp::ConfigList)`；T097 算出的覆盖经 `.config(键, 值)` 加进命令；运行用 `tier.prepare(cmd).run(clock, durations)`；指纹变化时按 `Prepared::read_only` 决定丢弃结果还是照常返回。T097 的接口：`parse(输出, 运行目录)` → `check_sources(&entries, &Bounds { git_dir, common_dir, writable })` → `overrides(&entries, &Setup { hooks, lfs, patch })`，`global` 放在子命令之前、`command` 放在子命令之后；覆盖后失败时只有 `required` 不为空才判 `unsafe_repo_config`；调用前后各取一次 `Fingerprint::take(&Layout::read(工作区)?, &entries)`，`changed` 就是 `repo_config_changed` 的 `paths`。`core.hooksPath` 以 `~/` 开头时 T097 没有展开（纯函数不读环境），需要的话由本任务传入展开后的路径 | T060、T097 |
| T061 | 在 `SnapshotOp` 里加快照档命令，生成补丁的要带 `--no-ext-diff --no-textconv`；在 `SnapshotTier::prepare` 里补 `GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`、`GIT_CONFIG_NOSYSTEM`、`GIT_CONFIG_GLOBAL`，并决定是否带 `HOME`；快照用 `GIT_SNAPSHOT` 超时 | T060 |
| T123 | 用户档加一个 `User` 标记和 `UserTier`：不加 hooks 的覆盖，带 `HOME`；`push` 用 `GIT_PUSH` 超时 | T060 |
| T096 | 从 `trust::Scan.files` 的 `path`、`content` 解析 findings（扫描已保留完整内容，不必再读文件）；`Finding.target`、`outside_workspace` 直接取 `ScannedFile` 上的字段 | T035 |
| T034 | 信任扫描放进阻塞线程池（模块是同步的）；错误映射：`ScanError` → `unavailable/scan_failed`，`DigestMismatch` → `conflict/digest_mismatch`；写库用 `Manifest::to_json`、`TrustedBy::as_str`；findings 在 T096 之前回空列表 | T035 |
| T044 | 每次 Attach、每次发 prompt 之前、每个回合结束时调用 `trust::scan` 和 `Trust::rescanned`；git 根目录经 T098 的仓库档取得 | T035 |
| T007 | `ControlConnection.client` 是 control 连接上的 `HostClient`，`host/status`、`host/exit` 直接用它的 `request`、`command`；退出确认挂在现有的 `will-quit` 之前；"无法验证宿主"对应的失败码是 `unauthenticated` 或 `host_unverified`（新的 UI 实例拿新 key 去连还在运行的旧宿主时出现）。启动等待和环境变量白名单的两处改动见 T007 的任务书 | T075 |
| T076 | 数据连接用同一个 launcher 打开：`open({role: "window", features: …})`；`SocketPort.ack` 现在是空操作，按窗口的流控由本任务做 | T075 |
| T090、T016 及打包 App 的任务 | `build:dev`、`build:e2e`、`test:launch` 都需要本机先跑过一次 `cargo xtask runtime-pack`（`host-bundle` 要用它的暂存目录）；原生扩展在 `Resources/native/`，宿主用 `extraFiles` 放进 `Helpers/` 并加 `signIgnore` | T075 |
| T044 | 档位用 `driver::claude::capabilities::tier()`；能力里 `permission_mode` 各取值的 `risk` 已按档位表给出，表外的模式是 `full_access` | T095 |
| 显示工具审批的界面任务 | T095 只填了 `interaction.requested.item_id`，`permission.tool.item_id` 仍为空；界面需要时另补 | T095 |
| T034 | T096 已合并：`project/add`、`project/trust_info` 把 `scan.findings()` 填进 `TrustScan.findings`（之前的"先回空列表"不再需要）。`.codex` 按叶子配置项列，文件多时 findings 很多，与 `detail` 不截断一起由本任务的帧上限处理覆盖 | T096 |
| T077 | preload 的 `requestPrivileged` 在 T076 里先返回 `{status: "failed"}`，由本任务接上；windowBridge 照常放行 `interaction/respond`、`git/revert`，真实输入检查由本任务加 | T076 |
| 更新器、深链接的任务 | preload 的 `updates`、`onNavigate` 在 T076 里只注册了监听，主进程还没有发送方，由这些任务接上 | T076 |
| T034、T044 | 仓库档调用写成 `tool.repo_tier(仓库).run(clock, durations, 全部项目和工作区根目录, GitCmd::repo(op).args(..))`；取 git 根目录用 `RepoOp::RevParse` 加 `--show-toplevel`；错误经 `GitError::code()`、`reason()` 转成协议错误；`notice()` 和 `Done::notice` 由调用方写进会话流，路径是绝对的 realpath，写进会话流时是否改成相对仓库的路径由调用方定 | T098 |
| `git/status` 等频繁走仓库档的任务 | 指纹每次要遍历整个工作区找 `.gitattributes`（合成仓库 5,000 个目录约 70 ms，带大 `node_modules` 的仓库到一秒级）。上线前缓存 `.gitattributes` 清单（按目录 mtime 失效），或只扫 index 里出现的目录 | T098 |
| 以后整理 git 模块时 | T097 的 `overrides.global` 里也带 `--no-pager` 和两个无条件的 `-c`，与 T060 档位固定的参数重复；无害（档位固定的仍排在最后），去重时在 `overrides` 里拿掉这三项 | T098 |
| T090 | E2E-quit-confirm：真实地弹出退出确认，需要一个运行中的回合或待处理的请求（例如用假 agent），再在主进程里替换 `dialog.showMessageBox`；关机路径可以经 `powerMonitor` 的 `shutdown` 事件触发。空闲时退出 App 不发 `host/exit`，宿主按空闲规则自行退出（编排者确认） | T007 |
| 更新流程的任务 | `before-quit-for-update` 现在只挂好了跳过退出确认的钩子；实现更新流程时确认它确实经 Electron 的 `autoUpdater` 发出这个事件 | T007 |
| 做"需要重启宿主"页面的界面任务 | 重启宿主的确认目前由主进程直接弹原生框；界面做页面时改为经 ShellAPI 触发 `startControl` 返回的 `restart` | T007 |
| T077 | 真实输入检查加在 `apps/desktop/src/main/bridge/window-bridge.ts` 的 `#send` 里，白名单检查之后、转发之前；`interaction/respond`、`git/revert` 现在照常放行。preload 的 `requestPrivileged` 现在返回 `{status: "failed", error: "unsupported"}` | T076 |
| T078 | 预测量脚本可以直接改成真实环境的测量：`docs/tasks/T076-desktop-window-bridge/premeasure.test.ts`，一次性的 vitest 配置在同一目录；e2e 断连钩子是 `__cupperE2E.disconnectWindow(webContentsId)`；预测量的数字和局限见该任务的 `notes.md` | T076 |
| T160 | 自用构建跑不起来：`vite build --mode local` 被 vite 拒绝（"local" 与 `.env.local` 的后缀冲突）。换模式名，或改由别的方式传入 flavor | T076 |
| 深链接、通知点击、更新器的任务 | preload 的 `onNavigate` 在 `shell:navigate` 通道上收，`updates.onState` 在 `updates:state` 通道上收，`installNow` 待接；主进程目前没有发送方 | T076 |
| T034 | 发现链已由 T055 写成 `supervisor::discover(login_env, clock, durations, reader) -> rusqlite::Result<Discovery>`，返回 `Discovery { host, login, locator }`；`GitTool` 从同一个 `locator` 取（`tool("git")`、`trusted_path()`），作为 `Discovery` 的新字段加进去，不另起一套。`CliLocator` 有 `boundaries()`，边界目前只有项目路径 | T055 |
| T044 | `AgentRuntime` 现在丢弃 `SetConfig` 命令；越权改回时要用它，接上 | T055 |
| T056 | `launch::production` 现在从 `<数据目录>/runtime/node`（对应清单 `files` 的 `Helpers/node`）和 `<数据目录>/runtime/bundle-claude-acp.mjs`（对应 `Resources/runtime/bundle-claude-acp.mjs`）拉起；运行时目录的布局定下后改这两个路径 | T055 |
| T140 | `AgentRuntime` 现在丢弃 `Steer` 命令 | T055 |
| T036 | 设置里的 `agents.env_passthrough` 还没接，`adapter_env` 拿到的放行名单是空的 | T055 |
| T110（第二种 agent） | 第二种 agent 接进来时需要一个自己的 `ActorKind` | T055 |
| T100 | 在 `Host::start` 里调用 `routes.presence(..)`（`session/mark_read`、`client/presence` 在那之前对真实宿主回 `unsupported`）；状态计算接在 `IndexActor::pending` 旁边；对账时按行补出的会话摘要只是起点 | T034 |
| T059 | `HostUpdated.pending_upgrade` 改为取自检的结果 | T034 |
| 用 git 的任务（T061、T109 等） | git 从 `Discovery::git`（`Option<GitTool>`）取；找不到 git 时为 `None` | T034 |
| T059（宿主启动接线） | 运行时安装器的后台校验：`Installer::new(runtime::plans(manifest::embedded()), Box::new(BundleDir::of_this_host()?), runtime::runtime_dir(&data_dir), in_use)`，放进 `spawn_blocking(move || installer.run(|s| tx.send_replace(s.clone())))`，状态写进索引流的 `agents`；要在宿主的 cdhash 自检之后才开始；`Failed(BundleChanged)` 时转入待升级；`in_use` 启动时为空 | T056 |
| T111 | codex-acp 的文件在 `runtime::plan::split` 里加 | T056 |
| T109 | 闸门用 `turn::Input::Gate(Some(PauseReason::ProjectConfigChanged))` 关、`Gate(None)` 开；闸门同一时刻只有一个原因，`config_unconfirmed` 和 `project_config_changed` 可能同时出现，要改成原因的集合 | T044 |
| T089 | 设定者的完整 `Principal` 只在内存里，库里只有 `mode_set_by` 的 ID；恢复会话时按 ID 找回 principal 的类型和 scope，或者把恢复的会话一律视为由创建者设定 | T044 |
| T059 | `AuditRecord.conn` 改成了 `Option<ConnId>`：会话自己发起的审计（例如 `config.unconfirmed`）没有连接；落盘时处理 `None` | T044 |
| T008、驱动层的后续任务 | 越权改回经 `AgentPort::send(DriverCommand::SetConfig{..})` 发出，结果要以带全部选项的 `DriverEvent::ConfigChanged` 回到会话；`AgentPort::send` 的文档目前只列了 Prompt、Cancel、Close、Answer，补上 `SetConfig`，并确认 `AgentRuntime` 不再丢弃它（T055 交接时还在丢弃） | T044 |
| T100 | `AgentNotes::agent(AgentStatus)` 是索引接 `agents` 状态的入口；宿主组装时现在传 `NoAgentNotes`，换成索引的实现（`IndexHandle` 实现它） | T009 |
| 以后需要刷新 git 的任务 | `agents/refresh` 重新发现 CLI，但不刷新索引手里的 `GitTool`（启动时由 `git_tool` 从同一次发现得出）；用户装了或换了 git 之后要重启宿主才生效 | T009 合并 |
| 新增 actor 或后台 task 的任务（T059、T100、T116 等） | 静止屏障（T047）：新的 actor 种类要在 `Host::start` 里登记，否则 IT-clock-advance 的种类枚举用例会失败；等待点包进 `Participant::idle(fut)`；脱离发起方的 task 持有 `Hold`，把结果交出去之后再释放。等待外部 I/O（适配器输出、子进程）不在屏障范围内 | T047 |
| T117 | `test/seed` 要求先有一个项目；seed 返回时索引快照里已经有这些会话 | T047 |
| T111 | `agents/restart` 调 `AgentRuntime::clear_crashed()`；Codex 的回收（超过 2 GiB，或 30 分钟没有回合）用 `AgentRuntime::resources()`（`watch` 接收端） | T008 |
| T059 | 宿主启动时的残留清理读 `adapters.groups`（`[[pgid, start_us], …]`，T008 加的列）以及适配器自己的进程组 | T008 |
| 会话侧显示错误详情的任务（MS-5） | `DriverEvent::AdapterExited` 已带 `stderr`（每个实例最后 64 KB），还没有写进 `error` 条目的详情 | T008 |
| M1b 的信任确认界面（加"不加载项目配置继续"的协议方法） | 协议方法直接调宿主内部入口 `SessionManager::ignore_project_config(&SessionId)`：会话空闲时关闭 agent 会话，下一个回合以 `Ignore` resume，并去掉 `project_config_changed` 这个闸门原因。界面区分两种 notice：`notice{project_config_changed}` 没有 `params.reason` 表示项目配置的内容变了；有 `reason` 表示重扫失败（`workspace_missing`、`project_missing`、仓库档错误的 reason、`ScanError::code()`），按不可信处理 | T109 |
| T089 | 从库恢复会话时：信任闸门的原因和"想加载项目配置"的标志只在内存里，可以从 `session.configured.project_config` 和恢复后的第一次重扫重新得出；"不加载项目配置继续"的选择（`ignore_for_good`）需要持久化，例如加一个会话行字段，否则重启后会话可能重新加载项目配置 | T109 |
| T089、T061 | `actor.rs` 的 `rescan` 里 `std::fs::canonicalize` 在 tokio 任务里同步执行；这里再加文件操作时一起挪进 `spawn_blocking` | T109 |
| T068 | IT-orch-trust-gating 只在进程内覆盖信任门控；进程级用例可以照它的 6 个场景（发 prompt 前变化、回合中变化后"继续"、未信任以 `Ignore` 运行、工作区消失、两个闸门原因的两种顺序、git 根目录的 `.claude/`）。会话夹具的项目是临时目录里真实存在的 `app/`；测试构建里有 `GitTool::with_test_env` | T109 |
| T129 | 构造 `Installer`：`plans` 用 `runtime::plans(manifest::embedded())`；`bundle` 用 `Arc::new(BundleDir::of_this_host()?)`；`dir` 用 `runtime::runtime_dir(data_dir)`；`in_use` 是运行中适配器所用的版本；`registry` 取 `test_config.runtime.registry_url`，没有时用 `npm::REGISTRY`；`proxy` 是登录 shell 的 `HTTPS_PROXY`；`durations`。在后台 `run(&clock, publish)`，状态写进索引流的 `agents`；排在宿主的 cdhash 自检（T059）之后；`Failed(BundleChanged)` 时转入待升级。接线之前，生产环境的运行时目录是空的，Claude 适配器拉起前的核对会失败，报 `RuntimeFailed`。另外 `npm::download` 用 `response.bytes()` 整个读进内存、没有大小上限，加一个上限（例如按固定的 64 MB 截断） | T115 |
| 本机单独跑 `cargo test` 的人 | `cargo clean` 之后，`driver::runtime` 的几个用例要先 `cargo build -p cupper-fake-agent`；`cargo xtask ci` 会先构建它 | T115 |
| T064 | 录制导出（`diag/recording_export`，test-harness、dev-auth 构建）：`blob/get` 有执行者（T029）之前，直接按 `blobs/sha256/<前两位>/<hash>` 读数据目录里的文件；头行的 `scenario` 先填会话 ID、`usage` 先填 `gold-b`、`replay`，入库时按剧本改写；跑 `tools/sanitize` 时，用例临时根目录的链接路径和真实路径都要作为 `--base` 传入；导出受 `max_blob_bytes` 限制；导出的 blob 没有被引用，过了宽限期会被当作孤儿清掉，要及时取走。按会话导出时没有 `sessionId` 的帧都保留，每个宿主只跑一个会话就不受影响 | T116 |
| T063 | 缺陷（T055 起）：`AdapterLog` 打开失败时 launch 不读适配器的 stderr、直接关掉管道，适配器再写 stderr 会收到 SIGPIPE，stderr 原始帧也没有了。修法：没有日志时也起一个只写原始帧的读取任务（T116 的 `notes.md`） | T116 |
| 需要等原始帧落盘的用例 | 原始帧不持有静止屏障的 `Hold`；IT-recording-export 轮询 `raw.db` 等 note 齐了再导出（T116 的 `notes.md`）。`instance_exited` 在进程退出时写，它的 `seq` 可能排在最后几行 stdout 之前 | T116 |
| T125 | 改标题和归档要通知索引：给 `orchestrator::session::Note` 加标题变体（标题、来源）；归档时状态核心删掉这一项，并写 `session.removed{archived}`（启动时的对账已经会补）。agent 起标题的任务同样走标题变体 | T100 |
| T150 | 系统通知接在 `IndexActor::step` 处理 `Action::Intent` 的地方，或者读索引流里的 `notification.intent`。意图的原因：进入 `needs_action`（待处理数从 0 变成大于 0）、回合 `completed`、回合 `failed`；`cancelled`、`interrupted` 不写；前台的 `desktop_ui` 在看这个会话时、或 100 ms 内请求已经答完时丢掉 | T100 |
| T089 | 恢复会话时，会话 actor 的 `Pending` 等通知会照常进入索引的状态核心；启动时核心已经从 `sessions` 行补入了这些会话。另有两件：① `StreamWriter::append` 遇到空的 `bodies` 直接返回，`with` 里的行写入不执行（文档里没写），给它补文档，并在 `bodies` 为空、`with` 非空时报错或照常执行行写入；② `session/get` 按行和会话视图自己算状态，规则与索引一致但实现两份，恢复时留意两边一致 | T100 |
| T046 | CLI 上报在场可以用 `Client::notify::<ClientPresence>`；索引不采信 CLI 的在场（只采信前台的 `desktop_ui`） | T100 |
| T036 | `AgentsUpdated.git` 现在为空：`GitTool` 不知道 git 和 git-lfs 的版本。设置页要显示时，把 T094 的 git 版本检测结果接进来 | T100 |
| T068 | 缺陷（T109 起）：`orchestrator/session/actor.rs` 的 `rescan` 用 `tokio::spawn` 另起任务，没有持有静止屏障的 `Hold`；任务里有数据库读取、git 子进程和 `spawn_blocking`，违反 T047 的规则（另起的 task 要持有 `Hold` 直到交回结果）。进程级信任用例里 `test/clock_advance` 可能在重扫完成前就返回。按 1.2 的上限就地修 | T100 合并时发现 |
| T080 | ui-core 的路由是一个小状态路由，形状 `Route = {page:"new", projectId?} \| {page:"session", sessionId}`，`RouterProvider` 持有、`useRouter()` 给出 `{route, navigate}`，没有 URL；布局状态在 `layout/store.ts`，偏好键 `layout.sidebarOpen`、`layout.sidebarWidth`、`layout.sidePanelOpen`、`layout.sidePanelRatio`。接管状态时：连续快速按手柄键，主进程回传的旧 prefs 值会让宽度短暂跳回，按序号丢弃过期回传。`ui-core` 禁止 import `electron`、`node:*` 的 ESLint 规则还没加 | T084 |
| T081、T086、T083 | ui-core 的插槽：`sidebarHeader`、`sidebarList`、`sidebarFooter`（T081）；`newSessionComposer`、`sessionComposer`（T086、T083）。默认的侧边栏头部是"Cupper"加宿主状态点，T081 替换。token 写法：颜色是 `light-dark()`，`data-theme` 固定主题；Tailwind `@theme static`，只有 `tokens.css` 定义过的 token 能生成工具类；用户状态（宽度等）只经 `style={{...}}` 写入 | T084 |
| T152 | 侧边面板开合的 spring 动画（0.5 s，bounce 0.1）没有做 | T084 |
| T153 | 顶栏的"打开"和"⋯"目前是占位按钮（T128 换成工具栏按钮并能挂菜单），菜单内容归本任务 | T084 |
| T106 | 侧边面板的默认宽度按 ui-spec 2.4 的公式 `max(320, min(窗口高×1.6, 主区宽−500), min(640, 主区宽−352))`，默认窗口下得 640，主区只剩 352；公式在 ui-spec 里标为【推测】，请核对 | T084 |
| T134 | 取真实输入证据用 `guardOf(event.sender)?.take()`（`apps/desktop/src/main/input/`）；证据的 `key` 为 `escape` 时由本任务拒绝，Esc 不能触发特权操作。窗口桥只对幂等方式为 `command` 的方法消耗输入，读请求和通知不消耗 | T077 |
| T078 | 窗口桥为了认出宿主的回应（`command_id` 重发表），入方向每个字节在主进程里多一次复制（`FrameDecoder` 的拼接），JSON 只在重发表非空时解析；测量主进程事件循环延迟时一并评估 | T077 |
| T090、T092 | Q26 已在 Electron 44.9.0 上复测（设计 13 的 Q26 已关闭）：Playwright 的键盘输入不触发 `before-input-event`，不会被记成真实输入；e2e 的按键要经 `webContents.sendInputEvent` 发出；`page.click()` 产生同一按钮的 `mouseDown`、`mouseUp`。复测脚本在 `docs/tasks/T077-desktop-input-privileged/q26/` | T077 |
| 作者（MAN-keyboard） | 实体键盘确认四项：Enter 批准、Esc 拒绝、数字键选择并提交、输入法选词时不触发。清单在 `docs/tasks/T077-desktop-input-privileged/notes.md`；涉及请求面板的部分要等 T083 做出面板。执行后按 `tests/ids.toml` 末尾的格式登记证据 | T077 |
| T081、T082、T083、T086 等界面任务 | 截图用例照 `tools/ui-screenshots` 的 notes"后续界面任务怎样加用例"：spec 标题以编号开头；`open(page, fixture)` 打开（夹具可设 `info` 与 mock 宿主的 `methods`，页面上有 `window.cupperShell`、`window.cupperHost`）；`pnpm ui:baseline` 生成基线，看图后连同 `meta.json` 一起提交；新文案出现字体子集外的字时 `fonts.test.ts` 报错，按 notes 重做子集。主文档的 CSP 现在在 `@cupper/shell-api` 的 `csp.ts` | T118 |
| T090 | 截图回归用 Playwright 自带的 Chromium（156），App 的 Electron 44 是 Chromium 152；如果以后要求截图引擎与 App 一致，改用 `_electron` 驱动 | T118 |
| T133 | 实现 `elevation::Requesters`（`examine(AuditToken) -> Examined`：`Clear(VerifiedRequester)` 或 `Descendant { signals }`），在 supervisor 里把 `Arc::new(FailClosed)` 换掉；`examine` 收到的是连接的固定令牌；`signals` 原样进审计。另加第二道防线：网关 `Granted::widen` 只靠索引的定时器撤销，自身不核对 `expires_at`，给网关一个 `Clock`，在 `widen` 里也按 `expires_at` 判断（索引 actor 卡住时授权不会超过 15 分钟） | T038 |
| T134、T090 | `auth/elevation_respond`：不认识的 `decision` 按拒绝，`scopes` 只取申请范围之内的；`auth/elevation_get` 对已不在未决的申请回 `not_found`；授权到期时间在 `clients.updated` 的 `elevated.expires_at`；`auth/elevate` 被拒回 `granted: []`、过期回 `timeout`、限频回 `rate_limited` 带 `retry_after_ms` | T038 |
| T059 | `conn.accepted` 审计记录里的对端可执行文件路径、签名 identifier、Team ID 还没填（`gateway/audit.rs` 开头的注释仍写"归 T038"，改掉）；T133 在 `cupper-sys` 加了按审计令牌取签名身份的函数，可以直接用。另有边界情况：`elevation.requested` 写入失败时，之后的 `elevation.resolved` 在视图里找不到申请 | T038 |
| T036 | 特权操作 `settings` 现在返回 `failed`（`unsupported`）：`settings/update` 接上后，照 `set_config` 的结构补上：`policy/describe`（`security_setting`）→ 原生确认框 → `settings/update`（`apps/desktop/src/main/privileged/actions.ts`） | T134 |
| T135 | 提权框加"申请方不是本 App 自带的命令行工具"：在 `apps/desktop/src/main/privileged/dialogs.ts` 的 `elevationBox` 里按 `VerifiedRequester.bundled_cli` 加一行醒目提示，文案放 `privileged` 命名空间；信任项目、新建会话的确认框要显示项目路径（`project/trust_info`、`policy/describe` 的 `details` 现在都只有 `project_id`） | T134 |
| 所有写中文文案的任务 | 截图夹具的中文字体是 GB2312 符号区加一级汉字的子集，T118 的 `fonts.test.ts` 检查全部 locale 文件；`⚠` 这类符号不在子集里，会让检查失败（T134 合并时遇到，改成了"【注意】"）。需要子集外的字时按 `tools/ui-screenshots` 的 notes 重做子集 | T134 合并 |
| T082、T083 | 行类型注册表在 `packages/ui-core/src/rows/registry.tsx`：`RowRegistry<R extends {kind: string}>` 的 `register`、`resolve`，`<RowView registry row>` 按 kind 画出组件；同一个 kind 注册两次抛错；不认识的 kind 只显示一句占位（不渲染行里的数据）。可以各建一个实例或共用一个，在模块加载时注册 | T128 |
| T153 | 顶栏"打开"和"⋯"已是挂着空菜单的工具栏按钮（`<Menu trigger={<ToolbarButton …/>}>`），菜单项归本任务 | T128 |
| T106、T152 | 待核对：侧边面板被自动收起时，开关仍显示为按下（跟随用户意图）；开关滑块用 `--color-surface-elevated`，暗色下偏深；tooltip 阴影在 ui-spec 3.6 有、5.7.10 没有 token | T128 |
| T135 | `VerifiedRequester.bundled_cli` 已加（T133）：提权框按它醒目标出"申请方不是本 App 自带的命令行工具"。开发和测试构建是 ad-hoc 签名，`bundled_cli` 恒为否；签名构建出来后实测一次为真的情况 | T133 |
| T090、T092 | CLI 提权的 e2e 可以照用 IT-elevate 的做法：探针 `cupper-elevate-probe`（`cupper-cli` 下，需要 `test-harness` 特性，参数 `--data-dir`、`--out`、`--setsid`，按 JSON 行写下每一步结果）；假 agent 的 `spawn <program> [args...]` 在自己的会话和进程组里拉起程序；未决和结束的申请可以从 `host.db` 里索引流的 `elevation.*` 事件轮询 | T133 |
| 写"拿锁、放开、再拿"或依赖 fd 不被复制的用例 | 同一测试进程里别的线程拉起子进程时，子进程 exec 之前持有本进程 fd 的副本，`flock` 随副本留着（带 `O_CLOEXEC` 也一样）。这类用例放进单独的测试进程：`cupper-host` 用 `gateway::testkit::alone`（环境变量 `HOST_TEST_ALONE`），`cupper-sys` 用 `testutil::alone` | T037 修复 1 |
| T081、T082、T083、T086、T137 | 状态层（T080，`packages/ui-core/src/state/`）：会话页用 `useStreams().session(id)` 拿 store，索引用 `useStreams().index`；条目用 `useEntry(view, key)`（索引的键如 `sessions`、`projects`、`elevations`、`session:<id>`、`project:<id>`、`host`、`clients`）；草稿、附件、滚动位置用 `useUi()`；查询键约定全局 `[method, …]`、会话 `["session", id, method, …]`，需要失效时在 `staleKeys` 里加一条；示例查询 `useArchivedSessions()` | T080 |
| 第一个用 `new Worker` 的任务（审查文档、Shiki 等） | Worker 的脚本 URL 必须经 `ui-core/src/security/policies.ts` 的 `workerScriptUrl(url)` 取（只放行 `app://cupper/assets/` 下的同源脚本）；不能另建 Trusted Types 策略 | T136 |
| 所有界面任务（及评审） | ESLint 安全、样式、import、文案规则已生效（T136）。JSX 文案和 `aria-label`、`title`、`alt`、`placeholder`、`label` 等属性的字面量走 `t()`，只放行不含文字字母的文本和产品名、agent 名；其他例外要带理由的行内禁用注释。静态检查拦不下的写法（`style` 的值、拼进 `className` 的 agent 文本、别处取到的 `<style>`、`window` 的别名、自定义组件用别的属性名接收文案）列在 `docs/tasks/T136-ui-security-rules/notes.md`，评审照它看 | T136 |
| 界面任务的截图用例 | 截图页的索引流夹具（T081）：mock 宿主对每个订阅回 snapshot 模式，再发一页 `stream/snapshot`，索引流的 op 来自夹具、其他流为空；会话流的夹具照此加 | T081 |
| T152 | 截图比对用 Playwright 默认的逐像素阈值 0.2，悬停底色（正文色 5.3%）这类细微差异比不出来；要不要调严，随视觉修正一并决定 | T081 |
| T153 | 侧边栏项目行悬停时的"+"旁有一个空菜单、会话行还没有菜单按钮和右键菜单，菜单项归本任务（`ui-core/src/sidebar/`） | T081 |
| 界面任务的截图用例 | 截图页的夹具 `streams`（T137，由 T081 的 `index` 推广）：按流名给 snapshot 的 op，例如 `index`、`session/s_7`；同一份夹具要被多个用例共用时放在同一个 spec 文件里（从别的 spec 文件 import 会让 Playwright 重复注册测试） | T137 |
| T106、T152 | 待核对的文案：会话页"未信任"警告框的说明在 1280 宽时折成两行；`stale` 的项目在侧边栏同样标"未信任"，要不要单独起名（例如"需重新信任"） | T137 |
| T063 | ① `tests/lifecycle.rs` 的进程级用例在 `terminate(&host)` 之前失败时，拉起的 `cupper-host` 连同假 agent 一直运行（`std::process::Child` 在 drop 时不结束子进程），本机曾积下十几个 ppid 为 1 的宿主；宿主进程也用 drop 守卫结束，做法参照 T008 修复 1 的 `Groups` 守卫（按组号加启动时间记录、drop 时只杀仍是原组的、记录写临时目录以便测试进程被杀后下次清理）。② 本地单独跑 IT-elevate 要先 `cargo build -p cupper-cli --features test-harness --bin cupper-elevate-probe`（CI 会自动做），探针缺失时让用例直接报出原因 | T008 修复 1 |
| 拉起孙进程的进程级用例 | 照 T008 修复 1：孙进程出现之后、停止之前建 `Groups` 守卫，用 `gone()`（最多 5 s）断言进程消失，靠 drop 兜底；检查残留的命令在 `docs/tasks/T008-adapter-lifecycle/notes-fix1.md` | T008 修复 1 |
| T129 | 安装器接进启动的接入点（T059）：在 `Host::start` 的 `Watch::new()` 之后启动安装器；`Failed(BundleChanged)` 时调 `Watch::bundle_changed()` 转入待升级；安装器忙闲填进 `Observed.installer_busy`（现在固定为 `false`，代码里有注释），空闲退出和待升级退出都要等它 | T059 |
| T090、T091 | 待升级退出时，中断回合的命令回复与退出赛跑，可能来不及发出；命令结果在账本里，重连后用同一个 `command_id` 重试能拿到，不要断言一定收到这条回复。空闲退出要等完全空闲 10 分钟（`supervisor_idle`，可经 `test_config.durations` 缩短），暂停中的排队消息不算活动 | T059 |
| T163 | `host.*` 的产生点：启动在 `Host::start`，退出在 `Host::run` 交出 `Exit` 的地方（空闲、孤立、待升级各有取值，host.log 已有一行 `orderly exit`），进入待升级在 `supervisor/watch` 的自检路径 | T059 |
| T055 等复用者 | 启动时的残留清理 `supervisor/reap.rs` 的 `reap()`：TERM → 按 `Clock` 每 50 ms 看一次 → 2 s 后 KILL，只发给仍是记录中那个的组；可以直接复用 | T059 |
| T164 | 长文本的接入点（T086）：`packages/ui-core/src/composer/send.ts` 的 `useSend().send(text, attachments)` 是唯一组装内容块的地方（新建会话的 `session/create.message` 与会话里的 `session/send.content` 都经过它）；预算取握手结果的 `limits.max_frame_bytes`（`useHost().handshake`），按整条请求 JSON 的 UTF-8 长度判断并给外壳留余量；组装完内容块之后、`client.command(...)` 之前上传并换成 `{type: "text_blob", blob: {hash, media_type, size}}`；上传失败走现有的失败路径（草稿放回、toast）。组件测试沿用 `composer/composer.test.tsx` 的 `setup()`，MockHost 收到的分片在 `conn.frames` | T086 |
| T083 | 请求面板替换输入框的位置：`composer/QueueList.tsx` 的 `ComposerArea` 里按有没有待处理的请求选择请求面板或 `Composer`，排队消息保持在两者上方；草稿和附件在 `useUi()` 里，切回来原样还在。Esc、Enter 的判定在 `composer/keys.ts`（纯函数） | T086 |
| T140、T141、T142、T145、T149 | 输入框的扩展点（T086）：底栏的项目放进 `composer/extensions/` 作为新文件（默认导出 `side`、`order`、`Component`）；浮层挂到 `OverlayContext.container`，打开时用 `setClose` 登记关闭函数；附件条已能显示 `image`、`file_ref`、`review_comment`，附件条的截图留给其中第一个能产生附件的任务。插话（`mode: steer`）、"已转为排队"的提示归 T140 | T086 |
| e2e（T090–T093） | 输入框的 DOM 标记：`data-composer`、`data-send-state`（`disabled`、`send`、`stop`）、`data-esc-armed` | T086 |
| T164 | host-client 的 `putBlob`（T029 没做）：用 `crypto.subtle.digest` 算 sha-256，按 `MAX_BLOB_CHUNK_BYTES` 切片，经已有的 `HostPort.sendBlob` 发出，再请求 `blob/put`。宿主端：单个 blob 上限 32 MiB、每连接合计 64 MiB；分片偏移必须接在已收字节之后，否则记为乱序（`conflict`）；分片按 `blob/put` 的 `write` scope 检查（含提权授予的） | T029 |
| 第一个在界面里读 blob 的任务（T145 等）、T064 | host-client 的 `getBlob` 没做：用 `onBlobChunk` 收集分片按 `offset` 拼起来，收齐 `blob/get` 结果里的 `chunks` 数为止（分片一定先于结果到达；空范围也有一个"最后一片"）。CLI 的 `Client` 已能收发分片（`send_blob_chunk`、`Incoming::Blob`），T064 读录制导出可以直接用 | T029 |
| T092 | 加固评估：网关的上传缓冲按哈希建条目，刚发第一片就超限或乱序的哈希会留下空条目，直到 `blob/put` 或断开；是否加"每连接未决上传数"的上限 | T029 |
| T090、T091、T092 | CLI（T046）：退出码 0 成功、1 宿主拒绝或本地错误、2 用法、3 宿主没在运行或已退出、4 宿主校验失败，`elevate` 另有 10（`granted: []`）、11（超时）、12（限频）、13（血缘）；`--json` 输出 JSON Lines，错误为 `{"error": {code, reason?, …}}`；`--json` 下 `elevate` 被拒时输出两行（原始回复加 `{"error": {"code": "denied"}}`）；`repl` 在同一条连接上执行，`--json` 下每条命令以 `{"repl": {"exit": n}}` 结束；一次性的写命令没有提权必被拒，连续写用 `repl`。手动时钟下索引按 100 ms 合并写出，要先 `test/clock_advance` 超过 100 ms 才看得到新会话。`elevate` 的非批准路径与人读格式、`sessions --watch`、`answer` 的类型转换还没有用例 | T046 |
| T092（或补单元测试的任务） | `cupper-cli` 的 `exit_code()` 映射（`elevate` 的 10–13）没有单元测试，补上 | T046 |
| 运行期间重新安装运行时的任务（M1b 起） | 安装器只在宿主启动时运行（T129），`in_use` 为空、没有运行期间的重启入口；要在运行期间重新安装时，需要把运行中适配器的版本传进 `in_use`，并加一个重新触发的入口。安装器只在 App 包里运行（`BundleDir::is_app()`），开发和测试用的 `target/debug/cupper-host` 不启动它；进程级用例经只在 test-harness 构建里生效的 `test_config.runtime.manifest` 注入测试清单 | T129 |
| 作者（MAN-keyboard） | 请求面板（T083）另加的实体键盘确认项：Enter 批准、Esc 拒绝（有特权选项时 Esc 不弹原生确认框）、数字键选择并提交（特权选项的编号弹原生确认框，效果未知选项的编号没反应）、输入法选词时按 Enter 不发送也不提交；建议一并看：打字时来请求面板不抢焦点，切回应用 1 秒内按 Enter 提示"请再按一次"。清单在 `docs/tasks/T083-ui-request-panel/notes.md`，与 T077 的那份一起执行 | T083 |
| e2e（T091–T093）、T167 | 请求面板的 DOM 标记（T083）：`data-request-panel`、`data-request-kind`、`data-ready`（待命 500 ms 结束后）、`data-activation`（`auto` 或 `manual`）；有待处理的请求时 `ComposerArea` 按创建顺序取第一个显示面板 | T083 |
| T166、T165、T146 | 消息流的行（T082）：形状以 `packages/ui-core/src/rows/stream.ts` 为准（`notes.md` 有对照表），每种行都有 `kind`、`key` 和要显示的数据，行组件是 `row` 的纯函数；登记用 `registerRow(kind, Component)`（属性 `{ row: RowOf<"kind"> }`，每种只能登记一次），并在 `rows/index.ts` 加 import；没有登记的 kind 显示占位。待 T166 登记：`agent_message`、`thought`、`tool`、`activity_group`、`notice`、`error`；T165：`request`（在它登记之前，待处理的请求条目在流里显示为占位，UI-request-panel 的基线里就是这样）；T146：`plan`、`late_tool_output`。消息流在 `app/pages.tsx` 的 `SessionPage` 里横幅之下、输入框之上，每行外层已有行间距 | T082 |
| T091（或 T090 之后的 e2e） | 消息流"插入历史后视口不跳"只在 `ScrollKeeper` 的单元测试里验证过（jsdom 没有布局），要在真实环境里确认一次 | T082 |
| 第一个在界面里读 blob 的任务 | 用户气泡里的长文本块现在只显示"长文本"，要等 host-client 的 `getBlob` | T082 |
| T163 | ① 有序退出时 `conn.closed{host_exiting}` 只进了 host.log、没进表（`Host::exit` 先停写线程再 `close_all`），加 `host.exiting` 时一并改成先关连接、排空审计写入、再停写线程；② `admit` 里的 `verify(ui_requirement)` 也在异步线程上同步调用 Security 框架，未正式签名的大程序或第一次调用会慢，检查一遍，必要时移到 `spawn_blocking`（T138 修 IT-idle-exit 时查出同类问题：`signing_identity()` 一次约 600 ms，卡住了宿主的定时器）；③ `project/add` 新路径扫描失败时还没有项目 ID，没有审计 | T138 |
| 读 `config.unconfirmed` 审计或事件的代码 | T044 的 `ConfigUnconfirmed` 现在是 `{reason, config: ConfigState}`（T138） | T138 |
| 在宿主的连接路径或异步线程上做系统调用的任务 | 不要在 tokio 工作线程上同步调用可能慢的系统 API（Security 框架的签名查询、读大文件等），用 `spawn_blocking`；否则会卡住宿主的定时器（监护、合并窗口都受影响），T138 的 IT-idle-exit 就是这样失败的 | T138 |
| T143 | 代码块现在是最简单的 `pre`/`code`（`packages/ui-core/src/rows/markdown.tsx`）：`rehype-sanitize` 的 schema 去掉了 `code` 上的全部 class，agent 写的 `language-*` 也在内。做高亮时语言信息要从别处取，或者在 schema 里只放行与已知语言列表相符的值；不能整体放开 class（T136 的关切：agent 的 class 可能命中本应用的样式） | T166 |
| 任何显示 agent Markdown 的任务（T143、T146 等） | 一律用 `rows/markdown.tsx` 的 `Markdown` 组件及它导出的 `remarkPlugins`、`rehypePlugins`，不另建管线：插件全部显式给出，没有 `rehype-raw`、开了 `skipHtml`；schema 只放行 http(s)；链接经 `ShellAPI.openExternal`；图片只显示成链接、从不加载。streamdown 的默认组件里 `pre`、`code`、`strong`、`table` 已换成普通元素，代码块控件和链接确认弹窗关掉了 | T166 |
| T167 | T090 为守住上限拿掉的部分随用到它们的辅助一起加：CLI 包装（`tools/e2e/src/cli.ts`）的 `repl`、`watch`、以 `cli` 身份连接；`showMessageBoxSync` 的替换（`tools/e2e/src/dialogs.ts` 现在只替换 `showMessageBox`，每次调用挂起到用例 `answer`） | T090 |
| T167、T093 | 添加项目时主进程调 `dialog.showOpenDialog`（`apps/desktop/src/main/shell.ts`），T090 没有替换。替换与记录放进 T167 的辅助，T093 只写用例 | T090 |
| e2e（T091–T093、T167） | 夹具在 `tools/e2e/src/fixture.ts`：每个用例一个临时根目录（`CUPPER_E2E_DIR` 布局）、临时 git 仓库（固定作者）、假 agent 默认配成 `agents.claude`，可用 `test.use({ testConfig })` 覆盖；`connected()` 等窗口连上并记下宿主 pid；结束时按关机路径退出 App，不弹退出确认。进程按 pid 加启动时间识别（`procs.ts`），只发信号给本用例记下的进程。E2E 编号在 `ids.toml` 里标 `nightly` 的只在 `ci e2e`、`ci nightly` 跑，`ci pr` 只跑 `@smoke` | T090 |
| 清理 worktree 的磁盘 | `ci e2e` 有意保留 `apps/desktop/dist/pack/e2e`（约 470 MB）、`target/runtime-pack`（约 190 MB）、`target/host-bundle`（约 3.7 GB），清 `target/` 时一并清掉 | T090 |
| T168 | "展开看全文"的接入点（T165）：`request/RequestPanel.tsx` 的 `Body` 把 `content_truncated` 作为 `truncated` 传给 `OptionsKind`；`request/logic.ts` 的 `isEnabled(option, truncated)` 决定允许类选项能否选；`request/kinds.tsx` 的 `Terminal` 已有"展开 / 收起"，`blob` 类内容的 `text()` 现在返回 `undefined`、不显示 | T165 |
| T146 | 请求记录行（`request/RequestRow.tsx`，登记为 `request`）的文字由纯函数 `record()` 决定：等待处理、已拒绝、已批准：{标题}、已询问 n 个问题、未回答。"已询问 n 个问题"按设计可以展开看回答，这次只写了数目 | T165 |
| T093 | E2E-reject-with-message（T165）：自由输入行只在权限请求有不持久的拒绝选项时出现，编号是"选项数 + 1"，按这个数字只聚焦、不提交；1 秒内打过字时按 Enter 只提示"点击提交"、不发请求；拒绝收到 `already_resolved`、`expired` 时 `respond` 返回 `elsewhere`，不发跟进，附言接在输入框原有草稿之后（空一行），toast"已在别处处理"；多个待处理请求时标题行有 `data-request-count` | T165 |
| T169 | 会话层的 `privileged.executed`（T163 的遗留）：网关只在应答成功后记 scope 为 `privileged` 的方法（`gateway/dispatch.rs` 的 `privileged()`），看不到会话事实。经 `session/create`、`session/set_config` 切到 `full_access`，经 `interaction/respond` 选写入权限规则的特权选项，由会话 actor 的 `tier_change`、`policy::respond` 判定，要在判定通过处发出，格式照 `supervisor/audit.rs` 的 `formal()`；只记代码和取值，不写文案 | T163 |
| 以后接入仓库档调用的任务（T061、T121 等） | `git.unsafe_repo_config` 目前只在 `index::projects::locate`（命令类别 `rev_parse`）发出。仓库档的其他调用接进来时照它的做法：拿 `GitError::refusal()` 的原因和工作区内的文件或键（不含值），经 `ScanContext` 的审计发送端发出 | T163 |
| 所有宿主任务 | 不要在 tokio 工作线程上同步调用慢的系统接口：Security 框架（签名、cdhash）、读整个大文件算哈希等一律放进 `spawn_blocking`，原处 `await` 结论，失败与 join 失败按拒绝处理。T138、T163 已把 `admit` 的 `verify`、审计的 `read_peer`、网关与适配器启动前的自检、运行时的 sha256 校验挪过去；`Host::start` 在开始服务之前的一次性调用除外 | T163 |
| T144 | 新建会话页（T085）在 `packages/ui-core/src/newsession/NewSessionPage.tsx`：agent 下拉现在只列 Claude，可用状态取自索引视图的 `AgentStatus`（不是 `agents/capabilities`，后者只有缓存的能力描述），原因码由导出的纯函数 `unavailable(status)` 推出（`cli_missing`、`not_logged_in`、`runtime_failed`，其余算 `unavailable`）；选中的 agent 和工作区经 `Composer` 的 `create` 属性传给 `useSend`（`composer/send.ts` 的 `NewSession`、`DEFAULT_NEW_SESSION`）。M1a 在 agent 不可用时仍允许发送、由宿主按语义码报错；按 AG-4"不支持的功能不出现"，是否直接禁止发送由本任务一并定 | T085 |
| T093 | E2E-new-session-agent-claude：新建会话页的标题里项目名就是选择器，菜单末尾是"添加项目"（走 `privileged({kind: "add_project"})`，主进程弹 `showOpenDialog`，替换见 T167）；动作条有"Claude ⌄"和"本地"；没有项目时 `data-send-state` 为 `disabled` | T085 |
| ui-core 里写组件测试、用 MockHost 回放索引的任务 | 只对 `index` 流回放索引的 ops：否则新建的会话流也会收到这些 ops，视图报错后反复重新订阅，vitest 会耗尽内存（T085 的 `newsession/new-session.test.tsx`） | T085 |
| 读 blob 的界面任务（T145 等） | host-client 的 `getBlob(ref)`（`client.ts`，挨着 `putBlob`）按 `offset` 收集分片，缺口、片数、总长、哈希任一不符就报错；`state/blob.ts` 的 `useBlobText(blob)` 给出 `idle`、`loading`、`loaded`、`failed` 和 `load()`，以计数器保证只有最近一次下载生效、卸载后丢弃结果。协议没有取消 `blob/get` 的方法，大输出要按范围读（`blob/get` 的 offset、length）时另行设计 | T168 |
| T093、T092 | 请求面板的截断内容（T168）：命令、详情是 blob 时显示在 `ContentBox` 里，收起时显示预览、展开才下载；每个 blob 都展开且载入完成（哈希核对通过）后允许类选项才放开，收起任一个或下载失败回到只能拒绝；提示为"已截断：展开看到全文后才能允许"。用户气泡里的 `text_blob` 显示"长文本（N KB）"，点"显示更多"才下载 | T168 |
| e2e（T091–T093） | T167 的辅助：`tools/e2e/src/helpers.ts`（`hostClient`、`disconnectWindow`、`advanceClock`、`events`/`untilEvent` 只读打开 `host.db`、`untilSession` 用 `sessions --watch`、`killHost`、`elevate`、`sampleNetwork`、`texts`）、`input.ts`（`focusWindow`、`press`、`type` 经 `sendInputEvent`；`requestPanel`、`panelReady`、`activatePanel`、`pressDigit`）、`cli.ts`（`repl`、`watch` 的 `until`/`stop`、`cli` 身份）、`dialogs.ts`（`showMessageBoxSync` 立即回 `cancelId`；`showOpenDialog` 挂起到 `choose`）。**凡是靠按键或点击算真实输入的用例都要打 `@focus`**（单 worker、排在最后跑），否则并行下另一个 App 会抢焦点而偶发失败。`__cupperTest` 目前只有 `counters()`、`state()`，其余导出加在渲染进程入口的 e2e 分支里，`apps/desktop/src/flavors.test.ts` 会检查它不进非 e2e 构建。夹具的 `login_env.PATH` 只含系统目录；e2e 要求本机有 Xcode 或命令行工具 | T167 |
| 动 preload、`contextBridge`、渲染进程入口或 host-client 连接的任务 | `contextBridge` 只复制对象自身的属性，经它交给主世界的对象不能是类实例、方法不能在原型上（T076 的 HostPort 曾因此是空对象）；浏览器原生函数（`requestAnimationFrame`、`setTimeout`、`performance.now` 等）不要存成属性或默认值再以别的 `this` 调用（T080 的 `Frames` 曾因此抛 Illegal invocation）。单元测试和截图回归都覆盖不到真实 Electron 的数据链路，守门的是 `ci pr` 里的 `@smoke`（渲染进程一侧断言侧边栏看到数据） | T167 |
| T160、T161（自用构建） | `vite build --mode local` 会失败：Vite 不接受 `local` 作 mode 名（与 `.env.local` 冲突）。换 mode 名，或者把 flavor 与 Vite mode 分开映射；`flavors.test.ts` 目前只检查 release、dev | T167 |
| T169、T175 | 会话的载入（T089）：`orchestrator/session/load.rs` 读 `sessions`、`queue_entries`、最后一个回合、已结束的 `interactions` 和会话视图，组成 `Parts` 与首个输入 `Input::Restored`；管理器先把邮箱放进表再起载入任务，同一会话只起一个 actor，`forget` 只在邮箱匹配时才从表里撤出（空闲退出用它）。测试里模拟重启用 `Fixture::restart`，启动清理是 `supervisor::recover::end_previous_run`。视图里缺配置时载入现在回内部错误，由 T169 改为重建 | T089 |
| T091、T069 | 宿主重启后（T089）：旧会话在第一次调用时按需载入；待处理请求补写 `interaction.resolved{expired, host_restarted}`，运行中回合补写 `turn.completed{interrupted}`；队列里有消息的一律以 `turn_interrupted` 暂停，要用户操作（例如 `queue/resume` 或新的 `send`）才继续。设定者按 `mode_set_by` 找回，来源不明或 CLI 设定的档位重启后可能进入 `config_unconfirmed`，要 `session/confirm_config` | T089 |
| 动 `policy/describe` 的任务 | `SessionPort::facts`（只供 `policy/describe` 用）仍要求会话已在内存里；会话未载入时描述里不带所选选项的信息（T089 没有改，重启后待处理请求都已过期，影响小） | T089 |
| T062、T063、T069（宿主的进程级用例） | 共用夹具在 `crates/cupper-host/tests/common/mod.rs`（T068 从 `lifecycle.rs` 挪出）：拉起宿主、harness 客户端、假 agent、等表、退出码、`session/create`、`session/send`、时钟推进。宿主守卫 `Running` 改为先 SIGTERM 再等退出，宿主能停下适配器，不再留下适配器和孙进程；新的测试目标要在 `Cargo.toml` 里要求 `test-harness` 特性 | T068 |
| T069 | 第一个回合可能停在准备阶段：`turn.started` 已写出、附加与发送前的重扫还没做完时项目配置变了，回合等闸门重新打开。T068 没有覆盖，补一条"信任恢复后这个回合能正常发出" | T068 |
| 显示信任原因的界面任务（M1b） | 闸门已因 `project_config_changed` 关着时，再次重扫失败（例如悬空链接）不追加提示，原因只在审计的 `project.trust.scan_failed` 里。界面要显示最新原因时要改宿主的提示规则 | T068 |
| T065、T064 | 假 agent 的回放（T067）：`--replay <录制>` 按 `--segment`/`--segment-file` 选段，不经 SDK、逐行读写 stdio；与下一个 `send` 帧比对（id 重映射、`cwd`/`path` 占位、`_meta` 忽略、其余精确），通过后按序发出 `recv` 帧，时间压到 1/100，比对失败以 71 退出并写出帧号和字段路径。段末的 `instance_exited` 按压缩后的时间退出，不区分退出是否因客户端先关了 stdin；宿主驱动的录制里如果退出是宿主关 stdin 造成的，回放可能比宿主更早退出，届时要在录制里标出"由客户端关闭"，回放据此等 stdin 关闭。路径字段只认 `cwd`、`path`；B 层只用宿主自己驱动的录制（设计 5.9.2） | T067 |
| T141 | `session/set_config` 的特权审计（T169 留下）：在判定通过处调用 `orchestrator/session/mod.rs` 的 `privileged()`，传入 `Confirmable::SetConfig` 和会话事实；`privileged()` 现在只为 `create`、`respond` 取了方法名，补一个分支即可 | T169 |
| T069、T091 | 崩溃后的重建（T169）：检查点作废、日志被修剪时，载入补写缺失的 `session.configured`、`session.capabilities`（只在连接过 agent 时，来自能力缓存）、`queue.updated`；准备阶段崩溃的会话在宿主启动时由 `SessionManager::reopen` 主动载入并重开第一个回合；被中断回合里没结束的条目先补写 `item.completed`（工具调用 `abandoned`，执行类命令按 agent 的中断是否停命令标 `may_still_run`）。日志被修剪掉的待处理请求不补写 `interaction.resolved`（表里已是 `expired`） | T169 |
| T175 之后改 NFR1-5 的任务（或 T175 自己） | E2E-NFR1-5（T091）现在先发下一条消息、再断言补写的 `interrupted`、`expired` 和界面显示，因为新宿主只在会话收到第一条命令时载入。T175 让订阅也触发载入之后，可以加一条"只订阅、不发命令也能看到补写"的断言 | T091 |
| T146（消息流的后续） | 插入更早的历史后视口偶有残余偏差：约每 10 次有 1 次下移 26 px，原因是视口按估计行高（44 px）放回、新插入行的实测高度不同、差额没补。T091 已修掉整页跳动（锚点改为控件之后屏幕上的第一行），用例只断言移动小于 40 px。建议等新行测量完再恢复，或测量后按差额补一次（`virtualizer.scrollToOffset` 试过无效） | T091 |
| T063 | 假 agent 不对外暴露收到的是 `session/resume` 还是 `session/new`，E2E-NFR1-5 只能确认下一条消息在新宿主上跑完。K 清单要直接断言 resume 时，给假 agent 加一个可观察的记录 | T091 |
| e2e（T092、T093） | 视图导出：界面的 `__cupperTest.views()` 按流给出 `{seq, complete, view}`；CLI 的 `cupper view index|<会话>`（test-harness 构建）每批事件后输出 `{stream, seq, resubscribed, view}`；`tools/e2e/src/nfr1.ts` 有"等两端到达宿主最后的 seq 再取视图"的辅助。`HostProvider` 在创建 client 时就调 `onClient`。T167 夹具 `processes()` 的注释说宿主不是 App 的子进程，实际是 Electron 主进程直接拉起的，T092 顺手改正 | T091 |
| T177、T069 | 会话 actor 的空闲退出（T175）：六条同时满足才算空闲（没有运行中回合、待处理交互、会出队的排队消息、核心计时器、在途重扫或连接、等信任检查的 `send` 回复），订阅者不算；时长 `session_idle` 600 s，可经 `test_config.durations` 缩短。到时 actor 关闭 agent 会话、放下全部部件（含流写者）、交回收件箱，管理器 `forget` 后把收件箱里的调用经按需载入重新投递。订阅会话流时经 `SessionPort::wake` 触发载入。测试里等在途工作结束用 `SessionManager::settled`（只在测试里编译）。T177 的接入点：`Parts`、`SessionDeps` 还没有 blob 存储；合并器的计时器键只有 `FlushTimer`，要扩成枚举 | T175 |
| T177 | T062 迁 SP-4a t1、t7 时暂缺两条 blob 断言（大工具输出转 blob、500 kB 工具输出）：现在一条约 500 kB 的 `item.completed` 超过一帧，在线客户端重置后再也收不到这个会话的事件，新订阅者被断开。T177 完成后把它们补回 `crates/cupper-host/tests/gateway_sync.rs` | T062 |
| T069 | 孤立修法（T062）后仍有微秒级窗口：关闭新回合之前已受理、会话 actor 还没置上 `running` 的回合可能被漏看，彻底消除要在受理时计数（接口层面）；`lifecycle.rs` 的 `it_gw_blob_put_and_get_through_the_gateway` 在机器高负载时失败过一次（SIGTERM 后 30 s 宿主没退出），没复现；一次附加有时连写两条 `session.capabilities`，同一回合的事件数因此不固定，不要断言精确个数 | T062 |
| T117 | 假 agent 的 `flood_mixed` 每个条目约 3.5 ms、线性增长（2,000 条要 7 s，SP-4a 的 `flood_mixed 6000 200` 要 20 s 以上），与每块长度无关；是否达标请在宿主性能测量里判断 | T062 |
| 宿主的进程级用例 | `tests/gateway_sync.rs`（T062）加入共用夹具：宿主可按自定载荷和 `Command` 拉起，客户端可按指定选项连接，假 agent 的数据目录与 `test_config` 分两步以便重启；停读的客户端要用 testkit 的原始套接字（CLI 库的读任务总在读）；宿主不是测试子进程时，守卫先按 pid 和启动时间核对再发 SIGTERM，用 `ExitWatch` 等退出 | T062 |
| 所有跑 e2e 的任务 | **e2e 一律经持锁入口跑**（T093）：整套用 `pnpm test:e2e`（两段，可跟 `-- <spec>`），单个 spec 或用例用 `pnpm --filter @cupper/e2e run test:e2e:one <spec> -g <名称>`（不加 `--`）；别的写法套一层 `cargo xtask lock -- <命令>`。**不要直接运行 `playwright test`**：几个 e2e App 同时开着会互相抢焦点、遮挡窗口，`@focus` 用例偶发失败。持锁运行的命令带 `CUPPER_MACHINE_LOCK=held`，嵌套不再等锁。运行期间 `caffeinate -d` 防止显示器睡眠；**屏幕已锁或显示器已睡时 globalSetup 直接报错**（`@focus` 用例需要可见、能拿到焦点的窗口），解锁后重跑 | T093 |
| e2e（T176 及以后） | T092 加的辅助：`tools/e2e/src/security.ts` 的共用步骤；`__cupperTest.command(method, params)`（只在 e2e 构建，经窗口的 client 与主进程的检查发命令）；假 agent 的剧本 `permission_options <ACP 选项 JSON>`（`kind` 写成没人认识的值即为效果未知）。夹具启动 App 时加了 `--disable-backgrounding-occluded-windows`、`--disable-renderer-backgrounding`、`--disable-background-timer-throttling`，被遮挡的窗口不再降频；期望的等待放宽到 20 s、用例上限 90 s。激活请求面板用"把焦点移到选项列表上"，Tab 可能落进"告诉 agent"输入框 | T092 |
| T145 等显示工具输出的界面任务 | 超长工具输出（T177）：转义后超过 `max_inline_bytes`（16,000）就停发 delta、`output_state` 转 `truncated`，`item.updated` 每秒至多一次更新 `output_tail`（最后 4,000 字符）和 `output_bytes`；结束时 `complete`，输出是 blob 引用加尾部预览，用 host-client 的 `getBlob` 取全文；回合结束时仍在运行的是 `abandoned`、带尾部、没有 blob。全文在宿主内存里封顶 32 MiB，`output_bytes` 记实际收到的字节数 | T177 |
| T069 | 会话 actor 在同一步里 await 写 blob（T177），写完之前不处理别的输入，32 MiB 的输出在慢盘上可能阻塞会话百毫秒级；故障测试里出问题的话，可改成后台写、挂起 `item.completed`。性质测试 `session_props` 不交回 `Spill`，生成长输出时要在测试驱动里补上 | T177 |
| e2e（T178 及以后） | T176 的测试工具：`tools/e2e/src/raw.ts` 的原始协议客户端（自己分帧、自己给 `auth`，能发 `0x02` 分片）与假宿主（占住 `<数据目录>/run/host.sock`，先发 `host/hello`，`host_proof` 是随机值，记下对方发来的一切）；E2E-SEC-egress 的做法是 `sampleNetwork` 加正对照（假 agent 拉起 `nc -u -w 20 192.0.2.1 9`，`connect` 不发包）。e2e 构建的 CLI 不带密钥时只按 `peer` 弱校验，会连上假宿主 | T176 |
| 网关的上传 | 每条连接最多 64 个未决 blob（`gateway/blobs.rs`，T176），零长度分片和乱序、超限留下的空条目都算在内，第 65 个的分片丢弃、它的 `blob/put` 以"分片加起来不等于大小"失败；只有带 `write` 的连接能上传 | T176 |
| T064 | 首批 B 层录制用的一次性生成程序在 `docs/tasks/T065-gold-b-runner/generate.rs`（物料，不编译），用法在同目录 `notes.md`；可重复的生成器由 `xtask compat --fake` 接手（计划 v1.3.47）。录制放在 `testdata/recordings/cupper-fake-agent/0.0.0/fake/`，金样例在 `testdata/golden/gold-b/` 的同一路径；更新金样例用 `UPDATE_GOLDEN=1` | T065 |
| 下一个动会话 actor 的任务 | 附加时 `session.capabilities` 连写两条（第一条命令列表为空，第二条来自 `available_commands_update`），附加的应答与第一条更新在 actor 里走两个通道、写进流的先后取决于时机。只影响事件顺序与条数；是否合并成一条由那个任务决定（T062、T065 都碰到） | T065 |
| 拿原始帧比对的任务 | 宿主发给 agent 的 JSON-RPC id 是 UUID，比对时不能按字面比，要像 T067 的回放那样重映射；B 层的同步依赖 SDK 3.3.0 的按序分发（`gold_b_fake_sdk_answers_an_unknown_request_after_what_came_before`），升级 SDK 时这条用例会先失败 | T065 |
| 所有任务（执行者） | **每次提交后跑一次 `cargo xtask size`**（T185）：统计本分支自己的增删（排除 lock、生成物、`docs/`、`testdata/`、截图基线、二进制，合进来的 main 不算），对照任务书的停止线报告余量，到 90% 或越线时以 `!!` 标出；推算或实际越线就停下汇报。新文件要先 `git add` 才计入。工具读当前 checkout 里的任务书，编排者在 main 上的裁定要等 main 合进分支后才读得到 | T185 |
| 编排者写任务书 | 任务书的"规模"一节用模板里固定格式的一行：`- 停止线：<n>；硬停止线：<n>`（`cargo xtask size` 优先认它）；裁定改上限时直接改这一行，并在通知执行者时写明新数 | T185 |
| e2e（T078 及以后） | e2e 夹具默认把运行时安装指向 `http://127.0.0.1:9/`（T178），宿主不再连公网；夹具自检用 `helpers.ts` 的 `offMachine(sampleNetwork(...))` 断言没有非回环连接，要测安装本身的用例用 `testConfig` 覆盖。对话框替身由 e2e 构建的主进程在启动时装好（`apps/desktop/src/main/e2e-dialogs.ts`，`globalThis.__e2eDialogs`），`tools/e2e` 用 `Dialogs.attach` 读取与应答 | T178 |
| 所有分支 | RUSTSEC 的新公告会让所有分支的 license 作业同时失败，日志是 "advisories FAILED"（不是拉取失败）。处理办法：在一条分支上单独提交依赖升级（只改 `Cargo.lock` 里那一个包）、随最先合并的任务进 main，再通知其他分支合 main。2026-10-26 的 RUSTSEC-2026-0382（`rustls-webpki` 升到 0.103.16）就是随 T178 进的 main | T178 |
| T066 | Q43 的真实观察：T063 用假 agent 五轮"崩溃、恢复"后宿主进程树大小不变，只证明宿主侧恢复不会多拉起进程；真实 `claude` 多次 resume 后会不会留下 CLI 进程（上游 #1011），要由兼容矩阵用真实 CLI 观察 | T063 |
| T112 | K14 的改判（T063）：prompt 以未登录失败、CLI 有版本号且低于已验证范围时改判为 `cli_too_old`、登录状态不变；会话打开时的认证错误还没有改判（claude 形态的假 agent 打开时不报未登录），codex 形态的同类情形归本任务 | T063 |
| 宿主的进程级用例 | 假 agent 的 `--calls <文件>` 记下每个收到的请求和 `session/cancel`（`{method, session, text}`），可直接断言 resume 与 new、prompt 重发次数；`withdraw_after <ms>` 一段时间后撤回下一个请求、剧本继续。`test_config.agents.<agent>.cli_version` 给注入的假 agent 指定版本，与探测到的版本走同一套分类 | T063 |
| T120 及以后用 git 的任务 | 快照档（T061）：每个仓库一个私有对象库 `<数据目录>/git/<repo_key>/objects/`，每个工作区一个 GIT_DIR `<数据目录>/git/<repo_key>/ws/<ws_key>/`（key 是 realpath 的 sha-256 前 8 字节，0700，没有 `hooks/`），可供 `@` 文件列表、`git/status`、diff 复用；`SnapshotTier` 要传一个 `Place`，`snapshot::take` 里有构造它的 `prepare`。`turns.snapshot_start`、`snapshot_end` 存 tree id。私有对象库与 sharedindex 副本 M1 不回收。`core.excludesFile`、`core.attributesFile` 是相对路径时按快照档的工作目录解析，与用户在仓库里运行 git 时不同（少见，没处理） | T061 |
| T186 | LFS 用例要把 `tool(…, lfs)` 指向 xtask 下载的 git-lfs；白名单配置里 LFS 取值的生成已有单元测试。超大未跟踪目录的判定是"文件数 × 目录数"的扫描，PERF-snapshot 可以一并量一下 | T061 |
| 动桌面数据通路的任务 | 测量闸门（T078）：`PERF-bridge-gate`（`tools/e2e/src/gate.spec.ts`，每晚）在判定负载下断言主进程事件循环延迟 p99 < 5 ms 且没有流重置，当前约 1.6 ms；渲染进程钩子 `window.__cupperGate`（只在 e2e 构建）可按流计数、跟随窗口持有范围之外的流、经 `getBlob` 下载、以游标 0 订阅重放。夹具 `launch` 的 `throttled` 选项保留 Chromium 默认节流。Playwright 下页面不会进入隐藏状态，真正的隐藏需另写只经主进程 inspector 测量的脚本 | T078 |
| T117 | PERF-snapshot（T186）是 `#[ignore]` 的每晚用例，`perf` 作业实现时用 `cargo test -p cupper-host --lib -- --ignored perf_snapshot` 跑，把打印的中位数写进历史数据；基准：1 万文件的合成仓库，热缓存下没有改动约 112 ms、改 10 个文件约 115 ms。回合指纹每次快照遍历整个工作区找 `.gitattributes`（含被忽略的大目录），10 万文件的仓库超预算时改为只看已跟踪和未忽略的 | T186 |
| 跑 git 相关测试的任务 | 测试用的 git-lfs 由 `cargo xtask git-lfs` 下载（锁定 3.8.0，核对哈希），放在 `target/git-lfs/git-lfs`；CI 的 `rust` 作业先执行这一步。只跑 `cargo test` 时没有 git-lfs，LFS 用例跳过并提示 | T186 |
| T182 | `runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md`（T006 生成）还没进 runtime-pack 的暂存和 App，要在 THIRD_PARTY_NOTICES 生成时一并放进去；manifest 的 `files` 哈希会随之变化 | T006 |
| 升级 codex-acp 的任务 | 先把新标签的 `package-lock.json` 放进 `runtime/locks/`，再运行 `node tools/license-gate/src/main.ts codex-notices`（要联网，下载各包 tarball 核对 integrity 后取 LICENSE），否则许可证门禁会因找不到锁文件或声明不全而失败 | T006 |
| T131（及加渲染入口的任务） | 加第二个渲染入口时，在 `tools/license-gate/src/main.ts` 的 `ENTRIES` 里登记它的环境名；`apps/desktop/vite.config.ts` 的插件把每个环境的模块写到 `dist/metafile/<环境>.json` | T006 |
| T121、T124（git 写操作与 worktree 的故障点） | 故障点夹具与断言库在 `crates/cupper-host/tests/crash_concurrency.rs`（T069）：完整性检查、每条流 seq 连续、实时事件是日志前缀、在途命令按原 `command_id` 重试符合账本语义、消息各恰好一次；按命中次数倍增扫描（第 1、2、4、8……次到达时以 99 退出）。新故障点在 `failpoint.rs` 登记后，加进扫描表即可 | T069 |
| 会话 actor 的测试 | `actor::held`（只在单元测试里编译，按项目路径登记）能让某次重扫停住，用来把配置变化放进"回合已开始、附加前的重扫还没做"这类窗口（T069）；会话夹具的 `events`、`attach`、`command` 等待都有 30 s 超时，超时时报出在等什么 | T069 |
| 读设置的宿主任务 | 设置（T036）在 `crates/cupper-host/src/settings.rs`：宿主内部经 `SettingsHandle::current()` 取；默认值是 `worktrees.root` 为 `~/.cupper/worktrees`、三个通知开关开、新会话默认 Claude、`host.idle_exit_minutes` 为 10、`diagnostics.verbose` 关、不额外放行环境变量、CLI 路径为空（自动发现）。`cli_path`、`env_passthrough` 变了会先重新发现 agent 再回复；其余键 M1a 不推送 | T036 |
| T126 | `apps.terminal`、`apps.editor` 的内置应用清单仍没有设计（T013 的交接），T036 的默认值暂用 macOS 自带的 Terminal 和 TextEdit | T036 |
| 改协议或 ShellAPI 的任务（T151、T147 等） | 契约已冻结（T016），只能附加，`ci protocol` 的 `schema-compat`、`shell-api-m1a` 会判。新加的 `Option` 字段必须带 `skip_serializing_if = "Option::is_none"`，否则宿主发出 `null` 而 schema 不允许（T190 补守卫前没有检查）；值本身可以为 `null` 的做成具名定义，照 `CliPath`。改完跑 `pnpm gen`。`schema-compat` 不比较描述、数值界的放宽、`x-errors`、`x-milestone` | T016 |
| T151 | `settings.updated` 事件与 `IndexView.settings` 在 `schema-compat` 下按附加通过（T016 用冻结副本演示过） | T016 |
| T190 | 守卫的做法：序列化金样例与夹具后，断言 schema 里的非必填属性上没有 `null`；`CliPath` 这类值本身可为 `null` 的具名定义除外。导出时的变换是 `cupper-protocol/src/schema.rs` 的 `absent_not_null` | T016 |
| T191 | PERF-snapshot 在 `git/snapshot/tests.rs`，每种情况现为 5 个样本；样本经 `cupper_host::perf::record` 写到 `CUPPER_PERF_OUT`。PERF-bridge-gate 进历史的做法写在 `docs/tasks/T117-host-perf/notes.md`：`gate.spec.ts` 也写 `CUPPER_PERF_OUT`，`ci/jobs/perf.rs` 的统计、追加、判定抽成共用函数给 e2e 作业调用 | T117 |
| 要提高条目吞吐的任务（M1b 之后） | `flood_mixed` 经宿主约 2.6 ms 一个条目：会话 actor 把不能合并的事件逐个提交，约 0.87 ms 一个。提速的方向是同一步里攒批追加。现在达标，没有做（数字在 `docs/tasks/T117-host-perf/notes.md`） | T117 |
| 改回合指纹的任务 | `Fingerprint::take` 遍历整个工作区找 `.gitattributes`，10 万文件约 43 ms，随被忽略的文件数线性增长，到百万级约 0.4 s 时再改成只看已跟踪和未忽略的 | T117 |
| T187 | 真实提示词、权限模式（Claude `default`）、准备文件与自我约束都没写，留给 T187；报告的 `config_files`、`cli_sessions` 已占位，语义检查要求真实模式必须有配置哈希。`session/create` 带显式权限模式后三份 B 层录制的 `config` 会变，用 `--fake --write` 重新产出并说明差异。cancel 剧本在真实 CLI 上是"允许后等 1 秒再中断"，`sleep 60` 没有输出，可靠性要在 T066 实际跑时确认 | T064（已写入 brief） |
| T188 | 读报告用 `report::parse` 加 `report::problems`；schema 在 `testdata/compat/report.schema.json`（键排好序）；`testdata_scan.rs` 的 `WORD_LIST`、`word_list` 已改成 `pub`。compat-framework 还没进 `ci pr`，照 ui 作业用 `git::changed_since_main` 按路径判断 | T064（已写入 brief） |
| T066 | 执行者是 cupper-host 的忽略测试 `compat_driver`，由 `cargo xtask compat` 用 `--ignored --exact compat_driver` 和 `CUPPER_COMPAT_PLAN` 拉起；`--write` 只写 gold-b 剧本，没有 `.private/sensitive-words.txt`（或 `--words`）时一开始就报错 | T064 |
| T187（合并时） | T188 的 `compat/reports.rs` 里 `counts` 用穷尽的 `match` 判断剧本结果；新取值的裁定：`not_applicable` 按通过算，`observed`、`real_only` 不阻止计入 | T188 |
| T066 | 已验证 CLI 范围由 `cargo xtask compat-reports --write` 从 `testdata/compat/reports/` 的报告生成，不手填；计入条件：真实模式、适配器版本等于锁定版本、至少一个剧本且全部通过。设计 5.10.3 说范围"每个适配器版本一份"，清单里每种 CLI 只有一个范围：适配器升级后基线怎么取由 T066 定 | T188 |
| T193 | 守卫在 `cupper-protocol/tests/fixtures.rs` 的 `unit_proto_absent_not_null_samples_send_no_null_on_optional_fields`，沿 `schema::document()` 检查样例；覆盖面用不带 `absent_not_null` 的生成器再导出一次算出（220 个里 135 个），没覆盖到的 85 个逐个列在 `docs/tasks/T190-absent-not-null-guard/notes.md`。视图的 proptest 生成器是库内的 `pub(super)`，集成测试拿不到；部分 ID 有格式约束，生成的实例要合法 | T190 |
| T197 | T189 剩下的 E2E-SEC-real-input 50 次连跑改在 GitHub 的 macOS runner 上做：一次调用 `test:e2e:one ... --repeat-each=50`，用同一份构建；本机那次 50 次里 45 次期间有用户输入，不算数。所有 `@focus` 用例都要求运行期间没人操作，runner 正好满足。`setup.ts` 会拒绝锁屏和睡眠的显示器，要确认 runner 的图形会话通过这项检查 | T189 |
| T197 | e2e 的 workers 现固定为 2，是按作者机器定的；有远端后按 runner 的核数重新定，并在 CI 上重跑一轮 20 次（计划 T090 的注） | T090 |
| T066 | 真实模式只经单元测试和 `--dry-run` 验证过，第一次实际运行时留意：e2e 宿主包在登录 shell 的环境下能否找到 `claude`；`--model` 在真实适配器上是否被接受；cancel 剧本"允许后等 1 秒再中断"是否可靠；`claude` 继承适配器的标准错误时 `lsof` 检查会判失败，先判断是适配器的问题还是要放宽规则；`~/.claude.json` 的字段摘要按值的 JSON 文本算，工作区开了 `preserve_order`，CLI 调换键序会误报"有变化"（只会误判失败） | T187 |
| T192 | 剧本编号以 `docs/tasks/T187-compat-real-mode/notes.md` 为准；`Scenario` 类型、`loosens`、进程检查（`lsof -F`）、真实宿主的启动方式都在 T187 里 | T187（已写入 brief） |
| T197、下一个改 e2e 调度的任务 | `ci e2e`、`ci nightly` 里 `gate.spec.ts` 和另一个 worker 的用例并行（`workers: 2`），PERF-bridge-gate 的 p99 会受旁边用例影响，趋势比单独跑时噪。重新定 workers 时考虑让它单独跑 | T191 |
| 编排者（下次本机 `ci nightly`） | PERF-bridge-gate 的第一行历史由本机的 `ci nightly` 写入（GitHub 上的 e2e 不提交历史）；跑完连同摘要一起提交 | T191 |
| T066、升级适配器的任务 | 评审录制用 `node tools/rec-shape-diff/src/main.ts [--json] <旧> <新>`（没有差异 0，有差异 1）；COMPAT 编号由 `trace-ids status --compat <报告>` 推导，编号在 `tests/ids.toml` 里登记了剧本和 CLI。`not_applicable`、`observed`、`real_only` 都不让编号完成；要把编号标为不适用，照计划 1.6 写 `status` 和理由 | T194 |
| 编排者（下次整理文档） | 计划 1.8 按有远端改写：逐行建议在 `docs/tasks/T197-github-actions/notes.md`；T084 的"macOS 只比对大版本"已改为"Chromium 必须一致、macOS 不同只提示"，计划里 1.8 的截图基线行、T118 行、R9 三处要改；设计 5.1.6"截图基线由专用脚本在本机生成"改为由 `ui-baseline` 工作流在 runner 上生成 | T197（已完成，计划 v1.3.69） |
| T099 | 20 次连跑在 GitHub 的 macOS runner 上做（nightly 的 e2e 全集或 `e2e-repeat`）；PERF 闸门在作者机器上判，托管 runner 上 `@perf` 用例不跑；IT-sys-peer-macos14/15 由 nightly 的摘要推导（`trace-ids status --summary`） | T197 |
| 改 CI 的任务 | 下载缓存的键由几个作业共用，只用 git-lfs 的作业会先存下一份不完整的；按作业分键可以省一次下载。`target/` 下非 cargo 目录会被 rust-cache 留下骨架，只凭"目录在"判断就绪的地方都要改成按内容判断 | T197 |
| T066 | T192 补齐了 21 个剧本，实际运行时留意：`config-scope`、`untrusted-project` 开工前就放好了 `.claude/settings.json`，宿主可能判为配置已变而暂停队列，剧本以"不确定"结束；`crash-resume`、`resume-procs` 在两回合之间 SIGKILL 掉 `claude`，靠 K6 自动 resume；`close-running` 允许命令后等 3 秒收尾；真实模式的回合等待上限是 5 分钟。详见 `docs/tasks/T192-compat-scenarios/notes.md` | T192 |
| T066 | 宿主的 `VERSION_PROBE` 只有 5 s；CLI 更新后第一次执行 `--version` 也要等 macOS 扫描新二进制，负载 35–50 下量到首次执行 4–29 s（用假 agent 和宿主副本量的，真实 CLI 没量过），机器忙时 CLI 发现可能超时。实际运行时量一次，需要的话放宽或先预热 | T196 |
| T199 | 照 T196 的做法：先在高负载下复现、量出实际等待再修，不加重试。macOS 首次执行新二进制要扫描，计时前用 `first_start`（`tests/common`）先跑一次；负载的造法与 pid 回收见 `docs/tasks/T196-flaky-under-load/notes.md` | T196 |
| 要在负载下复现或验证的任务 | 用 `test-repeat.yml`（cargo 或 Vitest 的单个用例重复 N 次、可选并行加压，参数 kind、package、args、filter、times、parallel、runner）或 `e2e-repeat.yml` 在 runner 上跑，请编排者触发；本机不造高负载（守则第 10 条）。`times` 是每份的次数，总次数是 `times × parallel` | T199 |
| 升级 claude-agent-acp 到 0.93 之前 | `/remote-control`（Q42）：先经安全评审，并由宿主拦截 | T003 |
| T063、T066 | 观察多次 resume 之后的 `claude` 进程数（Q43，上游 #1011） | T003 |
| 以后建远端、公开之前 | git 历史里有 SP-1 录制补充脱敏之前的内容，两份会议记录里也有带用户名的路径。要对全部历史做个人信息扫描并清理（计划 1.8、R12），由用户决定 | SP-1 补充脱敏 |
| T110 | Q31：`phase` 只对声明了 `clientCapabilities._meta.jetbrains.air` 的客户端透出（`_meta.jetbrains.air.phase`，`commentary`、`final_answer`）。cupper 现在的声明拿不到，按宿主规则判定最终回答。要不要声明 AIR 由本任务评估；副作用：模式选项带 `kind`，命令列表带 `skillPath`（本机路径），工具调用改用 AIR 的渲染方式（只核对过 `exec_command`） | T102 |
| T110 | Q25：codex-acp 2.2.2 的 4 个模式取值与 5.6.5 的档位表一致，不用改表。`revise_plan` 会结束回合（设计 5.3.6 已改为 ends_turn 是），对照表里这一项按实测写 | T102 |
| T111 | Codex 关闭会话后不释放 MCP 进程：T102 每轮多出约 18 个，三轮之后有 78 个。测长时间运行下的进程数和内存，核对 ADR-09 的回收阈值和 NFR-11 的 6 会话估算 | T102 |
| T111、T055 | K21：Codex 的工具命令（`zsh -c`）和 MCP 服务器都不在适配器的进程组里，按进程组清理杀不到它们。关闭前枚举适配器的全部后代，按各自的进程组逐个发信号，或者先让 CLI 自己正常退出（设计 5.8.4 已补 Codex 的结论） | T102 |
| T038 | Codex 的工具命令也在新的会话和进程组里，血缘信号只剩祖先链；MCP 服务器在适配器的会话里，"同一会话"能识别 | T102 |
| T113 | `tools/sanitize` 认不出 Codex 的 MCP 服务器名（工具标题 `mcp.<服务器>.<工具>`、`rawInput.server`、`_meta` 键名里的驼峰写法），声明 AIR 时也不换 `skillPath`。T102 用 spike 自己的预处理脚本绕过，录制入库之前要在 `tools/sanitize` 里补上并加测试；SP-1、T003 已入库的 Codex 录制里的 MCP 调用也要复查 | T102 |
| T113 | 真实配置下，模型会绕开 apply_patch 改用用户配置的 MCP 工具写文件（被 read-only 沙箱拒绝），所以剧本的 prompt 要点名使用哪个工具。所有 MCP 调用是否都受沙箱约束，交安全工程师确认。另外加一个 Codex 版的"关闭时有命令正在运行" | T102 |
| T113 | 2026-10-28 的最新版本：codex-acp 2.3.0（换用 ACP TS SDK 2.0，依赖 `@openai/codex ^0.164.0`），codex CLI 0.165.0（超出已验证上限 0.163.0）。升级适配器之前重跑 Q8、Q31。复用 T102 的 `config-hash.ts` 时，换成真正的 TOML 解析 | T102 |
| T150 | 第一条验收（T101 没验证的缺口）：宿主改跑 NSApplication 之后，"宿主在、UI 已退出时重复拉起宿主"和 Finder 的"…is not open anymore"对话框是否消失。测法见 T101 汇报的遗留第 1 条；消除不了时，考虑改由 LaunchServices 拉起宿主（影响 ADR-01、Q21），或者启用 T158 | T101 |
| T150 | 写任务书之前，编排者把 T101 汇报里"设计 5.2.9 要补的规则"6 条补进设计：宿主 bundle 先在 LaunchServices 登记（`LSRegisterURL`，在宿主进程启动之前）、主线程改跑 NSApplication、系统单独拉起的宿主只转交点击后退出（排在读 stdin 载荷之前）、有序退出时清掉通知、上报授权状态、宿主带图标和显示名称 | T101 |
| T150 | Info.plist：`CFBundleDisplayName`、`CFBundleName` 写构建种类的显示名称，图标复制进 `CupperHost.app/Contents/Resources`；交 URL 用 `NSWorkspace.open([URL], withApplicationAt:)`，目标 App 按宿主路径往上三级找再核对身份（同一个 bundle id 可能登记着好几份）；授权被拒后界面提示去系统设置打开；没测的有 macOS 14 和 15、Developer ID 签名、从 DMG 拖入时嵌套 helper 的登记 | T101 |
| T160 | 自用构建是第一个申请通知授权的构建，系统只在第一次登记时取名称和图标：宿主的 `CFBundleDisplayName`、`CFBundleName` 和图标要在第一次申请授权之前写对 | T101 |
| T160 | Vite 保留了 `local` 这个 mode 名（与 `.env.local` 冲突），`vite build --mode local` 直接报错，自用构建现在构建不出来；T105 在临时副本里改成用 `self-use` 映射到 local | T105（进行中的发现） |
| 显示 agent 状态的界面任务（AG-2、AG-5） | 等运行时安装期间，agent 的 `state` 仍是 `idle`，只有 runtime 显示 `installing`（启动中不通知索引）。界面要不要显示"适配器启动中"、要不要让宿主在启动开始时通知一次 `starting`，由这个任务定。回合因附加失败而结束时，失败原因不进会话流，界面要显示原因的话也在这里定 | T201 |
| 以后改重启退避的任务 | 安装失败后，每次启动尝试都计入重启退避，5 分钟内 5 次后 agent 变成 `crashed`，和以前哈希核对失败时一样。安装失败是不是该另算，由维护宿主生命周期的任务定 | T201 |
| 升级 claude-agent-acp 的任务 | 已验证范围的基线要和适配器版本绑在一起：ADR-08 的起始值只对 0.91.0 有效，升级后范围只由新版本的报告算出；新版本还没有计入的报告时，`compat-reports` 应当报错。0.93 候选版本要先补锁文件、清单条目（版本、产物哈希、新环境变量的归类）、`compat` 和 `host-bundle` 选择适配器的参数，以及 Q42 的拦截 | T066 |
| 升级 claude-agent-acp 的任务 | 环境变量的静态扫描（5.6.6 的已知局限）：建议改成对 `CLAUDE_CODE_` 等前缀整体禁用、只放行逐个列出的名字；按产物特征提取登记表，依赖 SDK 打包后的内部形状，每次升级都可能失效 | T066 |
| T111 | `VERSION_PROBE` 的 5 s：真实 `claude --version` 空闲时约 10 ms，复制后第一次执行约 2.1 s，T196 在高负载下量到过 29 s。编排者采纳：超时后用 30 s 再试一次，Codex 的探测也照此 | T066 |
| T113 | `rec-shape-diff` 的输出里，`toolResponse` 下随工具不同的键占了大半，要过滤；`tools/sanitize` 的 `words`、`scan` 会把绝对路径拼到工作目录后面（T066 只在调用处改成相对路径）；plan 模式下 CLI 会换用更贵的模型，估算额度时算进去 | T066 |
| T053 的后续、显示压缩的界面任务 | 宿主没有声明 `clientCapabilities.session.compaction` 时，适配器不发 `compaction_update`，改用一个合成的工具调用（"Compact conversation"，kind 为 `think`），实时和回放都一样；套件里没有录到实例 | T066 |
| 以后改桌面启动测试或 e2e 夹具的任务 | `apps/desktop` 的 `launch.test.ts`（手动运行）没有 `runtime.registry_url`，每次启动都会向公共 npm registry 请求运行时（不含个人信息），可以照 e2e 夹具指向关闭的本地端口；e2e 拉起的宿主沿用 Electron 进程真实的 `HOME`，宿主跑 git 时会读用户的全局 git 配置 | T203 |
| T113 等下一次真实运行 | compat 真实模式的拉起方式在 T203 里只改了调用点（行为不变），还没有真跑过；T202 的整套重跑早于 T203 合并 | T203 |
| T113 | codex-drift 抓到 codex 0.165.0 的漂移，已记进 `testdata/codex-drift/reviewed.json`，跟进任务就是本任务：`ThreadItem` 的 `imageGeneration` 变体去掉了 `result`、改成 `inputImages`，codex-acp 2.2.2 的 `ImageGenerationReporter` 还在读 `item.result`，图像生成的结果会丢失（不会崩溃）；其余 13 个方法只是新增了可选字段和枚举值。扩展已验证范围、升级 codex-acp 时，用 `tools/codex-drift write` 重写基线，旧的审阅记录随之失效 | T114 |
| 以后要让 nightly 开 issue 时 | 不要给运行 codex 二进制的作业写权限（它跑的是从 npm 下载的第三方二进制）。做法：把 `codex-drift` 移出矩阵，另加一个 `needs: codex-drift`、`if: failure()` 的报告作业，只给它 `issues: write`，按固定标题或标签查重 | T114 |
| T113 | `xtask compat --out` 只认绝对路径（执行者在 `crates/cupper-host` 下运行，相对路径找不到计划文件）；thinking 剧本的题目要能触发 adaptive 思考（T202 换成了答案唯一的逻辑题），Codex 的思考剧本照此出题 | T202 |
| 接 Bedrock、Vertex 等服务商的任务 | 宿主经 `extraArgs` 请求 `thinking-display: summarized`，在第三方服务商上是否生效没有核实 | T202 |
| T113 | `tools/sanitize` 对 Codex 的 MCP 服务器名改成了"先取名字再全局替换"，来源有四处，其中 OAuth 报错文本是 T204 新发现的；Codex 的报错里如果再见到带服务器名的新写法，要补进工具。Claude 的 MCP 服务器名仍只靠 `mcp__` 规则、`*mcp_servers` 数组和敏感词清单，"Some MCP servers are still connecting: <名字>" 这类文本只有清单兜底，要不要也按取名处理在本任务里定 | T204 |
| 以后改 `cargo xtask size` 的任务 | 它把 `spikes/` 下的录制也算进规模，改 spike 录制的任务都会被多算（T204 多算了 107 行）；把 `spikes/*/recordings/` 加进排除列表 | T204 |
