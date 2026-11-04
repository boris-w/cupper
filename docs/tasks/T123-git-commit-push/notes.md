# T123 notes

## 开工前的规模估计（2026-11-04）

停止线 500，硬停止线 600。

| 模块 | 内容 | 行 |
|---|---|---|
| 用户档类型（`git/cmd.rs`、`git/mod.rs`） | `User` 标记、`UserOp::{Commit, Push}`、`GitCmd::user`、`UserTier::prepare`：带 `HOME`、可信 PATH、`SSH_AUTH_SOCK`、`GIT_TERMINAL_PROMPT=0`，不加任何覆盖（hooks、签名、凭据助手与终端一致）；commit 和 push 的超时 | 55 |
| `git/commit` | 取工作区、`RepoLock`、`mark_external`、用户档 `commit -F -`（信息从 stdin）、再经仓库档取 HEAD 的提交作 `oid`、账本 | 45 |
| `git/push` | HEAD 的分支与上游（仓库档）、`--set-upstream <remote> HEAD`、`mark_external`、结果 `{remote, branch}` | 45 |
| 错误分类（纯函数） | 按退出码和 stderr 分出 `nothing_to_commit`、`hook_failed`、`rejected`、`auth_failed`、`lock_held`，签名失败另议；stderr 末尾 2 KiB 经 `redact::credentials` 放进 `data` | 30 |
| 指纹变化的确认参数 | 见下文的问题 1；按方案 B 估 | 30 |
| 接线 | 路由、UNIT-unsupported-set、时长登记 | 8 |
| 单元用例 | 错误分类；用户档的环境（照 T060 的假 git 记录环境，确认 `SSH_AUTH_SOCK` 传进去、没有覆盖） | 50 |
| IT-git-commit-push | 会话夹具加真实 git 和本地裸仓库：提交成功（`oid` 等于 HEAD）、pre-commit hook 拒绝、没有可提交的内容、`--set-upstream` 推送并设好上游、远端分叉后被拒、hook 打出的带凭据的 URL 在 `data` 里被脱敏、假的 `gpg.program` 失败时的签名错误 | 130 |
| 合计 | | 约 430 |

## 编排者的决定（2026-11-04）

1. 指纹变化的确认参数选 B：`git/status` 的结果附加可选字段 `repo_config_changed?: [path]`，宿主不强制确认。
2. 附加 `ErrorReason::signing_failed`，gpg、ssh、x509 三种签名失败都归到它。
3. 登记 `git_commit`，5 分钟；超时的处理与崩溃后的 `outcome_unknown` 一致，或者回复前比对 HEAD，二选一。
4. 提交拿 `RepoLock`，推送不拿，两者都写 `pending_external`。
5. 环境照 5.2.10：可信 PATH、`HOME`、`SSH_AUTH_SOCK`、`GIT_TERMINAL_PROMPT=0`、`LC_ALL=C`，不加别的覆盖；`commit -F -` 的 stdin 是提交信息，`push` 的 stdin 是 `/dev/null`。

## 实现要点

- 用户档：`GitCmd<User>`、`UserOp::{Commit, Push}`、`GitTool::user_tier(repo)`。环境与仓库档相同，不带任何 `-c`；参数只有 `--no-pager -C <repo>`。
  - `-c core.quotepath=false` 原本由 `GitCmd` 给每档都加（设计 5.2.10 的 `GitCmd` 构造器一条），现在移进仓库档和快照档的固定覆盖。它经 `GIT_CONFIG_PARAMETERS` 会传进 hook 里调用的 git，改变 hook 拿到的路径写法（编排者 2026-11-04 的修改意见 1）。
- 写路径：取工作区，提交时拿 `RepoLock`，`mark_external`（FULL fsync），然后执行。
  - git 正常退出但失败：按 stderr 分类后拒绝命令。
  - 超时被杀，或子进程没起来：丢弃票据，结果留在 Unknown，回复 `conflict/outcome_unknown`，与崩溃后的重试一致。
- 超时选的是 `outcome_unknown`，不比对 HEAD。原因有两条：
  - 推送没有本地可比的东西；
  - 提交比对 HEAD 时，hook 也可能改了 HEAD，比对并不可靠。
- 提交的 `oid`：提交成功后经仓库档读 HEAD。这次读取失败时票据留在 Unknown，重试得到 `outcome_unknown`；提交本身已经落地。
- 推送：
  - remote 取 HEAD 所在分支的 `%(upstream:remotename)`，remote 名里可以带 `/`。它加在 `head()` 原有那条仓库档 `for-each-ref refs/heads/<分支>` 的格式里，不多跑一次 git（编排者修改意见 2；git 2.16 起支持，最低 2.39）。没有上游时用 `origin`。
  - 结果里的 `branch` 是本地分支名，没有读 `%(upstream:remoteref)`。默认的 `push.default=simple` 下两者同名。
  - 带 `set_upstream` 时执行 `push --set-upstream <remote> HEAD`，否则执行不带参数的 `git push`，由用户的 `push.default` 决定推到哪里。
  - detached HEAD 时回 `git_error`，不带 reason。
- 失败分类（`git/user.rs`，读 `LC_ALL=C` 下 git 的英文输出和退出码）：

  | 情形 | reason |
  |---|---|
  | `index.lock` | `lock_held` |
  | 提交时：nothing to commit / added to commit | `nothing_to_commit` |
  | 提交时：failed to sign the data、ssh-keygen、user.signingkey | `signing_failed` |
  | 提交时退出码 1（hook 拒绝；git 自身的错误是 128） | `hook_failed` |
  | 推送时：rejected]、non-fast-forward、fetch first | `rejected` |
  | 推送时：Authentication failed、Permission denied、terminal prompts disabled | `auth_failed` |
  | 其他 | 不带 reason 的 `git_error` |

  stderr 末尾 2048 字节经 `redact::credentials` 脱敏，放在 `data.stderr`。
