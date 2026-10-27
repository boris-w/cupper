# T198 汇报

- 执行者：子 agent（T064、T187、T192 的执行者接着做）
- 分支：`fix/T198-fixed-waits`（PR https://github.com/boris-w/cupper/pull/8）
- 完成日期：2026-10-27

T198 做完了：`settle()` 这类固定等待已从测试里去掉，改为等事件本身发生。修前在负载下 100 次挂 20 次，修后 200 轮 0 失败。PR #8 的 8 个检查全部通过（其中 rust 是重跑后通过的，原因见"验证方式"）。规模 +85 −82，在停止线 200 以内。产品代码一行没改：`packages/host-client/src/contract.ts` 是 HostPort 的契约测试套件，只给测试用，不是产品代码。

## 做了什么

**HostPort 契约**（`packages/host-client/src/contract.ts`）
- 夹具不再提供 `settle()`。
- 新增并导出 `until(check)`，也就是 `vi.waitFor`，上限 5 s、每 5 ms 查一次。超时时报出最后一次检查的断言，也就是在等什么。
- 正向断言都改为等条件成立：回包到达、帧到达、重连的时间线、blob 帧。
- "什么都没到"的断言改用探针：之后再发一个 `host/status`，等它的回包回来。同一条连接上的消息按顺序处理，探针回来了，它前面的就都处理完了。用到这一招的有 `initialize` 留在本地、几种非法消息被丢弃这两个用例。
- 取消订阅的用例：先等被取消的回调收到第一条，再取消订阅、发第二条，然后等保留的回调收到第二条。两个回调是在同一次分发里拿到数据块的，所以只要保留的那个收到了，被取消的那个若没取消成功也早已收到。修前那条 `[2, 0]` 的失败就是这里漏了"先等第一条"。
- 两个实现随之去掉 `settle`：`contract.test.ts`（bridge）和 `mock/port.test.ts`（mock）。

**bridge 的其他测试**（`window-bridge.test.ts` 14 处、`bridges.test.ts` 1 处）
- 删掉了 `testing.ts` 里的 `settle`。
- 走 mock 端口（`bridged()`）的用例直接断言、不再等待。mock 端口没有延迟，bridge 处理窗口消息和宿主帧时，除了重连都是同步完成的。
- 重连要等 `controlReady` 和 `connect`，穿过 MessageChannel 的（`wired()`）也有异步，这两类都用 `until` 等条件成立。

**`privileged.test.ts`**：原来先等 10 ms 再发第二个请求、断言"忙"。现在等第一个请求的对话框真正弹出来再发。

## 关键取舍

- **两处"关闭之后什么都不发"保留了 50 ms 的停顿**：契约的 close 用例，以及 bridge 的 close 用例。端口关了就没有回包可等，也发不了探针。停顿只是给错误的发送或重连留出在空闲机器上暴露的时间；负载高时只会漏掉错误、让用例照样通过，不会让它失败。
- **没有用重试或放宽时长来掩盖问题。** 改的是等什么，而不是等多久。5 s 的上限只用来结束一个会永远等下去的用例，正常情况下等到条件成立就立刻往下走。

## 验证方式

- **修前**：在本机负载下（平均负载 90–106），`contract.test.ts` 连跑 100 次，失败 20 次。
  - 失败分布：取消订阅的用例 18 次、`initialize` 的用例 6 次、请求回包的用例 3 次。
  - 断言消息与任务书里 CI 上那 5 次相同。
- **修后**：改动涉及的测试（desktop 的 bridge、privileged，加上 host-client 的 mock 契约），在同样的负载下连跑 200 轮，**0 失败**。
  - 每 20 轮记一次 1 分钟负载：93、107、88、81、101、104、87、66、47、36。
  - 前约 140 轮在高负载下跑完；你停掉负载后，后约 60 轮的负载在回落。
  - 你已确认这些数据够用，不再补跑 `test-repeat`。
- **本机单份检查**：desktop 和 host-client 两套 vitest 全部通过，prettier、eslint、`tsc --build` 都通过。
- **GitHub CI**（PR #8，run https://github.com/boris-w/cupper/actions/runs/40442384484 ）：
  - compat-framework、e2e、license、protocol、testdata-scan、ts、ui 通过；
  - rust 第一次挂了 `it_gw_sync_reconnect_replays_without_gaps`，就是已排进 T199 的那个偶发用例；只重跑 rust 后通过（job https://github.com/boris-w/cupper/actions/runs/40442384484/job/121988605741 ）。

## 提交列表

```
c8dced2 Merge branch 'main' into fix/T198-fixed-waits   （编排者合入 main）
445f1e0 docs(task): T198 approach, the sweep of fixed waits, and the load runs
cd1c53e test(desktop): the privileged busy check waits for the open dialog
52f2780 test(desktop): the bridge's tests wait for what they expect
4b9ca19 test(host-client): the HostPort contract waits for what it expects
f5a6f34 docs(task): T198 reproduction before the fix and estimate
```

## 遗留问题 / 范围外发现

1. **本机负载让用户的电脑卡住了。** 我起的负载循环叠加其他任务，把负载推到了约 104，你用 SIGTERM 停掉了。这件事和之后的规则（守则第 10 条：本机不造高负载，高负载场景放 CI）都已记在 notes.md。本机没有遗留进程。
2. **同类排查**：清单和每一项的结论都在 notes.md。
   - 改了的：bridge 的 `settle`（15 处）、契约的 `settle`、`privileged.test.ts` 的 10 ms。
   - 其余都保留，各自写明了原因：
     - e2e 里等过输入守卫的 1 s 武装时间、2 s 空闲或冷却的 `waitForTimeout`，等的是产品规则规定的时长，这段时间过去没有事件可等；
     - 断言"按键被吞掉"的 600 ms 是负向断言，负载高时只会照样通过；
     - 其他是轮询间隔、性能测量窗口，以及按轮数而不是按时长的 `setTimeout(0)`。
3. **`it_gw_sync_reconnect_replays_without_gaps`** 在 PR #3 和 PR #8 上各挂过一次，T199 已在修。
