# SP-5 Git 快照与审查操作实测（脚本）

原型实测代码，只用于验证 ADR-10 的方案，不是产品代码，后续开发不直接依赖它。任务说明见 `docs/tasks/SP-5-git-snapshot/brief.md`，结论和数据见同目录的 `results.md`、`notes.md`。

## 运行环境

- macOS（脚本里的硬件信息采集用了 `sysctl`、`diskutil`、`sw_vers`，其他平台能跑，只是环境信息不全）
- git ≥ 2.48（`worktree add --relative-paths` 那一项需要；其余 ≥ 2.38 即可）
- Python 3.9+，只用标准库
- 可选：`uv`（跑 libgit2 对比，会临时装 pygit2，缓存放在 `$SP5_WORKDIR/uv-cache`）
- 可选：Rust 工具链（跑 gitoxide 对比，`gix-probe/` 编译到 `$SP5_WORKDIR/cargo-target`）
- 可选：git-lfs 二进制放到 `$SP5_WORKDIR/tools/bin/git-lfs`（跑 LFS 用例）。脚本只在测试仓库里用 `git lfs install --local`，不改全局配置。
- 磁盘约 8 GB（linux 浅克隆 2 GB，worktree 测试会再检出几份）

## 怎么跑

```bash
export SP5_WORKDIR=/path/outside/any/repo/cupper-sp5   # 默认 $TMPDIR/cupper-sp5
./run_all.sh                 # 全部，约 60 分钟（另加首次的 linux 浅克隆）
SKIP_LINUX=1 ./run_all.sh    # 只用合成仓库，几分钟
SKIP_LIBGIT2=1 ./run_all.sh  # 不跑 pygit2 对比
SKIP_GIX=1 ./run_all.sh      # 不编译、不跑 gitoxide 对比
```

也可以单独跑某个脚本（在任意不是仓库的目录下）：

```bash
python3 setup_repos.py                     # 准备样本仓库，记录环境信息
python3 edge_hunk.py [--only rename,crlf_raw]
python3 edge_snapshot.py [--only lfs] [--big-mb 1024]
python3 edge_worktree.py
python3 bench_snapshot.py --repos syn-10k,linux --runs 5 [--no-fsmonitor]
python3 bench_worktree.py --repos syn-10k,linux --runs 3
uv run --no-project --with pygit2 python edge_hunk.py --backend libgit2       # 或 libgit2+fix
uv run --no-project --with pygit2 python compare_libgit2.py --repos syn-10k,linux
python3 compare_gix.py --repos syn-10k,linux   # 先编译 gix-probe
python3 render.py                          # 汇总成 $SP5_WORKDIR/results/results.md
```

可以重复运行：样本仓库已存在就复用，边界用例的小仓库每次重建。

## 安全约束

- 所有数据都在 `$SP5_WORKDIR` 下，脚本不碰其他任何仓库。
- 每个 git 调用都设置 `GIT_CONFIG_GLOBAL=<空文件>`、`GIT_CONFIG_NOSYSTEM=1`，用户的全局配置不会影响结果；需要特定配置的用例用仓库级配置或 `-c` 显式设置。
- 对克隆下来的 linux 仓库只执行 git 命令，不执行其中的任何脚本或构建。
- 结果 JSON 落盘前会把本机路径替换成 `<tmp>`、`$HOME`，去掉用户名。

## 文件

| 文件 | 内容 |
|---|---|
| `lib.py` | 公共函数：隔离配置的 git 调用、计时（取中位数）、环境信息、路径脱敏 |
| `snapshot.py` | 被测的快照方法：临时 index（每次从用户 index 播种 / 保留 / 空 index）、私有对象目录（alternates）、按 `status` 结果限定路径的快照、`git stash create` |
| `hunks.py` | 把 `git diff` 输出按文件、按 hunk 拆开，生成单 hunk 补丁（按字节处理；权限位单独拆成一个"伪 hunk"）；给 libgit2 用的 hunk 头修正 |
| `setup_repos.py` | 浅克隆 linux；生成 1k / 10k / 100k 文件的合成仓库 |
| `bench_snapshot.py` | 快照、快照间 diff、单 hunk 暂存/取消暂存/还原的耗时；fsmonitor 对照 |
| `bench_worktree.py` | worktree 创建、安全检查、删除、按分支重建的耗时 |
| `bench_shadow.py` | 对照方案：单独的影子仓库（`GIT_DIR` 指向 cupper 自己的目录）的首次/后续快照耗时和占用 |
| `edge_hunk.py` | hunk 操作的边界用例，按预期内容逐项校验；后端可选 git CLI、libgit2、libgit2+fix（先把 hunk 头改成 new_start = old_start） |
| `edge_snapshot.py` | 快照的边界用例：锁、冲突、忽略规则、嵌套仓库、子模块、LFS、大文件、特殊文件、split index、稀疏检出、gc 等 |
| `edge_worktree.py` | worktree 的安全清理规则（`safety()`）、重建、命名、放在仓库内的问题、移动主仓库 |
| `compare_libgit2.py` | 同样的操作用 libgit2（pygit2）做一遍，比较耗时和结果（tree id 是否一致）、LFS 过滤器、特殊仓库兼容性 |
| `compare_gix.py`、`gix-probe/` | gitoxide（gix 0.89）对比：status、模拟快照（gix 没有 `add -A`）、tree diff、`add_worktree`、特殊仓库兼容性 |
| `render.py` | 把结果 JSON 汇总成 Markdown 表格 |
| `run_all.sh` | 按顺序跑全部脚本 |
