# T068 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：`crates/` 内净增 520 行（增 697、删 177），估约 580，在约 850 的上限之内；其中约 170 行是从 `lifecycle.rs` 挪进 `tests/common/` 的夹具，增删对冲。
- **CI**：执行者的摘要 `pr-0f1ee045b30c.json` 通过。合并 main（含 T089、T167、T168）时没有文本冲突，但编译不过：T089 新写的载入路径构造 `Parts` 时缺了本任务为重扫的 `Hold` 加的 `quiescence` 字段。编排者在合并提交里补上（照创建路径取会话依赖里的那一份），之后 `ci pr --record` 通过：`pr-cb7af07d5ab2.json`（rust 1923、protocol 185、ts 471、license、testdata-scan、e2e 4；ui 按路径过滤跳过）。第一次重跑的 license 失败是拉取 RustSec 漏洞库时 SSL 连接失败，环境问题，重跑通过。
- **验收**：七个用例连跑 10 次全绿，跑完没有残留进程；测试名以编号开头。

## 就地修的两处缺陷（都在计划 1.2 的上限内）

1. **`rescan` 没有持有静止屏障的 `Hold`**（T100 合并时发现）：`Parts` 带上 `Quiescence`，重扫 task 持有 `Hold` 直到结果交回 actor。增 7、删 1。修之前 IT-trust-stale 的"推进时钟后立刻查提示"连跑 10 次失败 10 次，修之后全过。
2. **适配器死在 prompt 中途时请求被记成 `cancelled`**（违反 SEC-10，写用例时新发现）：SDK 让在途 prompt 以 `incoming_transport_closed` 失败，常比运行时的 `AdapterExited` 先到，回合按普通失败结束、请求随之以 `turn_ended` 取消。`claude::agent_error` 把这个错误归为 `AgentErrorClass::AdapterExited`，回合机原有逻辑据此把请求标为 `expired{adapter_exited}`，两个事件谁先到结果都一样。增 6，只动驱动层内部分类。

## 逐项意见

1. **共用夹具挪进 `tests/common/`**：`lifecycle.rs` 和新文件共用，`allow(dead_code)` 写了理由。**`Running` 守卫改为先 SIGTERM 再等退出**：原来的 SIGKILL 让宿主来不及停下适配器，适配器和孙进程留下，这就是此前各 worktree 偶见的残留进程的来源。T062、T063、T069 直接受益。
2. **IT-config-unconfirmed** 用回合中由 agent 抬档（`bypassPermissions`）的场景，改回成功与改回失败两个分支都覆盖，照设计 5.9.8 的写法。
3. **"不加载项目配置继续"没有进程级用例**：线上方法要到 M1b 才有，理由成立；进程内与 T089 的重启用例已覆盖。
4. **`kill -KILL` 只对本测试宿主写进 `adapters` 表的 pid**。
5. **汇报被系统标成"像指令"**：原因是提到 `.claude/settings.json` 和 `bypassPermissions` 模式，这是用例在描述测试场景，内容无害。

## 记下的点

- **闸门已关时重扫失败不再追加提示**（例如已因 `project_config_changed` 关着，又遇到悬空链接），原因只在审计的 `project.trust.scan_failed` 里。符合现在"只在闸门打开时提示"的写法；界面要显示最新原因时另议，记交接。
- **第一个回合可能停在准备阶段**：`turn.started` 已写出、附加与发送前的重扫还没做完时项目配置变了，回合等闸门重新打开。用例没有覆盖，交给 T069 补一条"信任恢复后这个回合能正常发出"，记交接。
