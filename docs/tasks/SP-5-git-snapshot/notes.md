# SP-5 工作笔记

- 执行者：子 agent（后端工程师）
- 日期：2026-10-16
- 脚本：`spikes/SP-5-git-snapshot/`；结果表：本目录 `results.md`；原始数据：本目录 `data/*.json`

## 准备

- 所有测试数据放在 worktree 之外的临时目录（下文记作 `<tmp>`），没有碰本机任何已有仓库。
- 大仓库：`git clone --depth 1 --single-branch --no-tags` torvalds/linux，96,080 个跟踪文件，跟踪内容约 1.5 GB，index 约 10 MB。只对它执行 git 命令。
- 小仓库：脚本生成 1k / 10k / 100k 个小文本文件的合成仓库（看文件数的影响），边界用例另外各自建小仓库。
- 每个 git 调用都用 `GIT_CONFIG_GLOBAL=<空文件>`、`GIT_CONFIG_NOSYSTEM=1`，避免本机全局配置影响结果。需要特定配置的用例用仓库级配置显式设置。
- LFS：从 GitHub release 下载 git-lfs 3.8.0（darwin-arm64），核对了 `sha256sums.asc` 里的哈希，放在 `<tmp>/tools/bin`，只通过 `PATH` 和测试仓库的 `git lfs install --local` 使用，没有改全局配置。
- libgit2 用 `uv run --with pygit2`（pygit2 1.20.1，内置 libgit2 1.9.7）；gitoxide 用 gix 0.89.0 写了一个小探针。编译产物和 uv 缓存都在 `<tmp>` 下。

## 试错与发现过程

