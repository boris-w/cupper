# T061 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T061-git-snapshot`
- 完成日期：2026-10-26

## 做了什么

按裁定（计划 v1.3.51）收窄为快照档与回合快照。LFS 的实测、`cargo xtask git-lfs` 和 PERF-snapshot 归 T186。

1. **快照档的隔离**（`git/mod.rs`、`git/snapshot.rs`）
   - 每个仓库一个私有对象库 `<数据目录>/git/<repo_key>/objects/`，每个工作区一个 GIT_DIR `<数据目录>/git/<repo_key>/ws/<ws_key>/`。key 是 realpath 的 sha-256 前 8 字节。目录都是 0700，只有 `HEAD`、`refs/`、`info/`，没有 `hooks/`。
   - `SnapshotTier::prepare` 设齐了这些环境变量：`GIT_DIR`、`GIT_WORK_TREE`、`GIT_INDEX_FILE`、`GIT_OBJECT_DIRECTORY`、`GIT_ALTERNATE_OBJECT_DIRECTORIES`（仓库的对象目录及其 alternates）、`GIT_CONFIG_NOSYSTEM=1`、`GIT_CONFIG_GLOBAL`（cupper 的空文件，由 `GitTool::new` 创建），并带 `HOME`。
   - **白名单配置**（`git/snapshot/config.rs`，纯函数）：
     - 从 `config --list --show-origin --includes -z` 里只取设计列出的无害键，按 git 配置文件的语法转义后写入，写临时文件再改名；
     - `objectFormat` 不是 sha1 时同时写 `repositoryFormatVersion=1`；
     - git-lfs 的三条标准取值换成宿主的 git-lfs 绝对路径，`required` 照抄。
     - 这部分有单元测试。
2. **内容一致**
   - 复制工作区的 index 时保留 mtime。
   - 复制 `sharedindex.*`、`info/exclude`、`info/attributes`、`info/sparse-checkout`；源文件不在时，删掉之前留下的副本。
3. **排除**：先用两次 `ls-files -o`（不带和带 `--directory`）做预检。排除三类：超过 50 MB 的未跟踪文件、嵌套仓库、文件超过 10,000 个的整个未跟踪目录。
4. **过滤器占位**
   - 用户配置里定义了 `clean` 或 `process`、而快照档不执行的过滤器，用 `ls-files` 加 `check-attr -z --stdin filter` 找出命中的文件。git-lfs 由宿主执行时不算在内。
   - 这些文件不经 `add`：内容是固定格式 `cupper: not stored, filter <名称> not run; sha-256 <哈希>` 的占位 blob，用 `update-index -z --index-info` 放进 index，记为 `filter_unsupported`。
5. **快照过程**：
   - 排除和占位的路径经 `--pathspec-from-file` 排在 `add -A --ignore-errors` 之外，命令行固定加 `-c core.safecrlf=false -c advice.addEmbeddedRepo=false`；
   - 然后 `write-tree`；
   - 结束快照用 `diff-tree -r -z --numstat --no-renames --no-ext-diff --no-textconv` 统计相对起始快照的改动；
   - `add -A`、`write-tree` 用 `GIT_SNAPSHOT` 超时，其余用 `GIT_COMMAND`；
   - 文件系统操作都在 `spawn_blocking` 里；
   - 临时 index 用完即删。
6. **接入回合**（`session/actor.rs`、`rows.rs`）
   - 回合机器要快照时，actor 在提交之后另起任务：任务持有静止屏障的 `Hold`，计入在途工作，起始 tree 留在 actor 内存里，供结束时统计。
   - 结果回到机器，写 `snapshot.recorded`：
     - 有排除或 `add` 不完整记 `partial`；
     - 不是仓库记 `skipped/not_a_repo`；
     - git 失败或超时记 `failed/git_failed`，详情写日志；
     - 没有 git 时仍记 `skipped`。
   - tree id 写进 `turns.snapshot_start` 和 `snapshot_end`；`snapshot_status` 最后记的是结束快照的状态。
   - git 子进程照旧只经 `cupper-sys::spawn`。
7. **Q32 快照档逐键确认**：结果写在 `notes.md`。用仓库档的恶意配置样例，在四个仓库上拍起始和结束快照，跑完没有任何标记。涉及的键和来源：
   - fsmonitor、两处 hooks；
   - `filter.*` 的三种程序，含 `required` 的过滤器和 include 进来的定义；
   - `diff.*.textconv`、`diff.*.command`、`diff.external`；
   - `merge.*.driver`，以及 core、sequence、gpg、credential、uploadpack、remote、pager、submodule 下所有命名程序的键；
   - `include.path`；`info/attributes` 里的过滤器。

## 关键取舍

