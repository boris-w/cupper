# SP-4b 过程记录

- 执行者：子 agent（安全工程师）
- 日期：2026-10-16
- 代码与脱敏结果：`spikes/SP-4b-local-security/`

## 环境

- macOS 26.4（25E246），arm64，没有安装 Rosetta。Developer mode（DevToolsSecurity）关闭。
- 本机有两个 Apple Development 身份，分属两个 Team；没有 Developer ID 身份。下文用 `<TEAM_A>`、`<TEAM_B>` 指代。
- Claude Code 2.1.296，Codex CLI 0.159.2（npm 包，原生二进制在 `@openai/codex-darwin-arm64`）。

## 试错与取舍

1. **FFI 而不是 `security-framework` crate。** `security-framework-sys` 2.17 缺 `SecCodeCopySigningInformation`、`SecCodeCheckValidityWithErrors` 和 `kSecCodeInfo*`，干脆自己声明，约 60 行 `extern "C"`，配合 `core-foundation` 0.10。这也说明 ADR-05 估计的"几十行 Security 框架代码"是准确的。
2. **"未签名客户端"在 Apple Silicon 上跑不起来。** `codesign --remove-signature` 后的 arm64 二进制一执行就被 SIGKILL（退出码 137）。没有 Rosetta，无法测未签名的 x86_64。所以现实中的"未签名客户端"只能是由已签名解释器执行的脚本（node、python3、nc 都测了）。
3. **`kSecCSMatchGuestRequirementInKernel`（1<<23）在 26.4 上对我们的要求字符串返回 -67052（errSecCSReqInvalid）**，所有客户端都一样，包括本应通过的那个。推测它只接受某种特定形式的要求（可能是 LWCR），头文件没有说明，也没有标可用版本。本次结论：不能用它来绕开"磁盘代码被替换"的问题。
4. **`kSecCSDoNotValidateResources` 对动态代码对象非法**（-67070），只能用于静态代码。头文件对 `SecCodeCheckValidity` 的描述是"只校验确立身份所需的密封组件"，所以动态校验本来就不校验整个包的资源，大包不会明显变慢（本次只测了单个二进制，中位数约 0.9 ms）。
5. **`sandbox-exec` 的 `(deny process-info*)` 会让验证方自己启动就崩溃**（Trace/BPT trap），没有信息量，删掉了；`(deny process-info-codesignature)` 下校验照常成功，没能复现论坛上的 EPERM。
6. **exec 边界用例（G2–G4）是这次最重要的发现。** `LOCAL_PEERTOKEN` / `LOCAL_PEERPID` 返回的是"最近一次在这个套接字上做 I/O 的进程"的当前身份，而不是 connect 时的身份。连接方 exec 成另一个程序后，令牌变成新映像的。为此给服务端加了 `--pin` 模式（accept 时取令牌并校验 → 发 nonce → 之后每条消息的令牌必须与之完全相同），P1–P4 验证这个模式能挡住 G2–G4 那类情形。
7. **升级演练（第 3 项）最初有竞态**：上一个场景留下的套接字文件让 `wait_for_socket` 提前返回，后几个场景的"before"连不上。加了 `rm -f` 后重跑，结果稳定。
8. **血缘检查的"负责进程"信号**用的是私有 SPI（`responsibility_spawnattrs_setdisclaim` / `responsibility_get_pid_responsible_for_pid`，dlsym 取得）。不 disclaim 时它对无关进程也给出"同源"（误报），没有用；disclaim 后对所有逃逸方式都有效。是否在产品里使用私有 SPI 需要单独决定。
9. **第 6 项只做观察。** 原计划写注入用的 dylib 和 `task_for_pid` 探针，考虑到 brief 允许只读分析，改为记录签名标志和 entitlement，不做注入尝试。

## 第 4 项的意外副作用（需要用户知悉）

- Codex 三次 `codex exec` 都因 gateway 返回 HTTP 429 失败（模型 `gpt-5.4-mini`），一个命令都没执行。
- 但第一次运行（只读模式）仍然把 `[projects."<临时仓库路径>"] trust_level = "trusted"` 写进了 `~/.codex/config.toml`，`--ephemeral` 挡不住。这违反了"不修改 `~/.codex`"的约束。
- 处理：用 `scripts/codex_untrust.py` 删除了这个块（只删这一块）。但第一版脚本把块替换成了一个换行，而不是空串，于是在 `[shell_environment_policy.set]` 之前多留了一个空行，文件哈希和运行前不一致。之后修正这个空行、以及只读核对哈希的命令都被 Claude Code 的 auto mode 拦截，所以**没有再碰这个文件**。TOML 语义与运行前相同，多出的空行请用户自行决定是否处理。脚本已修正为替换成空串。
- 为了不再写入用户配置，没有重试 Codex 的真实会话。Codex 三种模式的结论来自它自己的沙箱执行器 `codex sandbox`（不调用模型，同一套 Seatbelt 策略）。
- Claude Code 那次运行留下了 `~/.claude/projects/-private-<tmp>-sp4b-run-agent-repo/memory/` 空目录，没有会话文件。

## 本机临时目录

- `$TMPDIR/sp4b-run/`：构建产物、签名副本、套接字、原始日志（未脱敏）。
- `$DARWIN_USER_CACHE_DIR/sp4b-agent.*`：第 4 项脚本结束时已删除。
