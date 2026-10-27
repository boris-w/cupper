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

## 改法

- **HostPort 契约**（`packages/host-client/src/contract.ts`）：
  - 夹具不再提供 `settle`。新增并导出 `until(check)`，即 `vi.waitFor`，上限 5 s，每 5 ms 查一次；超时时报出最后一次检查的断言，也就是在等什么。
  - 每个正向断言都改为等它成立。
  - "什么都没到"的断言改用探针：之后再发一个 `host/status`，等它的回包回来。同一条连接上的消息按顺序处理，探针回来了，它前面的就都处理完了。
  - 取消订阅的用例：等被取消的回调先收到第一条，再取消订阅、发第二条，然后等保留的回调收到第二条。两个回调是同一次分发里拿到数据块的，所以保留的那个收到了，被取消的那个若没取消成功也已经收到。
  - 只剩"关闭之后什么都不发"这一处保留 50 ms 的停顿，理由见下面的清单。
- **bridge 的测试**：
  - 走 mock 端口（`bridged()`）的用例全部同步完成，直接断言，不再等待。mock 端口没有延迟，bridge 处理窗口消息和宿主帧时，除了重连都是同步的。
  - 重连（要等 `controlReady` 和 `connect`），以及穿过 MessageChannel 的（`wired()`），都用 `until` 等条件成立。
  - 关闭 bridge 之后"不再重连"保留 50 ms 的停顿。
- **`privileged.test.ts`**：原来先等 10 ms 再发第二个请求，断言"忙"。现在等第一个请求的对话框真正弹出来再发。

## 同类排查（`apps/`、`packages/`、`tools/` 的 TS 测试）

| 位置 | 写法 | 结论 |
|---|---|---|
| `apps/desktop/src/main/bridge/testing.ts` 的 `settle`（10 ms），共 15 处用到 | 等固定时长再断言 | 已改，见上 |
| `packages/host-client/src/mock/port.test.ts` 的 `settle`（0 ms） | 契约夹具的一部分 | 随契约一起去掉 |
| `apps/desktop/src/main/privileged/privileged.test.ts:58`（10 ms） | 等固定时长再断言"忙" | 已改为等对话框弹出 |
| 契约"close ends the connection for good"（50 ms） | 断言关闭后什么都没发生 | 保留。端口关了就没有回包可等，也没有探针可发。停顿只是给错误的发送或重连在空闲机器上留出暴露的时间。负载高时只会漏掉错误、让它照样通过，不会让用例失败 |
| `window-bridge.test.ts`"close ends it for good"（50 ms） | 同上 | 保留，理由同上 |
| `packages/host-client/src/sync.test.ts:262` | 最多 50 轮 `setTimeout(0)`，直到收到第 n 条 | 不改。按轮数而不是时长：mock 端口用 `queueMicrotask` 投递，每轮都会先跑完已排上的微任务，与负载无关 |
| `packages/host-client/src/sync.test.ts:304` | 一次 `setTimeout(0)` 之后断言 | 不改。理由同上：通知是同步发出的，投递在微任务里，一个宏任务之后必定已到 |
| `apps/desktop/src/main/host/launch.test.ts:29、109`（20 ms） | 轮询进程是否退出的间隔 | 不改。等的是事件本身（进程退出）；钩子和用例有 vitest 自己的超时 |
| `tools/e2e/src/procs.ts:70`（100 ms） | 轮询遗留进程的间隔，有上限 | 不改，同上 |
| `tools/e2e/src/connection.spec.ts:160`（200 ms） | 两次网络采样之间的间隔 | 不改。不是断言前的等待 |
| `tools/e2e/src/gate.spec.ts` 的 `sleep` | 性能闸门的测量窗口（5 s、10 s、N 秒） | 不改。测的就是一段时间里的占用 |
| `tools/e2e/src/input.ts:32、81`（1100 ms） | 取得焦点后等过输入守卫的 1 s 冷却 | 不改。等的是产品规则里的时长，这段时间过去没有事件可等。多留 100 ms，负载只会让它等得更久 |
| `tools/e2e/src/security.spec.ts:104、209、254`（1100 ms） | 同上：等过 1 s 的武装时间，再按键 | 不改，理由同上 |
| `tools/e2e/src/security.spec.ts:148`（2100 ms） | 等过"空闲 2 s，面板才拿焦点"的规则 | 不改，理由同上 |
| `tools/e2e/src/interaction.spec.ts:183`（2100 ms） | 等过取消后 2 s 的冷却，并断言请求还在 | 不改，理由同上。其中"请求还在"是负向断言，负载高时只会照样通过 |
| `tools/e2e/src/security.spec.ts:126、193、203`（600 ms） | 按下应被吞掉的键，600 ms 后断言没有被应答 | 不改。负向断言没有事件可等，按键若被误收，应答要绕宿主一圈，在空闲机器上远少于 600 ms。负载高时只会漏掉错误，不会失败 |
| `tools/e2e/src/nfr1.spec.ts:187`（1000 ms） | 停掉第一个跟随者后等 1 s，再起第二个，让它从游标追上 | 不改。这段时间是为了让流在跟随者不在时继续往前走，后面的 `caughtUp` 等的是事件本身 |
| `packages/ui-core` 的定时 | 假时钟（`vi.useFakeTimers`） | 不受负载影响 |

产品代码里的 `setTimeout`（`apps/desktop/src/main/host/index.ts:126`、`launcher.ts`、`index.ts:141`）不在测试范围内，没看也没改。

## 修后验证与负载中止

- **修后在本机负载下连跑**：每轮跑一遍 `apps/desktop` 的 `src/main/bridge`、`src/main/privileged`，再跑一遍 `packages/host-client` 的 `src/mock/port.test.ts`，共 200 轮，**0 轮失败**。
  - 每 20 轮记一次 1 分钟平均负载：93、107、88、81、101、104、87、66、47、36。
  - 负载的造法与修前相同。
- **负载被中止**：系统负载到了约 104，用户的电脑卡得没法用，编排者用 SIGTERM 停掉了负载循环和它跑起来的测试二进制。
  - 前 140 轮左右是在负载 80–107 下跑完的；之后负载停止，最后约 60 轮的负载在 66、47、36 回落。
  - 修前那 100 次（20 次失败）的负载是 90–106，和前 140 轮相当。
  - 我的测量脚本已经结束，本机没有遗留的进程。
- **今后**：用户在用电脑时，本机不再造负载，也不并行跑多份整套测试。修后的验证改在 GitHub CI 上跑。如果还需要"负载下连跑 200 次"，等编排者通知后再跑，到时用 `nice -n 20`，并发不超过 2 份。