- **过滤器检测的条件**：设计写的是"属性来源里没有 `filter=` 时跳过"，实现改为"配置里没有定义快照档不执行的过滤器时跳过"。没有定义的过滤器，用户自己的 git 也按原样处理，快照与之一致；这样也不必逐个找 `.gitattributes`，而 `check-attr` 照样覆盖全部属性来源。**请确认。**
- **占位内容用固定的 ASCII 格式**，没有用设计里的中文例句：宿主不产生界面文案，界面可以按这个格式识别。**请确认，或改设计的例句。**
- **快照档带 `HOME`**：已获认可。不带的话找不到默认的全局忽略文件和属性文件；`GIT_CONFIG_GLOBAL` 已经挡住了 `~/.gitconfig` 和 XDG 配置。
- **不用 `mark_external()`**：快照档不改用户仓库，对象写进私有库，index 是临时复制的。
- **不发 `git.unsafe_repo_config`**：快照档不执行程序，没有拒绝；读不出配置时记 `failed/git_failed`。已获认可。
- **防护是分层的**：
  - 单独把 GIT_DIR 换成用户的 `.git`，测试仍然通过：命令行覆盖、占位和 `diff-tree` 的默认行为也都挡得住；
  - GIT_DIR 换成用户的、同时去掉占位，恶意用例才失败。
  - 所以恶意用例证明的是"这几层合在一起什么都不执行"，notes 里如实写了。

## 验证方式

- IT-git-snapshot-parity：
  - 基本场景（修改、删除、未跟踪、被忽略、被忽略但已跟踪、`info/exclude`）、autocrlf、split index、锥形稀疏检出加 sparse index、untracked cache、linked worktree、sha256，共 7 种情形，tree 与 SP-5 方案（用户自己的 git 在 `cp -p` 的 index 副本上写独立对象库）相同；
  - 三类排除；结束时的统计；
  - 会话层：一个回合的起始和结束快照都是 `ok`，统计正确，`turns` 表里有两个 tree。
- IT-git-racy：显式把文件和 index 设成同一个 mtime、大小不变，快照里是新内容；复制出的文件 mtime 等于原文件。
- IT-git-filter-placeholder：只在 `info/attributes` 里声明的过滤器被标为 `filter_unsupported`，快照里是占位 blob；快照对象库里所有对象都不含明文；过滤器没有执行。
- IT-git-snapshot-malicious：见上文。
- 变异检查（明细在 notes）：
  - 复制 index 时给新的 mtime → racy 和 parity 失败；
  - 不复制 `info/exclude` → parity 失败；
  - 不写占位 → 占位用例失败；
  - 用户的 GIT_DIR 加上不写占位 → 恶意用例和占位用例失败。
  - 只去掉"复制时设回 mtime"那一步，用例仍通过：macOS 上 `std::fs::copy` 本身就保留 mtime。显式设回是为了不依赖这一点。
- 在干净工作区（已合入 b07a716 的 main，含 T178）运行 `cargo xtask ci pr --record docs/tasks/T061-git-snapshot/ci/`，各作业通过，摘要 `pr-1dbbb4db9ffd.json` 已提交：
  - rust 2091、protocol 185、ts 473、license、testdata-scan、e2e 30；
  - ui 按路径过滤跳过；`sensitive-words` 因本机没有词表跳过。
- 没有残留进程。
- **`cargo xtask size`：+1129 −52**，在停止线 1,300 以内。实现比估计多约 40%，用例比估计少（借用了仓库档的恶意夹具）。

## 提交列表

```
f073f35 chore(ci): record T061's PR run
1dbbb4d docs(task): T061 notes: Q32 per key, mutations and line counts
8021ffe test(orch): a git project's turn takes its start and end snapshots
b07a716 Merge branch 'main' into feat/T061-git-snapshot
d4d916d feat(git): turn snapshots in the snapshot tier (in progress)
aa82cdc docs(task): T061 size estimate and split proposal
```

`d4d916d` 是你要求暂停时提交的进行中状态，提交信息里注明了"进行中"；剩下的部分在后面的提交里补齐。`b07a716` 是编排者把 main 合进来的合并提交。

## 遗留问题 / 范围外发现

1. **仓库可执行配置指纹**：设计要求在回合开始和结束时"随快照"计算，回合前后不同就写 `notice{repo_config_changed}`。这一条不在本任务的范围清单里，没有做，建议记交接给 T120 或单列。
2. **`core.excludesFile`、`core.attributesFile` 写的是相对路径时**：git 按自己的工作目录解析，快照档的工作目录是 `<数据目录>/git/`，与用户在仓库里运行 git 时不同。少见，没有处理。
3. **私有对象库不回收**：M1 就是这样设计的；sharedindex 的副本也会在工作区目录里累积。
4. **超大未跟踪目录的判定是"文件数 × 目录数"的扫描**：未跟踪目录很多时会慢。目前够用。
5. **T186**：LFS 用例需要把 `tool(…, lfs)` 指向 xtask 下载的 git-lfs；白名单配置里 LFS 取值的生成已经有单元测试。
6. **`<数据目录>/git/<repo>/ws/<key>/` 的 GIT_DIR** 之后可以供 `@` 文件列表、`git/status`、diff 复用（T120 等）。`SnapshotTier` 现在要传一个 `Place`，`snapshot::take` 里有构造它的 `prepare`。
