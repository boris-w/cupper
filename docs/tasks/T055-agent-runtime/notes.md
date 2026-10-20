# T055 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T088、T097 的执行者）

## 开工前读过的

- 任务书；`handoff-notes.md` 第一列含 T055 的 13 行，逐行读过；
- 设计 5.6.1、5.6.2、5.6.6–5.6.10；
- 现有代码：
  - `driver/acp.rs`（`AcpConnection`、`FromAgent`）、`driver/claude/`（`ClaudeSession`、`ask`、`capabilities`）、`driver/locate/`、`driver/probe/`、`driver/tap/`；
  - `diag/stderr.rs`（`AdapterLog`）、`supervisor/mod.rs`（`Part`、`Host::start`）、`startup/test_config.rs`（`agents`、`login_env`、`rss_samples`）、`manifest.rs`、`runtime/manifest.json`；
  - `cupper-fake-agent` 的参数和脚本命令。
- **两点发现，影响范围**：
  - 内嵌清单里有 node 和适配器 bundle 的 sha-256（`files`、`claude_agent_acp.bundle_sha256`），但运行时目录（`<数据目录>/runtime/...`）由安装器 T056 建立，还没有实现。所以生产路径"拉起前核对"能写、能用造的文件测，但拉不起真的适配器。用例一律走 `test_config.agents` 注入的假 agent。
  - 假 agent 的 `spawn_child` 只在自己的进程组里拉起孙进程；IT-K21 要的"后代在另一个进程组"需要给假 agent 加一个 `setsid` 的变体。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释。草估之后按 T043、T088、T097 的实测比例放大（1.03–1.28 倍，取 1.15）。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | `driver/runtime/mod.rs` | `AgentRuntime` actor：邮箱（会话的命令、`FromAgent`、子进程退出、定时器）；当前实例；路由表（`agent_session_id` → 会话的事件发送端和 `ClaudeSession`）；在途 prompt（`PromptTag` → 会话）；`AskId` → `Responder`；实现 `AgentPort` | 350 |
| 2 | `driver/runtime/launch.rs` | 拉起：注入的程序和参数，或运行时目录里的 node 加 adapter（按内嵌清单核对 sha-256）；`CliLocator`、`adapter_env`、登录 shell 环境；`cupper_sys::spawn`（setsid、管道）；写 `adapters` 表；stderr 接 `AdapterLog`；`StdioTransport` 加 `Tap`（共用一个 `RawSink`）加 `AcpConnection::open` | 200 |
| 3 | `driver/runtime/commands.rs` | `Attach`（`session/new`、`resume`，`settingSources` 的 `_meta`，之后 `set_config_option`）、`Prompt`（`PromptContent` → ACP 内容块：文本、`text_blob` 与图片读 blob、`file_ref` → `resource_link`、评论按 GT-2 排成文本）、`Cancel`、`SetConfig`、`Close`、`Answer`（权限与 elicitation 的回复） | 220 |
| 4 | 事件 | `FromAgent` → `ClaudeSession::update` → 按会话路由；权限、elicitation → 分配 `AskId`，`cancellation()` 另起 task 转成 `AskWithdrawn`；prompt 的回复 → `PromptDone`、`PromptFailed`；登录状态 | 150 |
| 5 | 退出与重启 | 子进程退出 → 广播 `AdapterExited`；退避 1、2、4…30 s；5 分钟内 5 次失败 → `crashed`；`cli_too_old` 的改判；时长登记 | 100 |
| 6 | 关闭（`Part::stop`） | 逐个 `session/close` → drop 连接关 stdin → 限时等待 → 枚举后代的进程组 → 逐组 TERM、限时、KILL | 90 |
| 7 | 能力缓存与探测 | `agent_capabilities` 表的读写与作废；探测目录 0700 重建、`session/new`、等命令列表 200 ms、`session/close`；`steering.supported` 从 `_meta` 读；`Versions` 从 CLI 定位那边拿 | 120 |
| 8 | 资源采样 | 30 s 一次：进程组内存、后代的进程组记进 `adapters` 表、`claude` 进程数（Q43）；`rss_samples` 注入 | 90 |
| 9 | `agents/capabilities`、`agents/refresh`、`AgentStatus` | 两个方法的端口与登记（从 UNIT-unsupported-set 删行）；重新跑登录 shell 和定位；语义码到协议字段的映射 | 130 |
| 10 | 宿主组装 | `Host::start`：登录 shell 环境、`CliLocator`、`RawSink`、`AgentRuntime`、`SessionManager`（`audit` 用 `records`，`index` 用 `NoIndex`）、`routes.sessions`、`Part` 的顺序；`SessionManager` 实现 `Part`（`activity`；`stop(force)` 要让 actor 把运行中的回合记为 `interrupted` 并写检查点，actor 里加一种邮件） | 180 |
| 11 | 杂项 | `CommandsChanged` 去重（按会话记摘要）；`driver/tap` 的文档链接；假 agent 加 `setsid` 的孙进程 | 40 |
| | 实现小计 | | **约 1,670**，×1.15 ≈ **1,920** |

