# T088 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-14a96a68ae78.json`。执行者那次的 `pr-3ef960802e15.json` 一并保留（它的第一次运行因协议目录测试里漏改 `interaction/respond` 的错误码表而失败，补了一个提交后通过）。
- 核对：没有新依赖；协议改动只有 `interaction/respond` 的 `x-errors` 加 `not_found`，`pnpm gen` 之后只有 schema 变化，TS 类型不变；5.3.4 表的这一行按授权改了。没有个人信息。
- 审阅：
  - **请求的顺序**：`Asked` 先送裁判（空闲时裁判先开自发回合）→ 进暂存表 → 有开着的回合时依次 `Interject`、由交互代理写出并带上回合 ID。回合收尾期间到达的请求先暂存，`turn.completed` 之后开的自发回合承载它，裁判等它解决才结束回合。T058 交接里"接线时请确认"的这一条已确认。
  - **Settle 的顺序**：结束开着的条目 → 解决挂着的请求 → 裁判 `Resolved` → 裁判 `Settled` → 合并器写 `turn.completed`，守住"回合在请求全部解决之前不结束"。
  - **性质测试改用真实的交互代理**，模型新增"请求只写进正在进行、还没 Settle 的回合"的检查；生成器权重不变，T058 的种子仍对应原操作序列。`PROPTEST_CASES=20000` 各跑 3 次通过；变异检查（裁判不等请求解决就结束自发回合）能抓到。
  - **同一个审计 channel**：`Gateway::new` 改为接收外面建好的审计发送端，宿主组装时把同一个发送端交给网关和会话。比会话另开 channel 更符合 T043 开工前的裁定，认可；`supervisor`、网关夹具、testkit 的调用处相应改了。
  - **`reply_to` 无效时回 `invalid_params`**，不新增错误码：客户端只在拒绝成功之后才发，出错说明客户端有 bug。认可。
  - **`interaction/get` 只看视图**：淘汰出视图且已解决的请求回 `not_found`，从表里补归 T089。认可。
- 小瑕疵：实现提交 `70cb93a` 单独编译测试代码时有一条 dead_code 警告（夹具字段到下一个提交才被读到），最终 HEAD 干净。不影响合并，提醒以后每个提交都单独过 clippy。
- 规模：增 886 行、删 195 行（不含笔记和生成的 schema），在约 1,000 的停止线以内。
- 转交事项已写进 `handoff-notes.md`：T055（`SessionDeps` 多了 `audit`、`index`）、T034（实现 `IndexNotes`）、T059（落盘 `interaction.responded`；`Gateway::new` 的审计发送端现在由宿主建好传入）、T089（`interactions.request_json` 与 `Input::Ended`；`interaction/get` 从表里补）、T053（`write_held` 填 `item_id`）。
