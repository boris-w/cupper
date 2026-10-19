# T018 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T018-protocol-methods-session`
- 完成日期：2026-10-19

## 做了什么

T018 已经收尾，可以合并。`session/archive` 按你的裁定移交 T013，分支里没有它的类型、样例、矩阵方法和 SEC-9 期望项。`pr` 门禁已在干净 worktree 里带 `--record` 跑过并通过。

**第 3 组 项目与 agent**（`methods/project.rs`）：
- `project/add`、`project/trust`、`project/trust_info`、`project/remove`；
- `agents/capabilities`、`agents/refresh`、`agents/restart`。
- 按 5.3.11 的裁定：
  - `findings` 条目是 `{kind, path, target?, outside_workspace, detail}`，`kind` 是开放枚举，取值 9 个；
  - `diff_from_trusted` 按文件列出 `{path, change}`，`change` 复用 `DiffChange`；
  - `project/trust_info` 加 `unavailable/scan_failed`。

**第 4 组 会话**（`methods/session.rs`，13 个方法，不含 `archive`）：
- `session/create`：`workspace` 是按 `kind` 区分的开放联合（`local`，或带 `base_ref`、`branch_hint?` 的 `worktree`）；有 `config?`、`message`；错误码加 `forbidden/scope`。
- `session/send`：有 v1.5 的 `reply_to`；结果的 `fallback_reason` 取 6 个值。
- `session/queue_edit`：v1.4 的 `QueueOp` 是按 `op` 区分的开放联合（`replace`、`steer`、`remove`、`move`）。
- 另有 `interrupt`、`queue_resume`、`set_config`、`confirm_config`、`rename`、`restore`、`mark_read`、`get`、`capabilities`、`list_archived`。

**第 5 组 交互请求**（`methods/interaction.rs`）：
- `interaction/respond`：
  - `outcome` 复用 `InteractionOutcome`；
  - `input` 是 `InputEvidence {kind, key?, digit?}`，`key` 含 v1.4 的 `digit`；
  - `digit` 的类型 `KeyDigit` 只接受 1–9，schema 里也是这个范围。
- `interaction/get`：结果是 `{requested, status, outcome?, resolved_by?, reason?}`，`status` 用 `InteractionStatus`（含 `pending`）。

**补充裁定"队列满"**：`ErrorReason` 加了 `queue_full`，归在 `too_large` 下。`session/send` 和 `session/queue_edit` 的 `ERRORS` 都加了 `too_large/queue_full`，有对应测试。

**权限矩阵**：
- 新增 `Sec9` 格子，表示 remote_device 持有 scope，但由策略按 SEC-9 拒绝或收窄。用在 `project/add`、`project/remove`，以及 `set_config`、`rename`、`restore`、`agents/refresh`、`agents/restart` 那一行。
- 加了 7 行，每行注明 5.8.3 的出处；`session/queue_edit` 按裁定放在 `session/send` 那一行。
- T012 的逐格比对抽成了共用的 `check_rows`。

**收尾**：
- `notes.md` 写明 `archive` 已移交 T013，并列出 T013 要补的地方：在 `methods/session.rs` 声明方法、样例、`register`；在 `MATRIX` 那一行加上方法名（行名已照 5.8.3 写全）；在 SEC-9 期望集合里加一项。
- CI 摘要已提交到 `ci/`。

## 关键取舍

- 你已认可要写进 5.3.11 的形状：
  - `agents/capabilities` 的 `{cached?}`；
  - `session/get` 的 `{summary, workspace, archived_at?}`；
  - `list_archived` 的结构化游标；
  - `interaction/get` 的形状；
  - `trust_digest` 用 `Digest`；
  - 每个方法各自一个 Params 类型。
- 其余的补充都记在 `notes.md` 的"实现时的取舍"里：
  - `project/add.trust` 的类型叫 `TrustScan`，因为 `ProjectTrust` 按命名规则是 `project/trust` 的方法类型；
  - `trust_info` 的结果把三个字段写全，不用 `flatten`；
  - `Disposition` 由 `session/send` 和 `queue_edit` 的 `steer` 结果共用；
  - `set_config.option` 用 `ConfigOptionId`；
  - `project/remove` 的冲突写成 `conflict/turn_running`；
  - 原因不止一种的 `forbidden`、`unavailable` 只写 code。
