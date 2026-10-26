# T063 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`test/T063-it-driver-k-list`（编排者已把 main 合入，合并提交 74b86a9）
- 完成日期：2026-10-26

## 做了什么

十个编号都写成了真实宿主进程上的用例，全部通过，用的是 claude 形态的假 agent；用例在新的测试目标 `crates/cupper-host/tests/driver_k.rs` 里，共用 `tests/common/`。按计划 1.2 就地修了两处缺陷。

| 编号 | 断言 |
|---|---|
| IT-K1-close-resume | `hang` 的 prompt 一直不返回；中断后，`interrupt_fallback` 到期，回合以 `cancelled` 结束。调用记录里 `session/cancel` 之后依次是 `session/close`、`session/resume`、新的 `session/prompt`；`whoami` 回 `resumed=true`。 |
| IT-K5-withdraw | agent 自己撤回（`$/cancel_request`）：请求以 `cancelled`/`withdrawn` 结束。用户中断：宿主先以 `cancelled`/`turn_cancelled` 结束请求，随后 Claude 的撤回不再起作用。两种情况下每个请求都只结束一次。 |
| IT-K6-crash-resume | `crash_cli` 的回合失败；下一条消息先遇到 Session not found，宿主恢复会话后重发一次并跑完。连续五轮：`whoami` 每轮发两次、`session/resume` 每轮一次，`crash_cli` 从不重发，宿主的进程树大小始终不变（Q43）。 |
| IT-K6-output-then-crash | 先有输出再崩溃：回合失败，输出保留；崩溃的那条 prompt 只发过一次；下一条消息在恢复后跑完。 |
| IT-K9-autonomous | prompt 返回后的自发更新开 `autonomous` 回合；静默超过 `autonomous_silence` 之后再有输出，又开一个。三个回合的来源依次是 user、autonomous、autonomous。 |
| IT-K14-old-cli-auth | CLI 版本 1.0.0（低于已验证范围）加 `auth_none`：回合以 `cli_too_old` 失败。 |
| IT-auth-none | 以未登录启动加 `auth_none`：回合以 `not_logged_in` 失败；`agents.updated` 的登录状态为 `logged_out`。 |
| IT-K19-unknown-kind-privileged | 用 `permission_options` 发一个不认识的选项 kind：它的 `requires` 为 `privileged`、效果为 `unknown`；普通选项仍是 `approve`，可以正常回答。 |
| IT-thought-claude | `think`：出现 `thought` 条目，带着思考的文本。 |
| IT-plan-entries | 两次 `plan_update`：只有一个计划条目，内容跟着更新到最新一版。 |

**假 agent 新增两项**：
- **`--calls <文件>`**：每个收到的请求和 `session/cancel` 写一行 `{method, session, text}`，带单元测试。K1、K6 用它直接断言收到的是 `session/resume` 还是 `session/new`，以及 prompt 重发了几次（T091 的交接）。
- **`withdraw_after <ms>`**：一段时间后撤回下一个请求，之后剧本继续执行，带单元测试。

## 就地修的缺陷（各自单独提交）

1. **`AdapterLog` 打不开时，适配器会收到 SIGPIPE**（T116 的交接），+67 −14：
   - **现象**：日志文件打不开时，launch 不读适配器的 stderr，直接关掉管道。适配器再往 stderr 写就收到 SIGPIPE，原始帧里也没有它的 stderr。
   - **原因**：只有拿到日志时才起读取任务。
   - **修法**：日志文件打不开时，supervisor 改用不带文件的 `AdapterLog::unwritten`。它照样读出每一行写进原始帧，并保留脱敏后的末尾。运行时里原来不带日志拉起适配器的测试也一并改用它。
   - **用例**：`unit_redaction_adapter_stderr_read_without_the_log_file`。
2. **K14 的改判缺失**，+47 −20：
   - **现象**：CLI 低于已验证范围、读不了配置时报"需要认证"，宿主一律当作未登录，还把登录状态标成未登录。
   - **原因**：`CliTooOld` 这个类别有定义，但没有任何地方产生它。
   - **修法**：prompt 以未登录失败、且 CLI 有版本号并低于已验证范围时，改判为 `cli_too_old`，登录状态不变。新增 `test_config.agents.<agent>.cli_version`，让注入的假 agent 有一个版本，与探测到的版本走同一套分类（`launch::cli_versions`）。
   - **用例**：IT-K14-old-cli-auth。

## 需要你定：K5 中断时，请求的原因是 `turn_cancelled` 还是 `withdrawn`

