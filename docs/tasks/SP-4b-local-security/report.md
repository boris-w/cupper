# SP-4b 汇报

- 执行者：子 agent（安全工程师）
- 分支：`spike/SP-4b-local-security`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-a6465afcfb0257e18`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

> **请先看这条：** `codex exec` 在 `~/.codex/config.toml` 里写入了一个信任项，违反了"不修改 `~/.codex`"的约束。我删掉了这一项，但留下一个多余的空行。之后再碰这个文件的命令都被 auto mode 拦截了，所以我没有继续处理。详见遗留问题第 1 条。

## 做了什么

代码在 `spikes/SP-4b-local-security/`，可以重复运行：
- Rust crate 含 4 个二进制：`peer-server`、`peer-client`、`bundle-host`、`lineage-host`；
- `scripts/` 下按项编号的 5 个脚本，另有脱敏、汇总、Codex 信任项清理三个工具脚本；
- 脱敏后的输出在 `results/`，README 写明用法和副作用；
- 过程记录在 `docs/tasks/SP-4b-local-security/notes.md`。

实测环境：macOS 26.4 arm64（没有 Rosetta，Developer mode 关闭），Claude Code 2.1.296，Codex CLI 0.159.2。本机有两个 Apple Development 身份，分属两个 Team，没有 Developer ID。

## 关键取舍

- **Security 框架自己写 FFI。** `security-framework-sys` 缺 `SecCodeCopySigningInformation`、`…WithErrors` 和 `kSecCodeInfo*`，所以自己声明，约 60 行。这印证了 ADR-05 "几十行 Security 代码"的估计。
- **第 6 项只观察，不做注入。** brief 允许只读分析。这一项只记录签名标志和 entitlement，没有写 dylib 注入或 `task_for_pid` 探针。
- **Codex 真实会话没有重试。** 原因见遗留问题第 1、2 条。Codex 的结论来自它自带的 `codex sandbox` 执行器：这个执行器用同一套 Seatbelt 策略，但不调用模型。
- **血缘检查测了私有 SPI。** 试了"负责进程"这个信号（`responsibility_*`，用 dlsym 取得），产品里用不用另行决定。

## 验证方式（逐项结论）

### 第 1、2 项：对端签名校验（`scripts/01-peer-verify.sh`，结果见 `results/01-summary.txt`）

**校验流程**：`getsockopt(LOCAL_PEERTOKEN)` → `SecCodeCopyGuestWithAttributes(kSecGuestAttributeAudit)` → `SecCodeCheckValidityWithErrors`，再检查动态状态字 `kSecCodeInfoStatus`：必须有 `CS_RUNTIME`，不能有 `CS_GET_TASK_ALLOW` 和 `CS_DEBUGGED`。

**带 Team ID 的要求**：`anchor apple generic and identifier "dev.cupper.spike.ui" and certificate leaf[subject.OU] = "<TEAM_A>"`

| 客户端 | 结果 |
|---|---|
| Team A，UI 的 identifier，开 hardened runtime | **接受** |
| 其他 identifier（cli、`.helper`） | 拒绝，-67050 |
| 同 identifier、Team B | 拒绝，-67050 |
| 同 identifier 的 ad-hoc 签名 | 拒绝，-67050 |
| linker 签名 | 拒绝，-67050 |
| 没开 hardened runtime | 拒绝（标志检查） |
| 带 get-task-allow | 拒绝（标志检查） |
| node、/usr/bin/python3、nc | 拒绝，-67050 |
| 未签名的 arm64 二进制 | 根本跑不起来，被 SIGKILL（137） |

**其他要求写法**：
- 加上 Developer ID 的标记 OID 后，Apple Development 签名的客户端全部被拒。正向通过没法测，本机没有 Developer ID。
- 只写 identifier：ad-hoc 和别的 Team 都能通过，这种写法没用。
- 按 cdhash 固定：只有那一个二进制能通过。
- `kSecCSMatchGuestRequirementInKernel`（1<<23）：对所有客户端都返回 -67052，不能用。

**耗时**：20 次取样，中位数约 0.85–0.93 ms，最大约 3.5 ms，远低于握手预算 100 ms。

**反方向（UI 校验宿主）**：
- 签名正确的宿主能通过。
- 同一用户可以在 0700 目录里 unlink 套接字，再绑一个假宿主。校验宿主的客户端会拒绝它（-67050，退出码 3）；不校验的客户端会照常和假宿主通信。

**边界情况，以及 EPERM 何时出现、怎么处理**：
- **EPERM 没有复现。** 让校验方在 `sandbox-exec (deny process-info-codesignature)` 下运行，校验照样成功；`(deny process-info*)` 会让校验方自己启动就崩溃。
- **实际观察到的错误：**
  - 对端已经退出：`getsockopt` 返回 ENOTCONN（57）；
  - 令牌过期，即对端 exec 过：-67065 errSecCSNoSuchCode；
  - 磁盘上的代码被删或被替换：100002 ENOENT，或 -67034 errSecCSStaticCodeChanged（见第 3 项）。
- **处理原则**：任何错误都按拒绝处理，把错误码分成"对端已退出 / 对端已 exec / 磁盘代码已变（多半是正在升级）/ 其他"，记日志，UI 提示用语义码。
- **最重要的发现（G2–G4）**：`LOCAL_PEERTOKEN` 和 `LOCAL_PEERPID` 返回的是*最近一次在这个套接字上做 I/O 的进程*的*当前*映像，不是 connect 时的身份。所以：
  - 连接方先写入、再 exec 成正版二进制，宿主事后校验会接受；
  - fork 出来、共享同一个 fd 的另一个进程，可以借一条只校验过一次的连接继续通信。
- **修复已验证（P1–P4，`--pin` 模式）**：
  - accept 时就取令牌并校验；
  - 发一个 nonce，第一条消息必须回显它；
  - 之后每条消息的令牌（含 pidversion）必须与 accept 时完全相同，不同就断开。

  P2、P3 被拒；P4 中，正版客户端把 fd 漏给 exec 出来的子进程，子进程的消息因令牌变化被拒。

### 第 3 项：替换整个 .app（`scripts/03-upgrade.sh`，结果见 `results/03-upgrade.txt`）

做了 v1/v2 两个 `Test.app`，里面各嵌一个 `Contents/Helpers/TestHost.app`，从内到外签名。

| 替换方式 | 旧宿主还活着吗 | 新客户端校验旧宿主 | 宿主自查 / 拉起前比对 cdhash |
|---|---|---|---|
| S1 先放新副本，旧包移走后删除（Sparkle 式） | 活着 | 失败：100002 ENOENT | 自查发现被替换；拉起前比对发现 cdhash 变了，转为 pending_upgrade |
| S2 同上，但旧包移到别处、不删除 | 活着 | **通过** | 同上 |
| S3 用 `ditto` 原地覆盖（会生成新 inode） | 活着 | 失败：**-67034 errSecCSStaticCodeChanged** | 同上 |
| S4 用 `cp` 覆盖正在运行的可执行文件（inode 不变） | **被 SIGKILL** | — | — |
| S5 旧宿主退出后，从新包拉起新宿主 | — | 通过 | 一切正常 |

另外，替换之后旧宿主读包内资源，读到的是**新版本**的内容，会出现新旧版本混用。

### 第 4 项：agent 沙箱能否挡住套接字（`scripts/04-agent-sandbox.sh`，结果见 `results/04-*`）

套接字放了三个位置：`$TMPDIR` 下、agent 的工作区里、`DARWIN_USER_CACHE_DIR` 下。判断以服务端日志为准。

**Claude Code**：开沙箱，权限模式为 default，模型 haiku，以 `-p` 运行，并加了 `--permission-prompts none` 和 `--setting-sources project,local`。
- 6 次连接尝试全部返回 EPERM，服务端一次连接都没收到。
- agent 没有尝试在沙箱外重试；整个过程 2 个回合。

**Codex 沙箱执行器（不调用模型）**：

| 模式 | 结果 |
|---|---|
| read-only | 全部被挡 |
| workspace-write（不开网络） | 全部被挡，包括工作区里的那个套接字 |
| workspace-write，并设 `sandbox_workspace_write.network_access=true` | **6 次全部连上** |
| workspace-write，加 `--allow-unix-socket <目录>` | 只有那个目录里的套接字能连上 |

开网络时全部放行，与 Codex 二进制内置策略的这一行一致：`(allow network-outbound (remote unix-socket))`。

**Codex 真实会话**：三种模式都因 gateway 返回 HTTP 429 失败（模型 gpt-5.4-mini），一个命令都没执行。在 0.159.2 里，"Default" 预设就等同于 Agent 模式，即 workspace-write；两者的区别只在于能否申请提权，而这一点没测到。

### 第 5 项：血缘检查（`scripts/05-lineage.sh`，结果见 `results/05-lineage.txt`）

宿主用 `POSIX_SPAWN_SETSID` 拉起一个充当适配器的进程，再按 4 个信号给每个连进来的对端打分：

| 逃逸方式 | 父进程链 | 进程组 | 会话 | 负责进程（disclaim 后） |
|---|---|---|---|---|
| 直接子进程、适配器的后台子进程 | ✔ | ✔ | ✔ | ✔ |
| 父进程退出后的孤儿、nohup、双重 fork、bash disown、zsh `&!` | ✘ | ✔ | ✔ | ✔ |
| fork 后 setsid | ✘ | ✘ | ✘ | ✔ |
| 无关的对照进程 | ✘ | ✘ | ✘ | ✘ |

- 不做 disclaim 时，"负责进程"这个信号对无关进程也报"同源"，这是误报，不能用。
- `launchctl submit` 没做，只分析：
  - 由 launchd 拉起的任务，上面 4 个信号全部失效；
  - `open`（经 LaunchServices）、cron/at、事先已在运行的长期进程（tmux server、ssh localhost 等）同样失效。
- 结论：进程组加会话 ID 能挡住常见逃逸，但挡不住 setsid；负责进程信号能挡住所有本地 fork 型逃逸，但要用私有 SPI，还会改变 TCC 归属；所有信号都挡不住经 launchd 或 LaunchServices 中转的路径。

### 第 6 项：其他注入面（`scripts/06-surfaces.sh`，结果见 `results/06-surfaces.txt`）

- **同一用户的进程能看到我们进程的 argv 和环境变量**（`ps -E`、`ps eww`），不管有没有开 hardened runtime。这一条**纠正**了讨论会上"看不到环境变量"的记录。在 Codex 只读沙箱里，`/bin/ps` 本身执行不了；走 sysctl 的路径没测。
- 我们开了 hardened runtime 的二进制：没有任何例外 entitlement，内核状态字是 `hard+kill+runtime`，没有 get-task-allow。按 Apple 的 Hardened Runtime 文档和 `man taskgated`（分析，没做注入）：
  - `DYLD_*` 环境变量会被忽略，库校验开启；
  - 没有 get-task-allow 就不能调试附加；
  - `task_for_pid` 只对带 debugger entitlement 的调用方开放，而且要管理员授权。
- 本机官方 Node 二进制虽然开了 runtime，但带着 `allow-dyld-environment-variables`、`disable-library-validation`、`allow-jit`、`allow-unsigned-executable-memory`、`disable-executable-page-protection`、`get-task-allow`。也就是说，同一用户的进程可以注入任何跑在 Node 里的东西。

### 签名方式的附带发现

`codesign` 用本机两个身份签名都是瞬间完成，没有任何弹窗。这说明同一用户、在沙箱之外运行的进程（包括 agent）可以用开发者自己的 Team ID 签任意代码。所以对自行构建的版本来说，"带 Team ID 的要求"在这台机器上可以伪造。

### 验收标准对照

- [x] 第 1 至 6 项各有实测结论，附命令和输出摘要（同上；可直接运行 `results/` 和 `scripts/` 复现）。
- [x] 列出了审计令牌校验的边界情况，以及 EPERM 的情况和处理方式（同上）。
- [x] 对 ADR-05 和 ADR-01 的结论（见下）。
- [x] "安全边界说明"草稿（见下）。

## 对 ADR-05 的结论

**方案 1 成立，不需要切换到 XPC。** EPERM 没有复现；所有失败都能解释，而且可以统一按拒绝处理。XPC 原生的优势是逐条消息带审计令牌，下面第 1 条的固定令牌握手可以达到同样效果。但需要把以下几条写进 ADR：

1. **必须做固定令牌握手**：accept 时取令牌并校验 → 发 nonce 挑战 → 每条消息都比对令牌，含 pidversion，变了就断开。只在连接开始时校验一次是**不安全的**（G2–G4）。
2. **发布构建的要求**加上 Developer ID 的标记 OID：`… and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists`。标志检查用动态状态字 `kSecCodeInfoStatus`：必须有 `CS_RUNTIME`，不能有 `CS_GET_TASK_ALLOW` 和 `CS_DEBUGGED`。
3. **退化模式**：
   - 只写 identifier 的要求没用；
   - 在开发机上，带本机 Team ID 的要求可以被不在沙箱里的 agent 伪造；
   - 所以开发构建和自行构建必须用继承 fd 传一次性密钥（与 ADR 现有写法一致），并明确提示"本地鉴权较弱"。
4. **UI 侧**：连接宿主的 fd 必须设 CLOEXEC，不能传给子进程；宿主应拒绝 SCM_RIGHTS（这一条没测）。
5. "UI 校验宿主"已验证，能挡住抢占套接字路径。
6. "沙箱开着时操作系统层面就挡住"这句话要加限定：
   - Codex 只在**没开网络访问**时才挡；
   - Claude Code 在没配置 `allowUnixSockets` 时才挡（这一条按文档，没实测放行的情况）；
   - 用户批准的沙箱外命令或提权命令不受沙箱限制，这时第二层的签名校验才起作用。

## 对 ADR-01 升级流程的结论和修改建议

- **"宿主退出之后再安装"是必须的。** 替换之后，新 UI 基本无法校验旧宿主（ENOENT 或 -67034），只有旧包还留在别处时例外。现在的流程是对的，这次实测给出了证据。
- **新增"宿主自检"**：每次有新客户端连接、每次拉起子进程之前，都比较运行中的 cdhash（csops）和磁盘上包的 cdhash。不一致就进入"待升级"：不再新建会话，空闲时退出。这一条在 S1–S3 三种替换方式下都有效，可以覆盖"用户手动拖入新版本"的情况。
- **新 UI 校验宿主时遇到 -67034 或 ENOENT**，就当作"旧宿主等待退出"：不发命令，显示等待状态，提供"重启宿主"。
- **安装器绝不能原地覆盖可执行文件**（S4 会导致 SIGKILL）。需要用真实的 electron-updater / Squirrel.Mac 确认它们是"写新文件再改名"。
- "运行中不读取包内资源"必须严格执行，范围包括 Info.plist 和嵌套的 helper（实测出现了版本混用）。
- "把宿主拷到包外"这个后备方案**不需要**：旧宿主照常运行，拉起前比对 cdhash 也有效。
- `kSecCSMatchGuestRequirementInKernel` 不能用来绕开磁盘校验。

## "安全边界说明"草稿（SEC-8）

**SEC-8 能做到的：**
- **agent 沙箱开着时，agent 执行的命令连不上宿主。**
  - 实测：Claude Code 开沙箱、默认权限；Codex 的只读和工作区模式（不开网络）。
  - 前提：Codex 没有打开网络访问（打开后会放行所有 Unix 套接字），Claude Code 没有配置 `allowUnixSockets`。
- **不在沙箱里的命令（用户批准过的，或 MCP、hooks）能连上，但只拿到只读权限。**
  - 宿主按代码签名认出 UI，其他进程一律不是 UI；
  - 写操作都要用户在 UI 上确认，审批类权限要单独申请；
  - 能确认是适配器后代的申请方（同一进程组或会话，例如 nohup、双重 fork、disown）直接拒绝。
- **冒充宿主会被 UI 识破。**
- **认证不依赖可被偷走的静态凭据**，每次连接都记审计日志。

**SEC-8 做不到的：**
- 挡不住一个同用户、在沙箱外执行任意代码、并且有明确恶意的进程。它可以：
  - 改 agent 自己的权限配置，改 cupper 的数据目录；
  - 杀掉宿主；
  - 经 launchd、`open` 或已在运行的长期进程（如 tmux）中转来脱离血缘检查；
  - 在用户确认框上做社会工程。
- 同一用户能看到 cupper 进程的命令行参数和环境变量，所以 cupper 不在其中放任何秘密。
- 开发构建和自行构建的本地鉴权较弱：本机的签名身份可以被同用户进程直接拿来签代码。
- 建议用户开启 agent 自带的沙箱，并且不要给 Codex 的工作区模式打开网络访问。

## 提交列表

```
cab6846 docs(spike): add SP-4b README and process notes
534a7dd test(spike): record argv/env visibility and hardening flags
6e8823b test(spike): measure lineage signals against detach techniques
b9ed457 test(spike): probe whether agent sandboxes block the host socket
7af4680 test(spike): rehearse whole-bundle replacement under a running helper
f5b7e6b test(spike): verify UDS peers by audit token and code signature
<编排者存档提交>
```

提交前扫描过整个分支 diff 和提交信息：Team ID、身份哈希、用户名、主机名、`$TMPDIR`、`$HOME`、邮箱、claude.ai 链接都没有出现。

## 本机留下的临时目录和会话记录

- `$TMPDIR/sp4b-run/`（约 52 MB）：构建产物、签名副本、套接字、未脱敏的原始日志，可以删除。
- scratchpad 下的 `sp4b/`：`ids.env`（两个签名身份的 SHA-1）、`run.sh`、`leakscan.sh`、`footprint.sh`。含身份名称和导出证书的文件已经删除。
- `$DARWIN_USER_CACHE_DIR/sp4b-agent.*`：脚本结束时已删除。
- Claude Code：
  - 留下 `~/.claude/projects/-private-<TMPDIR 编码>-sp4b-run-agent-repo/memory/`，是空目录；因为用了 `--no-session-persistence`，没有会话文件；
  - `~/.claude.json` 被更新（状态文件）。
- Codex：
  - 用了 `--ephemeral`，没有会话文件；共享的 state/log sqlite 文件被更新；
  - config.toml 的情况见遗留问题第 1 条。
- 没有残留进程。

## 遗留问题 / 范围外发现

1. **Codex 修改了用户配置。** 第一次 `codex exec` 往 `~/.codex/config.toml` 写入了 `[projects."<临时仓库>"] trust_level = "trusted"`，`--ephemeral` 挡不住。
   - 我删掉了这个块，但第一版清理脚本在 `[shell_environment_policy.set]` 之前多留了一个空行，文件哈希和运行前不一致。之后的修正，以及只读核对哈希的命令，都被 auto mode 拦截了，所以我没有再碰这个文件。
   - TOML 语义与运行前相同。是否删掉这个空行，请用户决定。
2. **Codex 真实会话还没跑成。** 三种模式都遇到 429，而且每次运行都会写信任项。需要用户决定怎么做，例如用单独的 `CODEX_HOME`，或者接受写入后再清理，然后再补测。Agent 模式下的提权行为也没测到。
3. 没有在 Developer ID 证书上做正向验证，可以放到 CI 上做。
4. 没有在 macOS 14、15（最低支持版本）上复测：`LOCAL_PEERTOKEN` 的"最后一个 I/O 进程"语义、csops、各种错误码。
5. 论坛上的 EPERM 案例没有复现，原因还不清楚。
6. 宿主还要拒绝 SCM_RIGHTS 传 fd（没测）。另外，Electron 主进程和 helper 的身份、`ELECTRON_RUN_AS_NODE` 要等 ADR-03。
7. 需要用真实适配器的进程树（claude-agent-acp、codex-acp）确认 agent 拉起工具命令时会不会自己 setsid、新建进程组或 disclaim，这会影响血缘信号。可以和 SP-1、SP-4a 一起做。
8. 是否使用私有 SPI `responsibility_*`，以及它对 TCC 归属的影响，留给 SP-10。
9. 校验大型 Electron 包的耗时、electron-updater 实际怎样替换包，都还没测。
10. 给 ADR-08 的提示：官方 Node 二进制带着去掉注入防护的 entitlement。如果我们重新签名并打包 Node，要决定保留哪些例外；Node 里永远不能跑可信代码。
11. 范围外：本机两个开发身份对 `codesign` 都是"始终允许"，这台开发机上任何不在沙箱里的进程都能用它们签代码。建议在贡献者文档里提一句。
