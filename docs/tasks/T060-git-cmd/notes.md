# T060 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）
- 日期：2026-10-20

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。T054 的实际偏差是实现 1.45 倍、测试 1.0 倍（实现多在文档注释和 rustfmt 展开的结构体字面量上），这里按这个系数推算。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `git/mod.rs` | `GitTool`：git、git-lfs 的绝对路径，可信 PATH，`SSH_AUTH_SOCK`，`<数据目录>/git/` 与 `empty-hooks` 的创建；固定环境（`PATH`、`LC_ALL=C`、`GIT_TERMINAL_PROMPT=0`、`GIT_PAGER=cat`、`SSH_AUTH_SOCK`）；三档的标记类型 | 80 |
| `git/run.rs` | 一次性拉起：stdin 可写入（`apply` 从 stdin 读补丁），stdout、stderr 分别收集并设上限，`Clock` 超时，超时后按进程组（含后代）结束并回收。T094 的 `probe::run` 只有 `/dev/null` 的 stdin、丢弃 stderr、1 MiB 上限，不够用 | 90 |
| `git/cmd.rs` | `GitCmd<档位>`：每类命令一组固定参数（数据表：子命令、固定参数、是否只读、超时键），统一 `-z`（命令支持时）、只读加 `--no-optional-locks`、`-c core.quotepath=false`；仓库档的命令表（5.2.10 列出的只读引用类和写操作）、快照档的最小入口（`ls-files`，其余由 T061 加）；`compile_fail` 的编译期证明 | 120 |
| `git/config.rs` | 解析 `config --list --show-origin --includes -z`；来源检查（工作区和 agent 可写位置拒绝，仓库自己的 `config`、`config.worktree` 例外，include 到工作区照样拒绝）；可执行类键的匹配（按段名和键名，大小写不敏感）与置空、git-lfs 标准取值改写成绝对路径；`required=true` 的过滤器被置空时记下，供"报错不重试"判定 | 110 |
| `git/fingerprint.rs` | 仓库布局（`.git` 目录或 `gitdir:` 文件、`commondir`）；指纹：config 及 include 的文件、`config.worktree`、`hooks/` 与 `core.hooksPath` 下的文件、`info/attributes`、工作区里全部 `.gitattributes`（跳过 `.git`，不跟随符号链接）；前后比较给出变化路径 | 100 |
| `git/repo.rs` | `RepoTier::run`：第 1–6 条的编排；`GitError`（thiserror，带 `ErrorReason`、notice 信号和 `paths`） | 110 |
| `durations.rs` | git 只读 30 s、快照 60 s、`worktree add` 10 分钟、`push` 5 分钟 | 12 |
| **实现小计** | | **620**，乘 1.45 约 **900** |

**测试**

| 用例 | 内容 | 草估 |
|---|---|---|
| IT-git-repo-tier-malicious | 临时目录里的恶意仓库（`HOME`、`XDG_CONFIG_HOME`、`GIT_CONFIG_GLOBAL`、`GIT_CONFIG_SYSTEM` 都指向临时目录）：fsmonitor、hooks（含 `core.hooksPath` 指向工作区）、`filter.*.clean/smudge/process`（含 `required=true`）、`diff.*.command`、`diff.*.textconv`、`include.path` 指向工作区、`core.sshCommand`、`core.pager` 等，每个"程序"一运行就写标记文件；断言没有标记、该拒绝的拒绝、`required=true` 报错不重试；git-lfs 标准取值用假的 git-lfs 核实绝对路径能用；中途改配置得到 `repo_config_changed`（用假 git 在命令中途改 `.git/config`） | 170 |
| UNIT-git-fingerprint | 每一类内容各改一次都能发现并给出路径；无关文件不影响 | 100 |
| 环境断言 | 假 git 脚本记下收到的环境和参数，断言只有规定的变量 | 50 |
| 配置规则 | `-z` 输出的解析、键的匹配（含子段、大小写）、含 `=` 的键拒绝 | 60 |
| 编译期证明 | `compile_fail` doctest 两个方向，加一个能编译的对照 | 25 |
| 拉起辅助 | stdin 写入、stderr 收集、超时（`ManualClock`） | 30 |
| **测试小计** | | **435** |

**合计**：约 **1,335 行**（实现约 900，测试约 435）。不乘系数也有约 1,055 行，超过约 1,000 的停止线。按 brief 和守则第 8 条，开工前停下来汇报，没有写代码。

