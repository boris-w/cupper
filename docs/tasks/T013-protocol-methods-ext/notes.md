# T013 工作笔记

## 行数估计（开工前）

### 估算方法

沿用 T012 笔记的密度（T012 实测：第 1 组估 305 行、实际 308 行；第 2 组估 300 行、实际 306 行，代码部分基本准确，测试偏多）：

| 写法 | 行数 |
|---|---|
| 结构体本身（derive、文档、括号、空行） | 约 4.5 行 |
| 字段（必填约 2 行，可选约 3 行） | 平均约 2.3 行 |
| `open_enum!` | 约 6 行，每个取值另加 1 行，带文档的取值再加 1 行 |
| `method!` 声明（带文档） | 约 8 行 |

### 按方法组估计

| 组 | 方法 | 类型数（结构体 / 联合 / 枚举，字段数） | 代码 | 测试 |
|---|---|---|---|---|
| 0 基础 | — | 构建门控：`rpc::Build`、`Method::BUILDS`（默认空，即所有构建都有）、`method!` 的可选 `builds:`、schema 写出 `x-builds`、`annotations.rs` 表格加一行 | 45 | — |
| 1 git、工作区与文件 | `git/status`、`diff`、`file_diff`、`stage`、`unstage`、`revert`、`commit`、`push`、`branches`，`worktree/check`、`files/search`、`workspace/open`（12 个） | 约 24 / 2（`git/diff` 的范围、`git/*` 的目标）/ 10（含 `WorktreeBlocker`），约 65 个字段 | 520 | 130 |
| 2 设置、认证、诊断、测试 | `settings/get`、`update`，`auth/elevate`、`elevation_get`、`elevation_respond`，`policy/describe`，`diag/export`、`recording_export`，`test/clock_advance`、`run_maintenance`、`seed`（11 个） | 约 22 / 0 / 6，约 60 个字段（`Settings` 和 `SettingsPatch` 各 11 个键） | 420 | 110 |
| 跨组测试 | — | `MATRIX` 加行（每行注明 5.8.3 出处）；格子加"策略拒绝""真实输入""仅特定构建"三种；矩阵检查抽成共享函数；`unit_proto_methods_ext_` 的样例覆盖、矩阵、门控标注、里程碑与错误码 | — | 130 |
| **合计** | 23 个方法 | | **约 985** | **约 370** |

**总计约 1,355 行**，按 ±10% 在 1,220 到 1,490 行之间。高于计划的 M（400–900 行），低于约 1,500 行的上限，所以先不停下来；但余量不大：

- **停止线**：每写完一组统计一次。按已完成组的实际偏差推算，总数预计超过约 1,450 行时，停下来汇报。
- 超出 M 的主要原因：本任务 23 个方法，比 T012 的 18 个多；git 组的结果结构多（状态、diff、hunk、检查结果各有嵌套）；`Settings` 和 `SettingsPatch` 两张键表。

## 实际行数

按 `git diff --numstat b877109 -- crates/` 统计（测试指 `#[cfg(test)]` 里的行）。

### 第 0、1 组之后（提交 `58a47e5`、`8fc6d1b`）

| 组 | 文件 | 代码 | 测试 | 小计 | 估计 |
|---|---|---|---|---|---|
| 0 基础 | `rpc.rs`、`schema.rs`、`annotations.rs`、`methods/mod.rs` 的宏 | 39 | — | 39 | 45 |
| 1 git、工作区与文件 | `methods/git.rs`、`mod.rs` 一行、`catalog()` 两行 | 531 | 156 | 687 | 650 |
| 跨组测试（本组部分） | `catalog.rs`：两种新格子、四行矩阵、`check_rows`、两个 ext 测试 | — | 96 | 96 | 130 中的一部分 |
| **小计** | | **570** | **252** | **822 新增 / 20 删除** | |

- 第 1 组代码与估计一致（531 对 520）。测试多出约 25 行：样例经 rustfmt 展开后比估计长，已压缩过一次（184 → 156 行）。
- 推算：第 2 组按代码 +2%、测试 +20% 计，约 430 + 130 行；跨组测试还剩约 55 行（"仅特定构建"格子、四行矩阵、门控标注与里程碑测试）。总计约 1,440 行，低于 1,450 行的停止线，继续；第 2 组写得紧凑一些。

