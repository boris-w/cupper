# T055 汇报（收窄版）

- 执行者：子 agent（Rust 工程师；T043、T088、T097 的执行者）
- 分支：`feat/T055-agent-runtime`
- 完成日期：2026-10-20

**先回复协调事项（发现链）**：这部分我已经写好了，T034 可以直接复用。

- 函数是 `supervisor::discover(login_env, clock, durations, reader) -> rusqlite::Result<Discovery>`，定义在 `crates/cupper-host/src/supervisor/mod.rs`，`Host::start` 里调用一次。
- 它返回 `Discovery { host, login, locator }`：
  - `host`：宿主自己的环境，`BTreeMap<String, String>`；
  - `login`：登录 shell 的环境，`LoginEnv`。`test_config.login_env` 有值时直接用它，经 `injected()` 传进来；
  - `locator`：`CliLocator`，建在登录 shell PATH 的可信条目上，边界是所有未移除项目的路径加上系统临时目录。
- 我给 `CliLocator` 加了 `boundaries()` 访问器，`check_config_dir` 要用。
- T034 的 `GitTool` 可以从同一个 `locator` 取（`tool("git")`、`trusted_path()`），再往 `Discovery` 里加一个字段即可。
- Claude 这边由 `claude_launch(..., &Discovery)` 调 `launch::production(clock, durations, data_dir, host, login, &locator)`。

## 做了什么

1. **拉起适配器**（`driver/runtime/launch.rs`）：
   - 两种拉起方式：
     - 测试构建用 `test_config.agents` 注入的程序；
     - 生产用运行时目录里的 node 加 bundle，拉起前按内嵌清单的 `files` 核对 sha-256，清单写了 cdhash 的也核对。
   - 生产路径用 `CliLocator` 定位 CLI，`check_config_dir` 核对配置目录，`adapter_env` 生成环境，再用 `version::probe` 加 `classify` 得出 `CliCompat`。
   - 进程经 `cupper_sys::spawn` 拉起：自成一个会话，三个管道。
   - 拉起后立即写 `adapters` 行；退出时补写 `ended_at` 和 `exit_status`，等写线程提交之后才广播退出。
   - stderr 接 `AdapterLog`，stdio 经 `StdioTransport`、`Tap`（共用宿主的 `RawSink`）接到 `AcpConnection`。
   - 清单补了 `node`、`claude_agent_acp`、`files` 三个字段；`AgentErrorClass` 加了 `CliMissing`、`RuntimeFailed`。
2. **`AgentRuntime` actor**（`driver/runtime/mod.rs`，实现 `AgentPort` 和 `Part`）：
   - 第一次 attach 时拉起实例。
   - 会话打开：`session/new` 或 `resume`；项目不受信任时带 `settingSources: ["user"]`；cwd 统一换成 realpath（K13）。之后逐个 `set_config_option`。
   - Prompt 的内容转换在 `content.rs`：文本照传；`text_blob` 和图片从 blob 读出，图片转 base64；`file_ref` 转成 `resource_link`；评论按 GT-2 排成文本。
   - Cancel、Close、Answer 照常处理；Answer 把权限和 elicitation 的回答转成 ACP 的回复。
   - agent 发来的更新经 `ClaudeSession` 发给对应的会话。权限和表单请求分配 `AskId`；agent 撤回时只告诉提出请求的那个会话（`AskWithdrawn`）。
   - prompt 的回复转成 `PromptDone` 或 `PromptFailed`。
   - 实例结束时向所有会话广播 `AdapterExited`。
3. **宿主退出时的会话**：
   - `TurnMachine` 加了 `HostExit` 输入：运行中的回合立即以 `interrupted` 结束，请求 `expired`（`host_restarted`），队列以 `turn_interrupted` 暂停。
   - 会话 actor 加了停止信道；`SessionManager` 实现 `Part`：`activity` 统计运行中的回合和待处理的请求，`stop(force)` 通知每个会话。
4. **宿主组装**（`Host::start`）：
   - 依次是 `discover`、Claude 的 launch、`AdapterLog`、`raw.sink()`、`AgentRuntime`。
   - `SessionManager` 的 `audit` 用 `records`，`index` 用 `NoIndex`，用 `routes.sessions` 登记。
   - `Part` 的顺序：先会话管理器，再运行时。
5. **假 agent 的路径约定**：
   - 测试用 `launch::fake_agent()`，它取测试二进制所在的 `target/<profile>/` 下的 `cupper-fake-agent`；文件不在时提示先 `cargo build -p cupper-fake-agent`。
   - `xtask ci` 的 rust 作业在测试之前先构建假 agent。
   - 约定已写进 `notes.md`。
6. **文档链接**：tap 指向 `diag::redact` 的链接现在已经能解析。`cargo doc -D warnings` 失败另有两处，这次一并修了：
   - `login_shell.rs` 里指向私有 `KEPT` 的链接（T094 的文件）；
   - `session/mod.rs` 里指向私有 `actor` 的链接。

## 关键取舍

- **实例何时算结束**：要等"进程已退出"和"连接已关闭"两件事都到。
  - `Started`、`Exited`、回复、更新、`Closed` 都走同一条按实例的信道，所以适配器最后写出的更新不会丢。
  - 退出时再给它的进程组发一次 SIGKILL，让组里残留的进程放开管道。
  - 代价是：`setsid` 的孙进程拿着 stdout 时，实例一直不结束。这要靠 T008 的后代进程组清理解决，已写入交接。
