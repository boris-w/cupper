# D02 工作笔记

- 执行者：子 agent（架构师）
- 分支：`docs/D02-design-revision`
- 日期：2026-10-16

## 1. 纪要 D8：独立 GIT_DIR 快照方案的评估

### 1.1 要回答的问题

纪要 D8 要求评估：快照改用 cupper 自己的 GIT_DIR，用 alternates 指向仓库的对象，只拷贝白名单里的无害配置和 `info/exclude`。需要判断三件事：

1. 与 SP-5 的快照方案（从用户 index 播种的临时 index、私有对象目录）能否兼容，产出的 tree 是否相同；
2. 对 `.gitattributes`、LFS、各种 index 形态的影响；
3. 性能是否可以接受。

### 1.2 实验做法

- 环境：本机 git 2.50.1（Apple Git-155），APFS 大小写不敏感，Python 3.9 写的一次性脚本，只调用 git。
- 隔离：全部仓库和状态都建在每个用户自己的临时目录下新建的目录里，结束后整个删除；`HOME` 指向实验目录里的空家目录，`GIT_CONFIG_NOSYSTEM=1`，`GIT_CONFIG_GLOBAL` 指向空文件，不读也不写本机的 git 配置。没有运行 Claude 或 Codex CLI，没有外发请求。
- 对照的两种方法：
  - **A（SP-5 方案）**：在用户仓库里执行，`GIT_INDEX_FILE` 指向复制出来的临时 index，`GIT_OBJECT_DIRECTORY` 指向私有对象目录，`GIT_ALTERNATE_OBJECT_DIRECTORIES` 指向仓库的对象；命令行固定加 `-c core.hooksPath=<空目录> -c core.fsmonitor=false -c core.safecrlf=false -c advice.addEmbeddedRepo=false`。
  - **B（D8 方案）**：`GIT_DIR` 指向 cupper 自己的目录（`init --bare` 后设 `core.bare=false`，删掉 hooks），`GIT_WORK_TREE` 指向工作区，`GIT_INDEX_FILE` 同 A；`objects/info/alternates` 指向仓库的对象；`config` 只写白名单键（值取自用户仓库的 `git config --list --show-origin --includes -z`）；复制 `info/exclude`、`info/sparse-checkout`、`sharedindex.*`；`GIT_CONFIG_NOSYSTEM=1`，`GIT_CONFIG_GLOBAL` 指向空文件；命令行覆盖与 A 相同。
- 两种方法都执行 `add -A --ignore-errors` 加 `write-tree`，比较 tree id。

### 1.3 结果

| 场景 | A 与 B 的 tree 相同 | 说明 |
|---|---|---|
| 修改、删除、未跟踪、被忽略、"被忽略但已跟踪"、`info/exclude` | 是 | B 保留了被忽略但已跟踪的文件（SP-5 的影子仓库漏掉的正是这类）；B 不复制 `info/exclude` 时 tree 不同，所以必须复制 |
| `core.autocrlf=true`，含 CRLF 的文件 | 是 | B 不复制白名单配置时 tree 不同，所以白名单是必需的 |
| 锥形稀疏检出 + sparse index | 是 | 复制 `info/sparse-checkout` 和相关配置；不复制时本例 tree 仍相同，但 git 会把 sparse index 展开成完整 index 并给出提示，所以照样复制 |
| split index | 是 | 必须复制 `sharedindex.*`，否则 git 报错退出（找不到共享 index） |
| index 带 untracked cache（UNTR）和 fsmonitor（FSMN）扩展 | 是 | B 没有配置 fsmonitor，git 照常读取这份 index，没有报错 |
| linked worktree（会话用的 worktree） | 是 | index 取自 worktree 自己的 git 目录 |
| sha256 对象格式的仓库 | 是 | 白名单里的 `extensions.objectFormat` 加 `core.repositoryFormatVersion=1` |
| 快照档生成补丁，在用户仓库 `apply --cached` | — | 应用成功，暂存出来的 blob 与 `git add` 的结果相同 |
| 默认的全局忽略文件（`~/.config/git/ignore`） | — | `GIT_CONFIG_GLOBAL` 指向空文件时仍然生效 |
| `ls-files --cached --others --exclude-standard` | — | B 与在用户仓库里执行的结果相同，`@` 引用的文件列表可以放进快照档 |
| `check-attr filter` | — | 在 B 里能读出工作区 `.gitattributes` 指定的过滤器名，用来标出 `filter_unsupported` 的路径 |

