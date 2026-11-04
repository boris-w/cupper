# T123 汇报

- 执行者：子 agent（Rust 工程师，宿主 Git 服务）
- 分支：`feat/T123-git-commit-push`
- 完成日期：2026-11-04
- PR：https://github.com/boris-w/cupper/pull/63
- CI 运行：https://github.com/boris-w/cupper/actions/runs/41763342554 。8 个检查在 `d58de3e` 上全部通过。

## 连跑 IT-git-commit-push

没有连跑过 10 次，也没有在 runner 上连跑。开发中在本机单份顺序跑过约 8 次：单独跑过，也随整套宿主测试跑过，全部通过。

请你触发 test-repeat，参数如下：

| 参数 | 值 |
|---|---|
| kind | `cargo` |
| package | `cupper-host` |
| args | `--lib` |
| filter | `it_git_commit_push_runs_as_the_user_does` |
| times | `10` |
| parallel | `1` |
| runner | `macos-15` |

完整的测试路径是 `orchestrator::session::commit_tests::it_git_commit_push_runs_as_the_user_does`，不需要额外的 feature。

## 做了什么

**`git/commit` 和 `git/push`（用户档，设计 5.2.10）**
- 新增用户档：`GitCmd<User>`、`UserOp::{Commit, Push}`、`GitTool::user_tier`。
- 环境与仓库档相同：可信 PATH、`HOME`、`SSH_AUTH_SOCK`、`GIT_TERMINAL_PROMPT=0`、`LC_ALL=C`。
- 不带任何 `-c` 覆盖，参数只有 `--no-pager -C <repo>`，所以用户的 hooks、签名和凭据助手照常运行。
- `commit -F -` 的 stdin 是提交信息，`push` 的 stdin 是 `/dev/null`。

**写路径**
- 提交拿 `RepoLock`，推送不拿；两者执行前都先写 `pending_external`（FULL fsync）。
- git 正常退出但失败时，拒绝命令。
- git 超时被杀（进程组）或者没能启动时，丢弃票据，回 `conflict/outcome_unknown`，与崩溃后的重试一致。

**提交**
- 提交成功后经仓库档读 HEAD，作为 `oid`。

**推送**
- remote 取 HEAD 所在分支的 `%(upstream:remotename)`，没有上游时用 `origin`。
- 带 `set_upstream` 时执行 `push --set-upstream <remote> HEAD`，否则执行不带参数的 `git push`。
- 结果为 `{remote, branch}`。

**失败分类（`git/user.rs`）**
- 读 `LC_ALL=C` 下 git 的英文输出和退出码，分出 `lock_held`、`nothing_to_commit`、`signing_failed`、`hook_failed`、`rejected`、`auth_failed`；其余回不带 reason 的 `git_error`。
- stderr 末尾 2048 字节经 `redact::credentials` 脱敏后放在 `data.stderr`。

**指纹变化的确认参数（方案 B）**
- `git/status` 的结果附加可选字段 `repo_config_changed: [path]`，宿主不强制确认。
- 会话 actor 维护一个集合，从它写入的 `repo_config_changed` notice 里补充；`git/status` 经 `SessionNotes::repo_config_changed` 向 actor 的邮箱取，不查数据库。
- 加载时从 `turn_snapshots.ops` 和未修剪的 `events` 重建这个集合，只解析含这个字样的行，所以宿主重启后也在。
- 范围是本会话期间，提交之后不清空。

**协议（只做了附加）**
- 新增 `ErrorReason::signing_failed`，归在 `git_error` 下。
- 新增 `GitStatusResult.repo_config_changed`。
- 补了两项 x-errors。
- 跑过 `pnpm gen`，生成物已提交。

**其他**
- 登记了 `git_commit` 时长（300 秒）。
- 加了路由，删了 dispatch 测试里 `git/commit`、`git/push` 两行 Later(M1b)。

