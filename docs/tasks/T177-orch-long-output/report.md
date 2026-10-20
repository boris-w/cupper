# T177 汇报

- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T177-orch-long-output`
- 完成日期：2026-10-20

## 做了什么

做法照 T175 的中途汇报：状态和节拍放在合并器里，只有写 blob 由 actor 来做。

1. **合并器（纯核心）**（`orchestrator/coalescer.rs`）
   - 工具输出在流式输出时先累积在内存里，同时记下转义长度。
   - 超过 `max_inline_bytes`（16,000）时：
     - 先把窗口里缓冲的 delta 发出去，再写 `item.updated{truncated, output_tail, output_bytes}`；
     - 之后停发 delta；
     - 全文继续累积，以 `max_blob_bytes` 封顶，在字符边界截断；`output_bytes` 记实际收到的字节数。
   - 尾部（最后 4,000 字符）和字节数每个节拍至多更新一次：
     - 节拍内又有新输出，等节拍结束时再写；节拍内没有新输出，就不写。
     - 节拍时长登记为 `output_tail`（1 s），只经 `Clock`。
     - 计时器键 `FlushTimer` 扩成了枚举 `Text | Tail(item_id)`。
   - 一次给出的整段输出（Claude）：放得下就内联；放不下也走这条路。
2. **结束时写 blob**（`core.rs`、`actor.rs`、`rows.rs`）
   - 工具结束时，合并器交出 `Action::Spill`。
   - actor 在当前这一步里 await `BlobStore::put`，再把结果作为 `Input::Spilled` 交回。合并器写出 `item.completed`：`complete`，blob 引用加尾部预览。所以 `item.completed` 一定在 blob 存好之后。
   - 写 blob 失败时，输出停在 `truncated`。
   - `blob_refs` 与 `item.completed` 在同一事务里登记。
   - `SessionDeps`、`Parts` 加了 blob 存储；会话夹具挂上了 `blob/get`、`blob/put` 路由。
3. **回合结束时仍在运行的超长工具**：记 `abandoned`，带最后的尾部和字节数，不写 blob，并取消它的节拍计时器。
4. **测试夹具**：测试用的 `Peer` 能读 `0x02` 分片；会话夹具加了 `blob(hash)`，走 `blob/get` 读回。
5. **IT-orch-long-output（进程内）**，2 个用例：
   - 第一段输出立即发出，第二段留在窗口里，第三段越过上限。断言：
     - 缓冲的 delta 先于 `truncated` 发出；
     - 尾部和字节数正确；
     - 节拍内的新输出等节拍再写，下一个节拍没有新内容时不写；
     - 结束时是 blob 加预览，`blob/get` 读回的内容完整，`blob_refs` 有登记。
   - 回合结束时的超长工具是 `abandoned`，带尾部，没有 blob。
6. **进程级检查**（`tests/gateway_sync.rs`，T062 的发现）：
   - SP-4a t1：补回断言。40,000 字符的工具输出是 blob，预览 4,000 字符，`blob/get` 读回的内容完整。
   - SP-4a t7：改回 `tool_done t1 500000`，断言以下三点，也就是没有帧超限：
     - 输出是 blob；
     - 输出过程中订阅着的客户端从未被重置（只订阅过一次）；
     - 晚来的订阅者视图相同。

## 关键取舍

- **状态在合并器里**：转 `truncated` 之前，必须先把合并器缓冲里的 delta 发出去。放在合并器里，顺序天然正确，节拍也是纯核心计时器。只有写盘这一步交给 actor。
- **actor 在同一步里 await 写 blob**：写完之前不处理别的输入，`item.completed` 一定在 blob 之后（设计 5.2.6）。代价是 actor 在写盘期间不响应其他输入；blob 最大 32 MiB，写入是一次性的。
- **全文在内存里封顶 32 MiB**：超出的部分不进 blob，`output_bytes` 仍按实际收到的字节数计。
- **回合结束不写 blob**：settle 是同步的，等不了写盘。已获认可。
- **整段输出与流式输出走同一条路**：所以 Claude 一次给出的大输出也不会超帧。

## 验证方式

- `cargo test -p cupper-host --lib`：全特性通过。clippy 在全特性和无特性下都没有警告。
- 变异检查：
  - 节拍不判断"正在计时"→ 进程内用例失败；
  - 合并器改成"总是内联"→ `gateway_sync.rs` 的 t7 失败（客户端连接被断开）。这说明进程级检查能抓到 T062 发现的问题。
  - 跑完没有残留进程。
- 在干净工作区运行 `cargo xtask ci pr --record docs/tasks/T177-orch-long-output/ci/`，各作业通过，摘要 `pr-5e64c760c062.json` 已提交：
  - rust 1998、protocol 185、ts 473、license、testdata-scan、e2e 19；
  - ui 按路径过滤跳过；`sensitive-words` 因本机没有词表跳过。
- **规模**：按提交逐个统计，不含合并进来的 main，共新增 **377 行**：实现 235、进程内用例 120、进程级 22。在调高后的上限约 380 以内（v1.3.46）。每个模块都统计了，记在 `notes.md`。

## 提交列表

```
897686f chore(ci): record T177's PR run
5e64c76 test(gw): SP-4a tests 1 and 7 assert the blob of a long tool output
f9ad7e6 test(orch): IT-orch-long-output in process
86b10fd Merge branch 'main' into feat/T177-orch-long-output
5f31293 feat(orch): tool outputs over the inline limit become truncated, then a blob
ddb5e81 docs(task): T177 notes and size estimate
```

（`86b10fd` 是编排者把 main 合进本分支的合并提交。）

## 遗留问题 / 范围外发现

1. **晚到输出**：被中断回合的工具在后续回合里另建晚到条目，它的输出同样经合并器，同样会转 `truncated` 和 blob。没有单独为它写用例。
2. **性质测试 `session_props`**：只驱动核心、没有 actor，`Spill` 在那里只是被观察，不会交回。目前生成的输出都在内联上限以内，不受影响；以后如果生成长输出，需要在测试驱动里补上交回这一步。
3. **actor 写 blob 期间不处理其他输入**：32 MiB 的输出在慢盘上可能阻塞会话百毫秒级。目前按设计要求先写后发；如果 T069 的故障测试里出现问题，可以改成写在后台、把 `item.completed` 挂起等待。
4. **界面侧**：`output_state` 为 `truncated` 时显示 `output_tail`，为 `complete` 且输出是 blob 时用 `getBlob` 取全文。这是 T145 等任务的事，这里只确认协议字段都齐了。
