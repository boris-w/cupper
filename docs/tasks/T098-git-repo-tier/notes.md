# T098 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行。实现按 T054、T060 的经验乘 1.3（文档注释和 rustfmt 展开），测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `git/cmd.rs` | `GitCmd::global_args`：T097 的 `Overrides::global` 是现成的 `OsString` 参数（键可能不是 UTF-8），不能经 `.config(String, String)` 转一道，否则有损转换会让置空对不上原来的键 | 8 | — |
| `git/mod.rs` | 声明 `repo` 模块；测试构建里给 `GitTool` 加额外环境变量的口子（用例要 `GIT_CONFIG_NOSYSTEM`，不读本机的系统配置） | 10 | — |
| `git/repo.rs` | `RepoTier::run`：`Layout::read` → `ConfigList` → `parse` → `check_sources` → `overrides` → `global_args` → 调用前指纹 → 执行 → 调用后指纹 → 判定；`~/` 开头的 `core.hooksPath` 展开后再交给 `Fingerprint::take`；`GitError`（thiserror，`code`/`reason` 按协议，notice 信号）；`RepoNotice`；失败时 stderr 末尾经 `diag::redact::credentials` 脱敏 | 140 | — |
| IT-git-repo-tier-malicious | 临时仓库与隔离环境（`HOME`、`GIT_CONFIG_NOSYSTEM`）；写标记文件的"程序"；恶意的 `.git/config`（5.2.10 第 4 条全部的键加新补的 5 类、fsmonitor、`core.hooksPath` 指向工作区）、全部常用 hooks、`.gitattributes`；读写各几条命令后没有标记；include 到工作区、子段带 `=` 拒绝；`required=true` 报错不重试；普通失败不误判；中途改配置（包一层假 git，命令后改 `.git/config`）；git-lfs 假脚本核实 `clean`、`smudge` | — | 200 |
| **合计** | | **约 158，乘 1.3 约 205** | **约 200** |

合计约 **405 行**，在约 500 的停止线以内。合成大仓库上指纹的耗时用临时用例量一次、写进本文件，不提交那段代码。

## 实际行数（逐模块）

`git diff --numstat`，不含本文件。

| 完成的模块（提交） | 新增 / 删除 | 实现 | 测试 | 估计 |
|---|---|---|---|---|
| `GitCmd::global_args`（`feat(host): GitCmd takes global arguments as they are`） | 14 / 9 | 14 | — | 8 |
| `RepoTier::run`、`GitError`、测试构建的环境口子与 IT（`feat(host): the repo tier's guarded run`） | 464 / 0 | 200 | 262 | 205（含上一行）+ 200 |

**合计 478 行**：实现约 216，测试约 262，在约 500 的停止线以内，但贴线。测试一次写完，比估计多约 60 行：恶意配置先写成 git config 文件的格式（约 50 行），改成"键 名字"一行一个、用 `git config -f` 写入后压到 262；剩下的主要是六个样例仓库的建法和 rustfmt 的展开。写完代码时已经没有要加的代码，后面只剩 CI 记录和本文件，所以没有中途停下；汇报里写明。

## 实现时的取舍

1. **`GitCmd::global_args`**：T097 的 `Overrides::global` 是现成的 `OsString`，键可能不是 UTF-8（子段是任意字节）。经 `.config(String, String)` 转一道要做有损转换，置空的键会和配置里的键对不上，等于没置空。所以加了按原样追加的入口，`.config` 也改成经它实现。`global` 里的 `--no-pager` 和两个无条件的 `-c` 与 T060 档位固定的重复，无害；档位固定的仍排在最后。
2. **路径统一用 realpath**：`config --list --show-origin` 的相对来源按 `-C` 的目录拼接，而 `Layout::read` 给的 git 目录是 realpath。临时目录在 `/var` 下（`/private/var` 的符号链接）时，同一个 `.git/config` 在指纹里会有两种写法、两个条目，变化时 `paths` 里出现两次（用例先失败了一次才发现）。现在 `run` 先取仓库的 realpath，再用它读布局、解析来源。include 里写的 `..` 不做规范化，是不同的文件名，不影响判断。
3. **判定顺序**：只读命令只要指纹变了就丢结果（不管成功与否）；否则失败时看 `required`；写操作失败而指纹也变了时，返回普通失败并在 `GitError::Failed.changed` 里带上路径，`notice()` 照样给出 `repo_config_changed`。
4. **错误码**：`reason()` 给协议的 `ErrorReason`（`unsafe_repo_config`、`repo_config_changed`、`not_a_repo`），`code()` 由 reason 推出；没有 reason 时超时是 `timeout`，其余是 `git_error`。普通失败带 stderr 末尾 2 KiB，经 `diag::redact::credentials` 脱敏（URL 里的账号和查询串、常见 token）。
5. **notice 信号**：`RepoNotice { code: NoticeCode, paths: Vec<PathBuf> }`，路径是绝对的 realpath。写进会话流时要不要改成相对仓库的路径（协议样例是 `.git/config`），由写 notice 的调用方决定。来源检查拒绝时，`paths` 是那个来源文件。
6. **`core.hooksPath` 的 `~/`**：在取指纹之前把最后生效的值展开成 `HOME` 下的路径，T097 的 `Fingerprint::take` 再按工作区拼接（绝对路径拼接后不变）。
7. **用例的隔离**：`GitTool` 在测试构建里多一个 `test_env`，用例放进 `GIT_CONFIG_NOSYSTEM=1`；`HOME` 是临时目录（仓库档本来就带 `HOME`），建仓库的脚本也用同一份环境，所以全局配置写在临时目录里。用例的 git 取测试进程 PATH 里的第一个 git，只在临时目录里运行。

## 用例的变异检查

把 `cmd.global_args(overrides.global)` 临时改成不加覆盖，`it_git_repo_tier_malicious_no_program_of_the_repository_runs` 在第一批调用就失败（恶意过滤器被执行、调用报错），改回后通过。没有提交这次改动。

## 指纹的耗时（T060 发现第 4 条）

临时写了一个用例，没有提交：5,000 个目录、20,000 个文件、100 个 `.gitattributes`，空的 hooks 目录，系统 SSD、热缓存。

| 构建 | 前后两次 `Fingerprint::take` 合计 |
|---|---|
| release | 139–147 ms |
| debug | 147–178 ms |

每次约 70 ms，几乎全花在遍历目录上。按目录数线性估算，带大 `node_modules` 的仓库（几万个目录）每次仓库档调用前后合计要接近一秒到数秒。M1a 仓库档调用不多（取 git 根目录、worktree 操作），先不缓存；`git/status` 一类会频繁调用仓库档的方法上线前，建议缓存 `.gitattributes` 的清单（按目录 mtime 失效），或者只扫描 index 里出现的目录。

## git-lfs（Q32）

本机没有 git-lfs。用假的 git-lfs 脚本（记下子命令，原样转发内容）核实：标准取值 `git-lfs clean -- %f`、`git-lfs smudge -- %f` 被改写成绝对路径后，`update-index --refresh` 跑了 `clean`，`worktree add` 的检出跑了 `smudge`，内容不变。`filter.lfs.process`（`git-lfs filter-process`，长连接协议）没有实测，假脚本不实现这个协议。
