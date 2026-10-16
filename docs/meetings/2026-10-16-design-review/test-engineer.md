# 设计评审：测试工程师

## 总体结论

**修改后可以进入 gate/design。** 测试分层清楚，SP-4a 的测试骨架（Harness、Mirror 的 `from` 连续性检查、`kill -9`、变异检查）可以直接沿用，基础很好。主要缺的是"怎样测得稳"的基础设施，共 1 条阻断意见：设计没有时间源抽象，而现有并发模型用不了 tokio 的虚拟时间。另有 20 条重要意见，要在阶段 ③ 拆任务前补进 5.9、5.12，主要涉及：测试替身的注入点、假 agent 的能力、金样例的口径、崩溃注入点、退出标准。

推演崩溃场景时还发现两处设计缺陷：已受理的第一条消息可能丢失（第 10 条）；迁移中途崩溃后会重跑已执行的脚本（第 11 条）。小实验还发现一个问题：Playwright 的键盘输入不触发 `before-input-event`（第 13 条）。

## 意见

| # | 级别 | 位置（文件与小节） | 问题 | 依据 | 建议 |
|---|---|---|---|---|---|
| 1 | 阻断 | `02-host.md` 5.2.3、5.2.6；`09-testing.md` 5.9.2、5.9.8 | 宿主里有大量和时间相关的逻辑：写入合并 40 ms、写线程攒批 2 ms、Preparing 3 s、Steer 5 s、中断兜底 10 s、自发回合 10 s、回答最短间隔 400 ms、IndexActor 合并 100 ms、actor 空闲 10 分钟、账本 48 小时、事件 7 天、原始帧 14 天、审计 90 天、孤儿 blob 1 天、重启退避和"5 分钟内失败 5 次"、Codex 回收 30 分钟。设计却没有时间源抽象。<br>后果有三：<br>① 现有并发模型用不了 tokio 的虚拟时间；<br>② 回放模式按 1/100 压缩真实时间，宿主侧的合并边界、自发回合判定、超时分支会随 CI 负载变化，金样例和"中断后 10 s 内一定结束"的性质测试都测不稳；<br>③ 48 小时、7 天这类规则在进程级测试里只能等真实时间。 | tokio 的 `time::pause`：只能用于 current_thread runtime；只影响 tokio 自己的 `Instant`；`spawn_blocking` 运行期间不会自动推进时钟（docs.rs，已核实）。而设计用的是多线程 runtime、std 线程写库、`spawn_blocking` 读库、UTC 毫秒时间戳。<br>SP-4a 的 `normalize.rs` 已经把 `now` 作为参数，是个好先例；但 `session.rs` 直接调用 `Instant::now()` 和 `sleep_until`。 | 在 5.2 增加"时间与可测性"小节，拆任务前定下：<br>① `Clock` 接口（单调时钟 + 墙钟），注入所有 actor、normalizer、账本、保留清理、supervisor；<br>② 回合状态机、队列、交互代理写成不做 I/O 的纯核心：输入是事件和当前时间，输出是新状态和要执行的动作；actor 只做 I/O 适配，性质测试直接驱动纯核心；<br>③ 驱动层的传输做成接口（stdio 或内存管道），金样例在进程内回放，用录制里的 `t` 推进虚拟时钟；<br>④ test-harness 构建的宿主接受时长覆盖参数（沿用 SP-4a `--retain-events` 的做法），并提供仅测试可用的 `test/clock_advance`、`test/run_maintenance`。<br>这些都会影响模块接口，拆任务后再补等于各模块返工。 |
| 2 | 重要 | `09-testing.md` 5.9.4；`07-desktop.md` 5.7.2、5.7.9；`06-agent-driver.md` 5.6.6；`02-host.md` 5.2.12、5.2.13 | 端到端和集成测试的接线方式没有设计：<br>① 宿主怎样把假 agent 当成 `claude` 拉起。正常路径要经过运行时安装器（联网下载 claude-agent-acp）、CLI 发现和 `$SHELL -l -i` 探测，测试里都要绕开。<br>② e2e 里宿主由 UI 拉起：harness 密钥怎样到达宿主、怎样交给测试里的 CLI、`kill -9` 后 UI 重新拉起时怎样再传一次。dev_key 和 harness 密钥都走 stdin，格式也没定。<br>③ 提权、`allow_always`、完全访问、退出确认这些原生确认框，在 e2e 里怎样应答。<br>④ 渲染进程导出视图用的测试钩子，以及可以断言的 DOM 状态。<br>⑤ 数据目录、userData、worktree 根目录怎样指向临时目录。worktree 根目录默认会写到真实的 `~/.cupper/worktrees`。<br>⑥ 系统通知的授权弹窗在 CI 上会抢走焦点，干扰 Playwright 的键盘输入。<br>⑦ 测试怎样拿到宿主的 pid。 | Playwright 文档：对 Electron 的支持是 experimental；不拦截原生对话框，需要用 `electronApp.evaluate()` 在主进程里替换（已核实）。<br>SP-4a 用数据目录里的 `agents.json` 指定假 agent，D01 没说这条路还保不保留。 | 在 5.9 新增"测试替身与注入点"表，列出每个注入点：agent 命令、CLI 路径、登录 shell 环境、npm registry 地址、通知器、时钟、故障点、原生对话框应答。每个写明所在的构建或特性、默认值，以及"发布构建中不存在"的检查方式。<br>覆盖 agent 命令等于决定 agent 运行哪个程序，只能在编译期门控，不能做成设置项或环境变量。<br>规定 e2e 中只有 UI 拉起宿主，测试和 CLI 都不拉起。 |
| 3 | 重要 | `09-testing.md` 5.9.2 | 假 agent 的能力覆盖不了 5.5 的流程、5.6.9 的两种 agent 差异和 NFR-7 的降级：<br>① **没有 persona（按 Claude 或 Codex 的形态回应）**。现有 `permission` 命令的选项 ID 是 `allow`、`always`、`reject`，对照表里都查不到，只会走兜底。所以测不到：选项映射、#556 的"拒绝即结束回合"、Claude 用 `$/cancel_request` 撤回审批、Codex 不撤回审批。<br>② **prompt 剧本管不到 prompt 之前的行为**：`initialize` 返回的能力（steering、promptQueueing、image）、`session/new` 的 configOptions、随后的 `available_commands_update`、`set_config_option`、`session/close`，以及 Codex 未登录时在 `session/new` 就报错。<br>③ **缺少的命令**：<br>　- 不响应取消、永不返回：CP-7 的兜底和 K1 的"先 close 再 resume"都要靠它。现有的 `wait_cancel`、`sleep` 收到取消就返回，测不到这两条路径；<br>　- 真实写文件：S1、S3 的快照和 diff 需要工作区里真有改动；<br>　- Steer 挂起（测 5 s 超时）；<br>　- 上游重试（K18）；<br>　- 回显 cwd 和 clientInfo（K13、K26）；<br>　- 先产生输出再 `crash_cli`（K6 的"不重发"分支）。<br>④ K19 的未知枚举值用 SDK 的封闭枚举发不出来；回放模式如果经过 SDK 的类型层，未知值和原样的 `_meta` 也会丢失。 | `spikes/SP-4a-host-skeleton/crates/cupper-fake-agent/src/main.rs`；SP-4a 报告："ACP Rust SDK 有些枚举是封闭的" | 假 agent 分成三部分：<br>- 启动配置文件：persona、能力、配置项、登录状态、故障注入；<br>- 剧本命令：补齐上面缺的；<br>- 原始行通道（`raw <json>`）：回放模式整体走原始行。<br>再加一条全局检查：假 agent 在同一个会话上收到第二个在途 prompt 时立即报错退出，让 K1 成为所有测试都会检查的不变式。 |
| 4 | 重要 | `06-agent-driver.md` 5.6.9 的"回归"列 | 这一列没法直接拆成测试任务：<br>- K1、K2、K4、K6、K11 测的是宿主的行为（排队、替用户应答 `Cancelled`、自动 resume 后重发、只在 Prompting 时 Steer），而引用的 SP-1 录制只能做提取层回放，测不到这些；<br>- K14 写的是"SP-2 数据"，K15 写的是"解析器单元测试"；<br>- K20、K23、K26 写的是"—"；<br>- K20 的回收依赖内存采样，没有注入点就测不了。 | 5.9.2 自己写明 s06 等录制不用于回放模式 | 每条都写成"测试层 + 用例名"。测试层分三种：提取层金样例、宿主集成测试加假 agent、单元测试。<br>K20：给资源采样留注入点。<br>K26：在假 agent 侧断言 `clientInfo.name == "cupper"`。<br>K23：写明"不测"和理由。 |
| 5 | 重要 | `09-testing.md` 5.9.2、5.9.3；`01-repo-build-ci.md` 5.1.6 的 `golden`；`04-data-model.md` 5.4.2 的 raw.db | ① **两处说法矛盾**：5.1.6 写"全部录制经假 agent 回放"，5.9.2 写部分录制只能做提取层回放。实际上，SP-1 的请求序列来自 SP-1 自己的 TS 客户端：`clientInfo` 是 `cupper-sp1-client`；按 SP-1 的安全策略固定发 2–3 次 `set_config_option`；s06 故意并发发送 prompt。这些几乎都不会和宿主的请求序列一致。M1a 开工时，能用于回放模式的录制可能一份都没有，要等维护者手动跑兼容矩阵。<br>② **被测对象没定义**：提取层回放只喂 recv 帧，但条目属于哪个回合，要看 `session/prompt` 和 `session/cancel` 的时机。被测的是什么、金样例里装什么，都没写清。<br>③ **多进程录制**：s07b、s09 中途重启了适配器，每个进程的 seq 从头计数。回放模式怎样模拟"进程退出、再拉起"没写。<br>④ **格式并不"一一对应"**：<br>　- SP-1 的 note 行用 `note`/`data`，不是 `msg`；<br>　- `dir` 是 `recv-nonjson`，raw.db 里是 `recv_nonjson`；<br>　- SP-1 的 `t` 是相对时间，raw.db 存的是绝对的 `ts`；<br>　- 一个会话跨多个适配器实例时，导出怎样合并也没写。 | 逐份查看了 `spikes/SP-1-adapter-behavior/recordings/`，以及 README 中的录制格式 | 金样例分两层：<br>- A 层（提取层）：逐帧转成 `DriverEvent`，是纯函数，48 份录制全部使用；<br>- B 层（会话层）：在进程内运行宿主核心，比对规范化后的事件，只用宿主自己驱动产生的录制，配合第 1 条的虚拟时钟。<br>给每份录制标注用途，CI 检查每份录制都有对应的金样例。<br>把录制格式写成规格：版本号、note 行、多实例分段。<br>M1a 早期用 Claude 跑一次兼容套件，产出第一批 B 层录制。 |
| 6 | 重要 | `09-testing.md` 5.9.3 的规范化规则 | 规范化规则不全，金样例会随机失败：<br>① `item.delta` 怎样切分受 40 ms 写入合并和机器负载影响，事件条数和 seq 都会变；<br>② 摘要 `d_` 由会话 ID 和请求 ID 算出，ID 换成 `s_1` 之后不重算就对不上；<br>③ `data` 里还有不少时间字段（`created_at`、`last_activity_at`、`expires_at`、`requested_at` 等），只去掉信封里的 `ts` 不够；<br>④ `cause.command_id`、适配器实例 ID `a_`、临时目录路径也需要替换。 | 5.3.5、5.3.6 的字段定义；SP-4a 的写入合并是"首个片段立即发出，之后 40 ms 合并一次" | - 同一条目连续的 delta 先折叠再比较；去掉 seq/from，按顺序重新编号；<br>- 在 schema 里给字段加注解（如 `x-id`、`x-time`、`x-path`），规范化按注解进行，新加的字段自动纳入；<br>- 金样例一行一个事件、键排好序，方便评审；<br>- CI 里如果设置了 `UPDATE_GOLDEN`，直接判失败。 |
| 7 | 重要 | 5.9.3、5.9.5；`06-agent-driver.md` 5.6.4；`03-protocol-v1.md` 5.3.6 | 适配器升级后，金样例怎样重新生成、怎样评审，没有写清：<br>- 真实模型的输出每次都不同，重新录制后金样例几乎全变。`UPDATE_GOLDEN=1` 生成的差异没法逐行审，评审会流于形式；<br>- 选项对照表"查不到就兜底，并记日志"，意味着适配器新增的选项 ID 会不声不响地走兜底。 | SP-1 中同一个场景的录制，在 codex-bundled 和 codex-local 之间内容就不一样 | ① 录制按版本目录保留，已验证范围内的旧版本录制和金样例不删。<br>② 做一个"形态差异报告"工具，对比新旧录制里的方法、update 种类、`_meta` 键路径、`option_id`、`stopReason`、错误码。评审看这份报告，而不是事件全文。<br>③ CI 检查：锁定版本的录制里出现的每个 `option_id`、每个权限模式值，都要在对照表和风险表里，否则失败。<br>④ 兼容剧本的断言只检查结构，例如"出现了一个 `exit_code` 不为 0 的工具调用"。模型不配合时允许重试，结果标为"不确定"。 |
| 8 | 重要 | `01-repo-build-ci.md` 5.1.6 的 `compat`；`09-testing.md` 5.9.5 | ① 5.1.6 写在维护者机器上用 self-hosted runner 跑。但这个仓库是公开的，而那台机器登录着用户的 Claude 和 Codex。<br>② 结果怎样记录、怎样比对没有定义，只写了"产出新录制、金样例更新、已验证范围"。<br>③ Q8 的并发剧本没有通过条件。<br>④ SP-1 期间 `~/.codex/config.toml` 被改过，原因不明。用用户的真实配置跑，录制里还会混进用户自己的 MCP 和 hooks。 | GitHub 官方安全指南："self-hosted runners should almost never be used for public repositories"（已核实）；SP-1 报告的"对用户配置的改动"一节 | - 去掉 self-hosted runner，只在本地运行 `cargo xtask compat`，结果以 PR 提交；<br>- 每次运行产出一份机器可读的报告：版本、剧本、结果（通过、失败、不确定）、耗时、配置文件哈希的前后对比、需要清理的 CLI 会话。清单里的"已验证范围"由报告生成，不手填；<br>- Q8 的通过条件见"对待定问题的意见"；<br>- 评估复制凭据、在隔离的 `CODEX_HOME`、`CLAUDE_CONFIG_DIR` 里运行（能否保持登录状态，未核实）。 |
| 9 | 重要 | `02-host.md` 5.2.3 规则 2、5.2.5、5.2.13；`05-flows.md` 5.5.6；5.9.4 | 崩溃测试只在两个时刻 `kill -9`：回合运行中、等待审批时。设计里明确承诺"状态一致"的那些时刻，都没有注入点：<br>- 命令受理后、事务提交前；<br>- 提交后、扇出前（5.2.3 规则 2）；<br>- `session/create` 提交后、第一个回合开始前；<br>- `worktree add` 进行中；<br>- `pending_external` 写入后、git 执行前，以及 git 执行后；<br>- 两个迁移脚本之间；<br>- 写检查点和回合快照的过程中。<br>NFR-9 的"不损坏已保存的历史"也没有写成可判定的断言。 | SP-4a 的 t5、t6 只覆盖进程级的重启 | test-harness 构建里加编译期门控的故障点（做法类似 `fail` crate），由测试打开"执行到此处就退出"。每个故障点都断言：<br>- `PRAGMA integrity_check` 通过；<br>- seq 连续；<br>- 崩溃前客户端看到的历史，是崩溃后历史的前缀；<br>- 用同一个 `command_id` 重试，结果符合账本的语义。 |
| 10 | 重要 | `02-host.md` 5.2.6"新建"、5.2.13 第 6 步；`05-flows.md` 5.5.2 | 推演故障点时发现：`session/create` 的受理事务里只有会话行、`session.configured`、`workspace.preparing` 和命令结果。第一条消息要等工作区准备好、Attach 完成、进入 Preparing 时才写入。<br>如果宿主在这段时间里崩溃（`worktree add` 可能要几分钟），已受理的消息就丢了：客户端重试只会拿回"已受理"，会话停在 `workspace.preparing`，启动恢复也不处理这种会话。 | 5.2.5 的受理语义要求命令结果和它产生的事件在同一个事务里落盘 | - 第一条消息随受理事务一起落盘，例如作为 `queue_entries` 里的一行，或者作为待发内容；<br>- 启动恢复时，对停在 preparing 的会话写 `workspace.failed`，或者重新执行准备；<br>- 补上对应的故障点测试。 |
| 11 | 重要 | `04-data-model.md` 5.4.6；`02-host.md` 5.2.13 的退出码；`07-desktop.md` 5.7.2 | ① 迁移的做法是"逐个执行脚本，最后更新版本号"，每个脚本单独一个事务。如果在两个脚本之间崩溃，重启后已经执行过的脚本会再跑一遍，例如 `CREATE TABLE` 失败，宿主以退出码 5 退出。<br>② 退出码 5 既表示"数据目录不可用"，也表示"迁移失败"（5.4.6 要求迁移失败时界面给出备份位置），界面和测试都没法区分。 | 同左两节的原文 | - 每个脚本的事务里同时更新 `schema_version`；<br>- 迁移失败用单独的退出码；<br>- 加一个"两个迁移脚本之间崩溃"的故障点用例。 |
| 12 | 重要 | `02-host.md` 5.2.8 的持久性；`04-data-model.md` 5.4.2；Q15 | Q15 只在 `NORMAL` 和 `FULL` 之间选，漏掉了 macOS 上真正决定持久性的开关 `fullfsync`。<br>另外，`pending_external` 必须在执行 git 之前真正落盘。否则断电后这条记录没了、提交却已经发生，重试会再执行一次。<br>计划中的验证方法也不成立：`kill -9` 区分不出这几种设置，VM 断电实验在 CI 上又做不了。 | - SQLite 文档：`fullfsync`、`checkpoint_fullfsync` 默认关闭，只有 macOS 支持；WAL + NORMAL 断电后可能丢失最近的事务；"Transactions are durable across application crashes regardless of the synchronous setting"；<br>- macOS 的 `man 2 fsync`：fsync 不保证盘内缓存写入，硬盘还可能打乱写入顺序，数据库应使用 F_FULLFSYNC；<br>- GitHub 的 arm64 macOS runner 不支持嵌套虚拟化。<br>以上都已核实。 | - 按文档决定配置（具体见待定问题 Q15），不依赖 VM 断电实验；<br>- `pending_external` 写入后先强制同步，再执行 git；<br>- CI 里用"高负载写入中反复 `kill -9`、再做完整性检查"来测崩溃一致性。 |
| 13 | 重要 | `07-desktop.md` 5.7.2 的 `inputGuard`、5.7.8 的真实输入检查 | 设计用 `before-input-event` 识别键盘的真实输入。实测发现，Playwright 的键盘输入不会触发这个事件，只触发 `input-event`。结果是"按 Enter 批准"（AP-1）的 e2e 会被当成脚本输入而拒绝。<br>另外，在一次真实点击之后，由脚本调用 `element.click()`，`navigator.userActivation.isActive` 仍然为 true，渲染进程这一层的检查挡不住。真正起作用的是主进程的"每次输入只能用一次"。 | 本机实验，用的是 Electron 34.5.8 和 playwright-core 1.56.1，详见核实记录；Electron 44 未验证 | - 键盘输入也改用 `input-event` 识别（`keyDown`、`rawKeyDown`），并在 M1a 用实体键盘人工确认一次；<br>- 安全 e2e"脚本发送应被拒绝"在写法上要排除前一次真实输入的影响：先把那次输入消费掉，或者等超过 1 秒；<br>- 在 Electron 44 上复测。 |
| 14 | 重要 | `01-repo-build-ci.md` 5.1.3、5.1.5、5.1.6；`10-release.md` 5.10.7；`07-desktop.md` 5.7.9 | "测试代码不进入发布构建"目前只是一句声明，没有自动检查：<br>① `pnpm test` 固定带 `test-harness`。权限矩阵的逐格测试只在测试特性下跑过；发布构建的特性组合下应有的行为（拒绝 harness 认证、没有 `diag/recording_export`、拒绝 `dev_key`）没有测试。<br>② cargo 特性合并可能把测试特性带进发布二进制，SP-4a 已经踩过一次。<br>③ 渲染进程的测试钩子、e2e 专用的主进程钩子，没写门控方式，也没有对产物的扫描。<br>④ fuses、"带调试参数启动就退出"、asar 内容，只在推送 `release/v*` tag 时才检查，发现问题时已经在发版了。 | SP-4a 报告的返工记录（serde_json 特性合并导致 schema 键顺序变化） | - CI 加一次不带任何特性的 `cargo test -p cupper-host`；<br>- 发布构建对每个二进制分别执行 `cargo build -p … --locked`；<br>- 宿主和 CLI 能输出构建信息（启用了哪些特性），发布流水线断言为空；<br>- e2e 钩子用 Vite `define` 的编译期常量门控，并在发布产物里扫描钩子的标记字符串；<br>- 每晚做一次"发布配置、不签名"的构建，跑 fuses、调试参数和产物扫描；<br>- 写明发布版 CLI 是否带 `--harness-key-fd`。 |
| 15 | 重要 | `07-desktop.md` 5.7.9；`08-security.md` 5.8.1；`10-release.md` 5.10.7；NFR-8；Q22 | 发布构建独有的路径，在 CI 里没有端到端覆盖：<br>- e2e 构建用的是 `dev_key`，`codesign` 认证要到 M1b 的升级演练才第一次走通；<br>- 退化模式下，界面崩溃后新 UI 连不上旧宿主（Q22），所以 e2e 构建只能测渲染进程崩溃，测不了 NFR-8 要求的"主进程崩溃后，新 UI 接上旧宿主"；<br>- 安全 e2e"假宿主占用套接字时 UI 拒绝连接"依赖 UI 校验宿主的签名，但 e2e 构建里 UI 怎样校验宿主没写。 | SP-4b 遗留项："没有在 Developer ID 证书上做正向验证，可以放到 CI 上做" | - 发布流水线签名之后加一步冒烟测试：启动签名后的 App，断言走的是 `codesign` 认证、`weak_auth` 为 false、CLI 默认只读、杀掉主进程后新 UI 能接上旧宿主；<br>- 每条安全 e2e 都写明在哪种构建上跑；写明 e2e 构建里 UI 怎样校验宿主。 |
| 16 | 重要 | `09-testing.md` 5.9.4 的 NFR-1 验收表 | 五个用例的写法，有的会不稳定，有的测不到要测的东西：<br>① **第 1 项（两端内容一致）**：比较视图 JSON 时没有同步点；UI 可能多翻了几页历史；`notification.intent` 不进快照；TS 视图的 JSON 形状不在导出的 schema 里。<br>② **第 2 项（CLI 操作）**：CLI 用的是 `test_harness` principal（拥有全部权限），没有走真实用户使用的"`cli` + `elevate` 提权"路径。<br>③ **第 3 项（同时回答）**：UI 有 500 ms 的输入保护，宿主有 400 ms 的最短间隔，Playwright 点得太早会被忽略，"两端同时回答"的结果随时序变化。<br>④ **第 4 项（断线补齐）**：重载渲染进程会丢掉内存里的游标，走的是快照，不是续传，UI 按游标续传的路径没被测到。<br>⑤ **第 5 项（宿主被杀后继续）**：什么时候执行 `kill -9`，没有用事件做屏障。 | 5.3.3（快照内容）、5.3.5（`notification.intent` 不进快照）、5.7.7（卡片出现 500 ms 后才响应）、5.2.5（宿主 400 ms） | ① 两端都到达同一个 `head_seq` 再比较，只比窗口内的内容，排除 `notification.intent`，视图的序列化格式纳入 schema。<br>② 至少一个用例用 `cli` 加 `elevate`，原生确认框用替换函数批准。<br>③ 卡片加一个"可以操作"的 DOM 标记，测试等标记出现再点击；断言写成"恰好一个成功，另一端显示'已在别处处理'"，不预设谁赢。<br>④ 加一个"只断开这个窗口的数据连接、不重载"的用例（用主进程的测试钩子）；`host-client` 暴露缺口、重置、重复的计数，供断言使用。<br>⑤ 用事件做屏障（等某个条目或交互请求出现），不用 sleep。 |
| 17 | 重要 | `12-milestones.md` 5.12.2、5.12.3 的退出标准 | - M1a 的"全部由自动化测试通过"没有稳定性要求，也没写在什么环境跑，跑一次全绿就算通过；<br>- M1b 的"作者用两种 agent 完成 S1–S4"是人工验证，但没有检查表；<br>- "NFR-2、NFR-9 的验收测试通过"没有列出具体是哪些用例。 | SP-4a 的端到端测试完整跑了 10 遍 | - M1a：NFR-1 套件在 CI 的 macOS runner 上连续 20 次全绿，不允许自动重试；<br>- M1b：写一份 S1–S4 × 两种 agent 的逐步检查表，包括步骤、通过条件、CLI 和适配器的版本、日期，结果存档；<br>- NFR-2、NFR-9 列出用例编号，NFR-9 的断言按第 9 条。 |
| 18 | 重要 | `12-milestones.md` 5.12.4 追溯矩阵的"验证"列 | - 大多数行只写了"e2e""单元测试""截图回归"，拆不出测试任务；<br>- NFR-4、NFR-7 写的是"—"。NFR-7 的降级完全可以用假 agent 的能力开关来测；<br>- SEC-6 的"e2e 抓包"在 CI 上怎样做没写；<br>- 只能人工验证的项目分散在各处，没有汇总。 | 矩阵原文 | - 给测试编号（IT-、E2E-、GOLD-、PROP-、MAN-），矩阵引用编号，并写明在哪个套件里跑（每个 PR、每晚、人工）；<br>- 新增"人工验证清单"小节，列出：S1–S4 真实 agent、AP-3 系统通知、UI-6 视觉、SP-6 的中文输入法、升级演练、Q2/Q21 的 TCC 和通知身份、Q3 的 Node 公证、兼容矩阵；写明谁做、什么时候做、结果记在哪里；<br>- SEC-6 和 NFR-4 选定一种可执行的网络外发检查方法，例如在 runner 上用 `nettop` 采样，或者让流量经过本地代理（两者的可行性都未核实）。 |
| 19 | 重要 | `05-flows.md` 5.5.5；`06-agent-driver.md` 5.6.9 的 K3；`03-protocol-v1.md` 5.3.5；`02-host.md` 5.2.8；Q14 | 晚到的工具输出被丢弃后，标记写在哪里，三处说法不一致：原工具条目上的一个字段、一个 `notice` 条目、"原条目标 `late_output_discarded`"。<br>原回合此时已经写了 `turn.completed` 和回合快照。再去修改它的条目，`turn_snapshots` 就和事件日志不一致，`history/page` 拿不到这个标记，金样例也没法定义期望结果。 | 5.2.8：检查点和回合快照在回合结束时写入 | - 定为在当前流里写一个 `notice` 条目，挂在当前回合或自发回合上，引用原工具的 `item_id`，每个工具只写一次；<br>- 或者明确回合快照允许事后追加。<br>用例见待定问题 Q14。 |
| 20 | 重要 | `09-testing.md` 5.9.6、5.9.7 | 性能预算给了阈值，但测量方法、环境和判定口径不全，每晚跑必然时好时坏：<br>① CI 的 arm64 runner 是 3 核、7 GB 的虚拟机，作者的开发机性能高得多（存档时编排者去掉了具体机型），两边却用同一个绝对阈值；<br>② "可用"的终点标记、什么算"冷"启动、用多大的数据集，都没定义；<br>③ 帧时间、agent 输出到屏幕的延迟、渲染进程内存，没写用什么方法测；<br>④ 主进程事件循环延迟如果用 `monitorEventLoopDelay` 的默认采样间隔 10 ms，测不准 5 ms 的阈值；<br>⑤ 只有 10 个样本时，p95 实际上就是最大值；<br>⑥ SP-6 以哪台机器的结果为准没写；场景 5（浸泡 8 小时）没有通过条件；场景 2 的输入延迟没有预算。 | GitHub runner 的规格；Node 文档中 `resolution` 默认为 10；MDN：Long Animation Frames 只报告超过 50 ms 的帧。以上都已核实 | - 绝对预算只在参考硬件上判定（记录机型）；CI 只看趋势，阈值取相对历史中位数的倍数（例如 1.5 倍）；<br>- 冷启动：从进程启动计到渲染进程的 `performance.mark('cupper:ready')`，这个标记表示索引快照已应用、当前会话已渲染、输入框可用；数据集为 200 个会话，当前会话 1,000 条消息；首次运行时系统的安全检查单独计时；<br>- 帧时间：用 requestAnimationFrame 的间隔或 trace 测量；<br>- 输出到屏幕的延迟：由假 agent 在输出片段里嵌入时间戳，渲染进程用 MutationObserver 记录上屏时间；<br>- 主进程事件循环延迟：设 `resolution: 1`；<br>- 浸泡测试：写明预热后各进程内存增长的上限。 |
| 21 | 重要 | `01-repo-build-ci.md` 5.1.6 | CI 的代价和排队没有估算：<br>- 每个 PR 有 6 个 macOS 作业，其中 e2e 还要打包 Electron；<br>- Free、Pro、Team 计划下，标准 macOS runner 最多同时跑 5 个作业；<br>- 截图回归依赖系统字体（SF Pro、苹方），基线会随 runner 镜像变化。 | GitHub Actions 的并发限制（已核实） | - 不依赖 macOS 的作业放到 ubuntu：许可证门禁、Vitest、schema 兼容比对、TS 与 Rust 视图对比（`cupper-protocol` 不做 I/O，可以在 Linux 上编译）；<br>- `rust` 和 `golden` 合成一个作业，共用构建缓存；<br>- e2e 冒烟按改动路径过滤；<br>- 固定 runner 标签（例如 `macos-15`，不用 `macos-latest`）；<br>- 截图基线只由 CI 的专用工作流生成。 |
| 22 | 建议 | `09-testing.md` 5.9.1、5.9.8 | 性质测试只列了原有的不变式，新设计引入的几条没有覆盖：<br>- 窗口快照加上全部 `history/page`，等于从头重放日志；<br>- 检查点加上其后的事件，等于从头重放（懒加载）；<br>- 按保留规则裁剪后，重建结果不变；<br>- 合并后应用等于逐条应用（ADR-04 要求，5.9.1 只写了"合并"两个字）。<br>"并发的订阅与追加"用真实线程跑，能不能撞上问题全凭运气。TS 和 Rust 视图只用金样例对比。 | ADR-04、ADR-06 中的性质测试要求 | - 补上这四条性质测试；<br>- `StreamState` 的追加和订阅协议用 loom 或 shuttle 穷举线程调度；<br>- TS reducer 与 Rust 视图做差分测试：Rust 用 proptest 生成事件序列，交给 Node 进程处理，比对两边的结果。 |
| 23 | 建议 | `02-host.md` 5.2.4 的分发、5.2.13 的自检 | D01 新改的两个行为没有对应用例：<br>- 同一条连接上发出 `send` 后不等回复就发 `interrupt`，执行顺序要有保证（SP-4a 是每个请求一个 task，顺序没有保证）；<br>- cdhash 不一致时进入"待升级"。 | 5.2.15 的 SP-4a 改动表 | - 前者：在一条连接上连发两条命令、不等回复，断言执行顺序；<br>- 后者：集成测试里把 ad-hoc 签名的宿主复制到临时 bundle，运行后替换磁盘上的文件。 |
| 24 | 建议 | `10-release.md` 5.10.4、5.10.5；`04-data-model.md` 5.4.6 | - 升级演练只在首次发布前做一次；<br>- 迁移测试只用静态的样例数据库；<br>- "宿主同时支持协议 N 和 N−1"没有测试。 | NFR-10 | - 每个改动了 schema 或运行时的版本，都按检查表做一次升级演练；<br>- CI 用上一个发布版的宿主加假 agent 生成真实的数据库，再用新宿主迁移；<br>- 用上一版的 CLI 连接新宿主。 |
| 25 | 建议 | `09-testing.md` 5.9.3 的脱敏 | `sanitize` 复扫的规则取自运行环境（`HOME`、`USER`、主机名）。在 CI 上跑，它只能查出 runner 自己的用户名，查不出维护者的用户名。 | SP-1 的 README 中脱敏一节 | 每个 PR 对 `testdata/` 再做一次通用模式的扫描，命中就失败：`/Users/` 后跟非占位符的路径、邮箱、token 样式、`/var/folders`。 |
| 26 | 建议 | 5.12.4 的 CP-1；5.9.7 场景 6 | "输入法组合过程中按 Enter 不发送"完全靠人工检查。 | — | 组件测试里用 `isComposing` 为 true、keyCode 为 229 的键盘事件覆盖判断逻辑；人工检查表只保留真实输入法的体验部分。 |

