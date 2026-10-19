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
| T024 | 写线程"只在 `SystemClock` 下等 2 ms"。`Clock` 没有"是否是虚拟时钟"的查询，从启动配置（`test_config.clock`）判断，或者给 trait 加方法 | T022 |
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
| T012 | `ErrorData.supported` 目前放在 `details` 里，依赖版本区间类型，T012 以附加方式改成具名字段 | T010 |
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
| 升级 claude-agent-acp 到 0.93 之前 | `/remote-control`（Q42）：先经安全评审，并由宿主拦截 | T003 |
| T063、T066 | 观察多次 resume 之后的 `claude` 进程数（Q43，上游 #1011） | T003 |
| 以后建远端、公开之前 | git 历史里有 SP-1 录制补充脱敏之前的内容，两份会议记录里也有带用户名的路径。要对全部历史做个人信息扫描并清理（计划 1.8、R12），由用户决定 | SP-1 补充脱敏 |