**测试**

| 用例 | 内容 | 草估 |
|---|---|---|
| 夹具 | 假 agent 的路径（见下）；临时数据目录、`AgentRuntime` 的直接驱动；`test_config` 的组装 | 80 |
| IT-process-hygiene | fd 不泄漏、环境只有白名单、自成进程组 | 60 |
| IT-K13-echo-cwd、IT-K26-client-info | `echo_env` 回显 realpath 和 `clientInfo.name` | 50 |
| IT-K21-reap-group | 关闭后两个进程组都没有残留，包括 `setsid` 的孙进程 | 60 |
| IT-adapter-restart | 退出广播、退避、5 次后 `crashed`，直接驱动 `AgentRuntime` | 80 |
| UNIT-commands-dedup | 相同列表不再发事件 | 30 |
| 宿主一个回合 | 真实宿主进程加假 agent，经网关 `session/create` 跑通一个回合 | 100 |
| 测试小计 | | **约 460**，×1.1 ≈ **500** |

**合计约 2,400 行**，是上限的 1.6 倍。按任务书，开工前停下，没有写代码。

## 拆分建议

**方案一（建议），拆成三份：**

| 拆分 | 内容（模块编号） | 用例 | 估计 |
|---|---|---|---|
| T055（收窄）运行时核心与宿主组装 | 1、2、3、4，5 里的退出广播，10，11 里的文档链接 | IT-process-hygiene、IT-K13-echo-cwd、IT-K26-client-info、宿主一个回合 | 约 1,450 |
| T055b 生命周期 | 5 里的退避与 `crashed`，6，8，11 里的假 agent `setsid` | IT-K21-reap-group、IT-adapter-restart | 约 650 |
| T055c 能力与状态 | 7、9，11 里的 `CommandsChanged` 去重 | UNIT-commands-dedup，以及能力缓存、`agents/refresh` 的用例 | 约 550 |

- 第一份做完，宿主就能经网关用假 agent 跑通回合，T047、T063 的大部分用例可以开始。
- 第二、三份都只依赖第一份，可以并行；两份都会在 `runtime/mod.rs` 上加接线，冲突在几处调用点。

**方案二（任务书举的两分法）**：

- 第一部分约 1,850（方案一的第一、二份合并）；
- 第二部分约 550。

第一部分超出上限约 25%。

**范围上请确认一点**：生产路径的"拉起前核对"要用运行时目录（T056 安装），现在只能拿造的文件测哈希核对，真的 node 加 adapter 跑不起来。建议本任务只做核对函数和"注入或运行时目录"两种拉起方式的分支，由 T056 接上真正的目录。

## 假 agent 的路径（打算的做法，开工后落实）

宿主 crate 的测试里，用 `escargot` 一类的库会多一个依赖。打算这样做：

- 测试辅助按 `CARGO_TARGET_DIR`（缺省为工作区的 `target/`）和当前 profile 推出 `target/<profile>/cupper-fake-agent`；文件不存在时提示先 `cargo build -p cupper-fake-agent`，不在测试里调用 cargo。
- `xtask ci` 的 rust 作业在跑测试之前，先构建一次 `cupper-fake-agent`。

## 收窄之后（裁定：方案一）

### 实际规模

按 `git diff --numstat`（相对估计提交 5de77db，不含 `Cargo.lock`）：新增 1,590 行、删除 35 行。实现约 1,360，测试约 230。上限约 1,500，硬线 1,600。

| 模块 | 文件 | 新增 |
|---|---|---|
| 拉起、核对、内容与回答的转换 | `driver/runtime/launch.rs`（含核对函数的单测 26 行、`fake_agent` 13 行）、`content.rs`，`manifest.rs`、`api.rs` 的两个错误类 | 约 520 |
| `AgentRuntime` actor | `driver/runtime/mod.rs` | 670 |
| 宿主退出时的会话 | `turn.rs` 的 `HostExit`，`session/actor.rs`、`manager.rs` 的停止信道与 `Part` | 约 70（另有测试 36） |
| 宿主组装与发现 | `supervisor/mod.rs`、`locate/mod.rs` 的 `boundaries()` | 约 127 |
| 测试 | `runtime/tests.rs`、`tests/lifecycle.rs`、xtask 构建假 agent | 约 156 |

超出约 1,500 的主要原因：`mod.rs` 比草估（350 + 150 + 220 的一部分）多，实例结束的顺序处理（见下）和请求撤回的路由占了不少；协调要求把发现链拆成独立函数，又多了约 20 行。

### 做法与取舍