## 对待定问题的意见

| 问题 | 意见 |
|---|---|
| Q1 连接模型 | **同意。** 从测试角度看是加分项：<br>- 每个窗口一条连接，才能做"只断开这一个窗口"的续传用例（第 16 条④）；<br>- control 连接由主进程解析，可以用帧级的 mock 宿主给主进程写单元测试，覆盖命令白名单、真实输入、原生确认、流控。<br>需要补两点：<br>- TS 的 mock 宿主要有套接字和帧级的版本（目前只有给浏览器截图测试用的那一种）；<br>- "超过 8 MB 暂停读取 → 宿主重置流 → 按游标重新订阅"列为一个集成用例。 |
| Q4 codex-acp 随包 | **同意随包。** 好处是 Codex 侧离线、版本固定，e2e 和兼容运行都不依赖 npm。<br>条件：<br>- claude-agent-acp 仍然要联网安装。安装器测试需要可注入的 registry 地址（只在 test-harness 中存在）和本地的包夹具，才能离线覆盖 SP-2 的 5 种故障；<br>- `runtime-pack --verify` 每晚跑一次，不要只在发版时跑，以便尽早发现上游 tarball 或哈希的变化。 |
| Q11 `threadStatus` 与回合边界 | **同意"有 prompt 在途时以 prompt 的响应为准"。** 用例：<br>- 用 codex s05 的录制做提取层回放；<br>- 假 agent 组合 `thread_status idle` 和 `plan_confirm`，构造"线程状态已是 idle、回合仍在等确认"的情形；<br>- 状态机性质测试加一条："prompt 在途时，任何 `RunState` 都不会结束回合"；<br>- "收到 idle 后结束自发回合"这个分支单独一个用例。 |
| Q14 晚到的工具输出 | **同意"不进入新回合，同时让用户看得到"，** 但要先定标记写在哪里（第 19 条）。用例：<br>- `late_tool` 分别在新回合进行中、会话空闲时到达；<br>- 同一个工具多次晚到，只标一次；<br>- `toolCallId` 不属于被取消回合的回合外更新，按 K9 开自发回合。 |
| Q8 同一适配器上多会话并发 | **要在 M1a 写驱动层之前测完。** 否则驱动层按"多会话共用一个适配器"写完再退回，要返工。<br>判定条件要写清，例如：两个会话同时各发 3 个 prompt，全部在 5 分钟内返回，输出没有串到另一个会话，重复 3 轮。<br>宿主侧加集成测试：6 个会话在同一个假适配器进程上同时流式输出，每条输出带会话标识，断言没有串扰（同时覆盖 NFR-11）。后备方案（该 agent 的 prompt 全局串行）也要有用例。 |
| Q10 自发回合的结束判定 | **误判不只影响状态显示。** 自发回合一结束，队列就会出队；如果 Claude 其实还在自发工作，出队的 prompt 就会和它并发，正是 K1。<br>建议：<br>- 出队前加保护，例如要求先收到带 `cost` 的 `usage_update`，或者等一段更长的静默；<br>- 用假 agent 的 `autonomous` 构造"静默超过阈值后又继续输出"的用例；<br>- 阈值走注入的时钟（第 1 条）。 |
| Q15 断电与 `synchronous` | **建议不做 VM 断电实验，**原因有三：<br>- GitHub 的 arm64 runner 不支持嵌套虚拟化；<br>- 本地 VM 强制断电模拟不了硬盘写缓存的丢失和重排（推理，未实测）；<br>- `kill -9` 区分不出这几种设置。<br>建议按文档决定：<br>- WAL 加 `synchronous=NORMAL`，再打开 `checkpoint_fullfsync`（或 `fullfsync`），保证断电后不损坏；<br>- 需要持久的少数写入（`pending_external`）单独强制同步。<br>如果一定要做断电级的测试，可以在 SQLite 的 VFS 层注入"丢掉最后一次同步之后的写入"（Rust 侧有没有现成实现，未核实）。 |
| Q16 保留参数 | **先按这些值实现，**但每个值都要能在 test-harness 中覆盖，并补边界的性质测试（例如游标正好落在保留边界上）。<br>"M1b 用实际数据调整"的前提是先有数据：宿主在本地日志里记录每次订阅的模式（续传还是快照），以及游标落后了多少。 |
| Q24 macOS 14、15 上的对端校验 | **macOS 15 不用等，现在就加。** `macos-14`、`macos-15` 的 arm64 标签都在 GitHub 标准 runner 的列表里（已核实）。<br>macOS 14 是最低支持版本，"失败只开 issue、不阻止"太松：<br>- 发布流水线应要求最近一次 14、15 上的校验通过；<br>- 改动 `cupper-sys` 的 PR 按路径过滤，直接在两个版本上跑。<br>GitHub 会定期下线旧镜像，macOS 14 下线后需要替代方案（下线时间未核实）。 |
| Q25 Codex `revise_plan` 与权限模式取值 | **同意由兼容矩阵补测。** 同时把"录制里出现的 `option_id` 和权限模式值都必须在对照表里"做成 CI 检查（第 7 条），这样新出现的取值不会悄悄走兜底。 |
| Q9 SDK 的扩展消息 | **补充一点：**如果 SDK 的 agent 侧也不支持扩展消息，假 agent 同样需要原始行通道（第 3 条），否则 `$/cancel_request`、`_auth/status_update`、`_session/steering` 都模拟不了。 |
| Q22 退化认证模式的局限 | **从测试角度补充一点：**接受这个局限，就意味着 NFR-8 的"界面崩溃不影响会话"只能在签名构建上验证，需要靠第 15 条的"签名后冒烟"来补。 |

