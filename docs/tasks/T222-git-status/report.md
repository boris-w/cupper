# T222 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务与 CLI）
- 分支：`feat/T222-git-status`（从 main `dd947c5` 开出）
- 完成日期：2026-11-03
- PR 和 CI 运行链接：还没有。`7f84a9e` 已请你推送；交汇报时还没开 PR，CI 也没跑。CI 失败就叫我回来修。

## 做了什么

1. **HEAD**（`git/head.rs`）：在 T120 只取提交的基础上，加回分支、上游和领先落后，经仓库档两次调用取得：
   - 先 `branch --show-current`；
   - 在分支上时再 `for-each-ref`，取提交、`upstream:short` 和 `upstream:track`；游离 HEAD 时改用 `rev-parse --verify -q HEAD`。
   - 两条命令不管 HEAD 处在什么状态都以 0 退出（T098 规则 5）。
   - `upstream:track` 的解析写成纯函数 `tracked`，有单元用例。
   - `staged` 范围改为用 `Head.commit`。
2. **`git/status`**（`git/status.rs`，从 `38f85e0` 取回）：
   - 文件状态在快照档的临时 index 上算：先 `update-index -q --unmerged --refresh`，再 `diff-index --cached -z --name-status -M <HEAD 或空树>` 得出 index 一栏，`diff-files -z --name-status` 得出工作区一栏，`ls-files -o` 得出未跟踪的文件。
   - `name_status` 解析有单元用例。
   - 进行中的操作看工作区 git 目录里的标记文件：rebase-merge、rebase-apply、MERGE_HEAD、CHERRY_PICK_HEAD、REVERT_HEAD、BISECT_LOG。
   - 新增快照档命令 `Refresh`、`DiffIndex`、`DiffFiles`。
3. **`git/branches`**：按项目路径在仓库档执行 `for-each-ref --format=%(HEAD)%00%(refname) refs/heads refs/remotes`，返回当前分支、本地分支和远程跟踪分支。
4. **`cupper diff <session> [--scope last-turn|unstaged|staged|<回合 ID>]`**：
   - 先调 `git/diff`，再对每个文件调 `git/file_diff`。
   - `--json` 时每个结果一行；文字模式先给一行统计，再逐个文件给出改动种类和行数，接着是各单位的表头和行；行放在 blob 里的，注明 blob 的大小和哈希。
   - 文字输出由纯函数 `patch` 生成，有单元用例；参数解析也有单元用例。
5. **接线**：`Routes::git` 加上 `git/status` 和 `git/branches`；service 按方法分派；UNIT-unsupported-set 去掉两行。

协议没有改动，所以 `schema-compat` 不受影响。

## 关键取舍

- **修了取回代码里的一个错误**：`diff-files` 对未合并的路径会报两次，先报 `U`，再拿 stage 2 和工作区比一次报 `M`。原来的代码让后一条覆盖了前一条，工作区一栏变成 `modified`。现在工作区一栏已是 `unmerged` 时保持不变。这是 IT-git-status 查出来的。
- **`cupper diff` 的测法**：
  - 文字输出用纯函数的单元用例测。
  - 进程级的 IT-cli-commands 加了一段，只确认 CLI 经宿主到 git 服务这条路是通的：夹具的项目不是仓库，返回 `not_a_repo`。
  - 进程级的成功输出没有测。要测就得在那个夹具里建一个仓库，项目目录里还放着数据目录，代价不小。

## 这次估计偏低的原因

开工前估约 440 行，暂停时推算约 530，最终 532。偏低在三处：

1. **`head.rs` 是改写**：差异按整个文件计，估了 25 行，实际约 +100。
2. **`status.rs`**：连单元用例 184 行，估的是 135 行加用例。
3. **`cupper diff`**：连单元用例 104 行，估了约 75 行。

## 验证方式

- **IT-git-status**（会话夹具加真实 git，`it_git_status_head_files_operations_and_branches`）：
  - 还没有提交时：分支 main、没有上游，文件 `a.txt added unmodified`。
  - 有提交、并设好上游后：`origin/main`，领先 1、落后 1；五个文件依次是工作区修改、index 新增、index 重命名（带 `old_path`）、工作区删除、未跟踪。
  - `git/branches`：current 是 main，local 是 [main]，remote 是 [origin/main]。
  - 游离 HEAD 加 MERGE_HEAD 加构造的冲突：没有分支，`in_progress` 是 merge，`u.txt` 两栏都是 unmerged。
- **单元用例**：`unit_git_status_head_*`（`upstream:track` 的四种写法）、`unit_git_status_name_status_*`（七种字母和重命名、复制）、`unit_cli_diff_shows_a_file_and_its_units`，CLI 参数用例里加了 diff 的几种 scope 和错误写法。
- **IT-cli-commands**：`cupper diff <s> --scope unstaged` 以 1 退出，reason 是 `not_a_repo`。
- **本机**（单份运行）：
  - `cargo test --workspace --all-features --no-fail-fast` 全部通过。
  - `cargo clippy --workspace --all-targets`，带和不带 `--all-features` 都没有警告。
  - `cargo xtask ci testdata-scan` 通过。
- **规模**：`cargo xtask size` 是 **+532 −46**（cupper-cli +104，cupper-host +428 −46），在放宽后的停止线 540 以内。分支上的 brief 还写着 450/550，工具按旧数字提示越线，可以忽略。
- **验收标准**：
  - IT-git-status 通过。
  - `schema-compat`：协议没有改动。
  - CI 的 8 项：等推送后的结果。

## 提交列表

```
7f84a9e feat(git): git/status, git/branches and `cupper diff`
91d1cbf feat(git): git/status and git/branches (in progress)
```

## 遗留问题 / 范围外发现

1. **本机跑 IT-cli-commands 的注意事项**：它用的是 target 目录里 e2e 构建的 `cupper` 二进制。只跑 `cargo test -p cupper-host` 时这个二进制不会重编，旧的没有 `diff` 命令，用例会失败。先 `cargo build -p cupper-cli --features test-harness`，或者用 `--workspace` 跑。CI 不受影响。
2. **给 T125**：`status::in_progress(git_dir)` 可以直接用在 ADR-10 的第 3 条规则。
3. **status 的开销**：一次调用要两次仓库档调用，每次前后各遍历一遍工作区找 `.gitattributes`，加上快照档的 refresh。和 T120 汇报第 5 条说的是同一件事，界面频繁轮询之前需要缓存。
