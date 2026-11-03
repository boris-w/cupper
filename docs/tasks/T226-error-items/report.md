# T226 汇报

- 执行者：子 agent（Rust 工程师，宿主编排）
- 分支：`feat/T226-error-items`（PR https://github.com/boris-w/cupper/pull/49 ，头提交 bca51c0）
- 完成日期：2026-11-03

宿主现在会为失败的回合写统一的 `error` 条目。PR #49 在 bca51c0 上 8 项检查全部通过；E2E-error-guidance 在 runner 上连跑 20 次，全部通过。协议没有改。

## 做了什么

1. **`FailureCode` 到 `(Guidance, retryable)` 的映射**（新文件 `crates/cupper-host/src/orchestrator/failure.rs`）
   - `guidance()` 是纯函数，对 `FailureCode` 做穷举匹配，以后新增取值时编译会失败，迫使补上映射。
   - `Failure {code, agent_code, raw}` 有两种来源：
     - `Failure::of(&AgentError)`：JSON-RPC 错误的 `agent_code` 写成字符串，例如 `"-32000"`；message 为空时不写 `raw`。
     - `Failure::exited(stderr)`：取 stderr 最后 2,000 个字符，从行首开始；单行超长时在字符边界截断；内容为空时不写 `raw`。
2. **写条目的三个时机**。条目都由回合机在 `end()` 里写，经会话流唯一的写者落盘。位置在 Settle 之前，因此也在结束快照和 `turn.completed` 之前。
   - attach 返回错误时，正处于 `Preparing` 的回合以这个错误结束。CLI 缺失、没登录、运行时失败、`project_untrusted` 都走这条路。
   - prompt 返回错误时（`Prompting` 或 `Cancelling` 中）。K6 的"会话找不到、resume 后重发一次"不会结束回合，所以不写。
   - 适配器意外退出时（`Prompting`、`Cancelling`、`Autonomous` 中），`raw` 是 stderr 尾部。`DriverEvent::AdapterExited` 原本就带着脱敏后的 `stderr`，这次第一次用上它。
3. **同一回合不重复写**：每个回合只会走一次 `end()`，之后要么进入 `Ending`，要么直接结束，所以不需要另记标志。用例覆盖了"prompt 先失败、适配器随后退出"，只写一条。
4. **顺序**：`SessionCore` 把 error 条目交给 actor 之前，先给合并器一个 `Interject`，和交互请求的做法相同：缓冲的文字先写出，活动组结束。
5. **脱敏**：会话 actor 在 attach 的错误结果和 `PromptFailed` 进入核心之前，对 `AgentError.message` 做完整脱敏，用的是 `Redactor`，与适配器日志同一套规则。生产环境经 `SessionDeps.redactor` 传入 `Redactor::for_this_machine()`。stderr 尾部在读入时已经脱敏过，不再重复处理。`notice_with` 的即时条目构造抽成了 `orchestrator::instant()`，error 条目和 notice 共用。

## 关键取舍

- **映射**：与任务书初稿基本一致，有两处改动或补充：
  - `host_restarted` 映射为 `retry`（初稿归入"其余 → none"）。回合被宿主重启打断，重发就是恢复办法。目前宿主不会用这个码写失败回合，这里只是先把映射定下来。
  - `project_untrusted`、`other` 和未知取值映射为 `none`。信任项目走信任流程，没有对应的 guidance 取值。
- **`retryable`**：只有 guidance 为 `retry` 时为真，含义是"不做别的、原样重发就可能成功"。登录、升级 CLI、换模型之后用户也要重发，但必须先做别的事，所以记为 false。
- **为什么写在 Settle 之前**：写在 Settle 之后会违反 `session_props` 的不变式"条目在回合 Settle 之前开始"。合并器在 Settle 时只结束条目，不开新条目，再加上 Interject 先把缓冲的文字写出，所以在视图里 error 条目仍然排在这个回合已有内容的后面。
- **为什么在 actor 里脱敏**：`AgentError.message` 的注释原来写着"截断、脱敏"，但 Claude、Codex 的分类器其实只截断到 `MAX_LABEL_CHARS`。
  - 在 runtime 里补需要改多个出口：attach 的各条回复路径和 `to_session`。
  - 会话 actor 是 agent 输出进入核心的唯一入口，只要改两处。
  - 注释已改成实际行为。
- **空闲时适配器退出**：没有回合失败，照旧只写 `agent_restarted` 通知，不写 error 条目。
- **规模**：`cargo xtask size` 为 +337 −17（停止线 350），其中宿主 +303、e2e +34。第一个模块做完时到了 +345，接着做了三处压缩，降到 +303 后才写 e2e：
  - 映射表用例改成一张字符串表；
  - 去掉与 IT 重复的单元断言；
  - actor 里就地脱敏，不再单写一个函数。

## 验证方式

