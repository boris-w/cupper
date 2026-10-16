# SP-4b macOS 本地安全实测

原型实测代码，只用于验证 ADR-05（本地传输与认证）和 ADR-01（升级流程）里标为"推"的假设，不是产品代码。任务说明见 `docs/tasks/SP-4b-local-security/brief.md`。

实测环境：macOS 26.4（arm64），Rust 1.96，Xcode 26.5 SDK，Claude Code 2.1.296，Codex CLI 0.159.2。

## 目录

```
rust/                    一个 crate，四个二进制
  src/lib.rs             LOCAL_PEERTOKEN / LOCAL_PEERPID、SecCode 校验、签名信息、csops、libproc 血缘
  src/bin/peer-server    UDS 服务端：按审计令牌校验对端签名（第 1、2、4 项）
  src/bin/peer-client    测试客户端：可选地反向校验服务端，含 exec / fork / 继承 fd 等边界用例
  src/bin/bundle-host    升级演练里的 helper 宿主（第 3 项）
  src/bin/lineage-host   血缘检查宿主（第 5 项）
scripts/
  common.sh              公共设置（工作目录、构建、签名、脱敏）
  01-peer-verify.sh      第 1、2 项
  03-upgrade.sh          第 3 项
  04-agent-sandbox.sh    第 4 项
  05-lineage.sh          第 5 项
  06-surfaces.sh         第 6 项（只观察，不做注入）
  sanitize.py            写入 results/ 之前去掉本机路径、用户名、主机名、签名身份、Team ID
  summarize.py / agent_summary.py   汇总日志
  codex_untrust.py       删除 `codex exec` 写进 ~/.codex/config.toml 的临时仓库信任项（见下文）
results/                 已脱敏的输出，文件名前缀对应各项
```

## 前置条件

- Rust 工具链、Xcode 命令行工具；第 1 项的客户端还用到本机的 `node`、`/usr/bin/python3`、`nc`。
- 本机已有的代码签名身份（只读使用，不创建、不导入、不删除证书）。用 `security find-identity -v -p codesigning` 查到 SHA-1，通过环境变量传入：
  - `SP4B_ID_A`：作为"我们"的签名身份（必需，第 1、3、6 项）；
  - `SP4B_ID_B`：另一个 Team 的身份（可选，用于"同 identifier、不同 Team"的用例）。
- 第 4 项的 C 部分需要已登录的 `claude` 和 `codex`。

## 环境变量

| 变量 | 作用 |
|---|---|
| `SP4B_WORK` | 构建、签名副本、套接字和日志所在的临时目录。不设置时在 `$TMPDIR` 下 `mktemp -d`。路径要短，套接字路径不能超过 104 字节 |
| `SP4B_ID_A` / `SP4B_ID_B` | 签名身份的 SHA-1 |
| `SP4B_RUN_AGENTS=1` | 第 4 项真的启动 Claude Code 和 Codex 各跑一个回合（默认不跑） |
| `SP4B_CODEX_UNTRUST=1` | 第 4 项跑完后用 `codex_untrust.py` 删除 Codex 写入的信任项 |

## 运行

在仓库根目录执行，每个脚本都可以单独重复运行：

```sh
export SP4B_ID_A=<sha1> SP4B_ID_B=<sha1>
sh spikes/SP-4b-local-security/scripts/01-peer-verify.sh
sh spikes/SP-4b-local-security/scripts/03-upgrade.sh
sh spikes/SP-4b-local-security/scripts/04-agent-sandbox.sh          # 只跑不调用模型的 B 部分
SP4B_RUN_AGENTS=1 sh spikes/SP-4b-local-security/scripts/04-agent-sandbox.sh
sh spikes/SP-4b-local-security/scripts/05-lineage.sh
sh spikes/SP-4b-local-security/scripts/06-surfaces.sh
```

原始日志留在 `$SP4B_WORK/logs/`，脱敏后的副本写入 `results/`。

## 副作用（运行前请知悉）

- 签名：`codesign` 会用钥匙串里的私钥签名测试二进制。本机实测没有弹窗，说明这些身份的私钥对 `codesign` 已是"始终允许"——这本身也是一条安全发现，见汇报。
- 第 4 项 C 部分：
  - Claude Code 用 `--no-session-persistence`，不留会话文件，但会在 `~/.claude/projects/` 下建一个以临时仓库路径命名的空目录，并更新 `~/.claude.json`。
  - `codex exec`（0.159.2）会把 `[projects."<临时仓库>"] trust_level = "trusted"` 写进 `~/.codex/config.toml`，`--ephemeral` 挡不住。
- 第 4 项会在 `$DARWIN_USER_CACHE_DIR` 下建一个 `sp4b-agent.*` 临时目录放第三个套接字，脚本结束时删除。

## 没有做的事

- 不执行 `launchctl submit`，不注册 LaunchAgent / SMAppService，不做会触发 TCC 的操作，不测通知（留给 SP-10）。
- 第 6 项不做 `DYLD_INSERT_LIBRARIES` 注入或 `task_for_pid` 调用，只记录决定这些行为的签名标志和 entitlement。
- 不做 Electron 相关检查（fuses、helper identifier），等 ADR-03。
- 没有 Developer ID 证书：带 Developer ID 标记 OID 的要求只验证了"Apple Development 证书会被拒绝"，没有验证正向通过。
