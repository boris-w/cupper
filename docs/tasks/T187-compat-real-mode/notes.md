# T187 工作笔记

- 执行者：子 agent（T064 的执行者接着做）

## 开工前读过的

- 任务书；设计 5.9.5；`handoff-notes.md` 第一列含 T187 的一行（T064 交来的）；`tests/ids.toml` 的 T187 段，只有 UNIT-compat-guards。
- T064 的执行者和剧本表：`crates/cupper-host/tests/compat.rs`、`tests/compat/scenarios.rs`。
- 真实模式要用的现有代码：
  - 宿主只在 app 包里安装运行时（`runtime::BundleDir::is_app`）。所以真实模式的宿主要用 `cargo xtask host-bundle --kind e2e` 产出的 `CupperHost.app`，执行者经 `spawn_from` 启动它。
  - 图片：`blob/put` 加 `0x02` 分片（`lifecycle.rs` 的 `send_chunks` 是现成写法），再发带图的 `session/send`。
  - 关闭会话：`session/archive`。
  - 假 agent 已有的命令：`crash_cli`、`spawn_group_child`、`echo_meta`、`write_file` 等。

## 规模估计

按 T064 的实际情况校准：rustfmt 展开后，实现约为草估的 1.1 倍，测试约 1.1 倍。T064 的执行者草估 270 行，实际 487 行；那份草估漏了导出和结果拼装，这次已经计入。

| 模块 | 内容 | 草估 |
|---|---|---|
| 剧本表扩展 | 类型：真实提示词、准备文件、privileged 声明、剧本种类（断言、观察、不适用、只在真实模式），约 25 行；21 个剧本的真实提示词约 35 行；准备文件（normal-turn、未信任项目、Q28、Q34）约 30 行；新增 11 个剧本的定义，每个约 12 行，共约 130 行 | 220 |
| 自我约束 | 每个剧本显式的权限模式（Claude `default`；Codex `read-only` 或 `workspace-write`）；privileged 和放宽档位只在剧本声明、而且只作用于临时仓库时放行；选项只取 `allow_once`、`reject_once`；临时仓库只在本用户自己的临时目录下 | 70 |
| 配置哈希 | 列出的真实配置文件（HOME 可注入）逐个算 sha256、前后比较，写进 `config_files`，有变化就判失败 | 45 |
| 真实模式接线 | xtask：不带 `--fake` 时构建 e2e 宿主包，把宿主路径交给执行者；运行前后算哈希；`real_config` 模式；`--write` 把报告写进 `testdata/compat/reports/`、把全部录制写进 `testdata/recordings/`。执行者：真实宿主的启动（登录 shell 环境、项目信任状态）、`cli_sessions` | 110 |
| `--dry-run` | 执行者只写计划，不起宿主：剧本、权限模式、准备文件、约束检查的结果；xtask 补上要算哈希的文件，打印出来 | 50 |
| 报告扩展 | 结果加"不适用""观察""只在真实模式"，加观察记录；`problems` 跟着改；重新生成 schema | 25 |
| 新步骤与执行者能力 | 追加发送 8；杀 CLI（真实模式找 `claude` 后代）20；重启宿主并重连 25；`session/archive` 8；回合中写文件 6；图片上传并发送 30；多会话并发（Q8：两个会话各 3 个 prompt、3 轮、查串话；6 个会话）55；关闭后命令和子进程都退出 30；多次 resume 后的进程数 30；未信任项目的标记文件和 `_meta` 检查 40；Q28、Q34 的布置与观察 55；后代进程列举 20 | 330 |
| `lsof` 断言 | 适配器的后代不持有宿主的管道和套接字（按 lsof 的管道和套接字标识比对），宿主退出后没有遗留进程 | 80 |
| | 实现小计 | 约 930，×1.1 ≈ 1,020 |
| 测试 | UNIT-compat-guards 90；配置哈希 40（临时 HOME，配置不变时通过、被改时失败）；报告新取值 15；`lsof` 输出解析 20 | 165，×1.1 ≈ 180 |

**合计约 1,200 行**（区间约 1,050–1,300），超过停止线 850，也超过硬停止线 950。还没写代码，先停下汇报，附拆分建议。

T064 时把这部分估成约 740 行，比现在少，因为当时漏了几项：21 个剧本的真实提示词和准备文件、"观察""不适用"的结果机制、`cli_sessions`、真实报告入库，以及 Q8、未信任项目、Q28、Q34 的布置。

## 拆分建议（交编排者裁定）

**方案一（推荐，两个任务，串行）**

| 任务 | 范围 | 估计 | 用例 |
|---|---|---|---|
| T187（缩小）：真实模式与安全约束 | 自我约束；配置哈希；`--dry-run`；真实宿主的启动；基本组 10 个剧本的真实提示词和准备文件；报告的新取值和 schema；`cli_sessions`；真实报告和录制入库；`lsof` 断言与遗留进程检查；Steer 剧本记"不适用（M1b）" | 约 660 | UNIT-compat-guards |
| 新任务：剧本补全 | 其余 10 个剧本：两种恢复、图片、Q8、6 个会话、关闭时有命令在跑、resume 后的进程数、未信任项目、Q28、Q34。每个都有假 agent 脚本和真实提示词，加上所需的步骤与执行者能力 | 约 540 | 不新增（COMPAT 编号由 T066 跑出结果） |

