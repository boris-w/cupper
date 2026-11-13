# 人工验证（MAN）：检查表与证据格式

- 依据：设计 5.9.10（人工验证清单、trace-ids）、5.12.3（M1b 退出标准 ①）；计划 1.6；先例 `docs/tasks/T099-m1a-exit/man-keyboard.md`（T173 据此归纳）。
- 本目录放**检查表**：每个需要逐步执行的 MAN 编号一份，例如 [man-s1.md](man-s1.md)。检查表只写怎么做、怎样算通过，不写结果。
- 每**执行一次**，在执行它的任务的卷宗里新建一份**记录**，再在 `tests/ids.toml` 末尾追加一条 `[[evidence]]`。trace-ids 据此判断这个编号是否完成。

## 1. 记录文件

- **位置与名字**：`docs/tasks/<执行任务>/<编号小写>.md`，例如 `docs/tasks/T179-m1b-exit/man-s1.md`。同一个编号在同一个任务里做第二次时，加日期：`man-s1-2026-12-01.md`。
- **内容**依次是：
  1. 标题：`# <编号>：<一句话>（<任务号>）`。
  2. **执行信息**，下面这些字段都要写：

     | 字段 | 写什么 |
     |---|---|
     | 检查表 | 按哪份检查表、它在哪个提交上（有检查表时） |
     | 日期 | 执行的日期，`YYYY-MM-DD` |
     | 执行人 | 谁动手、谁记录（"作者""编排者"，不写名字和账号） |
     | 构建 | 构建种类（自用、e2e、发布）和构建所在的提交 |
     | 版本 | 与 `[[evidence]]` 的 `versions` 相同的组件和版本（见第 2 节） |
     | 环境 | 系统、键盘与输入法、项目的性质（"作者的一个 Rust 仓库，有远端"），**不写本机路径和项目名** |
     | 判定依据 | 现场观察、界面截图、宿主的 `host.db` 或日志里的哪几项 |

  3. **逐项结果**：一张表，列是"项、结论、看到的现象"。结论只有三种：通过、不通过、未做。"未做"要写原因，例如"依赖的 T130 未合并"。
  4. **发现**：过程中看到的问题，写明另开了哪个任务；不影响结论的也写在这里。
  5. **证据**：要追加到 `tests/ids.toml` 的那条 `[[evidence]]`，原样贴出。
- 截图只截必要的部分，入库前过敏感词清单（主仓库的 `.private/sensitive-words.txt`）；不截账号、路径、项目里的代码。

## 2. `[[evidence]]` 与登记表

```toml
[[evidence]]
id = "MAN-S1"                       # 已登记、套件为 man 的编号
date = "2026-12-01"                 # 执行日期，不能晚于核对那天
record = "docs/tasks/T179-m1b-exit/man-s1.md"   # docs/ 下的记录文件
versions = { cupper = "1a2b3c4", macos = "26.4", claude = "2.1.300", "claude-agent-acp" = "0.91.0", codex = "0.80.0", "codex-acp" = "2.2.2" }
```

**`versions` 的组件名**（trace-ids 的 `COMPONENTS`）：

| 组件 | 写什么、从哪里取 |
|---|---|
| `electron` | App 里的 Electron 版本：`main.log` 第一行的 `electron` |
| `cupper` | 构建所在提交的短哈希（`git rev-parse --short HEAD`）；发布构建写版本号 |
| `macos` | `sw_vers -productVersion` |
| `claude`、`codex` | `claude --version`、`codex --version` 的版本号 |
| `claude-agent-acp`、`codex-acp` | 构建所在提交的 `runtime/manifest.json`；有了设置页的 agent 状态（T151）后以那里显示的为准 |

**登记表一侧**（`tests/ids.toml` 里这个编号的内联表）：

- `versions = { <组件> = "<精度>" }`：证据必须写出这些组件的版本。
  - `"any"`：只要求写上；不管现在是什么版本，证据都不会过期。可以用于全部组件。
  - `"major"`、`"exact"`：还要与当前版本相符（主版本相同，或完全相同），否则证据过期。只能用于 trace-ids 读得到当前版本的组件，目前只有 `electron`（取自 `apps/desktop/package.json`）。
- `max_age_days`：证据超过这么多天就过期。
- `parts`：分几次完成的编号拆成子编号，各自登记证据，子编号全部完成才算完成。
- 证据里多写的组件不检查，可以照写；它们留在记录里供以后对照。

**trace-ids 怎么核对**：