## 认可的部分

- "所有自动化测试都不需要登录、不消耗额度，只有兼容矩阵用真实 CLI"这条原则清楚，分层合理。
- 沿用 SP-4a 的 Harness、Mirror 的 `from` 连续性检查、`kill -9` 恢复、命令幂等，以及"去掉 CLOEXEC 应失败"的变异检查，起点很好。`normalize.rs` 已经把 `now` 作为参数。
- 命令结果与事件在同一事务里落盘、principal 改用稳定 ID、账本内存表常驻，让重试语义可以确定性地测试。
- 金样例严格模式（"类型已知但数据残缺"直接失败）、TS 与 Rust 视图用同一批金样例交叉验证、schema 与生成代码的一致性检查、整数安全检查，这些都在 CI 里落了地。
- e2e 构建与发布构建打包方式相同，只保留 `enableNodeCliInspectArguments`。这正是 Playwright 文档要求的（这个 fuse 关闭时启动会超时）。
- SP-5 的 82 个 hunk 用例改写成 Rust 测试，7 条清理规则逐条覆盖。权限矩阵用表驱动测试逐格断言。
- 对照表里查不到的选项、风险表里没有的权限模式，一律按最高风险处理。这样的默认对测试友好：漏掉的取值只会表现为多一次确认，不会悄悄放行。

