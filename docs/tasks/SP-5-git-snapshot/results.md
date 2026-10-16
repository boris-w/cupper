# SP-5 实测结果

- 日期：2026-10-16
- 脚本：`spikes/SP-5-git-snapshot/`（`run_all.sh`）。本次完整跑了一遍，其中 `bench_worktree` 的 linux 部分和之后的 libgit2、gix、影子仓库几段，是在修复脚本后分段重跑的（原因见 `notes.md` 第 11、12 条）。
- 数字是热缓存下的中位数，单位 ms；快照、状态类每项 5 次，worktree 每项 3 次，每次运行前都写入新内容。
- 本机路径记作 `<tmp>`。原始数据在 `data/*.json`，"附"部分是 `render.py` 的原样输出。

## 1. 环境

| 项 | 值 |
|---|---|
| 机器 | Apple M5 Pro，18 核，64 GB 内存 |
| 系统 | macOS 26.4（25E246） |
| 磁盘 | SSD（Apple Fabric），APFS，大小写不敏感 |
| git | 2.50.1（Apple Git-155） |
| git-lfs | 3.8.0 |
| libgit2 | pygit2 1.20.1 / libgit2 1.9.7（PyPI wheel） |
| gitoxide | gix 0.89.0（Rust 1.96） |

| 样本仓库 | 跟踪文件数 | 跟踪内容 | index 大小 |
|---|---|---|---|
| syn-1k（小） | 1,001 | 2 MB | 87 KB |
| syn-10k | 10,001 | 22 MB | 0.9 MB |
| syn-100k | 100,001 | 217 MB | 8.4 MB |
| linux（大，`--depth 1`） | 96,080 | 1.5 GB | 9.9 MB |

## 2. 汇总表（git 命令行，推荐方案）

改动量：修改 / 新增未跟踪 / 删除的文件数。syn-1k 的"中""大"档只有 500 个文件可改，syn-10k 的"大"档改 5,000 个。

| 操作 | syn-1k | syn-10k | syn-100k | linux | 正确性 |
|---|---|---|---|---|---|
| 基线：`status --porcelain=v2 -uall` | 13 | 25 | 139 | 233 | — |
| 回合快照，无改动 | 36 | 44 | 184 | 328 | 每次都校验：用户 index 未被改动；保留 index、私有对象目录、按 status 限定路径三种变体的 tree 与之相同 |
| 回合快照，小改动（10/5/1） | 46 | 65 | 228 | 346 | 同上 |
| 回合快照，中改动（1,000/100/10） | 330 | 659 | 1,247 | 1,672 | 同上 |
| 回合快照，大改动（10,000/1,000/100） | 716 | 2,787 | 6,099 | 8,338 | 同上 |
| 对照：空 index 起步（全量哈希） | 62 | 230 | 6,594 | 13,375 | 稀疏检出下结果错误 |
| 快照间文件列表 `diff-tree -r --raw -M`（小 / 大改动） | 12 / 42 | 11 / 37 | 14 / 101 | 16 / 154 | — |
| 快照间统计 `--numstat`（小 / 大改动） | 13 / 162 | 12 / 299 | 15 / 614 | 17 / 1,336 | — |
| 单文件逐 hunk 内容 | 11 | 10 | 10 | 10 | 可以解析出 hunk，并直接用于还原 |
| 快照对比当前工作区（再拍一次加 diff-tree，小改动） | 48 | 62 | 213 | 321 | 包含未跟踪文件 |
| 单 hunk 暂存 / 取消暂存 / 还原 | 10 / 10 / 10 | 13 / 12 / 9 | 33 / 33 / 10 | 36 / 36 / 10 | 边界用例 82 项中 81 项符合预期 |
| `worktree add -b`（含检出） | 124 | 608 | 6,240 | 8,254 | — |
| `worktree add --no-checkout` | 16 | 18 | 16 | 19 | 耗时几乎全在检出 |
| 清理前安全检查（刷新 index 后 / 刚建好时） | 141 / 179 | 162 / 282 | 409 / 799 | 600 / 1,289 | 12 种状态全部判对 |
| `worktree remove` | 80 | 601 | 5,095 | 5,558 | linux 要加 `--force`（大小写冲突，见 4.3） |
| 按分支重建 worktree | 118 | 607 | 6,409 | 7,945 | 已提交的内容完整恢复 |

**开启 fsmonitor 和 untracked cache 时**（linux，无改动 / 小改动）：`status -unormal` 为 40 / 46，`status -uall` 为 188 / 184；完整快照 269 / 323；按 status 限定路径的快照 162 / 220，tree 与完整快照相同。不开 fsmonitor 时这种快照在大改动下反而更慢（linux：16,546 对 8,338），因为几千条 pathspec 的匹配代价很高。

**其他快照方法对照**

| 方法 | 结果 |
|---|---|
| `git stash create` | 不含未跟踪文件；用户持有 `index.lock` 时失败；冲突状态和空仓库下失败；会顺带刷新、重写用户的 index |
| 影子仓库（cupper 自己的 `GIT_DIR`，`GIT_WORK_TREE` 指向用户目录） | linux 首次快照 71.6 s，占用 360 MB；之后无改动时 270 ms。**结果不对**：不知道哪些被忽略规则匹配的文件其实已被跟踪，linux 上漏掉 1,354 个这样的文件（例如 `.renames.txt`，以及 `.gitignore` 里的 `*.s` 在大小写不敏感时匹配到的 `*.S`）；也读不到用户仓库的 `info/exclude` 和过滤器配置 |
| 临时 index 加私有对象目录 | 耗时与写入用户对象库的方式相同（linux 无改动 308 对 328）；用户的 `.git/objects` 不写入任何对象 |

## 3. 实现方式对比（linux，另注明的除外）

