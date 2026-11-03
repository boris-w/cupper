# T228 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`test/T228-git-hunk-cases`（从 main `0404e8c` 开出）
- 完成日期：2026-11-03
- PR：https://github.com/boris-w/cupper/pull/59 ，CI 由你在盯，交汇报时结果还没出。

## 做了什么

只加了测试：新文件 `orchestrator/session/hunk_tests.rs`（IT-git-hunk），另在 `session/mod.rs` 登记了这个模块。产品代码没有改。

- **走的是产品路径**：每个场景先在临时目录建一个真实 git 仓库，开一个会话，然后依次走 `git/diff`、`git/file_diff`，按标记挑出单位，再调用 `git/stage`、`git/unstage` 或 `git/revert`。挑单位的方式有三种：第一个单位、mode 单位、改动行里含某段文字的单位。
- **表驱动**：每个 SP-5 场景一张 `steps!` 表，一行对应 SP-5 的一行，格式是"标签：操作 文件，单位 => 之后的内容"。宏会按操作对应的范围现做一次 diff：取消暂存用 `staged`，其余用 `unstaged`。
- **核对**：
  - 暂存和取消暂存，核对 index 里的 blob 和 mode；还原核对工作区的字节和 mode，符号链接核对它的目标。
  - 期望内容由编辑表算出来（行、换行符、替换、插入、删除），不是拿 git 的输出当答案。
  - 每一步还自动检查两件事：暂存和取消暂存不动工作区；还原不动 index 的条目，这对应 SP-5 的 "a worktree revert must never change index entries"。
  - 失败时报出 SP-5 的标签和文件名，例如 `stage with --whitespace=nowarn ("w.txt")`。
- **没有拆成表的步骤**：conflict、turn_revert、index_lock、rename、mode_change 的末段，要先拿到 token、再改仓库让 diff 过时，然后用旧 token 去操作。这几处直接调用 `apply` 和 `refused`。
- **T121 的交接**：加了 `it_git_hunk_revert_takes_no_staged_diff`。工作区与 index 相同时，git 会照做把 staged 的补丁反向打到工作区上，只有范围核对能拦下。

## 82 行与用例的对应

完整对应表在 `docs/tasks/T228-git-hunk-cases/notes.md`。对照的是 SP-5 的 "Hunk edge cases" 表里 git CLI 那一列；libgit2 的两列不适用，因为产品只用 git 命令行。

| SP-5 场景（行数） | Rust 用例 | 对应方式 |
|---|---|---|
| multi_hunk、crlf_raw、crlf_eol_attr、crlf_autocrlf（各 4） | 四个用例，共用 `multi_hunk` | 全部原样 |
| mixed_eol（2）、no_eol（5）、deleted_file（3）、symlink（3）、special_names（12）、conflict（4）、turn_revert（4）、index_lock（2） | 各一个同名用例 | 全部原样；turn_revert 用会话里真实回合的 `{turn: id}` 范围 |
| new_file（4） | `_new_file` | "untracked file shows in git diff?" 改为断言产品的 `unstaged` 里它是一个整文件单位；"stage untracked file via --no-index patch" 改为直接暂存这个单位；其余 2 行原样 |
| empty_file（5） | `_empty_file` | "empty new file patch has hunks?" 改为断言：一个整文件单位，没有行；其余 4 行原样 |
| rename（6） | `_rename` | 见下文第 1 条；另外 3 行改写，2 行原样 |
| binary（5） | `_binary` | 4 行原样，1 行不适用（见下文第 2 条） |
| mode_change（9） | `_mode_change` | 7 行原样；两行 "[keep mode lines]" 改为：同一个 diff 的 mode 和 hunk A 一次暂存，再用这个已经过时的 diff 暂存 hunk C |
| whitespace_cfg（2） | `_whitespace_cfg` | 1 行原样，而且是在配置了 `apply.whitespace=fix` 的仓库上；1 行不适用（见下文第 3 条） |

合计 82 行：79 行原样或按产品的等价行为改写，3 行不适用。

rename 改写的三行：
- "--no-renames staged diff"：重命名在产品里是两个整文件单位（删除旧路径、新增新路径）。
- "unstage hunk C of the same rename patch afterwards"（预期失败）：改为同一个单位、同一个 token 再取消暂存一次，得到 `stale_diff`。
- "unstaged view of a plain mv"：产品的快照里有新路径，所以 `unstaged` 显示为重命名（moved.txt ← old.txt），与 `git diff` 不同。

## 3 行不适用的理由

1. **rename："unstage hunk A of a rename (-M patch)"**：产品生成的补丁一律用 `--no-renames`（ADR-10），不会产生 `-M` 补丁，重命名按两个整文件处理。这一行在 SP-5 里 git 自己也失败，正是改用 `--no-renames` 的原因。
2. **binary："stage binary WITHOUT --binary (expected to fail)"**：产品生成补丁时总带 `--binary`，不存在不带它的路径，所以没有可测的等价步骤。
3. **whitespace_cfg："stage with user's apply.whitespace=fix (raw git apply)"**：产品执行 `apply` 时总带 `--whitespace=nowarn`，从不做裸 apply。这一行要防的风险由同场景的另一行覆盖：仓库配置了 `fix`，暂存后尾部空白仍原样保留。

## 三处变异检查

表驱动改写之后又重跑了一遍，都已还原：

1. hunk 单位前面带上 old/new mode 行（改 `hunks::pick`）：`it_git_hunk_mode_change` 失败，`pick` 的单元用例也失败。
2. 去掉写操作的范围核对：`it_git_hunk_revert_takes_no_staged_diff` 失败。
3. 暂存不带 `--whitespace=nowarn`（改 `RepoOp::Stage`）：`it_git_hunk_whitespace_cfg` 失败，报出 `stage with --whitespace=nowarn ("w.txt")`。

没有查出产品缺陷：19 个用例第一次运行就全部通过。

## 规模的经过

- 开工前估约 400 行（停止线 550，硬停止线 650）。
- 初稿写完约 470 行；cargo fmt 之后变成 900 行，越过了硬停止线。超出不是范围扩大：rustfmt 把参数超过 60 列的调用、元组和 `assert_eq!` 都拆成一行一个参数。当时停下报给了你。
- 你同意改成表驱动，停止线放宽到 700。
  - 先用元组表，rustfmt 照样拆开，降到 928 → 768；
  - 再改用 `steps!` 宏表。代码库里已有 `durations!`、`failpoints!`、`gold_a!` 这类表格宏，rustfmt 不改宏里的自定义语法，所以表能保持一行一个 SP-5 行；
  - 再把断言的两边先绑到短的局部变量，让断言能留在一行。
- 最终 `cargo xtask size` 为 **+698 −0**，在 700 以内。

## 验证方式

- `cargo test -p cupper-host --lib it_git_hunk`：19 个 IT-git-hunk 用例和 3 个 hunk 单元用例都通过。
- `cargo clippy -p cupper-host --all-targets --all-features` 没有警告；`cargo fmt --all -- --check` 干净；`cargo xtask ci testdata-scan` 通过。

## 提交列表

```
8151736 test(git): SP-5's hunk cases on the product's write path
```

## 遗留问题

- 没有新的范围外发现。T229（Q32、LFS、故障点）等明天派发。
- 失败时，`Held` 里的内容按字节数组打印，二进制文件之外读起来不方便。要改成按文本显示，约多 5 行，你定。
