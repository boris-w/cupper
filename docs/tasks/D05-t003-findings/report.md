# D05 汇报

T003 的结论已写进设计 v1.6 和计划 v1.3，任务书的 7 条都已落实；依赖和波次没有变，校验全部通过。分支 `docs/D05-t003-findings`，基于 `d1d34e4`。

**设计 v1.6**

1. **Q8、Q9**：
   - Q8 的 Claude 部分已关闭，Codex 部分仍由 T102 负责；
   - Q9 移入"已关闭"。
2. **线路分接层与 SDK**（5.6.1、5.6.10）：
   - 扩展消息全部走 SDK，分接层只负责编号与原始帧、结构化脱敏（含 `_auth/status_update` 的账号字段）、K19；
   - SDK 的两个坑写成驱动层约束，并新增 K27、K28 两条规避：兜底的 `on_receive_dispatch`；放弃等待时用 `detach()`，K1 的兜底显式发 `session/close`；
   - K5 改为 SDK 标记 `Responder::cancellation()`；
   - 5.6.7 写明 `steering.supported` 在 `initialize` 响应顶层的 `_meta` 里。
3. **SP-4b 遗留第 7 条**：
   - 5.8.4：对 Claude 的工具命令只剩祖先链一个信号；命令经 nohup、双重 fork 脱离后，三个信号都失效；
   - 5.6.8 的关闭顺序：先 `session/close`，再关 stdin、限时等待，发信号之前枚举全部后代，按各自的进程组发 SIGTERM，再 SIGKILL；
   - 适配器意外退出的情况：资源采样时把后代的进程组记进 `adapters` 表，宿主据此清理（5.2.13）；
   - K21 同步更新。
4. **版本**：
   - Electron 改为"44.x 的最新补丁"（当天 44.9.0），T074 开工时复核（5.10.6、5.1.4，`02-design.md` 第 213 行）；
   - claude-agent-acp 在 M1a 保持 0.91.0；
   - SDK 保持 `=3.3.0`，以后升级时重跑 T003 的探针。
5. **`CommandsChanged` 去重**：
   - 写进 5.6.2 和 5.3.5：内容不变就不发事件，也不写进会话流；
   - 5.7.1 的闸门负载注明 T003 复核后数字不变、大帧已去重。
6. **新增待定问题和兼容剧本**：
   - Q42：`/remote-control`，升级前要安全评审，并由宿主拦截；
   - Q43：上游 #1011 的孤儿 `claude` 进程；
   - 5.9.5 新增三个剧本：6 个会话并发、关闭时有命令正在运行、多次 resume 后的进程数。
   - 待定问题计数：已关闭 12 个，仍待定 31 个。

**计划 v1.3**

- **K1 与 T057**：K1 标为已通过，T057 标为"不触发"，它的用例按"不适用（K1 已通过）"处理；波次说明和 7.3 的返工表同步更新。
- **改写的任务**：
  - T050：扩展消息走 SDK；新增 UNIT-acp-unhandled、UNIT-acp-detach。
  - T055：`steering.supported` 的位置、命令去重、关闭顺序、采样记下后代的进程组和 `claude` 进程数；新增 UNIT-commands-dedup，IT-K21-reap-group 补"后代在另一个进程组"。
  - T038：IT-elevate 按"只剩祖先链"设定信号的期望。
  - T063：IT-K6-crash-resume 补"多次 resume 后的进程数"。
  - T066：claude-agent-acp 保持 0.91.0，0.93 只作为候选版本跑；新增 COMPAT-claude-six-sessions、COMPAT-claude-close-running、COMPAT-claude-resume-procs。
  - T074：Electron 用 44.x 的最新补丁。
  - T078：负载说明。
- **其他**：
  - 新增 2.8（v1.2 到 v1.3 的任务变化）；
  - 覆盖核对：5.4 加 K27、K28，5.5 去掉已关闭的 Q9、加 Q42 和 Q43，并改写 SP-4b #7 一行；
  - 第 6 节第 7 行注明 T066 多了三个剧本。

**校验**

- mermaid 24 个图都能解析；链接与锚点 420 处，0 处失效；
- 四处一致（137 个任务），无环；进程级用例闭包 136 个，0 个问题；表格列数一致；
- 个人信息 0 处：Q42 里原本的产品网址已改写为"Claude 的网页版"。
- 临时工具已删除。

**提交**

```
b4bd1e1 docs(plan): land T003 findings in plan v1.3
d144b72 docs(design): land T003 findings in design v1.6
```

**留给你的事**

- 第 9 节进度里，T003 还没打勾，按惯例由你勾。
- Q42 的宿主拦截要在升级 claude-agent-acp 之前另开任务。