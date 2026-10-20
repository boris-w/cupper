# T008 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 由 T055 拆出（计划 v1.3.18）。开工前估计约 760–830 行，贴线，执行者停下汇报；编排者裁定不拆、上限放宽到约 850（硬停止线 900），4 个问题定下（`adapters.groups` 列、`process_group(0)`、重启时机、`crashed` 的恢复归 T111）。
  - 进行中编排者追加一项：T044 合并后发现 `AgentRuntime` 丢弃 `SetConfig`，越权改回在生产路径上不生效，交给本任务接上。
  - 合并 main 两次：
    - 第一次与 T009 在 `driver/runtime/mod.rs` 等 4 个文件冲突（13 处），由编排者执行 `git merge`、执行者在工作树里解冲突并 `git add`、编排者提交（守则不让执行者 merge）。两边都保留；执行者顺带接上了 T009 交给 T008 的两项（状态里的内存采样值和 `crashed`），并给 T044 的用例补了 `AdapterExited.stderr`。
    - 第二次与 T047 在运行时的等待循环上冲突，编排者合成一个循环：在 `idle` 里同时等邮箱、wire 和定时器，之后再 `follow` 定时器；`alarm.rung()` 的 future 放在 `idle` 块外取，三个借用落在不同字段上。
  - 合并后重跑 `cargo xtask ci pr --record`：第一次 license 作业因拉 advisory 数据库时 SSL 连接失败（网络问题），重跑全部通过，摘要 `ci/pr-f3e5f9a16f19.json`。执行者那次的 `pr-0744b657a91b.json` 一并保留。
- 核对：新增的 `unsafe` 只在 `cupper-sys`（`resident_size`）；只向用例自己拉起的进程组发信号，`ProcessGroup::signal` 核对组号和启动时间；没有个人信息。调试中一次 panic 留下的两个 `sleep 3600` 进程，执行者已用 SIGTERM 清掉。
- 审阅：
  - **先枚举后代**：适配器一旦正常退出，后代就被 launchd 收养、从进程树上找不到，所以在 `stop` 一开始先枚举一次，第 3 步再枚举一次取并集。好的发现。
  - **`claude` 进程数按可执行文件的真实路径数**，不按进程名（原生安装的 CLI 文件名是版本号）。
  - **退避**：1 s 起、5 分钟内每多失败一次翻倍、最长 30 s，5 分钟内第 5 次失败进入 `crashed`；`stop` 最长 3 + 3 + 2 s。
  - **`SetConfig`**：把 cupper 的选项 ID 换成 agent 的 ID，回复里的全部选项以 `ConfigChanged` 送回。
- 规模：新增 869 行，超过约 850 的上限、在 900 的硬停止线以内；不加追加的 `SetConfig` 是 804 行，接受。
- 转交事项已写进 `handoff-notes.md`：T111（`agents/restart` 调 `clear_crashed()`）、T059（启动时的残留清理读 `adapters.groups`）、会话侧（`AdapterExited.stderr` 还没写进 `error` 条目的详情，MS-5）、Codex 的回收用 `resources()`。`driver/runtime/status.rs` 里"Sampling is T008's"的注释已过时，交给 T116 顺手改。
