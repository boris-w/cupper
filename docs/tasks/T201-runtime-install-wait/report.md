# T201 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`fix/T201-runtime-install-wait`（T054 worktree）
- 完成日期：2026-10-28
- PR：https://github.com/boris-w/cupper/pull/16 ，运行：https://github.com/boris-w/cupper/actions/runs/40635718492

**状态**：修复和测试都做完了。我已按要求跑完本机检查，并核对了修前修后的对照。交接时 8 个 CI 检查里有 4 个通过（protocol、ts、license、testdata-scan），**还有 4 个没跑完**（rust、e2e、ui、compat-framework，都在 macos-15 上）。我原本要等它们跑完再汇报，但运行环境要求现在交接，所以合并前需要编排者去 PR 上确认这 4 个。

## 做了什么

1. **启动适配器时等安装结束**（设计 5.2.12）。
   - 安装器原来就要把 Claude 适配器的 `runtime` 和 `failure` 报给索引（来自 `adapter_state`），现在同时经 `watch` 发给 `runtime::Progress`。
   - `launch::start` 遇到 `Program::Runtime` 时，先等状态变成 `ready` 或 `failed`，再照旧核对 node 和打包文件的哈希。
   - 装好了就照常启动。安装失败时回合仍以 `runtime_failed` 结束，错误文字里带上原因，例如 "the runtime's install failed: hash_mismatch"。
   - 安装器停了、或宿主不在 app 包里（没有安装器）时，发送端丢弃，不再等，直接核对文件，行为和以前一样。
2. **不阻塞别的工作**：等待发生在 actor 另开的启动 task 里，actor 照常处理其他邮件（别的会话、状态查询、探测）。
3. **等待期间停止**：原来运行时停止要等启动结束，现在会被安装拖住（下载上限 60 s，还有重试）。所以启动 task 带一个 oneshot，actor 收到停止时丢掉发送端，等待就结束。只有"等安装"这一段可以放弃；进程拉起之后不放弃，以免留下孤儿进程。
4. **按安装器的计划启动**（单独一个 refactor 提交）：
   - 以前 `claude_adapter` 固定用内嵌清单。测试包里安装器按 `test_config.runtime.manifest` 安装，启动却去内嵌清单的版本目录里找，所以测试里永远对不上。
   - 现在 supervisor 用同一份计划算出程序，交给 `Finder` 和 `launch::production`。生产里两份计划本来就相同。
5. **可观察的状态**沿用 `agents.updated` 里 runtime 的 `installing`，没有新增协议成员。
6. 宿主运行期间不读 App 包内的文件，这一条没有变。

## 关键取舍

- **等的是界面看到的那份状态**（`adapter_state` 合成的 runtime），而不是各组件各自的状态。这样做省掉一份重复的映射。第一版按组件传状态，写到 226 行，超了停止线 200；改成现在的做法后降到 187 行。
- **失败原因的位置**：回合结束时只写 `error_code`，`AgentError` 的文字不进会话流。所以失败原因由进程内用例核对；进程级用例核对 `agents.updated` 里的 `adapter.failure`。

## 验证方式

**修前修后对照**
- 修前：把 `Progress::done` 临时改成立即返回（等于启动前不等安装），跑 IT-runtime-first-prompt-waits。用例失败：索引里 runtime 是 `installing` 时，回合已经结束了。临时打印出的回合结束是 `failed`、`runtime_failed`、6 ms，和 T066 看到的现象一致。最终代码上又重跑了一次，同样失败。
- 修后：通过。

**IT-runtime-first-prompt-waits，进程级**（`tests/lifecycle.rs`）
- 夹具：
  - 宿主跑在假 `CupperHost.app` 里；
  - 假 node 只收到一个参数（适配器的包）时会 exec 假 agent；
  - 设置里的 `agents.claude.cli_path` 指向假 agent，这样不会去常见位置找到本机真实的 Claude；
  - 本地 registry 由测试放行才回复。
- 用例在安装完成之前发 prompt。测试看到 runtime 为 `installing`、回合为 `running` 之后才放行 registry，顺序靠事件保证，不用 sleep。
- 结果：
  - 第一段放行正确的包：回合 `completed`；
  - 第二段放行不符合 SRI 的字节：回合 `failed`、`runtime_failed`，`adapter.failure` 为 `hash_mismatch`。

**IT-runtime-first-prompt-waits，进程内**（`driver/runtime/tests.rs`，ManualClock）
- 等待期间停止，停止能返回，附加以失败结束；
- 安装失败时，启动返回 `RuntimeFailed`，错误文字带 `hash_mismatch`。
- 变异检查：去掉停止时放弃等待的那一行，用例挂住（40 s 后被 alarm 结束）；恢复后通过。

**本机检查**（都只跑一份，没有循环或并行）
- 通过：IT-runtime-startup、IT-runtime-install（含 `--self-test` 那条）、IT-runtime-download、`driver::runtime`、`runtime::`、`supervisor::`、`it_lifecycle_runs_a_turn_with_the_fake_agent`、GOLD-B（`gold_`）。
- fmt 通过；clippy 在无特性、dev-auth、test-harness、all-features 四组下都无告警。

**CI**（运行 40635718492）：protocol、ts、license、testdata-scan 通过；rust、e2e、ui、compat-framework 交接时还没跑完。

## 提交列表

```
b649eba fix(host): the adapter's start waits for the runtime install
bf1fd96 refactor(host): Claude's adapter starts from the installer's plans
```

规模（`cargo xtask size`）：新增 187、删除 34，停止线 200、硬停止线 260。行数估计、修前修后的记录和取舍都写在 `docs/tasks/T201-runtime-install-wait/notes.md`，随修复提交入库。

## 遗留问题 / 范围外发现

- **IT-runtime-startup 可能运行本机真实 CLI 的 `--version`**：这个已有用例没有设 `cli_path`，宿主沿用测试进程真实的 `HOME`。本机若在常见位置（`/opt/homebrew/bin/claude`、`~/.local/bin/claude` 等）装了 Claude，宿主启动时会对它跑一次 `--version`。新用例用设置里的 `cli_path` 避开了这一点，旧用例没有改。
- **等待期间 agent 状态不显示 `starting`**：启动中（`Running::Starting`）不会主动通知索引，所以等安装时索引里 agent 仍是 `idle`，只有 runtime 显示 `installing`。任务书要求沿用 runtime 的状态，这里没有改；要不要在启动开始时通知一次 `starting`，由编排者决定。
- **安装失败后的重试会累计**：安装失败后，每次启动尝试都会按一次失败计入重启退避，5 分钟内 5 次后 agent 变成 `crashed`。这和以前"哈希核对失败"的计数方式相同。