- **IT-error-item**（PR 路径），三个用例：
  - `it_error_item_for_each_way_a_turn_fails`（`session/error_tests.rs`）：同一个会话连续四个失败回合，依次是 attach 被拒（cli_missing）、prompt 失败（not_logged_in，-32000）、适配器退出（带 stderr）、prompt 失败后适配器再退出。断言：
    - 每个回合恰好一条 error 条目，`code`、`agent_code`、`guidance`、`retryable`、`raw` 逐项比对；
    - 夹具里的家目录路径在 `raw` 中写成 `~`；
    - 条目的 `turn_id` 就是它所在回合；
    - 条目和 `turn.completed` 之间只隔着结束快照；
    - 第一个回合之后，`session/get` 的状态是 `error`，`error_code` 是 `cli_missing`。
  - `it_error_item_guidance_of_every_failure_code`：映射表覆盖全部已知取值（与 `FailureCode::KNOWN` 对齐），外加一个未知取值。
  - `it_error_item_keeps_the_end_of_stderr`：尾部按整行截取，单行超长时在字符边界截断，内容为空时不写 `raw`。
- **E2E-error-guidance**（nightly）：`tools/e2e/src/error.spec.ts`，用真实 App、宿主和假 agent 跑两个场景：
  - `auth_none`：警告框里有登录指引、`claude` 命令，唯一的按钮是"复制命令"；
  - `stderr e2e-boom; sleep 100; exit 3`：警告框里有 stderr 文本，唯一的按钮是"重试"。
  - 运行：
    - runner 上的 e2e-repeat（20 次，2 个并行）：https://github.com/boris-w/cupper/actions/runs/41605708338 ，20 passed；
    - 本机单份跑 1/1 通过。
- **PR CI**：https://github.com/boris-w/cupper/actions/runs/41605719814 ，在 bca51c0 上 8 项检查（compat-framework、e2e、rust、ui、license、protocol、testdata-scan、ts）全部通过。
- **`schema-compat`**：协议没有改，protocol 作业通过。
- **金样例和视图快照**：没有受影响，不需要重新生成。GOLD-B 只有正常回合、取消、审批三个场景，`gold_b` 测试通过。既有用例都不需要修改。
- **本机检查**（各跑一次，没有高负载）：
  - `cargo test -p cupper-host --features test-harness`（含 lib、lifecycle、driver_k、interaction_trust、gold_b 等）全部通过；
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --check`、prettier、eslint、`trace-ids check` 都通过。
  - 有一次我用 `pnpm run test:e2e:one -- -g …` 想只跑这一个 e2e，过滤没有生效，结果整套 e2e 以 1 个 worker 跑了一遍（52 passed，4.1 分钟）。之后改成按文件只跑本 spec。

## 提交列表

```
bca51c0 docs(task): T226 notes
e5069da test(e2e): a failed turn shows its error and what to do
2c4343c feat(host): write the error item of a failed turn
```

## 遗留问题 / 范围外发现

1. **stderr 尾部可能漏掉最后几行**：实例结束时，`end()` 取 `Tail::text()`，但不等 stderr 的读取任务读到 EOF（`AdapterLog::read` 返回的 JoinHandle 被丢掉了）。进程写完就退出时，理论上会漏掉最后几行；实际上要先等 waitpid 和写库，窗口很小。E2E 脚本里加了 `sleep 100` 来规避。runtime 可以在结束前限时等读取任务读到 EOF（要防孙进程一直占着 stderr），建议另开小任务。
2. **attach 途中适配器退出**：这时回合以 attach 的错误结束（例如 "the adapter went away"），`raw` 里没有 stderr 尾部。
3. **给 T112（K17 文字错误）**：建议把识别出的文字错误交给回合机，作为这个回合的 `Failure`，由 `end()` 统一写，保持每个回合一条。不要另外再写一条；如果确实要在回合中途就写，需要让 `end()` 跳过。可以复用 `failure::guidance()` 和 `orchestrator::instant()`。
4. **设计修改建议**：
   - 5.6.2：`AgentError` 的 message 由分类器截断，由会话在写入流之前做完整脱敏（T226）。
   - 5.2.6 崩溃表、5.3.5：失败回合在 Settle 之前写一条 `error` 条目（每个回合至多一条）；适配器退出时 `raw` 取 stderr 最后 2,000 个字符、从行首开始；把映射表和 `retryable` 的含义写进去。
   - 5.5.2 失败分支：attach 失败时，error 条目写在以 `failed` 结束的那个回合里。
5. **界面**：T143 的警告框把 `raw` 整段显示出来，stderr 尾部最多约 2,000 个字符会占好几行，以后可以考虑折叠，归界面任务。

notes.md（规模估计、实现记录、取舍）已在分支上提交，位置 `docs/tasks/T226-error-items/notes.md`。
