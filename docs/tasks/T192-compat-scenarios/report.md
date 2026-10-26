# T192 汇报

- 执行者：子 agent（Rust 工程师；T064、T187 的执行者接着做）
- 分支：`feat/T192-compat-scenarios`（PR https://github.com/boris-w/cupper/pull/3）
- 完成日期：2026-10-26

**e2e 待 PR #4 合并后重跑。** 其余 7 个检查都已通过。e2e 失败的原因是 CI 缓存（见"验证方式"），T197 在 PR #4 里修。

剩下的 10 个剧本已经补齐，兼容套件现在有全部 21 个。`--fake` 下 18 个通过、steer 记为不适用、两个需要 CLI 读配置的记为 `real_only`，没有失败。三份 B 层录制和金样例都没变。规模 +541 −23，在停止线 620 以内。

## 做了什么

**新增的 10 个剧本**，编号沿用 T187 定下的那些。每个都有真实提示词和假 agent 脚本，并照常受 T187 的自我约束和进程检查约束，没有一个声明 `loosens`。

| 剧本 | 做法与检查 |
|---|---|
| `crash-resume` | 杀掉 CLI 后再发两条：假 agent 先执行 `crash_cli`，再执行 `whoami`；真实模式在回合之间杀掉 `claude`。检查有 `session_resumed` 通知，最后一回合完成 |
| `host-restart` | 回合结束后用同一个数据目录重启宿主并重连，再发一条。检查宿主发了 `session/resume`，最后一回合完成 |
| `image` | 上传一张 1×1 的 PNG（`0x02` 分片加 `blob/put`），再发带图的 prompt。协议这条路是通的，检查宿主发给 agent 的 `session/prompt` 里带着图片 |
| `concurrent-two`（Q8） | 2 个会话，每轮每个会话同时发 3 个 prompt，共 3 轮。每个 prompt 要求回一个执行者生成的标记，检查每个会话只出现自己的标记、没有别人的。真实模式等待上限 5 分钟 |
| `six-sessions` | 6 个会话各 1 个 prompt，检查同上 |
| `close-running`（K21） | 允许一条命令，3 秒后在命令还在跑的时候收尾。检查收尾时适配器下面有命令在跑；宿主退出后已有的遗留进程检查确认它也退出了 |
| `resume-procs`（Q43） | 5 轮"杀 CLI 再发一条"，每轮记下宿主的后代进程数，检查不比开始时多 |
| `untrusted-project` | 项目以未信任状态加入，事先放好带 hooks、`env`、`apiKeyHelper` 的 `.claude/settings.json` 和 `.mcp.json`。检查宿主录下的 `session/new` 里 `settingSources` 只有 `user`；用标记文件确认 hooks、MCP 服务器、`apiKeyHelper` 都没生效；用 `env.txt` 确认项目的 `env` 没生效 |
| `hooks-reload`（Q28） | 两条命令之间改写 hooks，用标记文件看新 hook 有没有在本回合生效。结果记 `observed`；假 agent 下记 `real_only` |
| `config-scope`（Q34） | 项目放在仓库的子目录 `sub`，在 git 根目录和根目录之外各放一份带 hook 的配置，看哪份生效。结果记 `observed`；假 agent 下记 `real_only` |

**执行者为此加的能力**
- 新步骤：上一回合结束后再发一条、先传图再发、杀真实 CLI、重启宿主、多轮 resume 并计数进程、多会话并发、回合中写文件、不等回合结束就收尾。
- 新检查：
  - 文件里没有某个文本；
  - 出现某种通知；
  - 没有串话；
  - 进程数不增加；
  - 收尾时有命令在跑；
  - `settingSources` 只有 `user`；
  - 图片发给了 agent；
  - 宿主发了某个方法；
  - 观察项。
- 剧本新增三个属性：`real_only`、`project`（子目录）、`untrusted`。
- 宿主现在放在 `User` 里，这样才能重启；客户端打开了上传 blob 的特性。
- 真实模式的等待上限由 30 秒改为 5 分钟，假 agent 仍是 30 秒。

## 关键取舍

- **`host-restart` 的检查改了。** 宿主重启后靠 `session/resume` 恢复会话，不会发 `session_resumed` 通知（这个通知只出现在 K6 的崩溃恢复里）。所以检查改为"宿主发了 `session/resume`"，看录制里宿主发出的帧。
- **`close-running` 在假 agent 下另找了起命令的办法。**
  - 假 agent 自带的 `spawn_group_child` 会持有它的标准输出，也就是宿主的管道，T187 的进程检查会判失败。
  - 改成 `write_file` 写一个 shell 脚本再用 `spawn` 运行：标准输入输出都重定向到 `/dev/null`，用 perl 的 `setpgrp` 让命令自成一个进程组（Claude 的命令也是这样），再 exec 成 `sleep 300`。
  - 这不是放宽约束，而是让假 agent 的命令更接近真实 CLI 的命令。