### 第 2 组之后（提交 `a69bccc`、`872a362`）

| 组 | 文件 | 代码 | 测试 | 小计 | 估计 |
|---|---|---|---|---|---|
| 0 基础 | 同上 | 39 | — | 39 | 45 |
| 1 git、工作区与文件 | 同上 | 531 | 156 | 687 | 650 |
| 2 设置、认证、诊断、测试 | `methods/admin.rs`、`mod.rs` 一行、`catalog()` 一行 | 392 | 83 | 475 | 530 |
| 跨组测试 | `catalog.rs`：三种新格子、十一行矩阵、三个 ext 测试 | — | 167 | 167 | 130 |
| **合计** | | **962** | **406** | **1,368 新增 / 13 删除** | 1,355 |

- 第 2 组比估计少：`Settings` 和 `SettingsPatch` 由一个小宏从同一张键表生成，比手写两张表少约 40 行。
- 跨组测试多出约 35 行：编排者中途裁定 `catalog.rs` 只做附加（T018 已把逐格检查抽成 `check_rows`），所以 `a69bccc` 把我先写的 `check_rows` 撤回，T012 的矩阵测试恢复原样；ext 测试只检查 scope 决定不了的部分（"真实输入"格子要有 `input` 参数，"仅特定构建"格子与 `x-builds` 一致）。

### 合入 main（含 T018）之后还要做的

编排者裁定（范围调整）：
1. `git/revert`（以及 `git/stage`、`git/unstage`）的 `input` 改用 T018 的 `methods::interaction::InputEvidence`，删掉 `git.rs` 里自己定义的 `InputEvidence`、`InputKind`、`InputKey`（约 −40 行）。
2. `session/archive` 改由本任务做，写进 T018 的 `methods/session.rs`，`blockers` 用 `WorktreeBlocker`；`MATRIX` 和 SEC-9 期望集合补上它（约 +45 行）。
3. ext 的矩阵测试改用 T018 的 `check_rows`；"策略拒绝"格子与 T018 的 `Sec9` 对齐（约 +15 行）。

推算合入后总计约 1,390 行，在 1,500 行以内。

### 合入 main 之后（提交 `ebe938d`、`e483842`、`d66fcf0`，最终）

口径：`git diff --numstat 32a8886 -- crates/`。`32a8886` 是合入本分支的 main（含 T018），所以只算本任务的改动。

| 组 | 文件 | 代码 | 测试 | 小计 | 估计 |
|---|---|---|---|---|---|
| 0 基础 | `rpc.rs`、`schema.rs`、`annotations.rs`、`methods/mod.rs` 的宏 | 39 | — | 39 | 45 |
| 1 git、工作区与文件 | `methods/git.rs`、`mod.rs` 一行 | 491 | 145 | 636 | 650 |
| 2 设置、认证、诊断、测试 | `methods/admin.rs`、`mod.rs` 一行 | 391 | 83 | 474 | 530 |
| `session/archive`（编排者移交） | `methods/session.rs` | 49 | 19 | 68 | 45 |
| 接线与跨组测试 | `catalog.rs`：`use` 与 `catalog()` 3 行；新格子、矩阵行、ext 测试、SEC-9 期望集合 | 3 | 191 | 194 | 130 |
| **合计** | | **973** | **438** | **1,411 新增 / 13 删除** | 1,400 |

- 改用 T018 的 `InputEvidence` 后，`git.rs` 少了 49 行（代码 38、测试 11）。
- 跨组测试比估计多约 60 行：rustfmt 把矩阵行的方法清单逐项展开（11 行矩阵约 95 行），另有 ext 测试 4 个。

## 实现时的取舍

形状没有写死、由我补上的地方，请编排者裁定是否写进 5.3.4：

**基础**
- **构建门控**：`Method::BUILDS: &[Build]`，`Build` 取 `TestHarness`、`DevAuth`，线上值就是宿主的 cargo 特性名。schema 只在非空时写出 `x-builds`，例如 `["test-harness", "dev-auth"]`；不写表示所有构建（含发布构建）都有。
  - 名字用 `x-builds`，不用 `x-features`，免得和握手的 `features`（`stream.coalesce` 等）混淆。
  - 与 `SCOPE` 等常量不同，`BUILDS` 有默认值（空）：绝大多数方法不门控。漏写由测试兜住：矩阵里"仅 … 构建"的格子（`Build`）必须恰好对应带 `x-builds` 的方法，另有一个测试写死门控方法的清单。
