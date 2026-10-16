# SP-5 汇报

- 执行者：子 agent（后端工程师）
- 分支：`spike/SP-5-git-snapshot`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-aa405e27ebea45d0b`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

## 做了什么

结论先说：**ADR-10 建议全部用 git 命令行实现，不用 libgit2，gitoxide 暂不引入。** 快照用临时 index 加 cupper 私有的对象目录，worktree 放在仓库之外，清理前按 7 条规则检查，具体见下文。

本次在 worktree 之外的临时目录（下文记作 `<tmp>`）里做了实测：
- **大仓库**：linux 浅克隆，96,080 个文件，内容约 1.5 GB。
- **小仓库**：脚本生成的 1k、10k、100k 文件合成仓库，用来看文件数的影响。
- 每个边界用例各建一个小仓库。
- 所有 git 调用都隔离了本机的全局配置。克隆的仓库只执行 git 命令。
- 没有碰本机任何已有的仓库。

实测覆盖：
- **快照**：快照方法和改动量对耗时的影响；快照之间的 diff；按 hunk 暂存、取消暂存、还原的正确性。
- **worktree**：创建、安全检查、删除、重建。
- **边界**：子模块、LFS、大文件、嵌套仓库等。
- **实现方式**：git 命令行、libgit2（pygit2 1.20.1 / libgit2 1.9.7）、gitoxide（gix 0.89）三者对比。

提交的物料：
- 脚本：`spikes/SP-5-git-snapshot/`，带 README，`run_all.sh` 可以一次跑完全部。
- 结果表：`docs/tasks/SP-5-git-snapshot/results.md`。
- 工作笔记：`notes.md`，记了 14 条试错和发现过程。
- 原始数据：`data/*.json`，已脱敏。

## 关键取舍

**快照方法**

| 方法 | 结论 | 原因 |
|---|---|---|
| 临时 index（`GIT_INDEX_FILE`）+ `add -A` + `write-tree` | 采用 | 每次从用户 index 复制一份作为起点 |
| 空 index 起步 | 否决 | 10 万文件要 6.6–13.4 s；会把稀疏检出的文件当成删除 |
| `git stash create` | 否决 | 不含未跟踪文件；用户持有 index 锁、冲突中、空仓库时都会失败；还会顺带重写用户的 index |
| 影子仓库（单独的 `GIT_DIR`） | 否决 | 首次 72 s，占用 360 MB；在 linux 上漏掉 1,354 个"被忽略规则匹配、但已被跟踪"的文件 |

快照对象写到 cupper 私有的对象目录，通过 alternates 读取仓库原有的对象。实测速度与直接写入用户对象库相同。好处是用户的 `.git` 一个对象都不写，也不受用户 `gc` 影响，并且不需要建 ref。用 ref 锚定的话，快照提交会出现在 `git log --all` 里。

**实现方式**：以实测为准。libgit2 和 gix 在部分只读操作上更快，但正确性和功能覆盖都有硬伤，见下文第 3 节。

**gix 的取舍**：gix 没有 `add -A`，快照只能自己拼。这部分模拟只用来衡量速度，不作为可替换的实现。

**hunk 补丁的处理规则**
- 按字节处理补丁内容。
- 权限位变化单独作为一个操作单元，否则同一文件的第二个 hunk 会对不上。
- 暂存类操作用 `--no-renames` 生成补丁。

## 验证方式

| 验收标准 | 结果 |
|---|---|
| 脚本放在 `spikes/SP-5-git-snapshot/`，可以重复运行，有 README | 满足。样本仓库会复用，边界小仓库每次重建；pyflakes 和 `bash -n` 都通过 |
| 结果表：大小仓库耗时取中位数，并给出正确性结论 | 满足。快照和状态类每项 5 次，worktree 每项 3 次，每次运行前写入新内容 |
| 边界情况清单，以及推荐的处理或降级方式 | 满足，见下文 |
| 实现方式对比和 ADR-10 建议 | 满足，见下文 |
| 提交内容里没有个人信息 | 满足。对全部提交文件和提交信息搜索了用户名、本机路径、邮箱、会话链接，均为零命中；路径都替换成了 `<tmp>` |

需要说明的一点：完整运行中，linux 的 worktree 基准因大小写冲突报错退出。修复后，从该处起的各段单独重跑。修复后的 `run_all.sh` 没有再从头完整跑一遍，但各段都单独跑通过。

本任务的外发请求只有四类：GitHub 克隆 linux、下载并校验 git-lfs 3.8.0、cargo 访问 crates.io、uv 访问 PyPI。都使用工具默认的请求头，不含任何个人信息，符合 main 上新增的守则第 7 条。

## 结果表

以下是 git 命令行的中位数，单位 ms，热缓存。测试机器：M5 Pro，macOS 26.4，APFS（大小写不敏感），git 2.50.1。

| 操作 | syn-1k | syn-10k | syn-100k | linux | 正确性 |
|---|---|---|---|---|---|
| 基线 `status --porcelain=v2 -uall` | 13 | 25 | 139 | 233 | — |
| 快照：无改动 | 36 | 44 | 184 | 328 | 用户 index 始终未被改动；几种快照变体的 tree 相同 |
| 快照：改 10 个 / 新增 5 个 / 删 1 个 | 46 | 65 | 228 | 346 | 同上 |
| 快照：改 1,000 / 新增 100 / 删 10 | 330 | 659 | 1,247 | 1,672 | 同上 |
| 快照：改 10,000 / 新增 1,000 / 删 100 | 716 | 2,787 | 6,099 | 8,338 | 同上 |
| 快照间文件列表 `diff-tree --raw -M`（小改动 / 大改动） | 12 / 42 | 11 / 37 | 14 / 101 | 16 / 154 | — |
| 单文件逐 hunk 内容 | 11 | 10 | 10 | 10 | 可以解析出 hunk，并能直接还原 |
| 快照对比当前工作区（再拍一次加 diff） | 48 | 62 | 213 | 321 | 包含未跟踪文件 |
| 单 hunk 暂存 / 取消暂存 / 还原 | 10/10/10 | 13/12/9 | 33/33/10 | 36/36/10 | 82 项用例中 81 项符合预期 |
| `worktree add -b`（含检出；不检出时 16–19） | 124 | 608 | 6,240 | 8,254 | — |
| 清理前安全检查（刷新 index 后 / 刚建好时） | 141/179 | 162/282 | 409/799 | 600/1,289 | 12 种状态全部判对 |
| `worktree remove` / 按分支重建 | 80/118 | 601/607 | 5,095/6,409 | 5,558/7,945 | linux 的删除需要加 `--force`（大小写冲突） |

从表中可以看出三点：
- **快照耗时**：无改动时只比 status 多约 0.1 s。有改动时与新增对象数成正比，每个约 0.4–0.6 ms；与 fsync 无关，`fsyncMethod=batch` 反而更慢。
- **fsmonitor**：开启 fsmonitor 和 untracked cache 后，linux 上 `status -unormal` 只要 40 ms。按 status 结果限定路径的快照为 162 ms，完整快照为 269 ms，两者 tree 相同。但不开 fsmonitor 时，这种做法在大改动下慢一倍。
- **worktree**：创建和删除的耗时几乎全在检出和删文件上。

**实现方式对比**（linux 数据）

| 项 | git 命令行 | libgit2 | gix |
|---|---|---|---|
| status | 233 | 607 | 197 |
| 快照 | 328 | 失败：`illegal byte sequence` | 模拟实现 196；不跑过滤器；13 个大小写冲突路径的结果与 git 不同 |
| hunk 用例 | 81/82 | 68/81；改写 hunk 头后 76/81 | 没有 apply |
| `worktree add` | 8.3 s | 22.2 s | 10.8 s |
| LFS、split index、稀疏检出 | 全部正确 | 三项都有问题（见第 3 节） | split index 和稀疏检出的 status 正常；LFS 未测（模拟实现不跑过滤器） |
| worktree 管理 | 全部有 | 没有删除前的脏检查 | 有 add 和 remove，没有 lock、repair、prune 的写接口 |
| 分发 | 依赖用户的 git | 需要链接 C 库 | 约 175 个 crate，编译 16 s，二进制 6.2 MB |

libgit2 的 `illegal byte sequence` 根因已查到：只要仓库的 `.gitattributes` 用了内置的 `diff=cpp` 或 `diff=python`，`add_all` 和生成 patch 都会失败。linux 用的是 `*.[ch] diff=cpp`。

## 边界情况清单

**快照**

| 情况 | 实测结果 | 推荐处理或降级 |
|---|---|---|
| 用户正在跑 git（`index.lock` 存在） | 快照正常，用户的 index 和锁都没被动 | 无需处理 |
| index 处于合并冲突中 | 快照成功，冲突标记作为文件内容记录，用户 index 保持未合并 | 无需处理 |
| 未跟踪文件和各类忽略规则 | 未跟踪文件被包含；`.gitignore`、`info/exclude`、`core.excludesFile`、否定规则都生效 | — |
| 嵌套仓库，里面有提交 | 记成子模块指针，内部改动不可见 | 降级：UI 标注"嵌套仓库" |
| 嵌套仓库，里面没有提交 | 整个 `add -A` 失败，加 `--ignore-errors` 也不行 | 预检 `ls-files -o`（不加 `--directory`），把以 `/` 结尾的条目排除 |
| 子模块 | 只记录子模块的 HEAD，内部未提交的改动不进快照；status 能报出子模块"脏"；`--submodule=log` 能汇总内部提交；指针变化可以用 `apply --cached` 暂存 | 降级：只显示指针变化和"脏"标记 |
| Git LFS | 快照存的是指针，diff 只显示指针变化；副作用是内容会写进 `.git/lfs/objects` | 按整文件处理 |
| git-lfs 不在 PATH 上 | 快照和 status 都失败 | 宿主从用户的登录 shell 读取 PATH |
| 大文件 | 100 MB 用 1.7 s；1 GB 用 17 s，并写入 1 GB 对象；预检后只要 39 ms | 排除超过 50 MB 的未跟踪文件，标注"未纳入" |
| 2 万个未被忽略的文件（例如 node_modules） | 首次快照 7.9 s，之后 1.2 s；预检 23 ms | 超过阈值时按目录排除 |
| 不可读文件 | `add -A` 整体失败 | 加 `--ignore-errors`，失败的路径记为"未纳入" |
| FIFO、socket | 被跳过，快照正常 | — |
| split index | 播种快照正常 | — |
| 稀疏检出 | 播种快照会保留未检出的文件 | 必须从用户 index 播种 |
| 用户对文件设了 assume-unchanged | 快照看不到这个文件的修改 | 已知限制，罕见 |
| 用户设置了 `core.safecrlf=true` | 原样执行 `add -A` 会失败 | 快照固定加 `-c core.safecrlf=false` |
| 空仓库 | 快照正常 | — |
| 在 linked worktree 里拍快照 | 正常 | 用 `rev-parse --git-path index` 取 index 路径，不要写死 `.git/index` |
| 未锚定的快照对象 | 用户执行 `gc --prune=now` 后会被删 | 改用私有对象目录 |
| 用户改写历史后立即 gc | 旧快照可能无法再 diff | 显示"快照不可用" |
| APFS 上只差大小写的路径 | 克隆后就是"脏"的，但两次快照一致，回合 diff 不受影响 | 见下面清理规则 |

**hunk 操作**（git 命令行）

以下情况全部正确：
- 多 hunk、三种 CRLF 配置、混合换行、文件末尾无换行
- 新文件（包括未跟踪文件和空文件）、删除（会恢复可执行位）、二进制（需加 `--binary`）
- 权限位、符号链接、文件名含空格、中文、引号、Tab

| 情况 | 实测结果 | 推荐处理 |
|---|---|---|
| 重命名文件，用 `-M` 补丁取消暂存其中一个 hunk | 整个重命名被撤销 | 暂存类操作用 `--no-renames`，重命名作为整文件处理 |
| 补丁与当前状态冲突 | 原子拒绝，多文件补丁也是全有或全无 | 刷新 diff 后提示用户 |
| 其他位置有改动或行号偏移 | 仍能正确应用 | — |
| 用户配置了 `apply.whitespace=fix` | 暂存的内容被改掉 | 固定加 `--whitespace=nowarn` |
| 暂存时 `index.lock` 被占用 | 拒绝，无副作用 | 退避重试；还原工作区不需要这把锁 |
| 撤销回合 diff 里的某个 hunk | 其他位置有改动时可以还原；同一处被用户改过时拒绝 | — |
| 未跟踪文件 | `git diff` 里看不到 | 用 `diff --no-index /dev/null <path>` 生成补丁，或直接 `git add -- <path>` |

**worktree**

| 情况 | 实测结果 | 推荐处理 |
|---|---|---|
| 有未推送的提交，或 detached HEAD 上有提交 | `git worktree remove` 会照删；detached HEAD 上的提交因此丢失 | cupper 必须自己检查 |
| 修改、未跟踪文件、合并中、被锁、含子模块 | git 自己会拒绝删除 | — |
| 有人手动删了 worktree 目录 | 同一分支或同一路径都无法重建 | 先 `prune` |
| 分支名冲突（`cupper` 与 `cupper/x` 互相阻塞，或同名分支已存在） | 创建失败 | 先用 `check-ref-format --branch` 校验，冲突时自动加后缀 |
| 分支名格式 | 空格、`..`、`.lock` 非法；中文合法 | 同上 |
| worktree 放在仓库里，又没被忽略 | 主仓库 status 里出现它，快照把它记成子模块指针 | 放到仓库之外 |
| 主仓库被移动 | worktree 失效 | `git worktree repair` 可修复 |
| 使用 `--relative-paths` | 仓库格式被升级，并写入 `extensions.relativeWorktrees` | 不要使用 |
| 刚建好就做只读 status | 慢一倍（同一秒写入，触发 racy git） | 建好后等 1 秒刷新一次 index |
| linux 新建的 worktree | 一开始就"脏"（大小写冲突） | 删除需要 `--force` |

## 对 ADR-10 的建议

**1. 实现方式：git 命令行**
- 宿主（ADR-02 提议用 Rust）通过子进程调用用户机器上的 git，所有读写都走这一条路。
- 每类命令固定一组参数，用 `-z` 输出；只读命令一律加 `--no-optional-locks`，否则 `git status` 和 `git diff` 会顺手写用户的 index。
- 启动时检测 git 版本。最低版本建议先定为 2.39，但本次只在 2.50.1 上实测过。
- 从用户的登录 shell 取 PATH，确保能找到 git 和 git-lfs。
- libgit2 不用：`diff=cpp` 仓库失败、LFS、split index、稀疏检出、符号链接、没有偏移容忍，问题太多。
- gix 留作以后只读热路径（例如 status 轮询）的优化选项，M1 不引入，避免出现两套结果来源。

**2. 回合快照**
- 方法：
  - 每次把 `git rev-parse --git-path index` 指向的文件复制成临时 index；
  - 执行 `git -c core.safecrlf=false -c advice.addEmbeddedRepo=false add -A --ignore-errors`，带上排除用的 pathspec；
  - 执行 `git write-tree`。
- 对象写入 `GIT_OBJECT_DIRECTORY=<cupper 数据目录>/snapshots/<仓库 id>/objects`，并设置 `GIT_ALTERNATE_OBJECT_DIRECTORIES=<仓库 objects 及其 alternates>`。tree id 存进事件存储；读取快照的命令也要带这两个变量。
- 预检：用一次 `ls-files -o` 排除超过 50 MB 的文件、嵌套仓库和超大未跟踪目录，并记录下来，UI 标注"未纳入"。
- 回合开始时同步拍，结束后异步拍。失败不阻塞回合，回合 diff 显示"不可用"。
- 不用的方案：stash create、影子仓库、空 index 起步、ref 锚定。
- P1 可选：仓库已开启 fsmonitor 时，按 `status -unormal` 的结果限定路径。cupper 不替用户开启 fsmonitor。

**3. diff**
- 文件列表用 `diff-tree -r -z --raw -M`，统计用 `--numstat`。
- 逐文件 patch 使用固定参数：`--no-color --no-ext-diff --no-textconv --src-prefix=a/ --dst-prefix=b/ --full-index --binary -U3`。
- "快照对比当前工作区"要再拍一次快照再做 diff-tree，不能用 `git diff <tree>`，因为它不含未跟踪文件。
- "未暂存"范围 = `git diff` 加上 status 里的未跟踪文件。

**4. hunk 操作**
- 暂存用 `git apply --whitespace=nowarn --cached`，取消暂存加 `-R`，还原工作区只用 `-R`。
- 权限位单独作为一个操作单元。
- 新增、删除、二进制、符号链接、子模块指针、LFS 文件都按整文件处理。
- 暂存类操作用 `--no-renames` 生成补丁。
- 补丁应用失败，说明状态已经变了：刷新 diff，不做 3-way 合并或模糊匹配。

**5. worktree 位置和命名**
- 放在仓库之外，建议 `~/.cupper/worktrees/<仓库名>-<短哈希>/<会话短名>`：路径不含空格，并且可以配置。
- 分支 `cupper/<短名>`：先校验名字，冲突时加后缀。
- 创建时显示进度：10 万文件需要 6–8 s。
- 会话运行期间 `git worktree lock --reason "cupper session <id>"`。
- 建好后等 1 秒执行一次 `update-index --refresh`。
- 不用 `--relative-paths`。主仓库被移动后用 `repair` 修复。

**6. 清理的安全规则**

以下任一条成立就不删除（检查全部只读）：
1. 有已修改或已暂存的文件；
2. 有未跟踪文件，被忽略的文件不算；
3. 有进行中的操作：MERGE_HEAD、CHERRY_PICK_HEAD、REVERT_HEAD、rebase-merge、rebase-apply、BISECT_LOG；
4. worktree 被锁，且锁不是 cupper 自己加的；
5. HEAD 上有其他地方都没有的提交，判断命令是 `rev-list --count HEAD --not --exclude=<分支名> --branches --remotes --tags`。注意 `--exclude` 的值不能带 `refs/heads/`，否则什么都不排除，我自己的检查脚本就犯过这个错；
6. 有未推送的提交（`rev-list HEAD --not --remotes`）。仓库没有远程时跳过这一条，只看第 5 条；
7. 有已初始化的子模块。

只提示、不阻止的情况：
- 被忽略的文件数量，因为删除时会把它们一起删掉。
- 只由大小写冲突造成的"脏"，由用户确认后用 `--force` 删除。

执行与重建：
- 删除用 `git worktree remove`，不加 `--force`，作为第二道防线。默认保留分支；用户要求删除分支时用 `branch -d`，不用 `-D`。
- 重建用 `worktree add <路径> <分支>`；必要时先 `prune`；分支已不存在时只读展示。

## 提交列表

```
f7f9919 docs(task): add SP-5 results and raw data
1f062e8 docs(task): add SP-5 working notes
37dbf14 docs(spike): correct SP-5 full-run time estimate
3e4e7e7 test(spike): probe libgit2 built-in diff drivers
f383592 fix(spike): record libgit2 add_all errors instead of aborting
b4f825d fix(spike): handle a fresh worktree that is not clean
bcbff3d test(spike): cover hunk operations while index.lock is held
11ce011 test(spike): add shadow-repository snapshot baseline
cb81694 test(spike): time first worktree safety check separately
2dd9405 test(spike): add SP-5 runner, result renderer and README
f2662e9 test(spike): add SP-5 gitoxide probe
f4a576f test(spike): add SP-5 libgit2 comparison
eb7900b test(spike): run SP-5 hunk cases through libgit2 with header fix
91791c7 test(spike): add SP-5 worktree safety checks and timing
ce445cd test(spike): add SP-5 snapshot boundary cases
f8351a6 test(spike): add status-guided snapshot and fsmonitor comparison
0aa0d54 fix(spike): sanitize git stderr before truncating it
032d1b6 test(spike): add SP-5 hunk patch builder and edge-case checks
57b6ce0 test(spike): add SP-5 snapshot methods and timing benchmark
06c2d58 test(spike): add SP-5 shared helpers and sample repo setup
<编排者存档提交>
```

## 遗留问题 / 范围外发现

1. **测试覆盖面**：只在一台机器上测过（APFS 大小写不敏感，git 2.50.1）。没测 Intel Mac、大小写敏感的卷、外置盘或网络盘、较老的 git，也没测冷缓存。松散对象的写入成本可能受环境影响。
2. **libgit2 的根因**：只追到"内置 cpp/python diff driver 的正则在 macOS 上编译失败"这一层，没有核对源码；Rust `git2` crate 自己编译的版本也没测。这不影响"不用 libgit2"的建议。
3. **快照对象的保留策略**（每个会话保留多少、总容量上限）需要和 ADR-06 一起定。
4. **大小写冲突造成的"假脏"**：在 GT-5 的界面上怎么呈现、是否允许确认后 `--force`，需要产品决定。
5. **gix 的一致性**：gix 对大小写冲突路径的判断与 git 不同。以后如果用 gix 做 status 轮询，需要做一致性校验。
6. **测试数据**：还留在会话 scratchpad 目录的 `sp5/` 下，约 2.7 GB（样本仓库、git-lfs 二进制、结果），可以直接删除。fsmonitor 守护进程已全部停止。
