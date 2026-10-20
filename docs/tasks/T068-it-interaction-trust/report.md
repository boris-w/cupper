# T068 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`test/T068-it-interaction-trust`
- 完成日期：2026-10-20

## 做了什么

七个进程级用例都写完了，连跑 10 次全绿，跑完没有残留进程。就地修了两处缺陷：任务书点名的 `rescan` 没持 `Hold`，以及写用例时新发现的一处 SEC-10 缺陷。`cargo xtask ci pr` 一次通过（rust 1,906、protocol 185、ts 459、e2e 4）。

**规模**：净增 520 行（增 697、删 177，只算 `crates/`），估计约 580，在约 850 的上限之内。其中约 170 行是从 `lifecycle.rs` 挪进 `tests/common/` 的夹具，增删两边对冲。

1. **夹具挪进 `tests/common/mod.rs`**（任务书第 3 项）
   - 挪出的是拉起宿主、harness 客户端、假 agent、等表、退出码、`session/create`、`session/send`、时钟推进；`lifecycle.rs` 和新文件都用。`common` 带一个写了理由的 `allow(dead_code)`，因为每个测试 crate 只用其中一部分。
   - **宿主守卫 `Running` 改成先 SIGTERM 再等退出。** 原来用 SIGKILL，宿主来不及停下适配器，适配器和孙进程就留下了，这就是之前各 worktree 看到的残留。T062、T063、T069 可以直接用。
2. **用例**：放在 `tests/interaction_trust.rs`。宿主跑 claude 形态的假 agent，测试以 harness 客户端驱动；项目经 `project/add`、`project/trust` 加入。
   - **IT-form**：表单变成 `question` 请求，索引里为 `needs_action`、`pending_kind: input`；回答后 agent 收到答案，请求 `resolved`，审计有 `interaction.responded`。
   - **IT-plan-confirm**：`plan_confirm` 请求，`pending_kind: approval`；选 `exit-plan-auto` 后 agent 切到 `auto` 模式并把话说完，会话没有进入 `config_unconfirmed`。
   - **IT-interaction-expire**：
     - 摘要不对时被拒，`conflict`、`digest_mismatch`；
     - 杀掉适配器后，请求转为 `expired`（`adapter_exited`），之后再回答被拒（`expired`）；回合以 `adapter_exited` 失败，索引状态为 `error`。
   - **IT-config-unconfirmed**：
     - 回合中假 agent 自己把模式抬到 `bypassPermissions`，宿主改回 `default` 并进入 `config_unconfirmed`；下一条消息排队、队列暂停，直到 `session/confirm_config`；审计有 `config.unconfirmed`、`config.confirmed`。
     - 改回失败的分支（假 agent 的 `set_config_error`）：回合被中断。
   - **IT-trust-stale**：
     - 项目的 `.claude/settings.json` 是指向工作区外的链接，回合中改写链接目标；回合结束后在手动时钟上 `test/clock_advance {ms: 0}`，一返回就能查到 `project_config_changed` 提示。
     - 下一条消息被拦下，索引和审计都有 `stale`；重新信任、恢复队列后消息发出。
   - **IT-trust-rescan-before-prompt**：
     - 项目是 git 仓库的子目录，回合中改写仓库根的 `.claude/`，回合中排着的消息在发出前被拦下；
     - 重新信任后消息发出；
     - 再放一个悬空链接，下一条消息被拦下，提示原因为 `dangling_link`，审计有 `project.trust.scan_failed`。
   - **IT-trust-settingsources**：未信任的项目，附加时 `_meta` 带 `settingSources: ["user"]`，`project_config` 为 `ignored`；信任之后新开的会话不带，为 `loaded`。
3. **`Cargo.toml`**：新测试目标要求 `test-harness` 特性，和 `lifecycle` 一样。否则不带特性的 clippy 编不过。

## 就地修的缺陷

1. **`rescan` 没有持有静止屏障的 `Hold`**（T100 合并时发现）
   - **现象**：手动时钟上，回合结束后 `test/clock_advance` 返回时，回合结束那次重扫还没做完，提示和闸门还没写出来。
   - **原因**：`orchestrator/session/actor.rs` 的 `rescan` 用 `tokio::spawn` 另起 task，里面读库、跑 git、在 `spawn_blocking` 里扫描，全程没有 `Hold`，违反 T047 的规则。
   - **修法**：`Parts` 带上 `Quiescence`，重扫 task 持有 `Hold`，把结果交回 actor 之后再释放。增 7、删 1。
   - **复现**：IT-trust-stale 中"推进时钟后立刻查提示"那一步，修之前连跑 10 次失败 10 次，修之后 10 次全过。
   - 起初的写法测不出：`session/send` 的回复要等发送前那次检查（`Purpose::Check`）做完才返回。没持 `Hold` 的是回合结束、恢复、附加这几种重扫。