- **错误码的写法**与 T018 一致：`code（a、b）` 写成 `code/a`、`code/b`；`code（含 a）` 只写 `code`，因为原因不止列出的那些。

**git 组**
- **`WorktreeBlocker`**（`methods::git`）：ADR-10 清理规则一条一个取值，按规则顺序为 `modified_files`、`untracked_files`、`operation_in_progress`、`foreign_lock`、`head_only_commits`、`unpushed_commits`、`initialized_submodules`。`worktree/check.blockers` 和 `session/archive.blockers` 共用。
- **`git/diff.scope`、`git/*.target`** 照 5.3.4 的线上形式：`"unstaged" | "staged" | "last_turn" | {"turn": id}`，`{"all": true} | {"paths": [...]} | {"hunk_ids": [...]}`。用 serde 的外部标签枚举实现，"恰好一个键"由 serde 保证。它们是封闭的：只有客户端发送，宿主拒绝不认识的取值（5.3.10）。`{"all": false}` 无效，由宿主检查。
- **`git/status`**：`branch` 改为可选（detached HEAD 时没有），与 5.3.4 写的必填不同；`git/branches.current` 同理。文件状态 `GitFileStatus` 取 git status 的各列：`unmodified`、`modified`、`type_changed`、`added`、`deleted`、`renamed`、`copied`、`unmerged`、`untracked`。另加 `old_path?`（改名、复制的来源）。`in_progress` 取 ADR-10 第 3 条的五种操作。
- **`git/diff.files[].change`** 复用 `DiffChange`（`add`、`update`、`delete`）；改名写成 `update` 加 `old_path`。`filter_unsupported` 只在为 true 时出现。`snapshot` 是 `{status: SnapshotStatus, excluded: [ExcludedPath]}`，`stat` 复用 `DiffStat`。
- **`git/file_diff`** 的 hunk：`DiffHunk {hunk_id, unit, header, lines?: [string], lines_blob?: BlobRef}`。`lines` 每项带 ` `、`+`、`-`、`\` 前缀，不含换行。`filter_unsupported` 的文件 `hunks` 为空。
- **`git/stage`、`unstage`、`revert` 共用参数类型 `GitApplyParams`**，是按方法名命名的例外；同类例外还有 `SettingsResult`（`settings/get` 和 `update`）、`DiagExportResult`（两个 `diag/*`）。`input` 用 T018 的 `interaction::InputEvidence`。
- **`git/commit.oid`** 标 `x-id`；`git/push.set_upstream` 只在为 true 时出现。
- **`worktree/check`**：`removable` 等于 `blockers` 为空。`warnings` 是 `{ignored_files: 数量, case_only_dirty: bool}`；`case_only_dirty` 时要删除，须用 `session/archive` 的 `remove_case_only_dirty`。
- **`files/search`**：`session_id`、`project_id` 是两个可选字段，恰好给一个，由宿主检查。没用 `flatten` 加 untagged 枚举，免得生成 TS 时出问题。`ranges_utf16` 是 `[start, end)` 的数组。
- **`workspace/open.target`**：开放枚举 `terminal`、`editor`、`finder`。

**设置、认证、诊断、测试组**
- **设置**：键名照 5.2.11，用点号连接的扁平键。`agents.<agent>.cli_path` 展开成 `agents.claude.cli_path` 和 `agents.codex.cli_path`。
  - 一个小宏从同一张键表生成 `Settings` 和 `SettingsPatch`。`Settings` 的每个键都会出现，`cli_path` 没设置时为 `null`。
  - patch 里只写要改的键；`null` 只能用来清空可为 `null` 的键（`cli_path`），其他键给 `null` 时解析失败。
  - 两处 schema 不够精确：`Settings` 的 `cli_path` 在 schema 里不是必填；`SettingsPatch` 的非空键在 schema 里也允许 `null`。方向都是客户端发给宿主，宿主解析时会拒绝。
  - `apps.terminal`、`apps.editor` 是宿主内置清单里的应用 ID（字符串）。设计里没有这份清单，也没写界面怎样拿到它。
- **`auth/elevate`** 的结果是 `{granted: [Scope], expires_at?}`：`granted` 可能比申请的少，为空表示被拒；`expires_at` 只在批准了某些 scope 时出现。
- **`auth/elevation_get`**：`verified` 是 `{executable, signing?, pid, parent_chain: [{pid, executable}], has_tty, origin_unverifiable}`，`self_reported` 是 `{client_name, reason?}`。
- **`policy/describe`**：参数是 `{method, params}`，`params` 原样给出。结果是 `{action, details}`。
  - `action` 是开放枚举，与 5.8.3 的"原生确认"格子一一对应：`trust_project`、`confirm_config`、`full_access`、`privileged_option`、`security_setting`、`grant_elevation`、`interrupt_turns`。
  - `details` 是语义参数的对象，不含界面文案。每种 `action` 带哪些键由 T031 定。
- **`diag/export.include_sessions`** 是会话 ID 的列表：5.11.3 写的是"用户逐个勾选会话"，5.3.4 的 `include_sessions?` 容易读成布尔值。
- **`diag/recording_export` 的 scope 是 `ui_local`**：5.3.4 的 scope 一栏只写了构建限制，`ui_local` 是唯一与 5.8.3 那一行逐格相符的 scope。
- **`test/*` 的参数**：
  - `test/clock_advance {ms: DurationMs}`；
  - `test/run_maintenance {task}`，`task` 取自 5.4.5：`ledger`、`raw_frames`、`audit`、`orphan_blobs`；
  - `test/seed {profile: {sessions, turns, items_per_turn, mix: sp1, large_turn_items?}}`；
  - 结果都是 `{}`，幂等为"无"。T047 实现时可以再细化。

**`session/archive`**（编排者移交）
- 参数 `{command_id, session_id, worktree: keep|remove|remove_case_only_dirty}`（`ArchiveWorktree`），结果 `{worktree: kept|removed|blocked, blockers?}`（`WorktreeOutcome`；`blockers` 为空时不出现）。
- `blocked` 时会话照样归档，只是保留了 worktree。错误码是 `conflict/turn_running`，与 `project/remove` 一致。

**权限矩阵**
- 本任务的行与 T018 的行分开，即使同属 5.8.3 的一行；ext 测试用共享的 `check_rows`，要求本任务的方法单独成行。
- 新增三种格子（scope 都允许）：
  - `Input`：真实输入，用在 `git/revert` 的 window 一列；
  - `Build`：仅特定构建，用在 `diag/recording_export`、`test/*`；
  - `CliOnly`：`auth/elevate` 不要求 scope，但除 `cli` 以外都由策略拒绝。
- remote_device 对 `git/stage`、`unstage`、`commit`、`push`、`revert` 和 `settings/update` 的 ✗ 属于 SEC-9（不能直接写文件、改设置），用 T018 的 `Sec9`，并补进 SEC-9 期望集合。
- `settings/update`（安全相关的键）那一行是资源级规则，不进 `MATRIX`：每个方法只能出现在一行里，方法级的 scope 由"普通键"那一行决定。

## 给后续任务的说明

- **T031（策略）**：
  - 新增的 `Sec9` 格子：git 的写操作和 `settings/update` 对 remote_device 一律拒绝；
  - `CliOnly`：`auth/elevate` 只接受 `cli`；
  - `Input`：window 发来的 `git/revert` 没有 `input` 就拒绝（`forbidden/user_activation_required`）；
  - `settings/update` 改安全相关的键（`agents.*.cli_path`、`agents.env_passthrough`、`diagnostics.verbose`）要 privileged；
  - `policy/describe` 每种 `action` 的 `details` 键。
- **T030、T033（网关、方法分派）**：带 `x-builds` 的方法只在相应特性的构建里注册；UNIT-unsupported-set 把 `diag/export`（M2）算进"返回 `unsupported`"的集合。
- **T015（schema 导出）**：方法条目可能带 `x-builds`。
- **T036（设置）**：键表和 patch 的语义见上文"设置"；`apps.*` 的内置清单还没有设计。
- **T047（测试接口）**：`test/*` 的参数形状见上文，可以按实现需要细化。
