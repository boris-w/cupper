# T080 汇报

- 执行者：子 agent（前端工程师，安全）
- 分支：`feat/T080-ui-state`（在 T054 的 worktree 里）
- 完成日期：2026-10-20

状态层已接好，UNIT-ui-store（7 个用例）和 UNIT-ui-query（2 个用例）都通过，截图基线没有变化，`cargo xtask ci pr` 全部通过。有一处需要你知道：本分支还没有 main 上新增的 UNIT-ui-query 编号，所以 CI 摘要里暂时缺这一条；你合并 main 后它会自然出现。

## 做了什么

1. **host-client**
   - 新增 `HostClient.notify(method, params)`：连接就绪时发通知，否则丢弃，不排队。
   - 新增校验模式 `validation: "all" | "non_delta"`（默认 `all`），经 `HostClient` 传给每条 `StreamSync`，再由 store 传给 `IndexView`、`SessionView`，并以 `client.validation` 公开。
   - `non_delta` 下，`item.delta` 只检查 `seq`、`from` 是不是整数：信封解析、"数据残缺"的日志检查、视图里的 `isKnownEvent` 三处都不再用 zod。
   - ui-core 在生产构建（`import.meta.env.PROD`）用 `non_delta`。
2. **流 store**（`ui-core/src/state/stream.ts`）
   - `StreamStore` 把视图包成 sink，消息到达即应用；host-client 处理完帧后才确认，这一点原本就有。
   - 通知组件按动画帧合并：`Frames` 每帧对每个变过的视图调一次 `versions.flush()`。
   - 组件用 `useEntry(view, key)`（基于 `useSyncExternalStore`）按条目订阅。
   - 只有实时事件会交给 `onEvent`，用于查询失效；快照不触发。
3. **订阅策略**（`state/subscriptions.ts`）
   - 索引流常驻；当前会话加最近 3 个保持订阅，更早的退订并丢弃视图。
   - 在新建会话页时只保留 3 个。
   - 打开一个未持有的会话走缓存未命中的路径：视图 `complete` 为假时显示加载中，订阅失败时 `error` 有值。
   - `StateProvider` 在渲染时按路由调用 `show`，下面的会话页渲染时就能拿到自己的 store。
4. **zustand 界面状态**（`state/ui.ts`）
   - 一个 store 存路由、布局、按会话的草稿、附件（协议的 `PromptBlock`）和滚动位置。
   - `LayoutStore` 对外形状不变，状态改放进这个 store，多了一个 `ui` 字段和可选的初始路由参数。
   - `Route` 类型和 `useRouter()` 不变；`RouterProvider` 只负责把菜单和 shell 的导航接进 store。
   - 修掉了宽度闪动：每个偏好键记一个"已发出、未收到回传"的写入序号队列，丢弃过期回传；写入失败的序号会出队。
5. **查询**（`state/query.ts`）
   - `createQueryClient` 不按时间过期，不重试。
   - 查询键约定：全局数据用 `[method, …]`，会话数据用 `["session", id, method, …]`，这样一个事件能让会话的全部查询失效。
   - 失效规则：`turn.completed` 使该会话的全部查询失效；`session.upserted`、`session.removed` 使归档列表失效；`agents.updated` 使 `agents/capabilities` 失效。每次重新连上后全部失效。
   - 示例查询是 `useArchivedSessions()`。
6. **接线**：`App` 里依次是 `HostProvider` → `UiProvider` → `StateProvider`（含 `QueryClientProvider`）→ `RouterProvider`。新增依赖锁定精确版本：`zustand` 5.0.15、`@tanstack/react-query` 5.104.1（都是 MIT），另外 ui-core 加了 `@cupper/protocol`。

## 关键取舍

- **"按帧"只用于通知组件**：应用视图一直是同步的，保证"应用后确认"不变。
- **测试不用 Node 模块**：ui-core 的测试直接用 `MockHost` 加 `conn.initialized = true` 驱动客户端，不依赖 `node:fs` 读样例，免得撞上 T136 的 import 规则。
- **会话的 store 在渲染时就建好**：`StateProvider` 在渲染时调 `show`，而不是放在 Effect 里，否则会话页第一次渲染拿不到 store。`start()` 会重新订阅 `close()` 之前在屏幕上的会话，所以 Effect 重跑时也不会丢。
- **生产构建里，数据真有问题的 delta**：fold 时抛错，`StreamSync` 照旧不带游标重新订阅。
- 更细的记录在 `docs/tasks/T080-ui-state/notes.md`。

## 验证方式

- **UNIT-ui-store**（7 个用例）：
  - 消息到达即应用，一帧只调度一次 flush，只通知变化的条目；
  - 当前会话加 3 个保持订阅，更早的退订，草稿和滚动位置仍在；命中时会话移到最近；在新建会话页只留 3 个；回到已丢弃的会话时重新订阅；
  - 缓存未命中先显示加载中；订阅失败时视图带 `not_found`；
  - 重连后索引流和会话流都按游标重新订阅；
  - `non_delta` 下坏数据的 delta 不再被 zod 拦下，而坏数据的 `turn.started` 照样被记为数据残缺；
  - `notify` 只在连接就绪时发出；
  - 过期 prefs 回传被丢弃，别的窗口的修改照常应用，写入失败不挡后续修改。
- **UNIT-ui-query**（2 个用例）：事件到失效键的映射；只有实时事件触发失效，另一个会话的查询不受影响，重新连上后全部失效。
- **截图回归**：T118 的 5 个用例通过，基线没有变化，UI-csp-zero 仍为 0。
- **CI**：在干净的工作区里 `cargo xtask ci pr --record docs/tasks/T080-ui-state/ci/` 通过：rust 1795、protocol 185、ts 388、license、testdata-scan、ui 7 全部通过；只跳过了 sensitive-words（本机没有词表），e2e 尚未实现。摘要是 `pr-4e28aa0201e9.json`，里面有 UNIT-ui-store，暂时没有 UNIT-ui-query（原因见开头）。

## 提交列表

```
c895eb8 docs(task): T080 CI summary
4e28aa0 feat(ui-core): the state layer: stream stores, subscriptions, UI store, queries
8b9742e feat(host-client): notify, and production builds skip zod for item.delta
88a1685 docs(task): T080 size estimate and split proposal
```

规模（相对 main，不含 notes 和 lock）：新增约 780、删除 68，其中测试 306。在约 950 的上限内。

## 遗留问题 / 范围外发现

- **需要同步 main**：合并 main 后 UNIT-ui-query 才会进入 CI 摘要。我没有自己合并。
- **给 T136**：ui-core 的测试文件不 import `node:*`，但用了 `@cupper/host-client/mock`。另外 ui-core 生产代码里没有 `innerHTML`、`<style>`、CSS 自定义属性，也没有 JSX 裸文案。
- **给界面任务**（T081、T082、T083、T086）：
  - 会话页用 `useStreams().session(id)` 拿 store，用 `useEntry(store.view, key)` 订阅条目；索引用 `useStreams().index`。
  - 草稿、附件、滚动位置用 `useUi()`，配合 zustand 的 `useStore` 读写。
  - 新的查询按键约定写，需要失效时在 `staleKeys` 里加一条。
- **构建提示**：rolldown 会提示 TanStack Query 文件头的 `"use client"` 指令不会保留。这只是提示，不影响构建和 CSP，没有处理。