# T091 工作笔记

- 执行者：子 agent（测试工程师；T090、T167 的执行者接着做）

## 开工前读过的

- 任务书；设计 5.9.4 的 NFR-1 表；`handoff-notes.md` 第一列含 T091、T167、e2e 的行；T167 的评审。
- 视图：
  - TS 的 `StreamView.toJSON()` 与 Rust 视图的 JSON 相同（schema 的 `IndexView`、`SessionView`，parity 用例守着）；
  - `notification.intent` 本来就不进视图。
- UI 能拿到视图和游标的地方：
  - 界面的视图在 ui-core 的 `Subscriptions` 里，渲染进程入口只拿得到 `HostClient`；
  - 游标在 host-client 的 `StreamSync` 里，是私有的。
- CLI 的 `Mirror`：
  - 遇到缺口会带着游标重新订阅，ViewError 时从快照重来，外面都看不出来；
  - 现有命令里没有一个能输出整个视图。
- 界面：
  - 请求记录行 `data-request-record`，文案"已批准：…"、"已拒绝"、"已询问 n 个问题"、"未回答"；
  - 别的客户端处理了请求时，面板收起并弹出提示"已在 {client} 处理"；
  - 同时回答失败的一方提示"已在别处处理"；
  - 中断回合的标题"你在 … 后停止了"，宿主重启后显示"已中断"；
  - 排队消息在 `QueueList` 里。
- 假 agent：
  - `flood <n> <长度>` 全速输出，`flood_rate <每秒> <秒> <长度>` 定速输出；
  - 文本片段经 40 ms 合并，成为带 `from` 的区间事件。

## 开工前的规模估计

行数指格式化后的行，含注释。实现按 1.15 倍、测试按 1.1 倍放大。

**视图导出与支撑（单列）**

| 模块 | 内容 | 草估 |
|---|---|---|
| UI 的视图导出 | 渲染进程入口的 e2e 分支：<br>• 包一层 `client.subscribe`，记下每条流的 sink（界面的 `StreamStore`）；<br>• 从实时事件的 `seq` 和快照的 `at_seq` 记下每条流到了哪个 seq；<br>• `__cupperTest.views()` 导出 `{流: {seq, complete, view}}`。<br>为此 `HostProvider` 要在创建 client 时就调 `onClient`，早于界面订阅（现在是在 effect 里调，晚于子组件的订阅）。 | 45 |
| CLI 的视图导出 | test-harness 构建里的 `cupper view <index\|会话>`：跟随这条流，每次一批事件处理完、已经 live 时输出一行 `{stream, seq, resubscribed, view}`。`resubscribed` 计数缺口和重来，NFR1-4 用它检查 `from` 连续。另含参数解析和单元测试。 | 45 |
| 用例共用（`nfr1.ts`） | 两端对齐：<br>• 从 `host.db` 取流的最新 seq，等 CLI 的输出行和 UI 的导出都到这个 seq；<br>• 只比窗口内的内容；<br>• 在界面里打开会话。 | 45 |
| | 小计 | **约 135**，×1.15 ≈ **155** |

**用例**

| 用例 | 内容 | 草估 |
|---|---|---|
| E2E-NFR1-1 | 索引和一个会话：UI 与 CLI 同时订阅，到同一个 seq 后比较视图 | 50 |
| E2E-NFR1-2 | CLI 依次发送、排队、中断、批准、回答，断言界面上的新回合、排队消息、"你在 … 后停止了"、面板与记录行。其中一条走 `cli` 身份提权的真实路径（`@focus`） | 100 |
| E2E-NFR1-3 | UI 批准后 CLI 看到 `resolved`（`@focus`）；CLI 批准后界面面板收起，并提示"已在 … 处理"；两端同时回答，只有一个生效（`@focus`） | 80 |
| E2E-NFR1-4 | 断开 CLI、重载渲染进程、只断开窗口的数据连接，各一次；断开期间用 `flood_rate` 持续产生事件；两端视图一致，`resubscribed` 为 0，host-client 计数为 0；顺带看插入历史后视口不跳 | 90 |
| E2E-NFR1-5 | 运行中、等待审批时各 `kill -9` 一次；界面拉起新宿主；回合"已中断"、请求失效、没有残留进程；下一条消息经恢复成功。要等 T089 | 70 |
| | 小计 | **约 390**，×1.1 ≈ **430** |

**合计约 585 行**，在约 850 的线以内，开工。

**请留意**：`cupper view` 是 CLI 里的代码，不是"只新增用例文件"。它和 T167 的 `clock-advance` 一样，只在 test-harness 构建里有。CLI 的 `Mirror` 视图只能从 CLI 进程里拿到，设计 5.9.4 要比的又正是它。
