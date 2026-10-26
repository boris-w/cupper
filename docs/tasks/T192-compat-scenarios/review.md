# T192 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+541 −23，停止线 620 以内。每写完一个模块统计了一次（232 → 537 → 541）。
- **CI**：第一个按 PR 合并的任务，PR #3：https://github.com/boris-w/cupper/pull/3 。
  - compat-framework、license、protocol、testdata-scan、ts、ui 都通过；
  - rust 第一次挂了与本任务无关的 `it_gw_sync_reconnect_replays_without_gaps`，编排者只重跑了这一个作业，重跑通过。这个用例已记为 T199；
  - e2e 第一次失败在 CI 缓存（运行时包只剩空目录），与本任务无关。T197 的 PR #4 合并后，编排者把 main 合进分支（1b435c9）再推送，新一轮 8 个检查全绿。
- **本机**：`ci compat-framework --record` 15 项通过；`--fake` 21 个剧本没有失败；`--real --dry-run` 列出全部计划；三份 B 层录制与金样例不变，`gold_b` 9 个通过。
- **没有运行真实 CLI**。

## 逐项意见

1. **剧本补全**：10 个剧本都有真实提示词和假 agent 脚本，都受 T187 的自我约束和进程检查约束，没有一个声明 `loosens`。UNIT-compat-guards 逐个检查全部剧本，新剧本也在其中。
2. **`host-restart` 的检查改得对**：宿主重启后靠 `session/resume` 恢复，不发 `session_resumed`（那个通知只出现在 K6 的崩溃恢复里），所以改为看录制里宿主发出的帧。
3. **`close-running` 的假 agent 命令**：没有为了让进程检查通过而放宽约束，而是让假 agent 的命令自成进程组、不持有宿主的管道，更接近真实 CLI。认可。
4. **观察型剧本**：观察结果写在 `detail` 里，不进检查列表，不会被统计成通过或失败；进程检查照常算。
5. **`untrusted-project` 在假 agent 下也跑**：`settingSources` 只有 `user` 是宿主一侧的行为，两种模式都有意义。
6. **没有新增单元测试**：串话和未信任两项判定，执行者都临时改成相反的期望验证过，会判失败。任务书没有要求新增编号，认可。

## 记下的点

- **T066** 实际运行时留意（执行者的 `notes.md` 里也写了）：
  - `config-scope`、`untrusted-project` 在开工前就放好了 `.claude/settings.json`，宿主可能判为配置已变而暂停队列，剧本会以"不确定"结束；
  - `crash-resume`、`resume-procs` 在两回合之间 SIGKILL 掉 `claude`，靠 K6 自动 resume；
  - 真实模式的回合等待上限是 5 分钟。
- **E2E-NFR1-4 在负载约 47 时等重连超时**：与 `it_gw_sync_reconnect_replays_without_gaps` 同属断线重连，并入 T199。
