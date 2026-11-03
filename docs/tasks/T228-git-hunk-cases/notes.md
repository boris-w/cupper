# T228 notes

## 开工前的规模估计（2026-11-03）

停止线 550，硬停止线 650。全是测试，写在新文件 `orchestrator/session/hunk_tests.rs` 里，挂在 IT-git-hunk 下，每个 SP-5 场景一个 `it_git_hunk_*` 用例：

| 部分 | 内容 | 行 |
|---|---|---|
| 共用设施 | 每个场景一个会话夹具加真实 git 仓库；按编辑表算期望内容（行、换行符、替换、插入、删除）；读 index 的 blob 和 mode、工作区的内容和 mode；经 `git/diff` → `git/file_diff` 按标记挑单位，再经 `git/stage`、`git/unstage`、`git/revert` 执行；还原后检查 index 条目没变 | 90 |
| 18 个场景 | multi_hunk 与三种 CRLF（共用一个函数）、mixed_eol、no_eol、new_file、empty_file、deleted_file、rename、binary、mode_change、symlink、special_names、conflict、whitespace_cfg、turn_revert、index_lock | 300 |
| T121 的交接 | revert 拿 staged token：工作区与 index 相同时 git 会照做，只靠范围核对拦下 | 10 |
| 合计 | | 约 400 |

## 规模（实际）

格式化前写出的约 470 行，经 rustfmt 后变成 900 行：rustfmt 把参数超过 60 列的调用和元组都拆成一行一个。改成表驱动之后是 +698 −0，在编排者放宽的 700 以内：每个场景一张 `steps!` 表，一行一个 SP-5 行。rustfmt 不改宏里的自定义语法，所以表能保持一行一个。

## 与 SP-5 的 82 行的对应

SP-5 的表见 `docs/tasks/SP-5-git-snapshot/results.md` 的 "Hunk edge cases"（git CLI 一列；libgit2 两列不适用：产品只用 git 命令行）。一个 SP-5 场景对应一个 `it_git_hunk_<场景>`，步骤的标签照抄 SP-5，失败时报出标签和文件。

| SP-5 场景（行数） | Rust 用例 | 说明 |
|---|---|---|
| multi_hunk、crlf_raw、crlf_eol_attr、crlf_autocrlf（各 4） | `it_git_hunk_multi_hunk`、`_crlf_raw`、`_crlf_eol_attr`、`_crlf_autocrlf`（共用 `multi_hunk`） | 4 行全对应：stage B、stage C、unstage C、revert A |
| mixed_eol（2） | `_mixed_eol` | 全对应 |
| no_eol（5） | `_no_eol` | 全对应 |
| new_file（4） | `_new_file` | "unstage staged new file"、"revert untracked file" 对应。"untracked file shows in `git diff`?"：产品的 `unstaged` 本来就包含未跟踪文件，改为断言它在 diff 里是一个整文件单位。"stage untracked file via --no-index patch"：产品直接暂存这个整文件单位，不需要 `--no-index` |
| empty_file（5） | `_empty_file` | "empty new file patch has hunks?" 改为断言：一个整文件单位，没有行。其余 4 行对应 |
| deleted_file（3） | `_deleted_file` | 全对应 |
| rename（6） | `_rename` | "staged diff shows rename"：`git/diff` 的 `old_path`。"unstage hunk A of a rename (-M patch)"（SP-5 里 git 也失败）：不适用，产品的补丁用 `--no-renames`。"--no-renames staged diff"：重命名的单位是两个整文件，代替了上一行。"unstage hunk C of the same rename patch afterwards"（预期失败）：改为同一个单位用同一个 token 再取消暂存，得到 `stale_diff`。"unstage the add side only" 对应。"unstaged view of a plain `mv`"：产品的快照包含新路径，`unstaged` 显示为重命名（`moved.txt` ← `old.txt`），与 `git diff` 不同 |
| binary（5） | `_binary` | "binary patch present"：单位的表头是 `GIT binary patch`。暂存、取消暂存、还原 3 行对应。"stage binary WITHOUT --binary (expected to fail)"：不适用，产品总带 `--binary`，没有等价的步骤 |
| mode_change（9） | `_mode_change` | 前 7 行对应（"units offered" 断言单位是 mode、hunk、hunk）。"[keep mode lines]" 两行：产品的 hunk 单位从不带模式行，改为同一个 diff 的 mode 和 hunk A 一次暂存，再用这个已经过时的 diff 暂存 hunk C |
| symlink（3） | `_symlink` | 全对应 |
| special_names（12） | `_special_names` | 4 个名字各 3 步，全对应 |
| conflict（4） | `_conflict` | 全对应，拒绝的都是 `conflict stale_diff` |
| whitespace_cfg（2） | `_whitespace_cfg` | "stage with --whitespace=nowarn" 对应，并且是在用户配置了 `apply.whitespace=fix` 的仓库上。"stage with user's apply.whitespace=fix (raw git apply)"：不适用，产品从不在不带 `--whitespace=nowarn` 的情况下执行 apply |
| turn_revert（4） | `_turn_revert` | 回合是会话里真实的回合，用 `{turn: id}` 范围，4 行对应 |
| index_lock（2） | `_index_lock` | 暂存被拒，返回 `git_error lock_held`；只动工作区的还原照常执行 |

合计：82 行里，79 行按原样或按产品的等价行为改写；3 行不适用，没有对应的步骤："-M 补丁取消暂存 hunk"、"不带 --binary 暂存二进制"、"不带 nowarn 的 raw apply"。另外加了一条：

- `it_git_hunk_revert_takes_no_staged_diff`（T121 的交接）：工作区与 index 相同时，git 会把 staged 的补丁反向打到工作区上，只有范围核对拦得下。

每一步还自动检查：暂存和取消暂存不动工作区；还原不动 index 的条目（SP-5 的 "a worktree revert must never change index entries"）。

## 变异检查（表驱动改写后重跑）

1. hunk 单位带上模式行（`pick` 在 hunk 前保留 old/new mode）：`it_git_hunk_mode_change` 和 `pick` 的单元用例失败。
2. 去掉范围核对：`it_git_hunk_revert_takes_no_staged_diff` 失败。
3. 暂存不带 `--whitespace=nowarn`：`it_git_hunk_whitespace_cfg` 失败，报出 "stage with --whitespace=nowarn ("w.txt")"。

没有查出产品缺陷。
