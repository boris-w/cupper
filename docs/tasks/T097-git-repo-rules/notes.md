# T097 工作笔记

- 执行者：子 agent（Rust 工程师；T043、T088 的执行者）

## 开工前核实的输出格式

在临时目录里用本机 git 2.50 看了一次 `git config --list --show-origin --includes -z` 的字节（只用于确定格式，不进用例）：

- 每项是 `<来源>\0<键>\n<值>\0`；没有值的键（`[core] flag`）只有 `<键>\0`；
- 来源是 `file:<路径>`、`command line:`（`-c` 给的），另有 `blob:`、`standard input:`、`submodule-blob:`；
- 仓库的配置显示为相对路径 `file:.git/config`；include 的文件按包含它的文件所在目录拼接，同样是相对于 git 运行目录的路径（`file:.git/../../inc`）。所以解析时要传入 git 运行的目录（`-C` 的路径），先拼成绝对路径再取 realpath；
- 键的段名和变量名是小写，子段保持原样，可以含 `.` 和 `=`（`filter.a=b.clean`）。

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释。模块文件名与 T060 的草图（`config.rs`、`fingerprint.rs`、`repo.rs`）错开，用 `git/rules.rs`、`git/stamp.rs`。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `git/mod.rs` | 模块文档与声明 | 10 |
| `git/rules.rs`：解析 | `Entry`（来源、键、可选的值，都按字节）、`Origin`（文件、命令行、其他）；按 `\0` 和第一个 `\n` 切分；相对路径按运行目录拼接 | 55 |
| `git/rules.rs`：来源检查 | `Bounds`（git 目录、common dir、agent 可写的根目录）；来源和根目录取 realpath；git 目录的 `config`、`config.worktree` 例外；无法解析的路径和未知来源一律拒绝；给出原因 | 50 |
| `git/rules.rs`：覆盖参数 | 无条件覆盖（`core.fsmonitor`、`core.hooksPath`、`--no-pager`）；生成补丁的命令加 `--no-ext-diff --no-textconv`；可执行类键的清单与匹配（段、子段、变量名，大小写按 git 的规则）；每个键取最后一个值；git-lfs 标准取值改写成加了 shell 引号的绝对路径，其余置空；子段带 `=` 的拒绝；记下被置空、设了 `required=true` 的过滤器 | 95 |
| `git/rules.rs`：错误 | thiserror 的 `Unsafe`，每种原因一个变体 | 20 |
| `git/stamp.rs` | `Layout::read`（`.git` 是目录，或是 `gitdir:` 文件；`commondir`）；指纹：配置的来源文件、`config.worktree`、`hooks/` 与 `core.hooksPath` 下的文件、`info/attributes`、工作区里全部 `.gitattributes`（跳过 `.git`，不跟随符号链接），按路径记 sha-256 或"不存在"；前后比较给出变化路径 | 130 |
| 实现小计 | | **约 360** |

**测试**

| 用例 | 内容 | 草估 |
|---|---|---|
| UNIT-git-repo-rules | 解析：值里有换行、`=`、非 UTF-8 字节，没有值的键，命令行来源，相对路径；来源检查的表：工作区内、其他项目内、git 目录例外、include 到工作区、`core.hooksPath` 指向工作区不受限、经符号链接逃逸、未知来源；覆盖参数的表：每一类可执行键（含新补的 5 类）、git-lfs 标准取值改写、非标准取值置空、子段带 `=` 拒绝、`required=true` 的记录、补丁命令的参数 | 160 |
| UNIT-git-fingerprint | 在临时目录造普通仓库和 worktree（`.git` 是文件）的布局；每一类内容改一次都能发现并给出路径；无关文件不影响 | 100 |
| 测试小计 | | **约 260** |

**合计约 620 行**（实现约 360，测试约 260）。T043、T088 的实际行数分别是估计的约 1.25 倍和 1.03 倍；按这个范围，最后会在 640–700 行之间。

估计超过约 550 行的停止线，并贴近 650 行的硬停止线。按任务书，开工前停下来汇报，没有写代码。

## 汇报与建议

验收要求逐项覆盖的情形很多，测试本身就要约 260 行。T060 的执行者对这一份估约 465 行，里面配置规则的测试只算了 60 行，比验收列出的情形少。

