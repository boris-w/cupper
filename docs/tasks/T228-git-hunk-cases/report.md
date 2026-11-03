# T228 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`test/T228-git-hunk-cases`（从 main `0404e8c` 开出）
- 完成日期：2026-11-03
- PR 和 CI 运行链接：还没有。`8151736` 已请你推送，交汇报时还没有 PR 和 CI 结果；CI 失败的话叫我回来修。

## 做了什么

- **新增 IT-git-hunk**：`orchestrator/session/hunk_tests.rs`，19 个 `it_git_hunk_*` 用例，全部走产品路径：`git/diff` → `git/file_diff` → `git/stage`、`git/unstage`、`git/revert`，每个用例用会话夹具加一个真实的 git 仓库。
- **共用设施**：
  - 按编辑表算期望内容：行、换行符，以及 SP-5 的 A/B/C 三处编辑；
  - 读 index 的 blob 和 mode、工作区的内容和 mode（含符号链接的目标）；
  - 按标记在 `git/file_diff` 的单位里挑出要操作的那个（第一个、mode 单位、或改动行含某个标记的单位）。
- **表驱动**：每个场景一张 `steps!` 表，一个 SP-5 行占一行，写法是 `"<SP-5 标签>": <操作> <文件>, <单位> => <之后的内容>;`。每一步当场取对应范围的 diff（取消暂存用 `staged`，其余用 `unstaged`），失败时报出这一行的标签和文件。
- **每一步自动加查两项**：暂存和取消暂存不动工作区；还原不动 index 的条目（SP-5 的 "a worktree revert must never change index entries"）。
- **补上 T121 的交接**：`it_git_hunk_revert_takes_no_staged_diff`。工作区与 index 相同时，git 会把 staged 的补丁反向打到工作区上，只有范围核对拦得下。
- **对应表**：82 行与 SP-5 的逐行对应写在 notes 里，79 行按原样或按产品的等价行为改写，3 行不适用：
  - 用 `-M` 补丁取消暂存一个 hunk：产品的补丁用 `--no-renames`，重命名是两个整文件单位；
  - 不带 `--binary` 暂存二进制：产品总带 `--binary`；
  - 不带 `--whitespace=nowarn` 的 apply：产品从不这样执行，用例改在设了 `apply.whitespace=fix` 的仓库上验证暂存后内容不被改动。
  - 其余与产品行为不同的行按产品的行为断言。例如"未跟踪文件在 git diff 里看不见"：产品的 `unstaged` 本来就包含它，是一个整文件单位；"普通的 `mv`"：`unstaged` 显示为重命名。

没有改产品代码，也没有查出产品缺陷。

## 关键取舍

- **用宏写表**：rustfmt 会把参数超过 60 列的元组和调用拆成一行一个，表因此看不出一行对一行；它不改宏里的自定义语法。代码库里已经有同类的表宏（`durations!`、`failpoints!`、`gold_a!`）。
- **mode_change 的两行 "[keep mode lines]"**：产品的 hunk 单位从不带模式行，所以改为同一个 diff 的 mode 和 hunk A 一次暂存，再用这个已经过时的 diff 暂存 hunk C。

## 验证方式

- IT-git-hunk 的 19 个用例全部通过，本机单份运行约 5 秒。
- **变异检查**（表驱动改写后重跑过）：
  1. hunk 单位带上模式行：`it_git_hunk_mode_change` 和 `pick` 的单元用例失败；
  2. 去掉范围核对：`it_git_hunk_revert_takes_no_staged_diff` 失败；
  3. 暂存不带 `--whitespace=nowarn`：`it_git_hunk_whitespace_cfg` 失败，报出 `stage with --whitespace=nowarn ("w.txt")`。
- `cargo clippy -p cupper-host --all-targets --all-features` 没有警告；`cargo fmt --check` 和 `cargo xtask ci testdata-scan` 通过。
- **规模**：`cargo xtask size` 为 **+698 −0**，在你放宽的 700 以内。没用表驱动之前是 +902，多出的部分全是 rustfmt 的拆行。
- **验收标准**：
  - IT-git-hunk 通过，与 SP-5 的对应写进了 notes；
  - CI 的 8 项：等推送后的结果。

## 提交列表

```
8151736 test(git): SP-5's hunk cases on the product's write path
```

## 遗留问题 / 范围外发现

- 断言失败时，`Held` 里的内容按字节数组打印，文本不好读。要改的话可以在打印时把字节按 UTF-8 显示，约 5 行，这次没有做。
- T229（Q32、LFS、故障点）可以复用这里的 `Case` 设施：它目前是本文件私有的，到时改成 `pub(super)` 即可。