| 项 | git 命令行 | libgit2（pygit2 1.20.1 / libgit2 1.9.7） | gitoxide（gix 0.89） |
|---|---|---|---|
| status（无改动） | 233 | 607 | 197 |
| 快照（无改动 / 中改动） | 328 / 1,672 | **失败**：`illegal byte sequence`（见下文 `diff=cpp`）；syn-100k 上为 399 / 1,681（CLI 184 / 1,247） | 只能模拟：196 / 760，不跑 clean 过滤器；13 个大小写冲突路径的结果与 git 不同 |
| tree diff（小改动） | 16（文件列表） | 带 hunk 的 patch **失败**（`diff=cpp`） | 16（只有路径） |
| 单 hunk 暂存 | 36 | 184 | 没有 apply |
| hunk 边界用例 | 81/82 | 68/81；改写 hunk 头后 76/81 | — |
| `worktree add` | 8,254 | 22,150 | 10,786 |
| LFS 仓库快照 | 存指针 ✓ | 存原始内容 ✗（不跑外部过滤器） | 模拟版未测 |
| split index | ✓ | 打不开：`unsupported mandatory extension: 'link'` | ✓（status） |
| 稀疏检出 | ✓ | 把 skip-worktree 文件当成删除 ✗ | ✓（status） |
| `.gitattributes` 里写了 `diff=cpp` 或 `diff=python` | ✓ | add_all 和生成 patch 都报 `illegal byte sequence` ✗ | ✓ |
| worktree 管理 | add / remove / lock / unlock / repair / prune 全有，remove 自带脏检查 | add / prune；没有脏检查 | add、remove（带脏、锁、子模块检查）；只读锁状态，没有 lock / repair / prune 的写接口 |
| 分发 | 不打包，依赖用户机器上的 git；要检测版本，并从登录 shell 取 PATH | 需要链接 C 库（Rust 用 `git2` crate 内置编译） | 纯 Rust；探针约 175 个 crate，干净编译 16 s（130 s CPU），二进制 6.2 MB |

## 4. 读表要点

### 4.1 快照

- 无改动时，快照只比 `status -uall` 多约 30–100 ms：10 万文件约 0.2–0.33 s。
- 有改动时，耗时与"新对象数"成正比，每个松散对象约 0.4–0.6 ms；和 fsync 无关，`core.fsyncMethod=batch` 反而更慢。linux 上 1,000 个文件约 1.7 s，1 万个文件约 8 s。
- 必须从用户 index（或会话自己保留的 index）播种：空 index 起步，10 万文件要 6.6–13.4 s，而且会把稀疏检出的文件当成删除。
- "保留 index"和"每次播种"的耗时差不到 60 ms。每次播种更简单，还能跟上用户对 sparse、assume-unchanged 等标记的改动。

### 4.2 hunk 操作

- 单 hunk 操作的耗时主要是重写 index（10 万文件约 35 ms）；还原只写工作区，约 10 ms。
- 唯一不符合预期的用例：`-M` 补丁里取消暂存重命名文件的一个 hunk，会把整个重命名撤掉。用 `--no-renames` 生成用于暂存操作的补丁即可。
- libgit2 的三个固有问题：没有偏移搜索；还原符号链接失败；往工作区应用时也要拿 index 锁（用户正在跑 git 时还原失败）。

### 4.3 worktree

- 创建耗时几乎全在检出（`--no-checkout` 不到 20 ms）。10 万文件约 6–8 s，要在 UI 上显示进度。
- 刚建好时文件和 index 在同一秒写入，第一次只读 status 会把所有文件重新哈希。刷新一次 index 后安全检查降到一半左右。
- 大小写不敏感的 APFS 上，linux 的 13 对大小写冲突文件让新 worktree 一开始就是"脏"的，`git worktree remove` 必须加 `--force`。安全规则需要单独识别这种情况。

## 附：render.py 原始输出

### Environment

| item | value |
|---|---|
| git | git version 2.50.1 (Apple Git-155) |
| os | macOS 26.4 (25E246) |
| cpu | Apple M5 Pro |
| cores | 18 |
| mem_gb | 64 |
| python | 3.9.6 |
| disk | SSD, Apple Fabric, APFS |
| git_lfs | git-lfs/3.8.0 (GitHub; darwin arm64; go 1.27.0; git aece9221) |

### Sample repositories

| repo | tracked files | tracked MB | index bytes | HEAD |
|---|---|---|---|---|
| syn-1k | 1001 | 2 | 88697 | d1c2a71fc537 |
| syn-10k | 10001 | 22 | 886114 | d11f2a68712d |
| syn-100k | 100001 | 217 | 8859801 | 980f885ad108 |
| linux | 96080 | 1565 | 10358513 | 2f2f7eeef8b0 |

### Snapshot cost (median of 5 runs, ms)

changes = modified / new untracked / deleted files; every run writes new content.

