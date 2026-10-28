# MAN-compat-claude：Claude 兼容套件的本机运行记录

- 日期：2026-10-28
- 运行者：T066 子 agent，在维护者本机，经用户许可使用真实的 Claude CLI
- 命令：`cargo xtask compat --real --write`（提交 fcc8912）
- 报告：`testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json`

## 环境

| 项 | 值 |
|---|---|
| 适配器 | claude-agent-acp 0.91.0（锁定版本） |
| CLI | Claude Code 2.1.296（Homebrew cask，宿主经登录 shell 的 PATH 找到，判为 verified） |
| 运行模式 | `real_config`：用户自己的配置，运行前后对比 |
| 模型 | haiku，effort low；thinking 剧本用 sonnet，effort 取默认 |
| 权限模式 | 每个剧本显式指定：`default`，plan 剧本是 `plan`；只选 `allow_once`、`reject_once` |
| 宿主 | e2e 种类的 `CupperHost.app`，每个剧本一个宿主、一个本用户临时目录下 0700 的临时仓库 |

## 结果

| 剧本 | 结果 | 说明 |
|---|---|---|
| new-session、normal-turn、command-exit、approval、reject、question、plan、cancel、cancel-approval | 通过 | |
| thinking | 失败 | 没有 thought 条目：近期模型默认不显示思考文本，宿主没有请求显示（产品问题，另开任务） |
| steer | 不适用 | M1b 的功能 |
| crash-resume、host-restart、image | 通过 | |
| concurrent-two（Q8） | 通过 | 2 个会话各 3 个 prompt、3 轮，没有串话，用时 49 s |
| six-sessions | 通过 | 6 个会话并发，没有串话 |
| close-running（K21） | 通过 | 收尾时命令在跑，宿主退出后它和子进程都已退出 |
| resume-procs（Q43） | 通过 | 5 轮"杀 CLI 再发"，宿主后代进程数 6、5、5、5、6、5 |
| untrusted-project | 通过 | 未信任时只传 `settingSources: ["user"]`；项目的 hook、MCP 服务器、`apiKeyHelper` 都没有留下标记文件，`env > env.txt` 运行了（退出码 0），其中没有项目 `env` 的变量。hook 一项有阳性对照：config-scope 里同样写法的 hook 在已信任的项目里运行了；MCP 和 `apiKeyHelper` 两项没有对照 |
| hooks-reload（Q28） | 观察 | 事先放的 hook 运行了；回合中改写的 hook 在同一回合内生效 |
| config-scope（Q34） | 观察 | 项目目录自己的 hook 运行了；git 根目录和它之外的都没有运行 |

- 每个剧本只跑了一次，没有重试。前一轮全套（同样的结果）因 `--write` 的一个路径问题没写完，修好后用已提交的代码整套重跑，入库的是这一轮。
- 每个剧本跑完都检查了：适配器以下的进程都不持有宿主的管道和套接字，宿主退出后 5 秒内没有遗留进程。

## 用户配置

`~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.codex/config.toml`，以及 `~/.claude.json` 里和权限有关、会拉起程序的字段：运行前后都没有变化，也没有新增项目条目。报告里只写名字和"是否变化"。

## 入库内容

- 19 份录制（`testdata/recordings/claude-agent-acp/0.91.0/2.1.296/`）：经 `tools/sanitize` 和本机敏感词清单脱敏，脱敏后的复扫、通用扫描、敏感词扫描都干净。thinking 失败，它的录制不入库。
- 金样例：GOLD-B-claude（normal-turn、approval、cancel）和同三份的 GOLD-A。
- 报告。因为 thinking 失败，这份报告不计入已验证范围，`runtime/manifest.json` 不变。

CLI 在本机留下的会话、项目目录等列在主仓库不入库的 `.private/cleanup/T066-leftovers.md`。

## 抽查

入库前由编排者扫描并逐个看过录制、报告和文档，用户接受了编排者的抽查结果，没有自己逐个文件查看（2026-10-28）。

## 报告已被取代（T202，2026-10-28）

- T202 在同一天用同一组版本整套重跑了一次：claude-agent-acp 0.91.0、Claude Code 2.1.296，其余剧本用 haiku，thinking 用 sonnet。那次宿主已经请求显示思考摘要，thinking 剧本也换了一道要推几步的题。
- 新报告全部通过：19 个通过、2 个观察、steer 不适用。它取代了上面这份报告，同名文件 `testdata/compat/reports/claude-agent-acp-0.91.0-claude-2.1.296.json` 已改写，并计入已验证范围；范围仍然是 2.1.200–2.1.296。19 份录制和 GOLD-A、GOLD-B 金样例也跟着重新生成，另外新增了 thinking 的录制。详情见 `docs/tasks/T202-claude-thinking-plan/notes.md`。
- 不新增 `[[evidence]]`：MAN-compat-claude 的证据仍然是这份记录。
