# T034 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,820–2,675 行，执行者停下汇报，拆分（计划 v1.3.19）：状态计算、已读与在场、通知意图归 T100。5 项待裁定全部定下。
  - 发现链：T055 先交回并已写好 `supervisor::discover`，编排者让本任务复用它；执行者把自己写的一份 revert 掉，`GitTool` 作为 `Discovery` 的一个字段。做到组装那一步时执行者提交干净的工作树、请编排者把 main 合进来（`ea1354e`，没有冲突），再接组装。
  - 合并 main 后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-1c39dbe2f3db.json`。执行者那次的 `pr-39fa8a19ac05.json` 一并保留。
- 核对：协议改动是三个附加的可选字段（`TrustScan.findings_omitted`、`ProjectTrustInfoResult.findings_omitted`、`diff_omitted`；比裁定多一个，`trust_info` 的 findings 同样可能装不下，认可），`pnpm gen` 的生成物随提交；没有新依赖；用例只在临时目录建仓库；没有个人信息。harness 给汇报加的"疑似指令"标记（settings-json）是用例描述里的 `.claude/settings.json` 文件名，正常内容。
- 审阅：
  - **唯一写者**：`IndexActor` 是索引流和 `projects` 表唯一的写者，慢操作（realpath、仓库档、扫描）另起 task，做完回送 actor，由它把行、`project.upserted` 和账本结果写进同一个事务。
  - **启动对账**：每次启动按两张表对账，第二次启动不写任何事件（幂等）；已有的会话摘要不覆盖（实时状态归 T100）。
  - **帧上限**：先装差异再装 findings（`stale` 时差异最要紧），IT 用例加 1,500 个 agent 验证了不超过帧上限、omitted 计数正确。
  - **`Routes` 拆分**：`session/mark_read`、`client/presence` 改由 `Routes::presence` 登记，留给 T100；`Routes::start` 两个都登记，UNIT-unsupported-set 不受影响。
- 规模：净增约 1,312 行（不含生成物），在收窄后约 1,500 的上限以内。
- 范围外发现：T037 的 `gateway::listen::tests::host_lock_is_held_once_and_names_its_holder` 偶发失败（释放 `host.lock` 后立刻再拿报"已被占用"），应是 T021 查明的那个窗口（别的线程 `posix_spawn` 时 fd 表被短暂复制）；T055 之后 crate 内拉子进程的用例多了，更容易碰上。另开修复任务，见交接。
- 转交事项已写进 `handoff-notes.md`：T100（`routes.presence(..)`、状态计算的接入点）、T059（`pending_upgrade`）、T047（`INDEX` 已登记）、用 git 的任务（`Discovery::git`）、锁用例的修复。