- `check`（每个 PR 的 `testdata-scan` 作业）检查三件事，有一件不满足就报错：
  - 证据的编号已登记，套件是 `man`；
  - `record` 是 `docs/` 下存在的文件；
  - 登记表要求的组件，证据里都写了。
- `status`、`verify` 只看同一编号里日期最新的一条证据：
  - 没有证据：待完成；
  - 日期晚于今天、超过 `max_age_days`、`major`/`exact` 与当前版本不符、当前版本读不到：问题；
  - 其余情况：完成。
- **只登记通过的执行。** 有一项不通过，或有一项因依赖未合并而未做，记录文件照样提交（结论写清楚），但不追加证据，编号保持待完成。试走一遍的记录也照此办理。

## 3. 现有的 MAN 编号

| 编号 | 检查表 / 做法 | 记录放在 | 登记表应要求的 `versions` | 说明 |
|---|---|---|---|---|
| MAN-S1 至 MAN-S4 | [man-s1.md](man-s1.md) 至 [man-s4.md](man-s4.md) | T179 | `cupper`、`macos`、`claude`、`claude-agent-acp`、`codex`、`codex-acp`，都是 `any`（本任务已登记） | 两种 agent 写在同一条证据里 |
| MAN-keyboard | T099 的 `man-keyboard.md`；重做时按 T107 的 `man-keyboard.md` 写的差异（发请求延时 45 秒） | T099、T107 | `electron = "major"`（已有） | Electron 大版本变了就要重做 |
| MAN-ime | SP-6 场景 6（5.9.7） | T108 | `electron = "any"`、`macos = "any"` | 输入法名称写进"环境"一栏 |
| MAN-notification | `-spike`（已有证据）加 `-product` | T101、T150 | `-product`：`macos = "any"`、`cupper = "any"` | `-spike` 的证据没写版本，按当时的规则仍然有效 |
| MAN-tcc | SP-10 | M2 | `macos = "any"` | 不在 M1 |
| MAN-ui6 | `-shell` 加 `-full`，与本机 Codex 并排比对 | T152、T148 | `cupper = "any"`、`macos = "any"` | 对照用的 Codex 客户端版本写进"环境"一栏，不作为组件 |
| MAN-compat | `-claude`（已有证据）加 `-codex`，即兼容套件的报告 | T066、T113 | `-codex`：`codex = "any"`、`codex-acp = "any"` | 记录就是报告的摘要；`-claude` 的证据没写版本，仍然有效 |
| MAN-upgrade | 发布任务的升级演练 | T183 | `cupper = "any"` | 新旧两个版本号都写进记录，`cupper` 写新版本 |
| MAN-node-notarize | 首次 Developer ID 构建 | T180 | `cupper = "any"`、`macos = "any"` | 计时结果写在"逐项结果"里 |
| MAN-security-doc | 用户文档的审阅 | T157 | 不需要 | 记录写审阅了哪一版文档、谁审的 |
| MAN-local-signed | 条件任务 | T162 | `cupper = "any"`、`macos = "any"` | 只有 K7 通过才做 |

除 MAN-S1 至 MAN-S4 外，"登记表应要求的 `versions`"这一列都是建议，由产出这些编号的任务在派发时登记：本任务只改自己负责的编号，已有的证据不会因此失效。

## 4. MAN-S1 至 MAN-S4 的共同约定

- **构建**：用自用构建 Cupper Local，按 [`docs/user/selfuse-install.md`](../user/selfuse-install.md) 构建和安装，先读过 [`docs/user/selfuse-notice.md`](../user/selfuse-notice.md)。不用开发构建或 e2e 构建（计划 1.10）。
- **项目**：作者自己的一个真实 git 仓库，有可以推送的远端（S3 要推送）。另开一个可以随时丢弃的分支来做这几个场景，不在主分支上动。
- **agent**：用真实的 Claude Code 和 Codex CLI，都已登录，会消耗额度。按须知开启 agent 自带的沙箱；遇到辅助功能、自动化等系统授权一律拒绝。
- **两种 agent**：MAN-S1 本身就同时用两种 agent；MAN-S2 至 MAN-S4 每一步按 Claude、Codex 各走一遍，结果表分两列。
- **依赖**：步骤标了任务号的，要等那个任务合并后才能走。T179 开始时这些任务应当都已合并；没合并的步骤记"未做"，这个编号就先不登记证据。
- **顺带检查**（计划 1.11、T179）：回合折叠、请求面板的按键、消息流贴底、从通知或侧边栏跳转。有问题写进记录的"发现"一节。
- **不通过时**：写清楚是哪一步、看到了什么，另开修复任务，修好后重做那个场景。