- `repo_config_changed` 的跟踪：
  - 会话 actor 维护一个集合，每次 `commit()` 时从写入的 `repo_config_changed` notice 事件里补充；`git/status` 经 `SessionNotes::repo_config_changed` 向 actor 的邮箱取，不查数据库。
  - 加载时从 `turn_snapshots.ops` 和未被修剪的 `events` 重建这个集合。只解析含 `"repo_config_changed"` 字样的行，所以重启后也在。
  - 提交之后不清空。

## LC_ALL=C 的实测

- 含中文的提交信息（`Add b\n\n加上 b，写在 LC_ALL=C 下。`）经 `commit -F -` 后字节原样落地（IT 第 2 步核对了 `git log -1 --format=%H%n%B`）。
- 含非 ASCII 路径的 hook：用户档不再带 `-c core.quotepath=false`，hook 里调用的 git 按用户自己的配置输出路径，与终端一致。
- Python 写的 hook（如 pre-commit 框架）：按 PEP 540，Python 3.7 起在 C locale 下自动进入 UTF-8 模式。这一点没有用真实框架实测。
- 没有发现 `LC_ALL=C` 让常见 hook 出问题的情形。

## Q23：自动验证与手动说明

自动验证了（IT-git-commit-push 和单元用例）：

1. `SSH_AUTH_SOCK` 传进用户档，且不带任何 `-c` 覆盖（`unit_git_cmd_tiers_pass_only_their_environment_and_fixed_arguments`）。
2. 签名失败：用假的 `gpg.program`，模拟 pinentry 没有终端时的输出，退出码 2，得到 `signing_failed`。
3. ssh 认证失败：用假的 `core.sshCommand`，输出 Permission denied (publickey)，退出码 255，得到 `auth_failed`。

同时验证了：仓库的 hook 照常运行（`core.hooksPath`），hook 输出里的凭据在 `data.stderr` 中被脱敏。

需要作者自己的配置、可选的手动验证：

- **SSH agent**：用一个 ssh 远端，从 Finder 或 Dock 启动 Cupper，在界面上推送。
  - 系统 agent 的 `SSH_AUTH_SOCK` 由 launchd 提供（`launchctl getenv SSH_AUTH_SOCK`），应当成功。
  - 1Password、Secretive 这类 agent 如果写在 `~/.ssh/config` 的 `IdentityAgent` 里，ssh 会读到（`HOME` 传进去了），也应当成功。
  - 只在 shell 配置里 `export SSH_AUTH_SOCK=...` 的，从 GUI 启动的宿主看不到，会得到 `auth_failed`。指引：改写进 `IdentityAgent`，或者用 `launchctl setenv`。
- **GPG 签名**：设 `commit.gpgsign=true`，并用终端版 pinentry（curses、tty），在界面上提交，应当得到 `signing_failed`。
  - 指引：`brew install pinentry-mac`，在 `~/.gnupg/gpg-agent.conf` 写 `pinentry-program` 指向它，然后 `gpgconf --kill gpg-agent`。
  - 改完再提交，应当弹出图形版的口令框并成功。
  - ssh 签名（`gpg.format=ssh`）由 `ssh-keygen -Y sign` 经 agent 完成。它只认 `SSH_AUTH_SOCK`，不认 `IdentityAgent`，所以 1Password 这类 agent 要另设 `gpg.ssh.program`（未实测）。
- **凭据助手**：用一个 https 远端，`credential.helper=osxkeychain` 并已存好凭据，在界面上推送应当成功；第一次访问时钥匙串可能弹出授权框。
  - 没有存凭据时，因为 `GIT_TERMINAL_PROMPT=0`，git 报 terminal prompts disabled，得到 `auth_failed`。
  - 指引：先在终端里推送一次存下凭据，或者用 `gh auth setup-git`。
- **依赖 PATH 的 hook**：可信 PATH 取自登录 shell 的 PATH，但去掉了项目、工作区和临时目录里的条目。direnv 这类工具往项目目录里加的 PATH 条目，hook 看不到。husky 自己会加 `node_modules/.bin`，不受影响。

## 实际规模

`cargo xtask size`：第一次提交时 +525 −36（不含 docs、生成物），编排者接受，硬停止线仍为 600。按修改意见改完后为 +557 −52：quotepath 移位约 +11，remote 名改读 `%(upstream:remotename)` 约 +16，带 `/` 的 remote 用例约 +18。

| 模块 | 估计 | 实际 |
|---|---|---|
| 用户档类型（cmd.rs、mod.rs 的实现部分） | 55 | 71 |
| `git/commit`、`git/push`、写路径（service.rs） | 90 | 118 |
| 错误分类（user.rs 的实现部分） | 30 | 51 |
| 确认参数（`repo_config_changed`：协议字段、actor、load、manager、SessionNotes） | 30 | 81 |
| 协议的 reason、x-errors、接线、时长 | 8 | 18 |
| 单元用例（user.rs 34、mod.rs 11、协议 3） | 50 | 48 |
| IT-git-commit-push（含 hunk_tests 的 10 行可见性改动） | 130 | 138 |
| 合计 | 约 430 | 525 |

超出的主要来源：

- 确认参数按决定 1 要求在 actor 里维护，并且重启后能从日志恢复，比估计多约 50 行。
- `GitTool::prepare` 为了让用户档不带覆盖，加了 `fixed` 参数，签名被 rustfmt 展开。
- `settle`、`refuse` 这两个票据收尾的小函数。
- x-errors 的补充。