## 核实记录

**外部来源**（请求不带任何个人信息；需要 User-Agent 时用 `cupper-research`）

- Playwright Electron API（https://playwright.dev/docs/api/class-electron）：
  - 对 Electron 的支持标为 experimental，支持 Electron v21 及以上；
  - 启动超时时，要检查 `nodeCliInspect` fuse 没有被关闭；
  - 不拦截主进程里的原生对话框，需要用 `electronApp.evaluate()` 替换。
- GitHub 托管 runner（https://docs.github.com/en/actions/reference/runners/github-hosted-runners）：
  - `macos-14`、`macos-15`、`macos-26` 是 arm64 (M1) 的标准 runner，3 核、7 GB 内存、14 GB SSD；
  - arm64 macOS runner 不支持嵌套虚拟化；
  - 文档没有提到 GPU 加速。
- GitHub Actions 的限制（https://docs.github.com/en/actions/reference/limits）：Free、Pro、Team 计划下，标准托管 runner 最多同时运行 5 个 macOS 作业。
- GitHub Actions 安全使用指南（https://docs.github.com/en/actions/reference/security/secure-use）："self-hosted runners should almost never be used for public repositories"。
- SQLite PRAGMA 文档（https://www.sqlite.org/pragma.html）：
  - `fullfsync`、`checkpoint_fullfsync` 默认关闭，只有 macOS 支持；
  - WAL 加 NORMAL 不会损坏数据库，但断电后可能丢失最近的事务；
  - 应用崩溃时，无论 `synchronous` 怎样设置，已提交的事务都是持久的。
