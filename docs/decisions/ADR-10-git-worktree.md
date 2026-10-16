# ADR-10 Git 与 worktree

- 状态：已采纳（2026-10-16，`gate/design`）
- 日期：2026-10-16
- 来源：SP-5 实测（`docs/tasks/SP-5-git-snapshot/report.md`、`results.md`）

## 背景

PRD 中与 Git 相关的要求有三类：

- 审查面板（GT-1 至 GT-4）以仓库的实际 Git 状态为准。"最近一回合"的 diff，指回合开始和结束时两次工作区快照之间的变化。
- 支持按 hunk 暂存、取消暂存和还原。
- CP-5、GT-5 要求最小可用的 worktree 支持，并且清理时不能丢数据。

所有 Git 操作都由会话宿主执行（ADR-01），宿主用 Rust 编写（ADR-02）。

## 可选方案

1. 调用用户机器上的 git 命令行（子进程）
2. libgit2（Rust 的 git2 crate）
3. gitoxide（gix）

SP-5 在 linux 内核仓库（9.6 万个文件）和多种合成仓库上做了实测：

- **libgit2**：只要仓库用了内置的 `diff=cpp` 这类 diff driver，快照和生成 patch 都会失败，linux 仓库就是这种情况。LFS、split index、稀疏检出也都有问题。81 个 hunk 用例只通过 68 个。
- **gix**：没有 `add -A` 和 `apply`；worktree 管理缺少写接口；对大小写冲突路径的判断与 git 不一致。
- **git 命令行**：功能完整，正确性最好。82 个 hunk 用例通过 81 个，未通过的那个已经找到规避办法。在大仓库上的性能也可以接受。

## 决策

**全部用 git 命令行实现。M1 不引入 libgit2 和 gix。**

**调用约定**
- 每类命令都有一组固定参数，统一用 `-z` 输出。
- 只读命令一律加 `--no-optional-locks`，避免顺手写用户的 index。
- 宿主启动时从用户的登录 shell 读取 PATH，检测 git 版本（暂定最低 2.39），同时检查有没有 git-lfs。

**回合快照**

> 设计评审修订（2026-10-16）：快照改在 cupper 自己的 GIT_DIR 里执行（用 alternates 指向仓库的对象，只拷贝白名单配置和 `info/exclude`），下面的步骤不变，但不再读取仓库本地的配置、hooks 和 `info/attributes`；复制 index 时保留 mtime。见文末。

1. 把 `rev-parse --git-path index` 指向的文件复制一份，作为临时 index。
2. 执行 `git -c core.safecrlf=false -c advice.addEmbeddedRepo=false add -A --ignore-errors`，带上用于排除的 pathspec。
3. 执行 `write-tree`。

快照对象写在 cupper 私有的对象目录里，通过 alternates 读取仓库原有的对象。这样用户的 `.git` 不会被写入任何东西，快照也不受用户执行 gc 的影响。tree id 存进事件日志。

拍快照之前先做预检，以下内容不纳入快照，界面上标注"未纳入"：
- 超过 50 MB 的未跟踪文件；
- 嵌套仓库；
- 超大的未跟踪目录。

回合开始时同步拍快照，结束时异步拍。快照失败不阻塞回合，界面上显示"回合 diff 不可用"。

性能：linux 仓库在没有改动时约 0.33 s；改了 10 个文件约 0.35 s；改了 1,000 个文件约 1.7 s。

**diff**
- 文件列表用 `diff-tree -r -z --raw -M`，统计用 `--numstat`。
- patch 的参数固定为：`--no-color --no-ext-diff --no-textconv --full-index --binary -U3`。
- "和当前工作区比较"的做法是：再拍一次快照，然后用 diff-tree 比较两次快照。
- "未暂存"的内容包括 `git diff` 的结果，加上 status 里的未跟踪文件。

**hunk 操作**
- 暂存：`git apply --whitespace=nowarn --cached`；取消暂存：再加 `-R`；还原工作区：只加 `-R`。
- 权限位变化单独作为一个操作单元。
- 新增、删除、二进制、符号链接、子模块指针、LFS 文件一律按整个文件处理。
- 用于暂存的补丁用 `--no-renames` 生成。
- 补丁应用失败，说明状态已经变了：刷新 diff，不做模糊匹配。
- hunk ID 按内容寻址，配合 ADR-04 的前置条件使用。

