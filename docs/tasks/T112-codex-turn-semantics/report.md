# T112 汇报

- 执行者：子 agent（Rust 工程师，驱动层与运行时）
- 分支：`test/T112-codex-turn-semantics`（头提交 `481f287`，基于 main `eb10caa`）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/55 。`481f287` 上 8 个检查全部通过：https://github.com/boris-w/cupper/actions/runs/41619361897
- 连跑（计划 1.2 要求新增的 IT 用例连跑 10 次）：https://github.com/boris-w/cupper/actions/runs/41619347067 。参数 kind=cargo、package=cupper-host、args=`--features test-harness --test driver_k`、filter=`it_k`、times=10、parallel=4、runner=macos-15，结果 40 次全部通过。每次运行包含 driver_k 里所有 `it_k` 开头的用例：本任务新增的 6 个、K14 用例，以及原有的 Claude 形态 K 用例。

## 做了什么

用例都放在 `tests/driver_k.rs`，用 codex 形态的假 agent（`--persona codex`）跑在真实的宿主进程上。为此新增了三个辅助函数：`host_of`（按 agent 起宿主，原来的 `host` 改为调用它）、`codex_session`（以 codex 建会话）、`items`（按条目类别取事件）。

| 编号 | 断言 | 结果 |
|---|---|---|
| IT-K2-reject-ends-turn | 拒绝选项 `cancel` 带 `ends_turn: true`；选它之后回合以 `cancelled` 结束；回合进行中排队的消息随后以 `queued` 开始，队列没有暂停 | 第一次就通过 |
| IT-K3-late-output | 中断时还在跑的命令以 `abandoned`、`may_still_run` 结束；晚到的输出写成 `late_tool_output`，`ref` 指向原条目，落在一个 `autonomous` 回合里，原回合不改 | 第一次就通过 |
| IT-K4-cancel-pending-approval | 中断时挂着的审批以 `cancelled`（`turn_cancelled`）结束；原始帧里有宿主发给 agent 的 `outcome: cancelled` 应答 | 第一次就通过 |
| IT-K7-restartable | `1001`、`restartable`：回合 `failed`，`error_code` 为 `restartable`；`error` 条目的 `guidance` 为 `retry`、`agent_code` 为 `1001`；下一个 prompt 正常完成 | 第一次就通过 |
| IT-K12-plan-confirm-idle | 线程先变为 idle，再发出计划确认请求；这时回合没有结束；选 `implement_plan` 后计划继续执行，回合 `completed` | 第一次失败，是用例把字段名写错了（`request.kind` 应为 `request.type`），改正后通过 |
| IT-K17-text-error | `systemError` 加一段文字：回合 `failed`，`error_code` 为 `model_unsupported`；只有一条 `error` 条目，`raw` 是原文，`guidance` 是 `change_model`；这段文字没有变成 agent 的消息 | 第一次就通过 |

交接事项：
- **K17 的 `error` 条目**（T110、T226 交接）：T226 已经做到了。文字错误经 `PromptFailed` 交给回合机，由 `Failure::of` 写出唯一一条 `error` 条目。IT-K17 守着这一点，宿主代码没有改。
- **K14 在会话打开时的改判**（T063 交接）：查出缺陷，就地修了，见下。

## 就地修的缺陷

1. **K14 在会话打开时没有改判**（47c5e58，约 25 行，没有改接口）
   - 现象：Codex 在 `session/new` 就返回 `-32000`。运行时只对 prompt 的失败做 K14 改判（CLI 低于已验证范围时改判为 `cli_too_old`）和登录状态更新。打开时的失败原样以 `not_logged_in` 返回，CLI 版本低时也一样，agent 的登录状态也不变。
   - 修法：改判和登录状态的更新合成一个 `judged()`，prompt 失败和会话打开失败都经过它。
   - 用例：`it_k14_old_cli_auth_codex_fails_on_opening`，挂在 T063 的 IT-K14-old-cli-auth 下。CLI 为 0.120.0 时回合以 `cli_too_old` 失败；版本不低时以 `not_logged_in` 失败，索引里 codex 的登录状态变为 `logged_out`。
2. **`notice: upstream_retry` 从来不写**（K18）：超出就地修的上限。按你的裁定（A）另开 T230，IT-K18-upstream-retry 移过去。本分支上没有留这个用例，跑过的草稿和需要改的地方记在 `notes.md`，留给 T230 取回。

## 关键取舍

- 用例放进已有的 `tests/driver_k.rs`，不新开测试文件：K 表的进程级用例本来就在这里，它在 `Cargo.toml` 里的 `test-harness` 声明也已经有了。
- K4 的"替用户应答 `Cancelled`"从假 agent 的 `--calls` 里看不到（它只记录 agent 收到的请求），所以改为读 `raw.db` 里宿主发出的应答帧。读之前先等这一帧落盘，因为原始帧不经过宿主的同步屏障。

## 验证方式

- 本机单份运行 `cargo test -p cupper-host --features test-harness --test driver_k`，17 个用例全部通过，其中 7 个是本任务新增的；clippy（test-harness）没有警告。
- 连跑和 PR 的 CI 结果见文首。

## 规模

增 263、删 29，停止线 600。其中宿主代码约 25 行（K14 修复），其余是用例和辅助函数；另有一个格式提交，增 7、删 1。

## 提交列表

```
481f287 docs(task): add the T112 notes
47c5e58 fix(host): judge Codex's login error as its session opens (K14)
a42dee6 test(host): run Codex's K cases on a host process
b709497 style(host): format the session tests' fixture
```

`b709497` 和 PR #53 的格式改动内容相同：不改的话，本分支在 `cargo fmt --check` 上会失败；#53 先合并的话，合并时也不会冲突。

## 遗留问题 / 范围外发现

1. **T230**：在同一回合里 upsert 的 `upstream_retry` notice，用例草稿在本任务的 `notes.md` 里。
2. **假 agent 的局限**：codex 形态的 `say` 不带 `messageId`。在 IT-K17 里，它正好扮演适配器写出的错误文字；但真实 Codex 里一般的 agent 消息总是带 ID 的（T110 的结论）。以后如果有用例要区分"模型说的话"和"适配器的文字"，需要给假 agent 加一个带 ID 的 `say`。
3. worktree 保留着，等编排者合并后再清理。

（说明：这次的报告交接通道已经用掉，所以经 SendMessage 送达，请按此存档为 T112 的 report.md。）