- **方案一（建议）**：不拆，上限放宽到约 700 行，硬停止线 780 行。两半都是 T098 的前置，拆开不会让 T098 提前开始，而且本任务在零余量的关键路径上，多一次派发和合并只会更慢。
- **方案二**：拆成两份并行。
  - T097 只做配置规则（解析、来源检查、覆盖参数），约 400 行；
  - 另立一个任务做布局与指纹，约 230 行。

  T098 要等两份都合并。适合有另一位执行者空闲的情况。

## 裁定之后

编排者采用方案一，不拆，上限约 700 行，硬停止线 780 行（任务书末尾"补充裁定"）。

### 实际规模

| 提交 | 内容 | 增 |
|---|---|---|
| `feat(host): the repo tier's configuration rules` | `git/rules.rs` 262、测试 197、`git/mod.rs`、`lib.rs` | 464 |
| `feat(host): fingerprint a repository's executable configuration` | `git/stamp.rs` 148、测试 84、`git/mod.rs` | 235 |
| 合计 | | **699** |

第一稿合计 706 行，超过 700 行的汇报线。把 shell 引号的写法压成几行后降到 699 行，没有删减任何检查。测试里的长清单都写成按空白切分的字符串，免得 rustfmt 一行一项地展开。

## 实现要点

- **解析**（`rules::parse`）：
  - 去掉末尾的 `\0` 后按 `\0` 切分，两段一组；不成对或末尾没有 `\0` 时，判为 `Malformed`；
  - 键和值都按字节保存，值在第一个 `\n` 处与键分开，所以值里可以有换行；
  - `file:` 后的相对路径拼到 git 运行的目录上；
  - 来源只认 `file:` 和 `command line:`，其余（`blob:`、`standard input:` 等）记为 `Other`，来源检查一律拒绝。
- **来源检查**（`rules::check_sources`）：
  - 来源文件和 agent 可写的根目录都取 realpath；已经不存在的根目录跳过；
  - 来源解析不了（`Unresolved`）时拒绝；
  - 只有 common dir 的 `config` 和本 worktree 的 git 目录下的 `config.worktree` 例外。所以 git 目录里的其他文件（例如 include 进来的 `.git/inc`）、主工作区的 `config.worktree`（当前是另一个 worktree 时）都照样拒绝；
  - `core.hooksPath` 的值不参与检查。
- **覆盖参数**（`rules::overrides`）：
  - 每个键取最后一个值；
  - 可执行类键按"段、有没有子段、变量名"匹配。git 输出的段名和变量名已是小写，子段区分大小写；
  - git-lfs 只认三个标准取值，程序改成单引号括起的绝对路径；
  - 给出 `global`（放在子命令之前：`--no-pager` 和 `-c`）、`command`（补丁命令的两个参数）和 `required`；
  - `--no-pager` 放在这里，是因为它属于 5.2.10 第 3 条的无条件覆盖；T060 若也加，重复无害。
- **指纹**（`stamp`）：
  - `Layout::read` 把 git 目录和 common dir 取成 realpath；
  - 指纹按路径记 sha-256，文件不存在记为 `None`。所以新建、修改、删除都会出现在 `changed` 的结果里；
  - `core.hooksPath` 取最后一个值，相对路径从工作区根目录算起；
  - 遍历 `.gitattributes` 时跳过 `.git`，不进入符号链接指向的目录。

## 已知的限制

- `core.hooksPath` 以 `~/` 开头时没有展开，因为纯函数不读环境变量。这个目录在仓库档里总被覆盖，不会执行；只是它的内容不进指纹。需要的话由 T098 传入展开后的路径。
- git-lfs 的 `filter-process` 没有实测，仍是 Q32。

## 给 T098 的接入点

- **调用顺序**：每次调用前
  1. `parse(输出, 运行目录)`；
  2. `check_sources(&entries, &Bounds { git_dir, common_dir, writable })`，`writable` 是全部项目和全部会话的工作区根目录；
  3. `overrides(&entries, &Setup { hooks, lfs, patch })`，`global` 放在子命令之前，`command` 放在子命令之后。

  `Unsafe` 的任一变体都对应 `git_error`（`reason: unsafe_repo_config`）。
- **覆盖后失败**：只有 `Overrides.required` 不为空时判为 `unsafe_repo_config`。
- **指纹**：调用前后各 `Fingerprint::take(&Layout::read(工作区)?, &entries)`，`changed` 不为空即 `repo_config_changed`，`params.paths` 取它的结果。