| repo | scenario | changes | status -uall | snap seed | snap keep | snap private ODB | snap guided | snap empty | stash create | same tree (keep/private/guided) | user index untouched | stash touched index | new loose objects / size |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| syn-1k | clean | 0/0/0 | 13 | 36 | 24 | 45 | 34 | 62 | 12 | yes | yes | no | 0 / 0 KB |
| syn-1k | small | 10/5/1 | 13 | 46 | 36 | 60 | 59 |  | 32 | yes | yes | yes | 26 / 104 KB |
| syn-1k | medium | 500/100/10 | 14 | 330 | 344 | 351 | 344 |  | 275 | yes | yes | yes | 623 / 2492 KB |
| syn-1k | large | 500/1000/50 | 15 | 716 | 699 | 670 | 726 |  | 268 | yes | yes | yes | 1523 / 6116 KB |
| syn-10k | clean | 0/0/0 | 25 | 44 | 35 | 55 | 46 | 230 | 25 | yes | yes | no | 0 / 0 KB |
| syn-10k | small | 10/5/1 | 25 | 65 | 59 | 75 | 78 |  | 49 | yes | yes | yes | 32 / 128 KB |
| syn-10k | medium | 1000/100/10 | 26 | 659 | 642 | 634 | 743 |  | 599 | yes | yes | yes | 1307 / 5228 KB |
| syn-10k | large | 5000/1000/100 | 40 | 2787 | 2771 | 2653 | 3506 |  | 2467 | yes | yes | yes | 6207 / 24848 KB |
| syn-100k | clean | 0/0/0 | 139 | 184 | 171 | 188 | 184 | 6594 | 139 | yes | yes | no | 0 / 0 KB |
| syn-100k | small | 10/5/1 | 146 | 228 | 226 | 234 | 254 |  | 209 | yes | yes | yes | 37 / 148 KB |
| syn-100k | medium | 1000/100/10 | 148 | 1247 | 1082 | 1037 | 1706 |  | 1081 | yes | yes | yes | 1952 / 7808 KB |
| syn-100k | large | 10000/1000/100 | 180 | 6099 | 6350 | 6092 | 12917 |  | 5970 | yes | yes | yes | 13046 / 52208 KB |
| linux | clean | 0/0/0 | 233 | 328 | 268 | 308 | 345 | 13375 | 269 | yes | yes | yes | 0 / 0 KB |
| linux | small | 10/5/1 | 230 | 346 | 330 | 339 | 367 |  | 256 | yes | yes | yes | 53 / 304 KB |
| linux | medium | 1000/100/10 | 233 | 1672 | 1488 | 1419 | 2202 |  | 1411 | yes | yes | yes | 2162 / 12800 KB |
| linux | large | 10000/1000/100 | 284 | 8338 | 8287 | 8227 | 16546 |  | 8817 | yes | yes | yes | 14624 / 84820 KB |

### Diff cost (ms)

T0 = clean snapshot, T1 = scenario snapshot.

| repo | scenario | diff-tree --raw -M | --numstat | full patch | patch size | one file patch | snapshot + diff vs now | diff T0 (tracked only) | unstaged numstat |
|---|---|---|---|---|---|---|---|---|---|
| syn-1k | small | 12 | 13 | 13 | 13 KB | 11 | 48 | 14 | 12 |
| syn-1k | medium | 13 | 65 | 48 | 378 KB | 10 | 84 | 69 | 63 |
| syn-1k | large | 42 | 162 | 106 | 1472 KB | 12 | 189 | 94 | 73 |
| syn-10k | small | 11 | 12 | 12 | 13 KB | 10 | 62 | 21 | 18 |
| syn-10k | medium | 20 | 80 | 63 | 621 KB | 9.9 | 139 | 119 | 64 |
| syn-10k | large | 37 | 299 | 237 | 3779 KB | 10 | 354 | 203 | 242 |
| syn-100k | small | 14 | 15 | 14 | 13 KB | 10 | 213 | 80 | 74 |
| syn-100k | medium | 43 | 104 | 86 | 621 KB | 10 | 327 | 241 | 124 |
| syn-100k | large | 101 | 614 | 486 | 6205 KB | 11 | 984 | 455 | 570 |
| linux | small | 16 | 17 | 17 | 17 KB | 10 | 321 | 155 | 78 |
| linux | medium | 52 | 213 | 364 | 729 KB | 12 | 495 | 338 | 275 |
| linux | large | 154 | 1336 | 1962 | 6652 KB | 16 | 1393 | 543 | 1429 |

### Single-hunk operations (ms, small scenario)

| repo | git diff -- file | stage (apply --cached) | unstage (apply --cached -R) | revert (apply -R) | content correct |
|---|---|---|---|---|---|
| syn-1k | 9.8 | 10 | 10 | 9.8 | yes |
| syn-10k | 11 | 13 | 12 | 9.2 | yes |
| syn-100k | 16 | 33 | 33 | 10 | yes |
| linux | 15 | 36 | 36 | 9.5 | yes |

### With core.fsmonitor=true + core.untrackedCache=true (ms)

| repo | scenario | status -uall | status -unormal | snap seed | snap keep | snap guided | guided = seed tree |
|---|---|---|---|---|---|---|---|
| syn-100k | clean | 95 | 30 | 155 | 143 | 94 | yes |
| syn-100k | small | 97 | 33 | 213 | 190 | 171 | yes |
| linux | clean | 188 | 40 | 269 | 250 | 162 | yes |
| linux | small | 184 | 46 | 323 | 287 | 220 | yes |

### Worktree operations (median of 3 runs, ms)

| repo | add -b (checkout) | add --no-checkout | safety check (1st, racy) | safety check | remove | remove needed --force | rebuild from branch |
|---|---|---|---|---|---|---|---|
| syn-1k | 124 | 16 | 179 | 141 | 80 | no | 118 |
| syn-10k | 608 | 18 | 282 | 162 | 601 | no | 607 |
| syn-100k | 6240 | 16 | 799 | 409 | 5095 | no | 6409 |
| linux | 8254 | 19 | 1289 | 600 | 5558 | yes: uncommitted changes | 7945 |

### Alternative: separate shadow repository (ms)

| repo | first snapshot | shadow store MB | next (clean) | next (small) | same tree as temp-index snapshot | paths only in temp-index snapshot |
|---|---|---|---|---|---|---|
| syn-10k | 7044 | 7 | 35 | 57 | yes |  |
| linux | 71602 | 360 | 270 | 342 | NO | 20: Documentation/.renames.txt, arch/alpha/boot/head.S, arch/alpha/kernel/entry.S |