**用例**
- `unit_git_user_failures_read_from_what_git_said`：表驱动，11 行。
- `unit_git_cmd_tiers_pass_only_their_environment_and_fixed_arguments`：加了用户档的环境、参数和 stdin 断言。
- `unit_git_status_head_reads_the_upstream_and_how_far_apart`：加了 remote 字段。
- IT-git-commit-push：见"验证方式"。

## 关键取舍

**超时：选 `outcome_unknown`，不比对 HEAD。**
- 推送没有本地可比对的状态。
- 提交时 hook（如 post-commit）也可能移动 HEAD，比对并不可靠。
- 这样处理和 T121 的写路径、崩溃恢复是同一套语义。

**读 `oid` 失败时：票据留在 Unknown。**
- 提交已经落地，只是回不出 `oid`，重试会得到 `outcome_unknown`，客户端刷新状态即可。

**签名失败的识别**
- 认 "failed to sign the data"（gpg、x509）、"ssh-keygen"（ssh 签名）、"user.signingkey" 三种措辞，排在 hook 判断（退出码 1）之前。

**remote 名的读取**
- 加在 `head()` 原有的那条仓库档 `for-each-ref refs/heads/<分支>` 的格式里，不多跑一次 git。

**确认参数的跟踪**
- 每次 `git/status` 都要用，所以放在 actor 的内存里。
- 重启后的恢复只在加载时读一次库；先用 `instr` 在 SQL 里筛行，避免解析整条日志。

**规模**
- 估计约 430，第一次提交 +525。超了停止线，已经停下汇报，你接受了，硬停止线仍是 600。
- 按两条修改意见改完后为 +557 −52（`cargo xtask size`，不含 docs 和生成物）。
- 超出主要在确认参数：按决定 1 在 actor 里维护并且能从日志恢复，估 30，实际约 81。
- rustfmt 的展开用"多行 git 命令文本 + 短局部变量"压掉了约 90 行。

## 验证方式

**本机（单份）**
- `cargo test --workspace --all-features`：905 个通过，0 失败。
- clippy，带和不带 `--all-features` 两种，都开 `-D warnings`。
- `cargo fmt --all --check`。
- `cargo xtask ci protocol`：通过，schema-compat 0 breaking、3 additive。其中 `project_untrusted` 一条来自 main 上已有的改动。
- `cargo xtask ci testdata-scan`：通过，含 ids 检查。

**GitHub CI**
- PR #63 在 `d58de3e` 上 8 个检查全部通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。

**对照验收标准**

1. IT-git-commit-push 通过。只用临时目录里的仓库和本地裸仓库，没有任何外发；ssh 远端和 gpg 都是脚本。覆盖的情形：
   - 没有可提交的内容，得到 `nothing_to_commit`。
   - 含中文的提交信息原样落地，核对了 `git log -1 --format=%H%n%B`。
   - pre-commit hook 拒绝，得到 `hook_failed`；hook 输出里的 `user:hunter2@` 在 `data.stderr` 中被脱敏。
   - 假的 `gpg.program` 失败，得到 `signing_failed`。
   - 带 `--set-upstream` 推送后，`git/status` 显示上游 origin/main、ahead 0。
   - remote 改名为 `my/fork` 后带 `set_upstream` 推送，得到 `{remote: "my/fork", branch: "main"}`。
   - 远端已有别人的提交，推送得到 `rejected`。
   - 假的 `core.sshCommand` 拒绝密钥，得到 `auth_failed`。
   - 回合结束后，`git/status.repo_config_changed` 含项目的 `.git/config`；重启宿主后仍含。
2. Q23 的结论：
   - 自动验证了三项：
     - `SSH_AUTH_SOCK` 传进用户档且不带覆盖（单元用例）。
     - 签名失败归到 `signing_failed`。
     - ssh 认证失败归到 `auth_failed`。
   - 写成作者可选的手动验证说明（在 notes 里）：
     - SSH agent：只在 shell 配置里 export 的，从 GUI 启动时看不到；改用 `IdentityAgent` 或 `launchctl setenv`。
     - GPG：终端版 pinentry 在没有终端时失败，改用 pinentry-mac。
     - 钥匙串凭据助手：没有存好凭据时，因为 `GIT_TERMINAL_PROMPT=0`，得到 `auth_failed`。
     - ssh 签名只认 `SSH_AUTH_SOCK`。
     - direnv 往项目目录里加的 PATH 条目不在可信 PATH 里。