- **观察型剧本**：观察结果写在 `detail` 里，不进检查列表，所以不会被当成"通过"或"失败"来统计。进程检查照常算。
- **`untrusted-project` 在假 agent 下也跑。** `settingSources` 那一项是宿主一侧的行为，两种模式都有意义；标记文件那几项在假 agent 下必然成立，所以这个剧本没有记 `real_only`。
- **没有加单元测试。** 原计划约 15 行的串话判定测试没写，因为判定依赖执行者的状态。改为临时让每个会话都回同一个标记、临时把项目改成已信任，确认 `six-sessions` 和 `untrusted-project` 会判失败；这些改动都没提交。新剧本都过自我约束，由已有的 UNIT-compat-guards 覆盖，它逐个检查全部剧本。

## 验证方式

**本机**
- `cargo xtask compat --fake`：21 个剧本中，18 个通过、steer 不适用、hooks-reload 和 config-scope 为 `real_only`，没有失败。
- `cargo xtask compat --real --dry-run`：列出了 21 个剧本的计划，没有启动任何东西。
- 三份 B 层录制与金样例：`--fake --write` 后三份都与入库的相同（unchanged），金样例不变，`gold_b` 9 个都通过。这一步本身也说明了所有剧本都没有失败，因为有剧本失败时 `--write` 会拒绝写入。
- `cargo xtask ci compat-framework --record`：15 项通过，摘要已提交（`ci/compat-framework-b466999e5153.json`）。
- cupper-host 的 `compat` 测试目标：3 个单元测试通过。

**GitHub CI**（PR #3，run https://github.com/boris-w/cupper/actions/runs/40316625556 ）

| 检查 | 结果 |
|---|---|
| compat-framework、license、protocol、testdata-scan、ts、ui | 通过 |
| rust | 第一次挂了 `it_gw_sync_reconnect_replays_without_gaps`，重跑通过（job https://github.com/boris-w/cupper/actions/runs/40316625556/job/121560878003 ） |
| e2e | 失败，待 PR #4 合并后重跑 |

- e2e 的失败与本任务无关：构建 host-bundle 时 codesign 找不到 `target/runtime-pack/Contents/Helpers/node`。rust-cache 恢复出了这个目录，但里面没有 node，于是 e2e 跳过了 `runtime-pack`。main 上那次 push 和 T195 的 PR 也是同样的失败。

**规模**：新增 541、删除 23。
- 剧本表类型和 10 个剧本 232 行，执行者约 309 行。
- 开工前估计约 470 行。每写完一个模块都统计过一次：232 行、537 行、541 行。

## 提交列表

```
f425267 Merge branch 'main' into feat/T192-compat-scenarios   （编排者合入 main）
0fab8fc docs(task): T192 compat-framework CI summary
b466999 docs(task): T192 notes on the scenarios and what T066 should watch
a5f6fbb test(compat): the remaining ten scenarios of design 5.9.5
84f950d docs(task): T192 estimate and approach
```

## 遗留问题 / 范围外发现

1. **CI 缓存问题**（已交 T197，在 PR #4 里修）：e2e 只看运行时包的目录在不在，目录在、node 不在时会跳过 `runtime-pack`。
2. **`it_gw_sync_reconnect_replays_without_gaps` 偶发失败**（gateway_sync.rs:372，"no range events live"）：在 PR #3 的 rust 作业里失败过一次，重跑通过。它依赖时序，断言 200 ms 的合并窗口里会出现 range 事件。你已记为下一批偶发失败的候选。
3. **本机 `ci pr --record` 遇到的两次失败**（新流程之前，机器负载约 47）：
   - `unit_trust_scan_forking_directory_links_hit_the_entry_limit` 超过了 10 秒的墙钟上限，T196 在查；
   - e2e 的 `E2E-NFR1-4: a window whose connection dropped resumes from its cursor` 等 "the window connects again" 超时（20 秒）。
   - 两者都与本任务无关，失败的摘要已删除，没有提交。
4. **留给 T066 的观察点**（notes.md 里也写了）：
   - `config-scope`、`untrusted-project` 开工前就放好了 `.claude/settings.json`。项目在库里直接记为已信任或未信任，没有信任时的扫描快照，宿主可能判为配置已变而暂停队列，到时剧本会以"不确定"结束。
   - `crash-resume`、`resume-procs` 在真实模式下是在两回合之间 SIGKILL 掉 `claude`，靠宿主的 K6 自动 resume 恢复。
   - `close-running` 在真实模式下，允许命令运行后等 3 秒就收尾。
   - 真实模式的回合等待上限是 5 分钟。