### libgit2 (pygit2 1.20.1 / libgit2 1.9.7) vs git CLI (ms)

| repo | scenario | lg2 status | cli status | lg2 snapshot | cli snapshot | same tree id | lg2 diff+hunks | cli diff -p | lg2 hunk stage | cli hunk stage |
|---|---|---|---|---|---|---|---|---|---|---|
| syn-1k | clean | 3.8 | 13 | 5.6 | 36 | yes | 0.1 | 12 |  |  |
| syn-1k | small | 4.6 | 13 | 33 | 46 | yes | 2.7 | 30 | 2.3 | 17 |
| syn-1k | medium | 6.2 | 14 | 466 | 330 | yes | 89 | 143 |  |  |
| syn-10k | clean | 40 | 25 | 39 | 44 | yes | 1.3 | 15 |  |  |
| syn-10k | small | 40 | 25 | 73 | 65 | yes | 3.0 | 34 | 14 | 18 |
| syn-10k | medium | 40 | 26 | 952 | 659 | yes | 157 | 283 |  |  |
| syn-100k | clean | 389 | 139 | 399 | 184 | yes | 12 | 26 |  |  |
| syn-100k | small | 395 | 146 | 418 | 228 | yes | 15 | 68 | 139 | 56 |
| syn-100k | medium | 399 | 148 | 1681 | 1247 | yes | 147 | 481 |  |  |
| linux | clean | 607 | 233 | ERROR (illegal byte sequence) | 328 | – | 42 | 33 |  |  |
| linux | small | 616 | 230 | ERROR (illegal byte sequence) | 346 | – | ERROR | 89 | 184 | 70 |
| linux | medium | 579 | 233 | ERROR (illegal byte sequence) | 1672 | – | ERROR | 701 |  |  |

| repo | lg2 add_worktree | cli worktree add -b |
|---|---|---|
| syn-1k | 151 | 124 |
| syn-10k | 1220 | 608 |
| syn-100k | 12568 | 6240 |
| linux | 22150 | 8254 |

LFS filter check: `{'lg2_blob_is_pointer': False, 'lg2_blob_size': 4000, 'cli_blob_is_pointer': True, 'cli_blob_size': 129}`

| libgit2 with built-in diff driver | status | add_all | patch |
|---|---|---|---|
| .gitattributes diff=cpp | ok | ERROR: illegal byte sequence | ERROR: illegal byte sequence |
| .gitattributes diff=python | ok | ERROR: illegal byte sequence | ERROR: illegal byte sequence |
| .gitattributes diff=rust | ok | ok | ok |
| .gitattributes diff=none | ok | ok | ok |

| libgit2 on special repo | result | detail |
|---|---|---|
| split index (core.splitIndex) | ERROR | GitError: unsupported mandatory extension: 'link' |
| sparse checkout (skip-worktree) | DIFFERENT tree | skip/0.txt \| 1 -  skip/1.txt \| 1 -  skip/2.txt \| 1 -  3 files changed, 3 deletions(-) |
| relative worktrees extension | ok |  |
| linked worktree created by git | same tree as CLI |  |

### gitoxide (gix v0.89.0) vs git CLI (ms)

gix snapshot is an emulation (status -> write blobs -> edit HEAD tree): no clean filters, assumes nothing staged.

| repo | scenario | gix status | cli status | gix snapshot (emulated) | cli snapshot | same tree id | gix tree diff | cli diff-tree --raw |
|---|---|---|---|---|---|---|---|---|
| syn-1k | clean | 6.2 | 13 | 5.7 | 36 | yes | 3.8 | 10 |
| syn-1k | small | 5.7 | 13 | 16 | 46 | yes | 5.2 | 12 |
| syn-1k | medium | 6.5 | 14 | 210 | 330 | yes | 5.1 | 14 |
| syn-10k | clean | 15 | 25 | 15 | 44 | yes | 5.2 | 11 |
| syn-10k | small | 16 | 25 | 27 | 65 | yes | 5.7 | 12 |
| syn-10k | medium | 16 | 26 | 411 | 659 | yes | 11 | 18 |
| syn-100k | clean | 106 | 139 | 106 | 184 | yes | 11 | 12 |
| syn-100k | small | 121 | 146 | 123 | 228 | yes | 12 | 13 |
| syn-100k | medium | 107 | 148 | 675 | 1247 | yes | 36 | 42 |
| linux | clean | 197 | 233 | 196 | 328 | NO | 14 | 14 |
| linux | small | 196 | 230 | 213 | 346 | NO | 16 | 17 |
| linux | medium | 213 | 233 | 760 | 1672 | NO | 45 | 52 |

| repo | gix add_worktree | cli worktree add -b | worktree status per CLI afterwards |
|---|---|---|---|
| syn-1k | 108 | 124 | clean |
| syn-10k | 1197 | 608 | clean |
| syn-100k | 10895 | 6240 | clean |
| linux | 10786 | 8254 |  M include/uapi/linux/netfilter/xt_connmark.h  M include/uap |

| gix status on special repo | result | detail |
|---|---|---|
| split index | ok | 3 |
| sparse checkout | ok | 1 |
| relative worktrees ext | ok | 0 |

### Hunk edge cases (✅ = result matches the expected content)

libgit2+fix = libgit2 with each single-hunk header rewritten to new_start = old_start.

