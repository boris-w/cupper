# T192 工作笔记

- 执行者：子 agent（T064、T187 的执行者接着做）

## 开工前读过的

- 任务书；设计 5.9.5；`handoff-notes.md` 第一列含 T192 的一行。
- T187 的 `notes.md`，剧本编号以它为准，不改。
- T063 的 `driver_k.rs`：`crash_cli` 之后再发一条，宿主会自动 resume，前后的进程数不变。
- 现有执行者 `crates/cupper-host/tests/compat.rs`、`tests/compat/scenarios.rs`。
- 协议里图片的路径是通的：`blob/put` 加 `0x02` 分片，再发 `{"type": "image", "blob": {hash, size, media_type}}`。

## 做法

- **新步骤**：
  - `Send`：上一回合结束后再发一条，真实提示词和假 agent 脚本各一份；
  - `KillCli`：真实模式下用 SIGKILL 杀掉宿主后代里的 `claude`；假 agent 用脚本里的 `crash_cli` 自己崩，这一步什么也不做；
  - `RestartHost`：用同一个数据目录重启宿主并重连；
  - `Image`：上传一张 1×1 的 PNG，再发带图的 prompt；
  - `Concurrent`：N 个会话、每个 K 个 prompt、R 轮。标记由执行者生成，真实模式在 5 分钟内、假 agent 在 30 秒内全部返回；
  - `Resumes`：杀掉 CLI 再发一条，重复 N 次，每轮记下宿主的后代进程数；
  - `WriteFile`：回合中改写文件；
  - `Leave`：不等回合结束，直接收尾。
- **新检查**：
  - 出现了某种通知（`session_resumed`）；
  - 没有串话：每个会话只出现自己的标记；
  - 多次 resume 后进程数不增加；
  - 收尾时适配器以下有命令在跑；
  - 在宿主录下的 `session/new` 里，`settingSources` 只有 `user`；
  - 文件里没有某个文本；
  - 观察项：标记文件在不在。
- **剧本属性**：
  - `real_only`：附原因，`--fake` 下直接记 `real_only`；
  - `project`：项目在仓库里的子目录，Q34 要用；
  - `untrusted`：项目以未信任状态加入。
- **真实模式的等待上限**从 30 秒改为 5 分钟。Q8 要求 5 分钟内返回，真实模型的回合也常常超过 30 秒。

## 规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| 剧本表类型 | 新步骤、新检查及其名字、`real_only`、`project`、`untrusted` | 45 |
| 10 个剧本 | 每个都有真实提示词、假 agent 脚本、准备文件、步骤和检查 | 150 |
| 执行者 | 宿主放进 `User`，以便重启；各步骤的实现；观察型结果；需要执行者状态的检查；真实模式的等待上限 | 220 |
| | 实现小计 | 约 415，×1.1 ≈ 455 |
| 测试 | 串话判定的单元测试；新剧本都过自我约束由现有用例覆盖 | 15 |

**合计约 470 行**，在 620 以内，开工。每写完一个模块就跑一次 `cargo xtask size`。

## 实际做法与发现

- **规模**：剧本表类型和 10 个剧本 232 行；执行者写完后合计 541 行。新增的单元测试一条也没写。
  - 原计划的"串话判定"单元测试没写，因为这个判定依赖执行者的状态。改为临时把假 agent 的回答换成别的会话的标记，确认会判失败。
  - 新剧本都过自我约束，由已有的 UNIT-compat-guards 用例覆盖（它逐个检查全部剧本）。
- **`host-restart`**：宿主重启后的 resume 走的是 `session/resume`，并不发 `session_resumed` 通知（这个通知只在 K6 的崩溃恢复里出现）。所以检查改成"宿主发了 `session/resume`"。
- **`close-running`**：
  - 假 agent 的 `spawn_group_child` 会持有它的标准输出，也就是宿主的管道，进程检查会判失败。
  - 改用 `write_file` 写一个 shell 脚本，再用 `spawn` 运行：脚本把标准输入输出都重定向到 `/dev/null`，用 perl 的 `setpgrp` 让命令自成一个进程组（Claude 的命令也是这样），然后 exec 成 `sleep 300`。
  - 收尾时这条命令还在适配器下面；宿主退出后它已经不在了。
- **`untrusted-project`**：
  - 宿主那一侧的检查两种模式都有意义：宿主录下的 `session/new` 里，`settingSources` 只有 `user`。
  - hooks、`.mcp.json`、`apiKeyHelper`、`env` 用标记文件和 `env.txt` 来判断；假 agent 不读这些配置，所以这几项在假 agent 下必然成立。
  - 临时把项目改成已信任，`settingSources` 这一项会判失败。
- **`hooks-reload`、`config-scope`**：假 agent 下记 `real_only`。真实模式下按标记文件在不在记为 `observed`。
- **真实模式的等待上限**改为 5 分钟，假 agent 仍是 30 秒。

## 验证

- `cargo xtask compat --fake`：21 个剧本中，
  - 18 个通过；
  - steer 记为不适用；
  - hooks-reload、config-scope 记为 `real_only`。
- `cargo xtask compat --real --dry-run`：列出了 21 个剧本的计划。
- 临时破坏检查，确认会判失败，改动都没提交：
  - 让每个会话都回同一个标记，`six-sessions` 判失败；
  - 把项目改成已信任，`untrusted-project` 判失败。
- 用 `--fake --write` 重新产出三份 B 层录制，结果都和入库的一样，金样例不变；`gold_b` 测试 9 个都通过。

## 留给 T066 的观察点

- `config-scope` 和 `untrusted-project` 开工前就放好了 `.claude/settings.json`。项目在库里直接记为已信任或未信任，没有信任时的扫描快照，宿主可能判为"配置已变"而暂停队列，到时剧本会以"不确定"结束。
- `crash-resume`、`resume-procs` 在真实模式下是在两回合之间 SIGKILL 掉 `claude`，再发下一条，靠宿主的 K6 自动 resume 恢复。
- `close-running` 在真实模式下，允许命令运行后等 3 秒就收尾，然后靠遗留进程检查判断。