- **现状**：用户中断时，宿主在给 agent 发 `session/cancel` 的同时，就以 `turn_cancelled` 结束挂着的请求。Claude 随后发来的 `$/cancel_request` 找不到挂着的请求，不起作用。只有 agent 自己撤回时，原因才是 `withdrawn`。两种情况下每个请求都只结束一次，状态都是 `cancelled`。
- **我倾向改设计，不改宿主**：
  - 中断是用户发起的，宿主先结束请求，不必等 agent 回话，也不依赖 agent 一定会撤回；
  - Codex 根本不撤回（K4），宿主替用户应答 `Cancelled`，两边走同一条路；
  - `turn_cancelled` 也如实说明了"是回合被中断"；`withdrawn` 留给 agent 自己撤回的情形，宿主已经是这样做的。
  - 建议把设计 5.6.9 K5 一行的"请求标为 cancelled（withdrawn）"改为："agent 自己撤回为 `withdrawn`；中断时宿主先以 `turn_cancelled` 结束，随后的撤回不再起作用"。
- **用户不会看到错误的提示**：界面的请求记录行和面板都不读原因，两种情况都显示"未回答"；"已在别处处理"的提示只在状态为 `resolved` 时出现。CLI 只是原样输出 JSON 里的原因。

## 关键取舍

- **规模**：`cargo xtask size` 报新增 478 行、删除 34 行，停止线 850。开工前估约 560，每次提交后都跑了这个工具。
- **K6 "不重发"的口径**：宿主只在"prompt 在任何更新之前以 Session not found 失败"时恢复并重发；`crash_cli` 本身的 −32603 归为 `process_exited`，不会重发。两条 K6 用例因此断言：崩溃的 prompt 只发过一次，下一条消息在恢复后重发一次。"先有输出、再遇到 Session not found"在假 agent 上造不出来（崩溃之后的请求都是立刻失败），这一支由进程内用例 `gating_tests.rs` 覆盖。
- **Q43 的观察**：五轮"崩溃、恢复"之后，宿主进程树的大小与开始时相同，已写进 `notes.md`。但假 agent 的崩溃只是模拟，进程并不退出，所以这只证明宿主侧的恢复不会多拉起进程；真实 `claude` 多次 resume 后会不会留下 CLI 进程（上游 #1011），仍要由 T066 用真实 CLI 观察。
- **IT-K5 的不稳定已消除**：撤回后 prompt 立刻结束时，宿主有时先处理回合结束，把请求记成 `turn_ended`，10 次里失败 2 次。改为撤回后剧本继续执行、prompt 晚 300 ms 结束，之后连跑 10 次全绿。这种先后关系在真实 Claude 上也可能出现，但结果一样：请求只结束一次，状态 `cancelled`。

## 验证方式

- **连跑**：`driver_k` 最后一轮连跑 10 次全绿，每次约 2.5–3.7 s（有一次本机负载高，23 s）。跑完本 worktree 没有 `cupper-host`、假 agent 或测试进程残留。合入 main 之后重跑，10 条全过。
- **测试名**：都以编号开头（`it_k1_…`、`it_auth_none_…` 等）。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T063-it-driver-k-list/ci/`，通过，运行期间没有写工作区。摘要 `pr-bb477e31c81b.json` 已提交，十个编号均为 pass。

  | 作业 | 结果 |
  |---|---|
  | rust | 2,093 通过 |
  | protocol | 185 通过 |
  | ts | 473 通过 |
  | license | 1 通过（RUSTSEC 的升级已随 main 进来） |
  | testdata-scan | 3 通过 |
  | ui | 跳过（没有改动） |
  | e2e | 30 通过 |

## 提交列表

```
2cb714b docs(task): T063 CI summary
bb477e3 docs(task): T063 notes: sizes, Q43, the two fixes, the K5 finding
74b86a9 Merge branch 'main' into test/T063-it-driver-k-list
1bc1172 test(fake-agent): a timed withdrawal lets the script go on
d6f3a8b test(host): the K list on the host process
a251afe test(fake-agent): withdraw a request after a while
41eab20 fix(host): an old CLI's 'Authentication required' fails as cli_too_old
7b8ae05 fix(host): read an adapter's stderr even when its log file did not open
4e591d3 test(fake-agent): record the calls it gets
e03f702 docs(task): T063 size estimate
```

## 遗留问题 / 范围外发现

1. **K5 的原因**：上面那一节，等你定改设计还是改宿主。
2. **K14 只覆盖了 prompt 失败**：会话打开时的认证错误还没有改判。假 agent 的 claude 形态在打开会话时不报未登录，所以本任务没有碰到。codex 形态的同类情形归 T112。
3. **设计 5.9.2 的剧本表要补三项**：`--calls`、`withdraw_after`，以及 T092 加的 `permission_options`。
4. **Q43 的真实观察仍待做**：要由兼容矩阵（T066）用真实 `claude` 跑。
