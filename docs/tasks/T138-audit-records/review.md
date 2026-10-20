# T138 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：净增 671 行（增 738、删 67，只算 `crates/`；实现约 380、测试约 290），在约 700 的上限之内。中途推算越线一次，执行者压缩了用例再继续。
- **CI**：执行者的摘要通过。合并 main 两次（先是 T059、T046、T029 等，没有冲突；再是 T129：`index/mod.rs` 的 import 冲突，两边都保留；T129 与本任务各自在 `lifecycle.rs` 里定义了一模一样的 `Running(Child)` 守卫，去掉一份）。之后重跑 CI，IT-idle-exit 稳定失败。

## 合并后发现的问题与修复（补充汇报）

- **现象**：合并 main（含 T059、T129）后，IT-idle-exit 稳定失败：客户端还连着，宿主就以空闲退出了。main 本身通过，所以是本任务与 main 的交互。
- **原因**（执行者查明，修在本任务）：
  - 本任务在网关的 `admit` 里新加了读对端签名（`AuditToken::signing_identity()`），它在异步工作线程上同步执行，对体积大、未正式签名的测试二进制一次约 600 ms；
  - 这段时间里宿主的定时器也停了：T059 的监护本该每 20 ms 观察一次，实际只在阻塞前后各观察一次；
  - 客户端要等 `admit` 结束才算进连接数，宿主看到的是一段超过 300 ms（用例的空闲时长）、没有客户端的空闲。
- **修法**：`conn.accepted` 只带钉住的 `AuditToken`，由审计写入 task 经 `spawn_blocking` 读路径和签名，读完再用 `unique_id` 核对令牌，防 pid 复用；写入 task 串行，记录顺序不变。修复净增 41 行。
- 修复后 IT-idle-exit 连跑 5 次通过；`cargo xtask ci pr --record` 的 `pr-ddf6762f1a6f.json` 通过（rust 1874、protocol 185、ts 436）。此后 main 上只多了纯 TS 的 T083、T082 和文档，没有重跑。
- 这是一处真实的问题：接受连接时同步做慢操作，会卡住整个宿主的定时器。执行者另外提醒 `admit` 里的 `verify(ui_requirement)` 也是同类调用，交给 T163 一并检查。

## 逐项意见

1. **审计写入**：新的静止屏障种类 `audit`，在 `Participant::idle` 里等接收端；先写 host.log 再写表；`stream.subscribed` 只进 host.log。90 天清理沿用 `Cleanup::Audit`。
2. **正式格式**：`formal()` 和 `kept()` 把映射集中在一处。`conn.*`、`interaction.responded`、`config.*`、`elevation.*`、`project.trust.*` 都按 5.8.6 映射。还没有产生者的三类在文档里注明由 T163 加，没有预先加空变体。
3. **`conn.accepted` 的对端信息**：取不到时留空，不让连接失败。
4. **两类新产生者**：`config.confirmed` 在离开 `config_unconfirmed` 时发出，带原因和设定者；`project.trust.*` 覆盖信任、变 stale、扫描失败的各条路径。
5. **一处安全修正**：握手失败只记错误码和 `data`，不记 `message`，因为参数非法时 serde 的报错可能引用客户端发来的内容。这是执行者自己发现的，好。
6. **IT-audit-retention 的验证方法**：不把手动时钟推进 91 天（会触发两千多次账本清理，而且途中每天的审计清理会先删掉旧记录），改为插入 90 天边界两侧的记录再跑 `run_maintenance`。方法巧妙，验证的是同一条规则。

## 范围外发现的归属

- **有序退出时的 `conn.closed{host_exiting}` 只进了 host.log、没进表**：`Host::exit` 先停写线程再 `close_all`。T163 加 `host.exiting` 时同样需要先关连接、排空审计写入、再停写线程，一并交给 T163。
- **`ConfigUnconfirmed` 的结构变了**：现在是 `{reason, config: ConfigState}`，记交接。
- **`project/add` 新路径扫描失败时没有审计**（还没有项目 ID）：交给 T163 考虑按路径记录。
