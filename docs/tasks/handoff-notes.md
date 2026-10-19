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
| 升级 claude-agent-acp 到 0.93 之前 | `/remote-control`（Q42）：先经安全评审，并由宿主拦截 | T003 |
| T063、T066 | 观察多次 resume 之后的 `claude` 进程数（Q43，上游 #1011） | T003 |
| 以后建远端、公开之前 | git 历史里有 SP-1 录制补充脱敏之前的内容，两份会议记录里也有带用户名的路径。要对全部历史做个人信息扫描并清理（计划 1.8、R12），由用户决定 | SP-1 补充脱敏 |