- 本机 `man 2 fsync`：fsync 不保证盘内缓存写入，硬盘可能重排写入顺序；数据库应使用 F_FULLFSYNC。
- tokio `time::pause`（https://docs.rs/tokio/latest/tokio/time/fn.pause.html）：只能用于 current_thread runtime；只影响 tokio 自己的 `Instant`；`spawn_blocking` 运行期间不会自动推进时钟。
- Node `perf_hooks.monitorEventLoopDelay`（https://nodejs.org/api/perf_hooks.html）：`resolution` 默认 10 ms。
- MDN 的 Long Animation Frames（https://developer.mozilla.org/en-US/docs/Web/API/PerformanceLongAnimationFrameTiming）：只报告超过 50 ms 的帧，不能用来测 16.7 ms 的帧时间。
- Electron 的 webContents 文档（https://www.electronjs.org/docs/latest/api/web-contents）：`before-input-event` 在页面派发 keydown、keyup 之前触发；`input-event` 在有输入事件发给 WebContents 时触发。

**小实验**

在会话临时目录里，用本机缓存的 Electron 34.5.8 和 playwright-core 1.56.1 做了一个最小应用：主进程记录 `before-input-event` 和 `input-event`，页面记录点击事件的 `isTrusted` 和 `navigator.userActivation.isActive`。实验目录已删除。结果：