- **何时跑登录 shell**：按设计 5.6.6 在启动时跑一次，最多会推迟监听 5 秒。IT-lifecycle 的宿主一律传空的 `login_env`，所以测试不会跑用户的登录 shell，也不会跑真实的 `claude --version`。
- **强制退出**：回合不等结束快照，立即完成。运行时停止时发 SIGTERM（强制时 SIGKILL），然后等实例结束，不限时；限时和 5.6.8 的完整关闭顺序归 T008。
- **规模**：相对估计提交，新增 1,590 行（不含 lock 文件），删除 35 行；其中实现约 1,360，测试约 230。
  - 超过了约 1,500 的上限，在 1,600 的硬线以内。
  - 超出主要来自 `runtime/mod.rs`（670 行），以及按协调意见把发现链拆成独立函数（约 +20 行）。为守住硬线压缩过测试和组装代码。

## 验证方式

- `cargo xtask ci pr --record docs/tasks/T055-agent-runtime/ci/` 在干净的 worktree 里全部通过：
  - rust 1434 passed；protocol 185 passed；ts 323 passed；license 1 passed；testdata-scan 3 passed；
  - 跳过了 sensitive-words（本机没有词表）；
  - 摘要 `pr-65595768ea28.json` 已提交。
- 验收逐条：
  - **IT-process-hygiene**：假 agent 的 `probe` 回显 `fds=0,1,2`，环境只有 `CLAUDE_CODE_EXECUTABLE,HOME,PATH,TERM`（宿主环境里的 `SECRET` 被过滤掉）；进程是自己会话和进程组的首进程；`adapters` 行记下了退出状态。
  - **IT-K13-echo-cwd**：传入 `/var/...` 下的临时目录，agent 回显的 cwd 是 `/private/var/...` 的 realpath。
  - **IT-K26-client-info**：`clientInfo.name == "cupper"`。
  - **宿主跑通一个回合**：`it_lifecycle_runs_a_turn_with_the_fake_agent` 启动真实宿主进程，注入假 agent，经网关 `session/create` 发 `say hello`，回合变成 `completed`；SIGTERM 后宿主以 0 退出，`adapters` 行已结束。
  - 另加了两条：`it_orch_lifecycle_forced_exit_interrupts_the_running_turn` 和 `turn_machine_interrupts_the_running_turn_as_the_host_exits`。
  - 生产拉起前的核对：`verify_checks_a_runtime_file_against_its_manifest_entry` 用造的文件测了哈希一致、不一致、cdhash 对不上、文件缺失四种情况，并确认内嵌清单里有这两个文件。
  - 核心和宿主内的用例用 `ManualClock`，没有睡眠。
  - 测试名以已登记的编号开头；`tests/ids.toml` 没有改。
- 每个提交都单独跑过 clippy：默认特性、`--all-features`，宿主相关的提交还跑了 `dev-auth`。

## 提交列表

```
61958b4 docs(task): T055 CI summary, two more handoffs
6559576 docs(task): T055 notes: sizes, choices, handoffs
9899b51 feat(host): run Claude's runtime and the sessions in the host
e975318 test(host): IT-process-hygiene, IT-K13-echo-cwd, IT-K26-client-info
1b41710 fix(host): the agent gets the real cwd; exits are recorded first
99347ae docs(host): drop doc links to private items
e868823 feat(host): sessions take part in the host's exit
3e8b25c feat(host): AgentRuntime actor for Claude
2494a03 feat(host): start adapter instances, checked against the manifest
5de77db docs(task): T055 size estimate and split proposal
```

## 遗留问题 / 范围外发现

交接都已写进 `notes.md`：

- **T008**：
  - 重启目前没有退避，下一次 attach 立即拉起新实例；
  - `stop` 不限时；
  - 只给适配器自己的进程组发信号。`setsid` 的孙进程不清掉，实例就一直不结束；
  - 每个实例的 stderr 尾部（64 KB，在 `Instance.tail`）还没有放进 `error` 条目：`AdapterExited` 还不带它。
- **T009**：
  - attach 返回的能力里命令列表是空的；
  - `AuthStatus` 被忽略，`Probe` 命令被丢弃；
  - 还没有写 `agent_capabilities` 表。
- **T044**：`SetConfig` 命令被丢弃。**T140**：`Steer` 被丢弃。
- **T056**：生产路径要的两个文件，以及它们在内嵌清单 `files` 里对应的条目：
  - `<数据目录>/runtime/node`，对应 `Helpers/node`；
  - `<数据目录>/runtime/bundle-claude-acp.mjs`，对应 `Resources/runtime/bundle-claude-acp.mjs`。

  布局定下来后，改 `launch::production` 里的这两个路径即可。
- **T034**：发现链的复用方式见开头。边界目前只有项目路径。
- 设置里的 `agents.env_passthrough` 还没接，`adapter_env` 拿到的放行名单是空的。
- 第二种 agent 接进来时需要一个自己的 `ActorKind`。
- 范围外的小改动：
  - `login_shell.rs` 和 `session/mod.rs` 的文档链接各改一行，原因见"做了什么"第 6 条；
  - `orchestrator::testkit` 改为 `pub(crate)`，供运行时的测试使用；
  - IT-lifecycle 的 `spawn` 一律带空的 `login_env`。