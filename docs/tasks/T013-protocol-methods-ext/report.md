# T013 汇报

- 执行者：子 agent（Rust 工程师，协议）
- 分支：`feat/T013-protocol-methods-ext`
- 完成日期：2026-10-19

## 做了什么

5.3.4 后两组的 23 个方法和编排者移交的 `session/archive` 都已定义，只有类型，不含实现。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。本任务共改动 1,411 行（新增）、13 行（删除），在 1,500 行的上限以内。

1. **按构建门控的方法**
   - 新增 `rpc::Build`（`TestHarness`、`DevAuth`），线上值就是宿主的 cargo 特性名。`Method::BUILDS` 默认为空，即所有构建都有。
   - `method!` 多了可选的 `builds: [...]`。schema 只在非空时写出 `x-builds`，例如 `["test-harness", "dev-auth"]`；`annotations.rs` 的文档表格加了一行。
   - 门控的方法有两类：`diag/recording_export` 在 test-harness 和 dev-auth 构建里有；`test/*` 只在 test-harness 构建里有。
2. **git、工作区与文件**（`methods/git.rs`，M1b，12 个方法）：
   - `git/status`、`diff`、`file_diff`、`stage`、`unstage`、`revert`、`commit`、`push`、`branches`；
   - `worktree/check`、`files/search`、`workspace/open`。
   - `git/diff` 带 `caveats`，并复用 T017 的 `DiffStat`、`ExcludedPath`、`SnapshotStatus`。
   - `git/stage`、`unstage`、`revert` 的 `input` 复用 T018 的 `interaction::InputEvidence`。
3. **阻塞原因枚举 `methods::git::WorktreeBlocker`**：ADR-10 清理规则一条一个取值，按规则顺序为：
   - `modified_files`、`untracked_files`、`operation_in_progress`、`foreign_lock`；
   - `head_only_commits`、`unpushed_commits`、`initialized_submodules`。
   - 每个取值的文档注明了对应第几条规则。`worktree/check.blockers` 和 `session/archive.blockers` 共用这个枚举。
4. **设置、认证、诊断、测试**（`methods/admin.rs`，11 个方法）：
   - `settings/get`、`settings/update`；
   - `auth/elevate`、`auth/elevation_get`、`auth/elevation_respond`；
   - `policy/describe`；
   - `diag/export`、`diag/recording_export`；
   - `test/clock_advance`、`test/run_maintenance`、`test/seed`。
5. **`session/archive`**（编排者移交，写在 T018 的 `methods/session.rs`）：
   - 参数 `{command_id, session_id, worktree: keep|remove|remove_case_only_dirty}`；
   - 结果 `{worktree: kept|removed|blocked, blockers?}`；
   - write、命令、M1b，错误码 `conflict/turn_running`；
   - 已补进它所在的矩阵行和 SEC-9 期望集合。
6. **权限矩阵**（`catalog.rs` 只做附加）：
   - `MATRIX` 加了 11 行，每行注明 5.8.3 的出处。
   - 新增三种格子，各自注明交给谁做：
     - `Input`：真实输入，T031 拒绝 window 发来的、没有 `input` 的请求；
     - `Build`：仅特定构建，宿主在其他构建里没有这个方法；
     - `CliOnly`：`auth/elevate` 不要求 scope，但 T031 对除 `cli` 以外的 principal 一律拒绝。
   - remote_device 对 git 写操作和 `settings/update` 的 ✗ 改用 T018 的 `Sec9`，并补进 SEC-9 期望集合。
   - ext 的矩阵测试改用 T018 的 `check_rows` 和 `names`。
7. **测试**：新增的测试都以 `unit_proto_methods_ext_` 开头，覆盖：
   - 两组方法的样例覆盖与往返；
   - 本任务的矩阵行逐格比对；
   - `Input`、`Build`、`CliOnly` 三种格子与 schema 一致；
   - 里程碑、错误码与门控标注；
   - git 的 `scope`、`target` 形状；
   - `WorktreeBlocker` 正好是 ADR-10 的 7 条；
   - 设置 patch 的设置与清空；
   - `session/archive` 的 blockers。
   - 各组的样例也加进了 T012 的跨组覆盖测试。

## 关键取舍

完整清单在 `notes.md` 的"实现时的取舍"一节。下面几处是设计没写死、由我补的，请裁定是否写进 5.3.4：

**门控与矩阵**
- **`x-builds` 的形式**：
  - 不叫 `x-features`，免得和握手的 `features` 混淆；
  - `BUILDS` 有默认值（空），与 `SCOPE` 等常量不同，因为绝大多数方法不门控；
  - 漏写由测试兜住：`Build` 格子必须恰好对应带 `x-builds` 的方法，另有一个测试写死门控方法的清单。
- **`diag/recording_export` 的 scope 定为 `ui_local`**：5.3.4 的 scope 一栏只写了构建限制，`ui_local` 是唯一与 5.8.3 那一行逐格相符的 scope。

**git 组**
- **`git/diff.scope` 和 `git/*.target`**：照 5.3.4 的线上形式，用 serde 的外部标签枚举实现：
  - `scope` 是 `"unstaged" | "staged" | "last_turn" | {"turn": id}`；
  - `target` 是 `{"all": true} | {"paths": [...]} | {"hunk_ids": [...]}`，只能有一个键，由 serde 保证；
  - 两者都是封闭的：只有客户端发送，宿主拒绝不认识的取值（5.3.10）。
- **`git/status.branch`、`git/branches.current` 改为可选**：detached HEAD 时没有分支。这与 5.3.4 写的必填不同。
- **`git/diff.files[].change` 复用 `DiffChange`**：改名写成 `update` 加 `old_path`。