| 操作 | 主进程收到的事件 | 页面记录 |
|---|---|---|
| Playwright `page.click()` | `input-event`：mouseMove、mouseDown、mouseUp | `isTrusted` 为 true |
| Playwright 键盘输入（窗口已聚焦，`isFocused()` 为 true） | 只有 `input-event`（keyDown、keyUp）；`before-input-event` 一次也没有触发 | — |
| 主进程 `webContents.sendInputEvent`（对照） | 会触发 `before-input-event` | — |
| 页面脚本 `element.click()`，紧跟在一次真实点击之后 | 没有任何事件 | `isTrusted` 为 false，但 `userActivation.isActive` 仍为 true |

结论用于第 13 条。Electron 44 上没有复测。

**阅读核对**

- 逐份查看了 SP-1 录制的格式和请求时间线（claude s04、s06，codex-bundled s07）：note 行的写法、`recv-nonjson`、SP-1 客户端的 `clientInfo` 和配置项请求序列。
- 读了 SP-4a 的假 agent、`e2e.rs`、`support/mod.rs`，以及 `session.rs` 和 `normalize.rs` 里的时间用法。
- 读了 SP-5 报告（82 个 hunk 用例和 7 条清理规则）、SP-4b 报告的遗留项。

**未核实**

- GitHub macOS runner 有没有 GPU 加速，对渲染帧时间的测量有多大影响；
- `macos-14` 镜像的下线时间；
- 每个 PR 的 e2e（构建加打包）在托管 runner 上的耗时；
- 复制凭据后，在隔离的 `CODEX_HOME`、`CLAUDE_CONFIG_DIR` 里能否保持登录；
- Rust 生态里有没有现成的、能模拟断电的 SQLite VFS；
- 在 runner 上检查网络外发的具体方法（`nettop` 采样或本地代理）；
- 本地 VM 强制断电能否模拟硬盘写缓存的丢失和重排（推理认为不能，未实测）；
- Electron 44 上 Playwright 键盘输入与 `before-input-event` 的关系。