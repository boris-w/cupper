# T201 笔记

## 行数估计（开工前）

停止线 200，硬停止线 260（`cargo xtask size` 按新增行计，含测试）。

| 模块 | 估计新增 | 内容 |
|---|---|---|
| `runtime/mod.rs` | 30 | 安装进度：安装器的各组件状态经 `watch` 发出，启动适配器时等它不再是 `missing`、`installing`；`Reason` 到 `RuntimeFailure` 的映射提出来复用；`claude_adapter` 改为按安装器的计划取文件 |
| `driver/runtime/launch.rs` | 20 | `Program::Runtime` 带上安装进度；`production` 改为接收调用方给的程序；`Launch::installed` 等安装结束，失败时 `RuntimeFailed` 带上原因 |
| `driver/runtime/mod.rs` | 15 | 启动任务先等安装再调 `launch::start`；等待期间来了停止，就放弃等待，停止不被安装拖住 |
| `driver/runtime/status.rs` | 3 | 模式匹配和用例里的新字段 |
| `supervisor/mod.rs` | 12 | 安装器的输入提前算好；`Finder` 持有运行时的程序；`publish` 同时发安装进度 |
| 测试 | 100 | IT-runtime-first-prompt-waits 进程级用例（`bundled_host` 加注册表闸门、假 node 在启动适配器时运行假 agent），进程内用例（失败原因进错误信息、等待中停止） |
| 合计 | 约 180 | |

## 实际行数

`cargo xtask size`：新增 187、删除 34（停止线 200）。第一版是 226，超了停止线：安装进度按组件状态传，另把 `Reason` 到 `RuntimeFailure` 的映射提成方法。改成直接传索引已经在用的 `adapter_state` 结果（Claude 适配器的 `runtime` 和 `failure`），这两处都省掉了，测试也收紧了排版，降到 187。

## 修前修后对照

- 修前：把 `Progress::done` 临时改成立即返回（等于启动前不等安装），其余不变。IT-runtime-first-prompt-waits 失败：索引里 runtime 已经是 `installing`，回合却已经结束，`until` 等不到 `running`。临时打印回合结束：`failed`、`runtime_failed`、`duration_ms` 6，和 T066 看到的现象一致（37 ms 失败）。
- 修后：两段都通过。装好的一段，回合 `completed`、`adapter.failure` 为空；装失败的一段（注册表给的字节不符合 SRI），回合 `failed`、`runtime_failed`，`adapter.failure` 为 `hash_mismatch`。
- 进程内用例的变异检查：去掉停止时放弃等待的那一行，用例挂住（40 s 后被 alarm 结束）；恢复后通过。

## 取舍

- **等什么**：启动适配器等的是 `agents.updated` 里 Claude 适配器的 `runtime`（`adapter_state` 合成的那一份），等到 `ready` 或 `failed`。这正是界面看到的状态（任务书第 2 条），不新增协议成员。安装器的任务结束（发送端丢弃）时不再等，照旧核对文件；不在 app 包里的宿主没有安装器，发送端立刻丢弃，行为与以前相同。
- **等在哪里**：在 `launch::start` 里、核对哈希之前；`start` 本来就在 actor 另开的 task 里运行，所以等待不占 actor，别的会话、`agents/list`、探测都照常（附加在 `waiting` 里排队，本来就是这样）。
- **停止**：等待期间宿主退出（`host/exit`、SIGTERM），运行时的停止原本要等 `Started` 才结束，会被安装拖住（下载上限 60 s，还会重试）。所以启动任务带一个 oneshot，停止时 actor 丢掉发送端，等待就结束，启动按失败返回。只有等安装这一段可以放弃；进程已经拉起之后不放弃，免得留下孤儿进程。
- **失败信息**：回合结束只写 `error_code`（`runtime_failed`），`AgentError` 的文字不进会话流；失败原因在错误文字里（"the runtime's install failed: hash_mismatch"），进程内用例核对；进程级用例核对 `agents.updated` 的 `adapter.failure`。
- **按安装器的计划启动**（单独一个提交）：以前 `claude_adapter` 固定用内嵌清单，测试包里安装器按 `test_config.runtime.manifest` 安装，启动却去找内嵌清单的版本目录，测试里永远对不上。改成和安装器用同一份计划；生产里两者本来就是同一份。
- **进程级用例的夹具**：假 node 收到单个参数（适配器的包）时 exec 假 agent；CLI 用设置里的 `agents.claude.cli_path` 指向假 agent（它是 Mach-O，能通过解析），这样不会去常见位置找到本机真实的 Claude。注册表等测试放行才回复：测试看到索引里 runtime 是 `installing`、回合是 `running` 之后才放行，顺序由事件保证，不靠 sleep。
