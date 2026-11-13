# T262 笔记

## 开工前的估算（2026-11-13）

### 标记放在哪里

- 窗口的数据连接由主进程建立和握手（`ShellAPI.host.open()`），渲染进程拿到端口后 `client.attach`，状态变成 `ready`（`host-client/src/shell.ts`）。
- 索引流由 `Subscriptions.start()` 订阅；快照应用完时，`StreamStore.live()` 被调用（`ui-core/src/state/subscriptions.ts`、`stream.ts`）。
- 新会话页一挂上就有输入框；输入框本身从不禁用，只有发送按钮会。所以"输入框可用"就是 `Composer` 挂载。
- 三个标记都放在 ui-core 里（渲染进程才有），不放在 host-client：主进程的控制连接也用 HostClient，放在那里主进程也会打标记。
  - `cupper:connected`：窗口的 HostClient 第一次进入 `ready`，即窗口的数据连接握手完成（在 `HostProvider` 里）。
  - `cupper:index`：索引流第一次 `live`，即快照已应用到视图（在 `Subscriptions` 里，给索引的 sink 包一层）。
  - `cupper:ready`：索引已应用，且输入框已挂载，两者都成立的那一刻（在 `Composer` 挂载时，以及索引 live 时检查）。
  - 每个标记只打一次，只读状态，不改变行为。一个小模块 `ui-core/src/app/startup.ts` 记下这三件事。

### 行数估计

| 模块 | 内容 | 估计 |
|---|---|---|
| `startup.ts` | 三个标记、只打一次、`ready` 的两个条件 | 30 |
| 接入点 | `HostProvider`、`Subscriptions`、`Composer` 各一处 | 15 |
| 测试 | `startup.ts` 的单元测试（顺序、只打一次、条件的先后两种）；在 App 的组件测试里用 mock 宿主验证三个标记按顺序出现 | 60 |
| **产品代码和测试合计** | | **约 105** |
| `spikes/`（另列） | `startup.spec.ts` 读这三个标记和导航计时（`performance.getEntriesByType("navigation")`：渲染进程开始导航、DOMContentLoaded），每次冷启动分成"主进程 → 宿主 → 控制连接 → 页面开始 → DOM 就绪 → 窗口连接 → 索引 → ready" | 25 |
| 修复 | 原因查清后再估 | — |

停止线 250，标记部分约 105，修复留有约 145 行的余地。
