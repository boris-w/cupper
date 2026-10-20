# T028 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-4860d2cdea2d.json`。执行者那次的 `pr-8250a81123e9.json` 一并保留。宿主的四种特性组合下 clippy 都干净（执行者核对）。
- 核对：
  - 新增依赖只有 `tracing =0.1.44`（ACP SDK 已经用同一版本，锁文件只多一行）；没有引入 `tracing-subscriber` 和轮转 crate。
  - `unsafe` 只在 `cupper-sys`（新增的 `host_name()` 与 `boot_session_uuid` 共用 sysctl 读取代码）。
  - 用例结构上写不到真实的 `~/Library/Logs`，执行者跑完全部用例后核对过那里没有新建任何东西。没有个人信息，样例里的家目录、用户名、主机名都是虚构值。
- 审阅：
  - **凭据规则只有一份**：`driver/tap/redact.rs` 原样移到 `diag/redact.rs`，原始帧分接改为调用它，UNIT-tap 原样通过。
  - **完整脱敏**的顺序和"按字段名放行 ID、哈希"符合 5.11.2；"`/` 多的长串按路径看待"避免误伤 Homebrew 路径，认可。
  - **`info` 不记内容**用 `Content` 类型实现，`verbose` 开关是 T036 的接口。
  - **启动顺序**：读完载荷后初始化日志（第 2 步），之后所有构建才把 stdio 指向 `/dev/null`；发布构建推迟重定向没有行为差别，启动用例断言了日志里的启动和退出两条。
  - **日志目录按数据目录推出**，不新增注入点：编排者采纳，已写进设计 5.11.1。
  - 保留的 `eprintln!`（日志之前的失败原因、`--build-info`、网关夹具、故障点）各有理由，认可。
- 流程：这次每完成一个模块统计一次行数，没有超线。
- 规模：新增 993 行（测试约 330），删除 31 行，在约 1,200 的汇报线以内。
- 转交事项已写进 `handoff-notes.md`：T059（审计记录同时写 host.log）、T055（每种 agent 一个 `AdapterLog`）、以后加日志的任务（ID、哈希作为字段，内容经 `Content`）。主机名脱敏只覆盖 `kern.hostname`，"电脑名称"（ComputerName）没有覆盖，出现在日志里的可能性小，记录在案。
