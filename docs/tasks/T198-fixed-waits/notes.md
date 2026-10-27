# T198 工作笔记

- 执行者：子 agent（T064、T187、T192 的执行者接着做）

## 修前复现

- **负载**：照 T196 的做法，在后台起 3 个 `cupper-host` 库测试二进制的死循环、3 个 `packages/ui-core` 整套 vitest 的死循环。pid 记进文件，结束时建一个停止文件，再对每个循环及其子进程发 SIGTERM。
- **测法**：`apps/desktop` 下 `vitest run src/main/bridge/contract.test.ts`，连跑 100 次。
- **结果**：100 次里失败 20 次，期间平均负载约 90–106（其他 agent 的任务也在跑）。按用例统计：

  | 用例 | 失败次数 |
  |---|---|
  | `each chunk is its own ArrayBuffer, and an unsubscribed callback gets no more` | 18 |
  | `initialize is answered with forbidden/already_initialized and stays local` | 6 |
  | `a request reaches the host re-serialized, and its response comes back` | 3 |

- **断言消息**与 CI 上的相同：`expected [] to match object [ { id: 9, … } ]`、`expected [] to deeply equal [ { jsonrpc: '2.0', id: 7, … } ]`，以及 `expected [ 2, +0 ] to deeply equal [ 2, 1 ]`。
  - 最后这一条说明：取消订阅之前，第一条通知还没穿过 MessageChannel 到达，所以被取消的那个回调一条也没收到。

## 规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| HostPort 契约 | 去掉夹具里的 `settle`；正向断言改为等条件成立（`vi.waitFor`，上限 5 s）；"什么都没到"的断言改用屏障：之后再发一个探针请求，探针的回包到了，说明在它之前发出的都已处理完 | 35 |
| 两个契约实现 | `contract.test.ts`、`mock/port.test.ts` 不再提供 `settle` | 4 |
| bridge 的其他测试 | `window-bridge.test.ts` 14 处、`bridges.test.ts` 1 处，改为等条件；删掉 `testing.ts` 的 `settle` | 45 |
| 同类排查中能改的 | `privileged.test.ts` 先等 10 ms 再断言"忙"，改为等对话框真的弹出 | 6 |
| | 合计 | 约 90 |

在停止线 200 以内。