1. **快照的耗时跟"新对象数"成正比，不是跟仓库大小。** 中等改动（1,000 个文件）在 1k 仓库上也要 300 ms。先怀疑 fsync：`core.fsync=none` 没有差别，`core.fsyncMethod=batch` 反而慢 1.7 倍。再用 `hash-object --stdin-paths` 对比：只算哈希 35 ms，加 `-w` 写对象 463 ms（1,100 个文件）。结论：成本在写松散对象（每个对象一个文件，zlib 压缩），约 0.4–0.5 ms/个。没有在其他机器上核实；环境因素（比如安全软件扫描新建的文件）可能放大这部分开销。
2. **fsmonitor 对 `add -A` 几乎没用。** 第一次 fsmonitor 测试 status 几乎没变快。查 trace 发现：`--untracked-files=all` 和 `add -A` 都不用 untracked cache，只有 `-unormal` 用。暖机时如果跑的是 `-uall`，untracked cache 根本不会写进 index。改成 `-unormal` 后 linux 上 status 从 228 ms 降到 36 ms。由此加了"按 status 结果限定路径"的快照（`snapshot_guided`），结果 tree 与完整 `add -A` 一致，开 fsmonitor 时快约 40%。
3. **重命名的单 hunk 取消暂存会撤销整个重命名。** `-M` 生成的补丁里每个 hunk 都带 `rename from/to` 头，反向应用其中一个 hunk 会把重命名一起撤掉，第二个 hunk 随后失败。改用 `--no-renames` 生成用于暂存操作的补丁就没有这个问题（重命名变成"删除 + 新增"两个整文件单元）。
4. **libgit2 的 apply 没有偏移搜索。** 它在 hunk 头的 `new_start` 处精确匹配。从多 hunk 补丁里单独拿出后面的 hunk，`new_start` 带着前面未应用 hunk 的偏移，所以失败；文件在别处插了几行也失败。把 hunk 头改成 `new_start = old_start` 后通过率从 67/79 升到 75/79，剩下的是 libgit2 本身的限制（无偏移容忍、符号链接还原失败、空文件补丁解析失败）。另外 libgit2 往工作区应用补丁时会重写 index 文件（只刷新 stat，条目不变），也就是会去拿 index 锁。
5. **自己写的安全检查有个 bug，被用例抓到了。** `git rev-list --exclude=<pattern> --branches` 里的 pattern 不能带 `refs/heads/` 前缀，带了就什么也不排除，"只有这个分支才有的提交"永远是 0。修正后本地仓库用例从"0 → 0"变成"1 → 0"（合并前阻止、合并后放行）。ADR 里要提醒实现者。
6. **结果里漏出过部分本机路径。** 先截断 stderr 再做路径替换，被截断的路径就替换不上。改成先脱敏再截断，并在 `GitError` 里统一处理。
7. **刚建好的 worktree，第一次只读 status 很慢。** 检出的文件和新 index 在同一秒写入，git 把它们当成 racy 条目全部重新哈希（10k 文件：518 ms，刷新一次 index 后 147 ms）。cupper 建好 worktree 后可以等 1 秒跑一次 `git update-index --refresh`（这是 cupper 自己的 worktree，可以写它的 index）。
8. **linux 克隆在 macOS 上一开始就是"脏"的。** 13 对只差大小写的路径在大小写不敏感的 APFS 上只能检出一个，`git status` 永远显示修改。两次快照之间的 diff 不受影响（两次都一样），但"未暂存"视图里会一直有这几个文件。
9. **嵌套仓库探测要用不带 `--directory` 的 `ls-files -o`。** 带 `--directory` 时只列出最外层的未跟踪目录（`vendor/`），看不到里面的 `vendor/lib/.git`；不带时 git 不会进入嵌套仓库，直接把它列成 `vendor/lib/`。
10. **git-lfs 不在 PATH 上时，LFS 仓库的 `add -A` 和 `status` 都会失败**（"the remote end hung up unexpectedly"）。macOS 上从 Finder 启动的 App 默认 PATH 只有 `/usr/bin:/bin:/usr/sbin:/sbin`，Homebrew 装的 git-lfs 不在里面。
11. **完整运行时 linux 的 worktree 基准中断了。** 原因同第 8 条：新建的 linux worktree 本来就"脏"，不加 `--force` 的 `git worktree remove` 拒绝删除，脚本直接报错退出。修改后先尝试普通删除，被拒绝时改用 `--force` 并记录下来，然后用 `--merge` 只重跑 linux。`run_all.sh` 后面的几段改为单独重跑，数据都来自同一台机器、同一天。
12. **libgit2 在 linux 上快照失败，查到了根因。** `add_all()` 报 `illegal byte sequence`。排除过程：路径全是 ASCII；不是浅克隆造成的；不是大小写冲突造成的；`status()` 和单文件 `add()` 都正常。之后连不涉及工作区的 tree 对 tree 生成 patch 也失败，于是怀疑 diff driver。最小复现：只要 `.gitattributes` 里写了 `diff=cpp` 或 `diff=python`（linux 用的是 `*.[ch] diff=cpp`），pygit2 1.20.1 / libgit2 1.9.7 的 macOS wheel 在 `add_all` 和生成 patch 时都会失败；`diff=rust`、`diff=dts` 和不设置都正常。推测是这两个内置 driver 的函数头正则在 macOS 的 `regcomp` 下编译失败（REG_ILLSEQ），没有看 libgit2 源码核实。Rust 的 `git2` crate 自己编译 libgit2 时用的正则后端可能不同，没有测。
13. **影子仓库的 tree 和临时 index 的不一致。** 比对路径后发现，影子仓库少了 1,354 个"被忽略规则匹配、但已被跟踪"的文件（`git ls-files -ci --exclude-standard`）。影子仓库从空 index 开始，不知道这些文件已被跟踪，就按忽略规则把它们跳过了。另外，在大小写不敏感时，`*.s` 会匹配到 `*.S`。
14. **gix 模拟快照在 linux 上的 tree 和 git 不一致**，差异正好是那 13 个大小写冲突的路径：gix 和 git 对同一对冲突文件的判断不同。合成仓库上的结果完全一致。

## 取舍

- 计时取中位数，默认 5 次（worktree 3 次），每次运行前写入新内容，保证测的是"有新 blob"的真实情况。没有清系统文件缓存（需要 sudo），数字是热缓存下的。
- 外发请求只有：从 GitHub 克隆 linux、下载 git-lfs release 和查询 release 元数据（curl 默认 UA）；cargo 访问 crates.io；uv 访问 PyPI（pygit2、pyflakes）。都用工具默认的请求头，没有带任何个人信息。
- gitoxide 没有 `add -A`，快照只能模拟（status → 写 blob → 改 HEAD tree）。模拟版不跑 clean 过滤器、假设没有已暂存的改动，所以它的速度只能说明"库能做到多快"，不能直接当成可替换的实现。
- `git stash create` 只做了对照，没有深入：它不含未跟踪文件、需要用户的 index 锁、冲突状态和空仓库下直接失败，三条都和需求冲突。
- worktree 的放置位置和命名的建议基于测试中观察到的问题（放在仓库内、移动主仓库、分支名冲突），其他工具的做法只作参考，没有逐一核实。