| case | operation | git CLI | libgit2 | libgit2+fix | detail |
|---|---|---|---|---|---|
| multi_hunk | stage hunk B (middle) | ✅ | ✅ | ✅ |  |
| multi_hunk | stage hunk C (after B staged) | ✅ | ✅ | ✅ |  |
| multi_hunk | unstage hunk C | ✅ | ❌ | ✅ | libgit2: FAILED: hunk at line 25 did not apply; index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'line 00\nline 01\nline 0 |
| multi_hunk | revert hunk A (worktree) | ✅ | ❌ | ✅ | libgit2: note: index file rewritten (stat refresh, entries unchanged); index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'l |
| crlf_raw | stage hunk B (middle) | ✅ | ✅ | ✅ |  |
| crlf_raw | stage hunk C (after B staged) | ✅ | ✅ | ✅ |  |
| crlf_raw | unstage hunk C | ✅ | ❌ | ✅ | libgit2: FAILED: hunk at line 25 did not apply; index[f.txt]: want b'line 00\r\nline 01\r\nline 02\r\nline 03\r\nline 04\r\nline 05\r\nline 0'… got b'line 00\r\nline 01\ |
| crlf_raw | revert hunk A (worktree) | ✅ | ❌ | ✅ | libgit2: note: index file rewritten (stat refresh, entries unchanged); index[f.txt]: want b'line 00\r\nline 01\r\nline 02\r\nline 03\r\nline 04\r\nline 05\r\nline 0'… go |
| crlf_eol_attr | stage hunk B (middle) | ✅ | ✅ | ✅ |  |
| crlf_eol_attr | stage hunk C (after B staged) | ✅ | ✅ | ✅ |  |
| crlf_eol_attr | unstage hunk C | ✅ | ❌ | ✅ | libgit2: FAILED: hunk at line 25 did not apply; index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'line 00\nline 01\nline 0 |
| crlf_eol_attr | revert hunk A (worktree) | ✅ | ❌ | ✅ | libgit2: note: index file rewritten (stat refresh, entries unchanged); index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'l |
| crlf_autocrlf | stage hunk B (middle) | ✅ | ✅ | ✅ |  |
| crlf_autocrlf | stage hunk C (after B staged) | ✅ | ✅ | ✅ |  |
| crlf_autocrlf | unstage hunk C | ✅ | ❌ | ✅ | libgit2: FAILED: hunk at line 25 did not apply; index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'line 00\nline 01\nline 0 |
| crlf_autocrlf | revert hunk A (worktree) | ✅ | ❌ | ✅ | libgit2: note: index file rewritten (stat refresh, entries unchanged); index[f.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got b'l |
| mixed_eol | stage hunk B | ✅ | ✅ | ✅ |  |
| mixed_eol | revert hunk A | ✅ | ✅ | ✅ |  |
| no_eol | stage last-line hunk (no EOL) | ✅ | ✅ | ✅ |  |
| no_eol | unstage it | ✅ | ✅ | ✅ |  |
| no_eol | revert last-line hunk | ✅ | ✅ | ✅ |  |
| no_eol | stage 'add trailing newline' | ✅ | ✅ | ✅ |  |
| no_eol | revert 'remove trailing newline' | ✅ | ✅ | ✅ |  |
| new_file | unstage staged new file | ✅ | ✅ | ✅ |  |
| new_file | untracked file shows in `git diff`? | ✅ | ✅ | ✅ | git CLI: no (expected): untracked files need `git diff --no-index /dev/null <path>` or status |
| new_file | stage untracked file via --no-index patch | ✅ | ✅ | ✅ |  |
| new_file | revert untracked file (apply -R deletes it) | ✅ | ✅ | ✅ |  |
| empty_file | empty new file patch has hunks? | ✅ | ✅ | ✅ | git CLI: hunks=0 |
| empty_file | stage empty untracked file | ✅ | ❌ | ❌ | libgit2: FAILED: unexpected header line 4; index[e.txt]: want b'' got None / libgit2+fix: FAILED: unexpected header line 4; index[e.txt]: want b'' got None |
| empty_file | stage truncation to empty | ✅ | ✅ | ✅ |  |
| empty_file | unstage truncation | ✅ | ✅ | ✅ |  |
| empty_file | revert truncation | ✅ | ✅ | ✅ |  |
| deleted_file | stage deletion | ✅ | ✅ | ✅ |  |
| deleted_file | unstage deletion | ✅ | ✅ | ✅ |  |
| deleted_file | revert deletion (recreate file + exec bit) | ✅ | ✅ | ✅ |  |
| rename | staged diff shows rename | ✅ | ✅ | ✅ | git CLI: rename=True hunks=2 |
| rename | unstage hunk A of a rename (-M patch); want rename+C kept | ❌ | ❌ | ❌ | git CLI: index[new.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got None; index[old.txt]: want None got b'line 00\nline 01\nline 02 / libgit2: index[new.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got None; index[old.txt]: want None got b'line 00\nline 01\nline 02 / libgit2+fix: index[new.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline'… got None; index[old.txt]: want None got b'line 00\nline 01\nline 02 |
| rename | unstage hunk C of the same rename patch afterwards | ✅ | ✅ | ✅ | git CLI: expected failure: error: new.txt: does not exist in index |
| rename | --no-renames staged diff | ✅ | ✅ | ✅ | git CLI: new / deleted |
| rename | unstage the add side only (--no-renames) | ✅ | ✅ | ✅ |  |
| rename | unstaged view of a plain `mv` | ✅ | ✅ | ✅ | git CLI: git diff shows: ['deleted']; new path is untracked (only visible via status/snapshot) |
| binary | binary patch present | ✅ | ✅ | ✅ | git CLI: GIT binary patch |
| binary | stage binary (--binary patch) | ✅ | ✅ | ✅ |  |
| binary | unstage binary | ✅ | ✅ | ✅ |  |
| binary | revert binary | ✅ | ✅ | ✅ |  |
| binary | stage binary WITHOUT --binary (expected to fail) | ✅ | ✅ | ✅ | git CLI: expected failure: error: img.bin: patch does not apply |
| mode_change | units offered for s.sh | ✅ | ✅ | ✅ | git CLI: hunk0,hunk1,mode |
| mode_change | stage hunk A (mode lines stripped) | ✅ | ✅ | ✅ |  |
| mode_change | stage mode pseudo-hunk | ✅ | ✅ | ✅ |  |
| mode_change | stage hunk C | ✅ | ✅ | ✅ |  |
| mode_change | stage mode-only change | ✅ | ✅ | ✅ |  |
| mode_change | unstage mode-only change | ✅ | ✅ | ✅ |  |
| mode_change | revert mode-only change (chmod -x) | ✅ | ✅ | ✅ |  |
| mode_change | [keep mode lines] stage hunk A | ✅ | ✅ | ✅ |  |
| mode_change | [keep mode lines] stage hunk C from the same (stale) diff | ✅ | ✅ | ✅ |  |
| symlink | stage symlink retarget | ✅ | ✅ | ✅ |  |
| symlink | unstage symlink retarget | ✅ | ✅ | ✅ |  |
| symlink | revert symlink retarget | ✅ | ❌ | ❌ | libgit2: FAILED: could not find '<tmp>/scratch/hunk-libgit2-symlink/ln' to open: No such file or directory; worktree[ln]: want b'target-a' got b'target-b' / libgit2+fix: FAILED: could not find '<tmp>/scratch/hunk-libgit2+fix-symlink/ln' to open: No such file or directory; worktree[ln]: want b'target-a' got b'target-b' |
| special_names | stage 'dir with space/file name.txt' | ✅ | ✅ | ✅ |  |
| special_names | unstage 'dir with space/file name.txt' | ✅ | ✅ | ✅ |  |
| special_names | revert 'dir with space/file name.txt' | ✅ | ✅ | ✅ |  |
| special_names | stage 'unicode/文件-名.txt' | ✅ | ✅ | ✅ |  |
| special_names | unstage 'unicode/文件-名.txt' | ✅ | ✅ | ✅ |  |
| special_names | revert 'unicode/文件-名.txt' | ✅ | ✅ | ✅ |  |
| special_names | stage 'quote/a"b.txt' | ✅ | ✅ | ✅ |  |
| special_names | unstage 'quote/a"b.txt' | ✅ | ✅ | ✅ |  |
| special_names | revert 'quote/a"b.txt' | ✅ | ✅ | ✅ |  |
| special_names | stage 'tab/a\tb.txt' | ✅ | ✅ | ✅ |  |
| special_names | unstage 'tab/a\tb.txt' | ✅ | ✅ | ✅ |  |
| special_names | revert 'tab/a\tb.txt' | ✅ | ✅ | ✅ |  |
| conflict | stage: index changed in the same lines -> refuse | ✅ | ✅ | ✅ | git CLI: expected failure: error: x.txt: patch does not apply |
| conflict | revert: worktree changed in the same lines -> refuse | ✅ | ✅ | ✅ | git CLI: expected failure: error: x.txt: patch does not apply |
| conflict | revert: unrelated edits + 3-line offset -> applies | ✅ | ❌ | ❌ | libgit2: FAILED: hunk at line 14 did not apply; worktree[x.txt]: want b'new top 1\nnew top 2\nnew top 3\nline 00\nline 01\nline 02 EDIT-A'… got b'new top 1\nnew top 2\nn / libgit2+fix: FAILED: hunk at line 14 did not apply; worktree[x.txt]: want b'new top 1\nnew top 2\nnew top 3\nline 00\nline 01\nline 02 EDIT-A'… got b'new top 1\nnew top 2\nn |
| conflict | revert 2-file patch, 2nd conflicts -> atomic, x untouched | ✅ | ✅ | ✅ | git CLI: expected failure: error: y.txt: patch does not apply |
| whitespace_cfg | stage with user's apply.whitespace=fix (raw git apply) | ✅ | – | – | git CLI: index[w.txt]: want b'line 00\nline 01\nline 02 trailing   \nline 03\nline 04\nline 05\n'… got b'line 00\nline 01\nline 02 trailing\nline 03\nline 04\nline 05\nl |
| whitespace_cfg | stage with --whitespace=nowarn | ✅ | ✅ | ✅ |  |
| turn_revert | turn diff includes untracked file created in the turn | ✅ | ✅ | ✅ |  |
| turn_revert | revert turn hunk A after unrelated user edit | ✅ | ✅ | ✅ |  |
| turn_revert | revert turn-created untracked file | ✅ | ✅ | ✅ |  |
| turn_revert | revert turn hunk B after user rewrote it -> refuse | ✅ | ✅ | ✅ | git CLI: expected failure: error: t.txt: patch does not apply |
| index_lock | stage while index.lock is held -> refuse, nothing changed | ✅ | ✅ | ✅ | git CLI: expected failure: remove the file manually to continue. |
| index_lock | revert (worktree only) while index.lock is held | ✅ | ❌ | ❌ | libgit2: FAILED: the index is locked; this might be due to a concurrent or crashed process; worktree[k.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\ / libgit2+fix: FAILED: the index is locked; this might be due to a concurrent or crashed process; worktree[k.txt]: want b'line 00\nline 01\nline 02\nline 03\nline 04\nline 05\ |

### Snapshot boundary cases

| case | check | result | detail |
|---|---|---|---|
| index.lock held by user | temp-index snapshot succeeds | yes |  |
| index.lock held by user | user index + lock untouched | yes |  |
| index.lock held by user | git stash create | NO | error: could not write index |
| merge conflict in index | index has stage 1/2/3 entries | yes | 3 unmerged entries |
| merge conflict in index | snapshot succeeds (conflict markers captured as content) | yes |  |
| merge conflict in index | user index untouched (still unmerged) | yes |  |
| merge conflict in index | git stash create | NO | Cannot save the current index state |
| untracked + ignore rules | untracked files included | yes | keep.log, new.txt, sub/deep/new2.txt |
| untracked + ignore rules | .gitignore / info/exclude / core.excludesFile / negation honored | yes |  |
| untracked + ignore rules | exec bit recorded | yes | 100755 |
| untracked + ignore rules | git stash create includes untracked files | NO | stash='' |
| nested repo (untracked dir with its own .git) | inner repo WITHOUT commits: plain add -A | NO | fatal: adding files failed |
| nested repo (untracked dir with its own .git) | ... with --ignore-errors | NO | error: 'vendor/lib/' does not have a commit checked out |
| nested repo (untracked dir with its own .git) | ... with nested repo dirs excluded by pathspec | yes | excluded ['vendor/lib'] |
| nested repo (untracked dir with its own .git) | inner repo WITH a commit -> recorded as gitlink (160000) | yes | ('160000', 'commit', '7a2e50f4f594bde802df46a2c21c0e5c5a056d7e') |
| nested repo (untracked dir with its own .git) | uncommitted change inside nested repo visible in snapshot diff | NO | no: the gitlink only records the inner HEAD |
| submodule | dirty file inside submodule changes the snapshot | NO | no: only the gitlink (submodule HEAD) is recorded |
| submodule | status --porcelain=v2 reports the submodule as modified | yes | 1 .M S.M. 160000 160000 160000 ef8259bffeac535a3ffd92a9afd6e |
| submodule | new commit inside submodule shows as gitlink change | yes | +Subproject commit c235b485d89243a678ec9986e76099766669b36c |
| submodule | --submodule=log summarises inner commits | yes | Submodule sub ef8259b..c235b48: |
| submodule | stage gitlink change via git apply --cached | yes |  |
| Git LFS | snapshot stores the LFS pointer, not the content | yes | oid sha256:5b5e5427dee79d796115ad0d72bf7b24691967125dbb8f3ee3d89e69d5db4479 |
| Git LFS | side effect: clean filter copies content into .git/lfs/objects | yes | 1 -> 2 files |
| Git LFS | turn diff shows pointer change only | yes |  |
| Git LFS | snapshot when git-lfs is NOT on PATH (GUI app PATH) | NO | fatal: the remote end hung up unexpectedly |
| Git LFS | git status when git-lfs is NOT on PATH | NO | fatal: the remote end hung up unexpectedly |
| large untracked files | snapshot with one 100MB random untracked file | yes | 1746 ms, object store +113 MB (loose) |
| large untracked files | snapshot with one 1024MB random untracked file | yes | 16880 ms, object store +1024 MB (pack: above core.bigFileThreshold) |
| large untracked files | snapshot with one 1024MB zeros untracked file | yes | 3082 ms, object store +1 MB (pack: above core.bigFileThreshold) |
| large untracked files | same 1024MB file with size pre-filter (>50MB excluded) | yes | 39 ms incl. ls-files -o + stat |
| many untracked files (e.g. a non-ignored node_modules) | first snapshot with 20000 untracked files | yes | 7889 ms |
| many untracked files (e.g. a non-ignored node_modules) | cheap pre-check: ls-files -o --directory | yes | 9 ms, 1 top-level untracked entries |
| many untracked files (e.g. a non-ignored node_modules) | full pre-check: ls-files -o (every untracked file) | yes | 23 ms, 20000 files |
| many untracked files (e.g. a non-ignored node_modules) | next snapshot with kept temp index (no new content) | yes | 1210 ms |
| FIFO / socket / unreadable file | FIFO + unix socket in worktree: snapshot succeeds | yes | recorded: nothing |
| FIFO / socket / unreadable file | unreadable untracked file: plain add -A | NO | fatal: adding files failed |
| FIFO / socket / unreadable file | ... with --ignore-errors: exit code / tree written | yes | add exit=1; tree=yes; stderr: error: open("secret.txt"): Permission denied |
| snapshot inside a linked worktree | snapshot works, uses <common-dir>/worktrees/<name>/index as seed | yes | worktrees/snap-wt-linked/index |
| snapshot inside a linked worktree | neither the worktree's nor the main index is touched | yes |  |
| core.splitIndex=true | seeded snapshot works with a split index | yes | sharedindex files=1; 51 paths |
| sparse-checkout (skip-worktree entries) | skip/ removed from worktree | yes |  |
| sparse-checkout (skip-worktree entries) | seeded snapshot keeps skip-worktree files (not shown as deleted) | yes |  |
| sparse-checkout (skip-worktree entries) | EMPTY-seeded snapshot treats them as deleted | yes | => always seed from the user's index (or a kept temp index) |
| assume-unchanged / skip-worktree set by the user | seeded snapshot sees the edit | NO | no: flag is copied with the index; add -A skips the file |
| core.safecrlf=true + mixed EOL file | plain `git add -A` with user's safecrlf=true | NO | fatal: CRLF would be replaced by LF in m.txt |
| core.safecrlf=true + mixed EOL file | snapshot with -c core.safecrlf=false | yes |  |
| empty repo (no commits yet) | snapshot works without HEAD / index | yes |  |
| empty repo (no commits yet) | git stash create | NO | You do not have the initial commit yet |
| object lifetime: gc, anchor ref, private object dir | unanchored snapshot survives `git gc --prune=now` | NO | no: unreachable objects are pruned (default grace period 2 weeks) |
| object lifetime: gc, anchor ref, private object dir | snapshot anchored by refs/cupper/... survives gc | yes |  |
| object lifetime: gc, anchor ref, private object dir | ... but `git log --all` now shows cupper snapshot commits | NO | 1 snapshot commit(s) visible in log --all |
| object lifetime: gc, anchor ref, private object dir | private ODB: nothing written to the repo's .git/objects | yes |  |
| object lifetime: gc, anchor ref, private object dir | private ODB: diff works with env, fails without | yes | with env=True, without=False |
| object lifetime: gc, anchor ref, private object dir | private ODB: user's gc cannot prune snapshot objects | yes |  |
| object lifetime: gc, anchor ref, private object dir | private ODB: snapshot still diffable after user drops a commit + gc --prune=now | NO | no: blobs that only lived in the dropped commit are gone -> show 'snapshot unavailable' |
| case-only path collision on case-insensitive APFS | fresh checkout is clean | NO | status: 'M README.md' |
| case-only path collision on case-insensitive APFS | two consecutive snapshots are identical (turn diff not polluted) | yes |  |

### Worktree safety / cleanup cases

| case | check | result | detail |
|---|---|---|---|
| clean | safety gate | safe | ok |
| clean | plain `git worktree remove` | REMOVED |  |
| modified | safety gate | BLOCK: uncommitted changes | ok |
| modified | plain `git worktree remove` | refused: fatal: '<tmp>/scratch/wt-root/modified' contains modified or untracked files, use --force to delete it |  |
| staged | safety gate | BLOCK: uncommitted changes | ok |
| staged | plain `git worktree remove` | refused: fatal: '<tmp>/scratch/wt-root/staged' contains modified or untracked files, use --force to delete it |  |
| untracked | safety gate | BLOCK: untracked files | ok |
| untracked | plain `git worktree remove` | refused: fatal: '<tmp>/scratch/wt-root/untracked' contains modified or untracked files, use --force to delete it |  |
| ignored-only | safety gate | safe | ok |
| ignored-only | plain `git worktree remove` | REMOVED |  |
| ignored-only | ignored files reported (warn, not block) | 1 ignored |  |
| committed-unpushed | safety gate | BLOCK: commits not on any other ref, unpushed commits | ok |
| committed-unpushed | plain `git worktree remove` | REMOVED |  |
| committed-pushed | safety gate | safe | ok |
| committed-pushed | plain `git worktree remove` | REMOVED |  |
| detached-commits | safety gate | BLOCK: commits not on any other ref, unpushed commits | ok |
| detached-commits | plain `git worktree remove` | REMOVED |  |
| detached-commits | branch switched / detached detected | DETACHED |  |
| merge-in-progress | safety gate | BLOCK: uncommitted changes, operation in progress, commits not on any other ref, unpushed commits | ok |
| merge-in-progress | plain `git worktree remove` | refused: fatal: '<tmp>/scratch/wt-root/merge-in-progress' contains modified or untracked files, use --force to delete i |  |
| locked | safety gate | BLOCK: locked | ok |
| locked | plain `git worktree remove` | refused: fatal: cannot remove a locked working tree, lock reason: cupper session running |  |
| with-submodule | safety gate | BLOCK: initialized submodules | ok |
| with-submodule | plain `git worktree remove` | refused: fatal: working trees containing submodules cannot be moved or removed |  |
| local-only repo | unique commits before / after merging into main | 1 -> 0 (unpushed=None) | safe after merge |
| branch-del | `git branch -d` on unmerged cupper branch | refused | error: the branch 'cupper/branch-del' is not fully merged |
| branch-del | worktree removed but branch (and its commits) kept | yes |  |
| rebuild | `worktree add <path> <existing branch>` restores committed work | ok |  |
| rm -rf | `worktree list` marks it prunable | yes |  |
| rm -rf | re-add branch at a new path before prune | refused | fatal: 'cupper/branch-del' is already used by worktree at '<tmp>/scratch/wt-root/branch-del' |
| rm -rf | re-add at the SAME path before prune | refused | fatal: '<tmp>/scratch/wt-root/branch-del' is a missing but already registered worktree; |
| rm -rf | after `git worktree prune` | ok |  |
| branch in use | add a 2nd worktree on the same branch | refused | fatal: 'cupper/branch-del' is already used by worktree at '<tmp>/scratch/wt-root/branch-del' |
| naming | existing `cupper/*` branches block a branch named `cupper` | refused | fatal: cannot lock ref 'refs/heads/cupper': 'refs/heads/cupper/branch-del' exists; cannot create 'refs/heads/cupper' |
| naming | existing branch `cupper` blocks `cupper/x` (D/F ref conflict) | refused | fatal: cannot lock ref 'refs/heads/cupper/x': 'refs/heads/cupper' exists; cannot create 'refs/heads/cupper/x' |
| naming | `-b` with an existing branch name | refused | fatal: a branch named 'cupper/branch-del' already exists |
| naming | check-ref-format cupper/'fix login' | invalid |  |
| naming | check-ref-format cupper/'修复-登录' | valid |  |
| naming | check-ref-format cupper/'a..b' | invalid |  |
| naming | check-ref-format cupper/'x.lock' | invalid |  |
| naming | check-ref-format cupper/'ok-name_1' | valid |  |
| inside repo | main repo status shows the nested worktree | yes | ?? .cupper/worktrees/in/ |
| inside repo | main repo snapshot records it as gitlink (160000) | yes |  |
| move main repo | worktree usable after the main repo moved | broken | fatal: not a git repository: <tmp>/scratch/wt-main/.git/worktrees/move-test |
| move main repo | `git worktree repair <wt>` from the moved repo fixes it | ok | repair: .git file broken: <tmp>/scratch/wt-root/untracked |
| relative paths | `worktree add --relative-paths` supported | yes |  |
| relative paths | side effect on the user's repo config | extensions.relativeWorktrees=true, repositoryformatversion=1 | older git / libgit2 may refuse a repo with unknown extensions |