**恶意配置仓库**：用户仓库的配置里有 `core.fsmonitor`、`filter.evil.clean/smudge`、`diff.evil.textconv`、`include.path` 指向工作区里的一个配置文件（其中定义了另一个过滤器）、`info/attributes` 里的过滤器，`hooks/` 下有 `post-checkout`、`post-index-change`、`reference-transaction`。每个程序被执行时都会写一个标记文件。

| 做法 | 被执行的程序 |
|---|---|
| A，只有无条件覆盖（fsmonitor、hooksPath） | 三个 clean 过滤器（仓库配置里的、工作区 include 里的、`info/attributes` 引用的），`add -A` 和 `write-tree` 都会执行 |
| A，`write-tree` 不带任何覆盖 | fsmonitor 和 `post-index-change` hook |
| A，另把每个过滤器和 textconv 用 `-c <键>=` 置空 | 无；git 按原样处理内容，没有报错 |
| A，置空，但过滤器设了 `required=true` | 无；git 报错退出（`clean filter 'evil' failed`） |
| B | 无；tree 与 A 相同；在 B 里执行 `diff-tree -p` 也没有执行 textconv |

**racy 检测**：第一次跑基本场景时，A 与 B 偶尔得到不同的 tree。原因是复制 index 时文件的 mtime 变成了当前时间：一个文件在 index 写入的同一秒内被改写、大小不变，复制发生在下一秒时，git 认为它没有变（racy 检测依据 index 文件的 mtime），快照里留下了旧内容。专门测了 12 次：用普通复制，12 次全错；保留 mtime 复制（等价 `cp -p`），12 次全对。这个问题与 GIT_DIR 无关，SP-5 的方案同样有，设计里改为"复制 index 时保留 mtime"。

**性能**（合成仓库，热缓存，5 次取中位数，单位 ms）

| 仓库 | A 无改动 | B 无改动 | A 小改动 | B 小改动 | B 中重新生成配置等准备工作 | 其中读一次有效配置 |
|---|---|---|---|---|---|---|
| 2,000 个文件 | 46 | 87 | 59 | 100 | 50 | 10 |
| 20,000 个文件 | 70 | 115 | 104 | 160 | 62 | 12 |

B 去掉准备工作后，`add -A` 加 `write-tree` 与 A 相当。实验脚本的准备工作调用了 5 次 git，实现时合并成"读一次配置 + 一次 `rev-parse`"，预计每次多 15–25 ms。

### 1.4 结论

- **可行，快照路径采用 B。** 与 SP-5 兼容（同样从用户 index 播种、写私有对象库），各种 index 形态下 tree 相同；SP-5 否决影子仓库的三个原因（首次 72 s、360 MB、漏掉被忽略但已跟踪的文件）都来自"空 index 起步、没有 alternates"，B 不存在。
- **B 的适用范围不止快照**：`last_turn`、`turn:<id>`、`unstaged`、`staged` 四种范围的文件列表和补丁，`git/status` 的文件状态，`@` 引用的文件列表，都可以在 B 里算。这样，在用户仓库里执行的自动调用只剩只读引用类命令和少数写操作（暂存、还原、worktree 管理），这些继续用"每次调用前读配置并覆盖"的做法。
- **对 `.gitattributes` 的影响**：工作区里的 `.gitattributes` 仍然生效，但引用的过滤器和 diff 驱动在 B 的配置里没有定义，按原样处理内容。结果是：
  - 内置的 diff 驱动（如 `diff=cpp`）、`text`、`eol` 等转换照常；
  - LFS：需要 cupper 在 B 的配置里写入 git-lfs 的标准取值，否则 LFS 文件会被当成内容已变化。本机没有 git-lfs，这一点未实测，记入待定问题 Q32；
  - 其他自定义过滤器（如 git-crypt）不执行，这些路径在 `unstaged`、`staged` 范围里标为 `filter_unsupported`，不提供 hunk 操作，避免把明文暂存进去。
- **对仓库档的启示**：`write-tree` 这类命令也会因为 racy 检测执行 clean 过滤器，并触发 fsmonitor 和 `post-index-change` hook，所以仓库档的防护必须覆盖每一次调用，包括辅助命令。
- **Q32 的部分结论**：`-c filter.<名>.clean=` 置空后按原样处理内容；`required=true` 时 git 报错退出，按纪要 D8 当作失败，不去掉覆盖重试。`diff.*.textconv`、`merge.*.driver` 等其他键的置空效果，仍按 Q32 在 M1b 的 git 服务任务里用"恶意配置仓库"样例逐键确认。