**设置组**
- **扁平键**：设置用 5.2.11 的点号键名，`agents.<agent>.cli_path` 展开成 claude、codex 两个键。一个小宏从同一张键表生成 `Settings` 和 `SettingsPatch`。
- **patch 里的 `null`**：只能用来清空 `cli_path` 这类可为空的键，其他键给 `null` 时解析失败。

**其他方法组**
- **`policy/describe`**：`action` 是开放枚举，与 5.8.3 的"原生确认"格子一一对应，共 7 个取值；`details` 是不带文案的语义参数对象，每种 `action` 带哪些键留给 T031。
- **`diag/export.include_sessions`** 是会话 ID 的列表，依据 5.11.3 的"用户逐个勾选会话"。
- **`test/*` 的参数形状**由我补上，T047 实现时可以再细化：`clock_advance {ms}`、`run_maintenance {task}`、`seed {profile}`。

**写法约定**
- **共用类型（命名规则的例外）**：`GitApplyParams`（stage、unstage、revert）、`SettingsResult`、`DiagExportResult`。
- **错误码写法与 T018 一致**：`code（a、b）` 写成 `code/a`、`code/b`；`code（含 a）` 只写 `code`。

## 验证方式

- **CI**：在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T013-protocol-methods-ext/ci/`，通过，摘要为 `ci/pr-97c7c2e4577d.json`。
  - rust：546 个通过（含 fmt、各特性组合的 clippy、release/dev-auth/test-harness 的宿主测试、cargo-deny）；
  - ts：64 个通过；
  - license：1 个通过。
- **单独核对**：`cargo test -p cupper-protocol` 90 个全过；`cargo clippy -p cupper-protocol --all-targets -- -D warnings` 无警告。
- **iOS 模拟器**：`cargo build -p cupper-protocol --lib --target aarch64-apple-ios-sim` 通过。xtask 的 pr 流程里没有这一步，是我手动跑的。
- **对照验收标准**：
  - 每个方法的参数和结果都能往返，取值用 5.3.4 的典型值：`git::tests::samples`、`admin::tests::samples`，`archive` 的样例在 `session::tests::samples`。
  - scope 与 5.8.3 逐格一致：
    - 新增行由 T012 的全表测试和 `unit_proto_methods_ext_scopes_match_the_matrix` 检查；
    - scope 决定不了的格子由 `unit_proto_methods_ext_cells_beyond_the_scope_match_the_schema` 检查。
  - 门控标注能在 schema 里查到：`unit_proto_methods_ext_milestones_errors_and_builds_are_in_the_schema` 写死了这 4 个方法及其 `x-builds`。
  - T012 的跨组测试仍然通过；新增测试都以 `unit_proto_methods_ext_` 开头。
- **行数**：每完成一组都统计一次，记在 `notes.md`。
  - 估计约 1,355 行，加移交的 `archive` 约 45 行；
  - 实际 1,411 新增 / 13 删除，其中代码 973、测试 438；
  - 写完第 1 组时推算约 1,440 行，接近 1,450 行的停止线，于是压缩了测试，第 2 组也写得紧凑一些。

## 提交列表

```
7fbea26 docs(task): record the T013 pr CI summary
97c7c2e docs(task): record the T013 final line counts and choices
d66fcf0 test(protocol): check the T013 rows with the shared check_rows
e483842 feat(protocol): add session/archive
ebe938d refactor(protocol): reuse the input evidence of interaction/respond
305f058 Merge branch 'main' into feat/T013-protocol-methods-ext
8a2651e docs(task): record the T013 line counts after the last group
872a362 feat(protocol): add the settings, auth, diagnostics and test methods
a69bccc test(protocol): keep the matrix changes additive
d75ef66 docs(task): record the T013 line counts after the git group
8fc6d1b feat(protocol): add the git, worktree and file methods
58a47e5 feat(protocol): mark methods that only some host builds have
b877109 docs(task): estimate the T013 line counts by group
```

## 遗留问题 / 范围外发现

1. **5.3.4 有几处要改**：
   - `git/status.branch` 写成了必填，但 detached HEAD 时没有分支，本任务已改为可选；
   - `diag/export.include_sessions?` 容易读成布尔值，实际是会话 ID 的列表；
   - `diag/recording_export` 的 scope 一栏没有给出 scope。
2. **两处 schema 不够精确**（都是客户端发给宿主的方向，宿主解析时会拒绝，不影响正确性）：
   - `Settings` 的 `cli_path` 实际总会出现（值可为 `null`），但 schema 里不是必填；
   - `SettingsPatch` 的非空键在 schema 里也允许 `null`。
3. **`apps.terminal`、`apps.editor` 的内置应用清单没有设计**：界面怎样拿到可选的应用列表，需要在 T036 或 T126 里补上。
4. **交给 T031**：
   - 新增的 `Sec9` 格子：git 写操作和 `settings/update`；
   - `CliOnly`：`auth/elevate`；
   - `git/revert` 的 `input` 证据；
   - `settings/update` 改安全相关的键要 privileged；
   - `policy/describe` 每种 `action` 的 `details` 键。
5. **交给 T030、T033**：带 `x-builds` 的方法只在相应特性的构建里注册；UNIT-unsupported-set 要把 M2 的 `diag/export` 算进"返回 `unsupported`"的集合。
6. **交给 T015**：方法条目可能带 `x-builds`，导出时要保留。
7. **CI 流程**：`cargo xtask ci pr` 里没有 `aarch64-apple-ios-sim` 的构建步骤，5.1.6 要求每个 PR 都跑。目前只能手动检查，建议补进 xtask。
8. **`notes.md` 里有一节已过时**：中途写的"合入 main 之后还要做的"已经全部完成，保留在那里作为过程记录。