- 5.8.3 的"按档位规则"行和"privileged 选项"行是资源级规则，没有放进 `MATRIX`，因为每个方法只能出现在一行里。

## 验证方式

- 新增 UNIT-proto-methods-core-session，测试名都以 `unit_proto_methods_core_session_` 开头，共 9 个：
  - 各组样例恰好覆盖本组方法，参数和结果都往返通过；
  - 本任务的矩阵行逐格比对，且本组方法单独成行；
  - SEC-9 格子（期望集合，并核对 remote_device 确实持有这些 scope）；
  - 里程碑和补充的错误码；
  - `digit`：9 能解析，0、10、255 都被拒绝，schema 是 1–9；
  - `QueueOp` 和工作区联合的开放性；
  - 队列满的错误对象；
  - 两个开放枚举的往返。
- T012 的跨组测试（`catalog.rs`）加入新方法后仍然通过。
- `cupper-protocol` 能编译到 `aarch64-apple-ios-sim`。
- 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T018-protocol-methods-session/ci/`：rust 510 个通过，ts 64 个通过，license 通过。摘要是 `pr-512b8e4da01b.json`。

## 规模

最终相对分支起点 **1,355 行新增、12 行删除**（只算 `crates/`），其中代码约 825 行，测试约 530 行。估计是 1,290 行（不含 `archive`），在约 1,500 行的上限以内，也没有超过 1,450 行的停止线。

| 部分 | 行数 |
|---|---|
| `project.rs` | 代码 216，测试 66 |
| `session.rs` | 代码 447，测试 153 |
| `interaction.rs` | 代码 151，测试 87 |
| 共享文件（`catalog.rs`、`testkit.rs`、`mod.rs`、`error.rs`） | 约 235，几乎都是测试 |

代码比估计少约 120 行，测试多约 180 行。测试超出主要因为 rustfmt 把矩阵行的方法清单逐项展开。每完成一组都统计过，各组数字见 `notes.md`。

## 提交列表

```
e4c9c92 docs(task): record the T018 pr CI summary
512b8e4 docs(task): hand session/archive over to T013
fba70cd docs(task): record the T018 line counts and shape choices so far
03c0b7c feat(protocol): return too_large/queue_full on a full queue
82864b8 feat(protocol): add the interaction methods
a400a35 feat(protocol): add the session methods
a00b881 feat(protocol): add the project and agent methods
0aed0c5 docs(task): estimate T018 lines by method group
```

## 遗留问题 / 范围外发现

- **T013**：
  - 补 `session/archive`，具体位置见 `notes.md` 的"进度"一节；
  - `git/revert.input` 复用 `methods::interaction::InputEvidence`。
  - 它多半也会在 `catalog.rs` 的矩阵测试附近加辅助函数，合并时可能冲突，到时需要去重。
- **T031**：
  - 实现所有 `Sec9` 格子；
  - 核对 `key` 与 `digit` 一致；
  - 核对 `digit` 与所选选项的序号相符。
- **T035**：`trust_digest` 按 `d_` 加 64 位十六进制输出。
- **历史提交**：`a400a35` 的提交信息写着"`archive` follows once its blockers exist"，已被移交 T013 取代。按守则没有改写历史。

## 追加：审阅后的修改

已改好，新摘要是 `pr-945a03176cae.json`。

- **代码修改**（`945a031`）：`KeyDigit::try_from` 的错误类型改成 `DigitOutOfRange(pub u8)`，用 thiserror，提示文字还是 "digit N is not 1 to 9"，serde 的 `try_from` 照常能用。
- **用例**：现在对 0、10、255 先直接断言 `KeyDigit::try_from(bad) == Err(DigitOutOfRange(bad))`，再检查 serde 报的错误文字。
- **CI**：旧摘要 `pr-f9b5d24a562f.json` 已删掉，没有提交。在干净的 worktree 里重跑了 `cargo xtask ci pr --record docs/tasks/T018-protocol-methods-session/ci/`，rust 538 个、ts 64 个、license 1 个全部通过。新摘要已提交（`7e2500e`），里面没有本机路径或用户名。