## 拆分建议

依赖关系：T061（快照档）只需要 `GitCmd`、拉起辅助和快照档类型；T044、T034 要经仓库档取 git 根目录，需要完整的防护。brief 举的"把指纹单独拿出来"只拿走约 245 行，剩下约 1,090 行仍然超线。

**方案一（推荐，三份，前两份并行）**

| 拆分 | 内容 | 用例 | 估计 |
|---|---|---|---|
| T060a GitCmd 与三档类型 | `GitTool`、环境、工作目录、拉起辅助、`GitCmd<档位>` 与命令表、快照档最小入口、编译期证明、时长登记 | 环境断言、编译期证明、拉起辅助 | 约 545 |
| T060b 仓库档的规则与指纹（纯函数） | 配置输出的解析、来源检查、覆盖参数的计算（置空、git-lfs 改写、`=` 拒绝、`required` 记录）；仓库布局与指纹 | UNIT-git-fingerprint、配置规则 | 约 465 |
| T060c 仓库档的执行 | `RepoTier::run` 把 a、b 接起来：读配置、拒绝、覆盖、运行、前后指纹、报错不重试、`GitError` 与 notice 信号 | IT-git-repo-tier-malicious | 约 330 |

a、b 互不依赖（b 的输入是配置输出的字节、仓库路径和边界，输出是拒绝或覆盖参数），可以同时做；c 在两者之后。T061 在 a 之后就能开始，T044、T034 等 c。按工作量，到 T044 的链约 545 + 330，比一份顺序做完（约 1,335）短。

**方案二（两份，顺序）**：T060a 同上（约 545）；T060b 合并方案一的 b 和 c（约 795）。T061 在 a 之后开始；T044、T034 等 b。

**方案三（不拆）**：上限提到约 1,400 行，由一个执行者顺序完成。

## 实现前想到的几点（供执行者，与拆法无关）

1. **`-c <键>=` 置空遇到子段里带 `=` 的键会失效**：git 在第一个 `=` 处切分 `-c` 的参数，`[filter "a=b"] clean = ...` 的置空会变成给 `filter.a` 赋值，原来的 `filter.a=b.clean` 仍然生效（属性里可以写 `filter=a=b`）。已在临时仓库里用本机的 git 2.50 核实：`-c 'filter.a=b.clean='` 之后 `config --get filter.a` 是 `b.clean=`，`filter.a=b.clean` 仍是原值。打算对这种键直接拒绝（`unsafe_repo_config`），不尝试转义。
2. **"覆盖后报错"的判定**：Apple 的 git 自带系统配置 `credential.helper=osxkeychain`，几乎每次调用都会置空它，不能把"置空过任何键且失败"都算成 `unsafe_repo_config`。打算只在被置空的过滤器设了 `required=true` 时这样判定，其余失败按普通 git 失败返回。
3. **环境里没有 `HOME`**：按 brief，git 子进程的环境只有那五个变量，git 因此读不到用户的全局配置（`~/.gitconfig`）。仓库档的只读命令不受影响，但 `worktree add` 的检出会忽略用户全局的 `core.autocrlf` 等设置；用户档（T123）的 `commit`、`push` 需要 `HOME`（身份、ssh 密钥）。建议编排者确认 `worktree add` 是否需要 `HOME`。
4. **指纹要遍历整个工作区找 `.gitattributes`**，大仓库（几十万个文件）每次仓库档调用前后各一次，可能要上百毫秒到一秒。打算先按设计实现（跳过 `.git`，不跟随符号链接），实测后再看是否需要缓存或改用 index 里的路径。
5. **设计第 4 条清单之外的可执行类键**：`diff.external`、`core.alternateRefsCommand`、`gpg.ssh.defaultKeyCommand`、`submodule.*.update = !命令`、`pager.<命令>`、`alias.*`（`!` 开头）。其中 `pager.*` 有 `--no-pager` 兜底，`diff.external` 有 `--no-ext-diff` 兜底，`alias.*` 只在按别名调用时生效，另外几个只在仓库档不会用到的命令（fetch、push、签名、submodule update）里执行。打算按设计的清单实现，把这几项交给安全评审决定是否加入清单。
6. 本机没有 git-lfs，标准取值的改写用假的 git-lfs 脚本核实（只覆盖 `clean`、`smudge`；`filter-process` 走长连接协议，假脚本不实现，记为未实测）。