**worktree**
- 位置在仓库之外：`~/.cupper/worktrees/<仓库名>-<短哈希>/<会话短名>`，可以在设置里修改。
- 分支命名为 `cupper/<短名>`。先用 `check-ref-format --branch` 校验，重名时自动加后缀。
- 会话运行期间执行 `git worktree lock`。创建后等 1 秒，再执行一次 `update-index --refresh`。
- 不使用 `--relative-paths`。主仓库被移动后，用 `repair` 修复。

**清理规则**

以下任何一条成立，就不删除 worktree：
1. 有已修改或已暂存的文件；
2. 有未跟踪文件（被忽略的文件不算）；
3. 有进行中的合并、cherry-pick、revert、rebase 或 bisect；
4. worktree 被不是 cupper 加的锁锁住；
5. HEAD 上有其他任何分支、远程分支、标签里都没有的提交；
6. 有还没推送的提交（仓库没有远程时跳过这一条）；
7. 有已经初始化的子模块。

执行时：
- 删除用 `git worktree remove`，不加 `--force`；
- 默认保留分支，用户要求删除时用 `branch -d`，不用 `-D`；
- 只因为大小写冲突而显示"脏"的情况，提示用户，由用户确认后再删。

## 后果

- **好处**：
  - 正确性与用户日常使用的 git 完全一致；
  - 不引入 C 库，也不引入大量依赖；
  - 用户的仓库不会被写入任何东西。
- **代价**：
  - 依赖用户机器上的 git，要检测版本；
  - 每次操作都要启动一个子进程，耗时约 10 ms，可以接受；
  - 快照对象的存储需要定保留策略，和 ADR-06 一起定。
- **待验证**：
  - 大小写敏感的卷、外置盘、较老的 git 版本、冷缓存的情况，在实现阶段的 CI 和用户实际使用中观察；
  - 如果以后用 gix 优化 status 轮询，要先做一致性校验。

## 设计评审修订（2026-10-16）

来源：设计评审会（`docs/meetings/2026-10-16-design-review/minutes.md`）、用户决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）、详细设计 v0.7 的 5.13.2。本节是细化，不推翻上文；上文中被替换的写法已就地改正并注明。

- **宿主自动调用 git 分三档**（纪要 D8，详细设计 5.2.10）：
  - **快照档**：回合快照、`unstaged` 和 `staged` 两种 diff 范围、`git/status` 的文件状态、`@` 引用的文件列表，都在 cupper 自己的 GIT_DIR 里执行，配置只放行白名单（D02 实验验证与 SP-5 方案得到相同的 tree）；
  - **仓库档**：在用户仓库上执行的其余自动调用，每次调用前读取配置并用 `-c` 覆盖可执行类键（`core.fsmonitor`、`core.hooksPath` 无条件覆盖），配置来源位于工作区或其他 agent 可写的位置时拒绝执行（仓库自己的 git 目录除外，由覆盖和指纹兜底），调用后比对指纹，覆盖后 git 报错一律不重试；
  - **用户档**：只有用户在界面上点"提交""推送"时才运行仓库的 hooks、签名和凭据助手，与在终端里执行一致，按钮旁注明。
- **仓库可执行配置的指纹**：回合前后计算，变化时在消息流和审查面板醒目提示。
- **复制 index 时保留 mtime**：D02 实验发现，不保留时同一秒内改过、大小又没变的文件会被当成未修改，SP-5 的方案同样有这个问题。
- 自定义过滤器（例如 git-crypt）的路径标 `filter_unsupported`：检测覆盖全部属性来源（快照档复制 `info/attributes`），快照里只存占位和哈希，不提供 hunk 操作，`git/stage` 前再用 `check-attr` 核对一次；LFS 只放行标准取值（Q32 实测）。
- **git 和 git-lfs 的路径**：宿主启动时解析成绝对路径，只从 PATH 的可信条目里查找；git 子进程的 PATH 只留可信条目，工作目录设为 cupper 自己的目录；快照档的 LFS 过滤器写成绝对路径。
- hunk 操作用 `diff_token` 指向宿主缓存的补丁；M1 提供最小的 `staged` 范围，只用于取消暂存（Q13）；按仓库加锁的范围；git 子进程的环境变量约定（`LC_ALL=C`、`GIT_TERMINAL_PROMPT=0`）。