- 两个任务改的都是 `tests/compat/scenarios.rs` 和 `tests/compat.rs`，并行会在这两个文件上冲突。所以建议串行：先做 T187（缩小），新任务接着做。
- 新任务的剧本 `id` 现在就定下（见下），T188 可以照着登记。
- T066 依赖两者。

**方案二**：不拆，停止线提到约 1,200、硬停止线约 1,350。

## 剧本编号（现在定下，之后不改）

T064 已有 10 个：new-session、normal-turn、command-exit、approval、reject、question、plan、cancel、cancel-approval、thinking。

新增 11 个：

| id | 5.9.5 的剧本 | 对应编号 |
|---|---|---|
| `crash-resume` | CLI 或 app-server 被杀后恢复 | |
| `host-restart` | 宿主重启后 resume | |
| `image` | 图片输入 | |
| `steer` | Steer（M1a 不适用） | |
| `concurrent-two` | 两个会话在同一个适配器上并发 prompt | Q8 |
| `six-sessions` | 6 个会话在同一个适配器上并发 | COMPAT-claude-six-sessions |
| `close-running` | 关闭时有命令正在运行 | COMPAT-claude-close-running，K21 |
| `resume-procs` | 多次 resume 同一个会话后的进程数 | COMPAT-claude-resume-procs，Q43 |
| `untrusted-project` | 未信任项目 | COMPAT-untrusted-project-claude |
| `hooks-reload` | 回合中改写 hooks | Q28 |
| `config-scope` | 上级目录与 git 根目录之外的配置 | Q34 |

## 需要确认的

1. **真实模式的宿主**：用 `cargo xtask host-bundle --kind e2e --debug` 产出的 `CupperHost.app`。只有在 app 包里，宿主才会安装运行时，进而找到适配器。启动时用登录 shell 的环境，这样 PATH 里能找到 `claude`。
2. **运行方式**：不带 `--fake` 就是真实模式（设计写的是维护者执行 `cargo xtask compat`），`--dry-run` 两种模式都能用。也可以改成必须显式写 `--real`，多一道防误跑的保护。
3. **要算哈希的文件**：`~/.claude/settings.json`、`~/.claude/settings.local.json`、`~/.claude.json`、`~/.codex/config.toml`。只读不改，凭据文件不算。
4. **Q8 的"没有串话"**：每个 prompt 让 agent 原样回一个标记，检查每个会话的回答里只有自己的标记。这一条要看文本，是结构断言之外唯一的例外。

## 裁定之后（计划 v1.3.60）

按方案一收窄，停止线 720、硬停止线 800。其余 10 个剧本归 T192。四点确认的结论：
- 真实宿主用 e2e 种类的 `CupperHost.app`，环境取自登录 shell，宿主的环境过滤照常生效；
- 必须显式写 `--real`；
- 哈希照 T003 的 `config-hash.ts`，报告里只写名字和是否变化；
- Q8 归 T192。

### 收窄后的估计

| 模块 | 内容 | 草估 |
|---|---|---|
| 执行者：剧本表 | 真实提示词（基本组 10 个加 Steer）、必填的权限模式、放宽声明、准备文件、"不适用"的理由；新增 `steer` 剧本 | 50 |
| 执行者：自我约束 | 静态检查：权限模式在允许清单里，选项种类只有 `allow_once`、`reject_once`，放宽要有声明；运行时检查：所选选项不持久、不要求 privileged，会话的模式风险是 `normal`，临时仓库在本用户的临时目录下 | 55 |
| 执行者：真实模式 | 计划里的 `real`（宿主路径、模型）；以登录 shell 环境启动 e2e 宿主；临时仓库 `git init`；适配器和 CLI 的版本取自导出的头行；`cli_sessions` 取自 `sessions.agent_session_id` | 45 |
| 执行者：`--dry-run` | 只打印计划和约束检查的结果，不起宿主 | 20 |
| 执行者：`lsof` 与遗留进程 | 列出宿主的后代；适配器以下的进程不持有宿主的管道和套接字；宿主退出后这些进程都不在了 | 70 |
| xtask：参数与真实流程 | `--fake`/`--real` 恰好写一个，`--dry-run`、`--model`；构建 e2e 宿主包；运行前后的配置快照；汇总需要清理的项 | 75 |
| xtask：配置快照与比较 | 照 T003：三个文件按整文件，`~/.claude.json` 按字段；本次临时仓库的新项目条目列进清理项，其他新增或改动判失败 | 90 |
| xtask：报告 | 结果加 `not_applicable`、`observed`、`real_only`；`config_files` 只写名字和是否变化；`cli_sessions` 改为 `cleanup`；`problems` 跟着改 | 25 |
| xtask：入库 | 真实模式写全部剧本的录制，报告写进 `testdata/compat/reports/` | 20 |
| `ci compat-framework` | 加跑执行者的单元测试 | 3 |
| | 实现小计 | 约 455，×1.1 ≈ 500 |
| 测试 | UNIT-compat-guards：静态与运行时约束 50、配置快照与比较 45、`lsof` 输出解析 20；报告新取值 10 | 125，×1.1 ≈ 140 |

**合计约 640 行**，在 720 以内，开工。