3. `schema-compat` 0 breaking：是。
4. PR 上 8 个检查全过：是，链接见开头。

**LC_ALL=C**：没有发现它让常见 hook 出问题的情形。中文提交信息实测正常。Python hook 按 PEP 540 在 C locale 下会进入 UTF-8 模式，这一点没有用真实框架实测。

## 设计偏差 / 补充（请合并时写进设计）

1. **5.3.4 `git/status`**：结果附加可选字段 `repo_config_changed?: [path]`。
   - 取本会话 `repo_config_changed` notice 列出的文件，提交后不清空，宿主重启后从日志恢复。
   - 宿主不强制确认（决定 1，方案 B）。
2. **5.3.4 ErrorReason**：新增 `signing_failed`，归在 `git_error` 下；gpg、ssh、x509 三种签名失败都归到它（决定 2）。
3. **5.3.4 x-errors**：
   - `git/commit` 补 `git_error/signing_failed`。
   - `git/push` 补 `conflict/outcome_unknown`：推送也写 `pending_external`，超时或启动失败时回这个。
4. **5.2.10 时长**：登记 `git_commit`，300 秒；推送沿用已有的 `git_push`（决定 3）。
5. **5.2.10 锁**：提交拿 `RepoLock`，推送不拿，两者都写 `pending_external`（决定 4）。
6. **5.2.10 超时与失败的语义**：
   - 超时或启动失败时，结果为 Unknown，回 `conflict/outcome_unknown`，不比对 HEAD。
   - 提交成功但读 HEAD 失败时，结果同样为 Unknown。
7. **5.2.10 `GitCmd` 构造器**：原文写"固定加 `-c core.quotepath=false`"，现改为只在仓库档和快照档的固定覆盖里加；用户档不加，以免经 `GIT_CONFIG_PARAMETERS` 改变 hook 里 git 的输出（你说合并时改）。
8. **错误数据**：失败的 git 输出放在 `data.stderr`，为 stderr 末尾 2048 字节，凭据已脱敏。
   - message 为 "git commit failed" 或 "git push failed"。
   - 分类规则见 notes 里的表；提交时退出码 1 判为 `hook_failed`。
9. **推送的 remote 与 branch**：
   - remote 取 `%(upstream:remotename)`，没有上游时用 `origin`。
   - `branch` 是本地分支名，没有读 `%(upstream:remoteref)`；默认的 `push.default=simple` 下两者同名。
   - 不带 `set_upstream` 时执行不带参数的 `git push`，推到哪里由用户的 `push.default` 决定。
   - detached HEAD 时回不带 reason 的 `git_error`。
10. **`oid`**：提交后经仓库档读 HEAD 得到，复用 `head()`（`branch --show-current` 加 `for-each-ref`）。`Head` 新增了 `remote` 字段。
11. **"包含未暂存的更改"**：按 07-desktop 由界面分两步做（先 `git/stage` 再 `git/commit`），宿主没有对应参数，本任务没有改动。

## 提交列表

```
d58de3e fix(host): leave hooks git's own path quoting; read the push remote
8223341 feat(host): git/commit and git/push in the user tier
```

## 遗留问题 / 范围外发现

- IT-git-commit-push 的 10 次连跑还没做，请你按上面的参数触发 test-repeat。
- Q23 的手动验证需要作者自己的 SSH、GPG 和钥匙串配置，属于可选项，说明在 notes 里。
- 设计 02-host 5.2.10 里有关 `core.quotepath` 的那句需要随合并修改（第 7 条）。
- 界面（T130）需要按语义码给出这些指引：`signing_failed`（pinentry-mac）、`auth_failed`（agent 或凭据），以及没有上游时提示带 `set_upstream`。没有上游时直接推送，git 报 "has no upstream branch"，得到的是不带 reason 的 `git_error`。

worktree 请保留到后续轮次结束。