- **一个实例一条线**：回复、更新、`Started`、`Exited` 都经同一个按实例的信道送进 actor，`Started` 一定在 `Exited` 之前；连接的 `Closed` 也在这条线上。**实例在"进程已退出"且"连接已关闭"两者都到之后才结束**，这样适配器最后写出的更新不会丢。进程退出时，再给它的进程组发一次 SIGKILL，让组里残留的进程放开管道。
- 退出状态先写进 `adapters` 表（等写线程提交），再广播 `AdapterExited`。
- `$/cancel_request` 撤回的请求只发给提出它的会话（`AskWithdrawn`）；会话答 `Cancelled` 之后，`Responder` 才回给 agent。
- cwd 在 `session/new`、`session/resume` 之前统一 `canonicalize`（K13），不依赖调用方。
- 选项在 attach 时逐个 `set_config_option`，只设与当前值不同的。
- `Part::stop(force)`：会话先停，强制时运行中的回合立即 `interrupted`（不等结束快照），请求 `expired`（`host_restarted`），队列以 `turn_interrupted` 暂停、不再出队；运行时随后给适配器进程组发 SIGTERM（强制时 SIGKILL），等实例结束。没有超时，5.6.8 的完整顺序是 T008 的。
- **发现链**：`supervisor::discover(login_env, clock, durations, reader) -> Discovery { host, login, locator }`。`host` 是宿主自己的环境，`login` 是登录 shell 的环境（`test_config.login_env` 可以覆盖），`locator` 是建在登录 PATH 可信条目上的 `CliLocator`（边界是所有未移除项目的路径加系统临时目录）。`launch::production(..., host, login, &locator)` 复用它。T034 的 `GitTool` 可以从同一个 `locator`（`tool("git")`、`trusted_path()`）取。
- 登录 shell 按设计 5.6.6 在宿主启动时跑一次（5 秒超时），会推迟套接字开始监听的时间。IT-lifecycle 的宿主一律传空的 `login_env`，测试不会跑用户的登录 shell，也不会跑真实的 `claude --version`。

### 假 agent 的路径（已落实）

- `launch::fake_agent()`（`test` 或 `test-harness`）：从测试二进制所在的 `target/<profile>/deps/` 往上一级，取 `target/<profile>/cupper-fake-agent`；文件不在就失败并提示先 `cargo build -p cupper-fake-agent`。测试里不调用 cargo。
- `xtask ci` 的 rust 作业在 `cargo test` 之前加了一步 `fake-agent`（`cargo build -p cupper-fake-agent --locked`）。

### 交接

- **T008**：
  - 重启目前没有退避：实例结束后，下一次 attach 立即拉起新实例；
  - `stop` 没有限时；
  - 只给适配器自己的进程组发信号。如果一个 `setsid` 的孙进程拿着适配器的 stdout，连接就一直不 `Closed`，实例就一直不结束。所以枚举后代的进程组、逐组发信号这一步要在实例结束之前完成。
- **T009**：
  - attach 返回的能力里，命令列表是空的；
  - `AuthStatus` 被忽略；`Probe` 命令被丢弃；
  - `agent_capabilities` 表还没有写入。
- **T044**：`SetConfig` 命令被丢弃（attach 时的选项已经设置）。
- **T140**：`Steer` 被丢弃。
- **T056**：生产路径要用到的两个文件，都要按内嵌清单 `files` 核对：
  - `<数据目录>/runtime/node`，对应清单里的 `Helpers/node`；
  - `<数据目录>/runtime/bundle-claude-acp.mjs`，对应 `Resources/runtime/bundle-claude-acp.mjs`。
  
  实际目录布局由 T056 定，届时改 `launch::production` 里的两个路径即可。
- **T034**：发现链见上。现在的边界只有项目路径；工作区和 worktree 的根目录要等它们出现后再加。
- 第二种 agent 接进来时，需要一个自己的 `ActorKind`，因为 `AGENT_RUNTIME` 只能登记一次。
- **写适配器退出 `error` 条目的任务**（T008 的 `crashed` 一带）：每个实例的 stderr 尾部（`Instance.tail`，最后 64 KB，已脱敏）已经留着，但 `DriverEvent::AdapterExited` 还不带它，`error` 条目的详情里还没有。
- 设置里的 `agents.env_passthrough` 还没接：`adapter_env` 目前拿到的放行名单是空的。

### 范围外的发现

- `driver/tap/mod.rs` 指向 `diag::redact` 的文档链接已经能解析（`diag::redact` 是公开的）。`RUSTDOCFLAGS=-D warnings cargo doc` 失败另有两处，都是指向私有项的链接，这次一并改了，各改一行：
  - `driver/probe/login_shell.rs` 的 `KEPT`（T094）；
  - `orchestrator/session/mod.rs` 的 `actor`（T043）。
- `orchestrator::testkit` 改为 `pub(crate)`，供运行时的测试使用。
