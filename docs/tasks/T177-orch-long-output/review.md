# T177 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 377 行（实现 235、进程内用例 120、进程级 22），在调高后的上限约 380 以内（计划 v1.3.46）。执行者写完合并器与 actor 后统计为 238（估 140），推算加上用例约 363、超过原硬停止线 340，在写用例之前停下；编排者裁定不拆、提高上限。每个模块都统计了，记在 `notes.md`。
- **CI**：编排者先把 main（含 T062 的 `gateway_sync.rs`）合进分支，执行者在其上跑 `ci pr --record` 通过：`pr-5e64c760c062.json`（rust 1998、protocol 185、ts 473、license、testdata-scan、e2e 19；ui 按路径过滤跳过）。之后 main 又进了 T092（只动 e2e、CLI 测试与假 agent 的剧本，与本任务不重叠），编排者再合一次 main，`cargo check --workspace --all-targets --all-features` 通过、`gateway_sync` 12 个测试通过后合并。
- **变异检查**：节拍不判断"正在计时"，进程内用例失败；合并器改成"总是内联"，`gateway_sync.rs` 的 t7 失败（客户端被断开），证明进程级检查抓得到 T062 发现的问题。

## 逐项意见

1. **状态与节拍在合并器（纯核心）里**：转 `truncated` 之前必须先发出缓冲的 delta，放在合并器里顺序天然正确；节拍只经 `Clock`，计时器键扩成 `Text | Tail(item_id)`。只有写盘交给 actor。
2. **actor 在同一步里 await 写 blob**，`item.completed` 一定在 blob 存好之后（设计 5.2.6）；写失败时停在 `truncated`；`blob_refs` 与 `item.completed` 同一事务登记。
3. **全文在内存里封顶 `max_blob_bytes`（32 MiB）**，在字符边界截断，`output_bytes` 记实际收到的字节数。
4. **整段输出与流式输出走同一条路**：Claude 一次给出的大输出也不会超帧。
5. **回合结束时仍在运行的超长工具**记 `abandoned`、带尾部、不写 blob，并取消节拍计时器。
6. **进程级检查**：SP-4a t1 补回"40,000 字符的输出是 blob、预览 4,000 字符、读回完整"；t7 改回 `tool_done t1 500000`，断言输出是 blob、在线客户端从未被重置、晚来的订阅者视图相同。

## 记下的点

- **晚到输出**经同一个合并器，同样会转 `truncated` 和 blob，没有单独的用例。
- **性质测试 `session_props`** 只驱动核心、不交回 `Spill`；以后生成长输出时要在测试驱动里补上交回。
- **actor 写 blob 期间不处理其他输入**：32 MiB 在慢盘上可能阻塞会话百毫秒级。T069 的故障测试里如果出现问题，可以改成后台写、挂起 `item.completed`。记交接给 T069。
- **界面侧**：`truncated` 时显示 `output_tail`，`complete` 且是 blob 时用 `getBlob` 取全文，归 T145 等。
