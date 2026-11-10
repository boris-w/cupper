# T246 评审

- 评审人：编排者
- 日期：2026-11-09
- 结论：**通过，合并**（PR #90）

## 核对

- **规模**：+0 −0（记录数据和文档不计）。
- **CI**：PR #90 https://github.com/boris-w/cupper/pull/90 。交接时 `license` 因 RUSTSEC-2026-0459 失败；T249 合并后关掉重开 PR，再跑一轮，结果见下文"合并"。
- **codex-drift**：分支上的 nightly 判通过（run 42599030369），2 passed。

## 逐项意见

1. **分清了 0.167.0 新出现的变化**：另取 0.166.0 做两两比较，确认新增的只有 4 处；任务书点名的 `imageGeneration.result`、`detached` 是 0.166.0 就有、T209 审阅过的。
2. **逐条查了 codex-acp 2.2.2 的源码**：3 处无影响；音频 `url` 改成和 `fileId` 二选一，只在 `session/load` 回放时显示成 `[@audio](undefined)`，不抛错。cupper 在 M1 不用 `session/load`，所以碰不到，如实单列。
3. **隔离**：只运行 `generate-json-schema`，`HOME`、`CODEX_HOME` 都指向空目录，没有登录，外发只到 npm。

## 记下的点

- **写进交接事项**：
  - 给 T113：2.2.2 的 `userInputToContentBlocks` 处理音频时不区分 `fileId`，2.6.0 仍未修；
  - codex-drift 工具看不到变体里再嵌一层的 `anyOf`，"二选一"会显示成"去掉了必填字段"，要不要补以后再定。
- 合并后，nightly 每晚的 `codex-drift` 失败就会消失。