2. **适配器死在 prompt 中途时，请求被记成 `cancelled` 而不是 `expired`**（写 IT-interaction-expire 时发现，违反 SEC-10）
   - **现象**：请求为 `cancelled`（`turn_ended`），回合失败码为 `other`；设计 5.2.6 要求回合 `adapter_exited`、请求 `expired`。
   - **原因**：SDK 在适配器输出结束时让在途的 prompt 以 `incoming_transport_closed` 失败，这个失败常比运行时的 `AdapterExited` 先到会话。回合按普通失败结束，挂着的请求随之以 `turn_ended` 取消。
   - **修法**：`claude::agent_error` 把这个错误归为新的 `AgentErrorClass::AdapterExited`（失败码 `adapter_exited`）。回合机原有的逻辑会据此把请求标为 `expired`（`adapter_exited`），两个事件谁先到结果都一样。增 6，只动驱动层内部的分类，没有改协议或接口。

## 关键取舍

- **IT-config-unconfirmed 用回合中由 agent 抬档的场景**：这是设计 5.9.8"配置档位"那一条的写法，改回成功和改回失败两个分支都覆盖了。由创建者直接以全权限建会话的场景，进程内用例已经覆盖。
- **"不加载项目配置继续"没做进程级用例**：它在线上要到 M1b 才有方法（`ignore_project_config` 只是宿主内部入口），所以信任部分选了能在线上驱动的场景：改了配置后发 prompt、回合中改了配置、未信任、悬空链接、git 根目录下的 `.claude/`。
- **用 `kill -KILL` 杀适配器**：用的是本测试宿主写进 `adapters` 表的 pid，只针对测试自己拉起的进程。

## 验证方式

- 七个用例一次跑完约 8 秒；连跑 10 次全部通过，跑完没有残留的宿主、假 agent 和 `sleep`。
- 用全部特性（含 `dev-auth`）跑 `interaction_trust`、`lifecycle` 都通过；`lifecycle` 改用 `common` 后 16 个用例照常通过；全部 `cupper-host` 测试通过。
- `cargo clippy -p cupper-host --all-targets` 在默认特性和 `test-harness,dev-auth` 下都无警告。
- 测试名都以用例编号开头（`it_form_…`、`it_plan_confirm_…`、`it_interaction_expire_…`、`it_config_unconfirmed_…`、`it_trust_stale_…`、`it_trust_rescan_before_prompt_…`、`it_trust_settingsources_…`）。
- **CI**：`cargo xtask ci pr --record docs/tasks/T068-it-interaction-trust/ci/` 通过，跑的期间没往工作区写文件。摘要 `pr-0f1ee045b30c.json` 已提交。估计、统计和过程记录在 `notes.md`。

## 提交列表

```
6d6e892 docs(task): record T068 CI summary
0f1ee04 docs(task): record T068 counts, cases and fixes
f7087ee test(host): interactions, project trust and the tier on a host process
aa4c8f9 fix(host): expire the requests of an adapter that died under a prompt
ff46028 fix(host): hold the quiescence barrier while a session rescans its project
e618c88 test(host): share the process-level fixtures in tests/common
8c307aa docs(task): estimate T068 by case
```

## 遗留问题 / 范围外发现

1. **重扫时闸门已关就不再发提示。** 闸门已因 `project_config_changed` 关着时，再次重扫失败（例如悬空链接）不会追加提示，原因只能从审计的 `project.trust.scan_failed` 看到。这符合现在"只在闸门打开时提示"的写法，不是缺陷；如果界面需要显示最新原因，要另议。
2. **会话的第一个 prompt 可能被拦在已开始的回合里。** 第一个回合已写出 `turn.started`，但附加和发送前的重扫还没做完时，如果项目配置变了，这个回合会停在准备阶段，等闸门重新打开。用例没有覆盖这条（我把改配置放在 agent 开口之后），建议在 T063 或 T069 补一条：信任恢复后，这个回合应能正常发出。
