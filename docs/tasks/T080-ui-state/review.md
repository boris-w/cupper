# T080 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增约 780、删除 68（测试 306；不含 notes、lock），在约 950 的上限之内。开工前估全量约 1,160，拆出 T136（计划 v1.3.28，查询留在本任务）。
- **CI**：执行者的摘要 `pr-4e28aa0201e9.json` 通过，但当时分支上没有 UNIT-ui-query 的编号。
- **合并 main（含 T128）时有冲突**：`ui-core` 的 `package.json`、`App.tsx`、`index.ts`、`pnpm-lock.yaml`，由编排者解决。
  - 依赖和导出取两边的并集；
  - `App.tsx` 以本任务的 Provider 结构为准，再包上 T128 的 `TooltipProvider`；初始路由仍归本任务的 `LayoutStore`；
  - lockfile 取 main 的，再用 `pnpm install` 加入本任务的两个依赖。
- 解决后跑了 `xtask ci ts`（403 通过）和 `xtask ci ui`（7 通过），再在本 worktree 重跑 `cargo xtask ci pr --record`：`pr-1aa3b5310de2.json` 通过（rust 1795、protocol 185、ts 403、license、testdata-scan、ui 7），UNIT-ui-store、UNIT-ui-query 都在结果清单里。

## 逐项意见

1. **"按帧"只用于通知组件**：视图在消息到达时同步应用，host-client 处理完帧后才确认，所以"应用后确认"不变。`Frames` 每帧对每个变过的视图调一次 `versions.flush()`。符合设计 5.7.6。
2. **订阅策略**：
   - 索引常驻；当前会话加最近 3 个保持订阅，更早的退订并丢弃视图；草稿和滚动位置留在 zustand 里；
   - 缓存未命中先显示加载中，订阅失败时 `error` 有值；
   - 重连后按游标重新订阅。
   - 用例覆盖完整。
3. **会话的 store 在渲染时建好**：放在 Effect 里的话，会话页第一次渲染拿不到 store。`start()` 会重新订阅 `close()` 之前在屏幕上的会话，所以 Effect 重跑时也不会丢。理由清楚。
4. **生产构建只校验非 delta 事件**：
   - `item.delta` 只查 `seq`、`from` 是不是整数，信封、日志检查、视图三处都不再用 zod；其余事件照旧全部校验；
   - 真坏的 delta 在 fold 时抛错，`StreamSync` 不带游标重新订阅，数据不会悄悄错下去。
   - 用例里坏的 `turn.started` 照样被记为数据残缺。
5. **过期 prefs 回传**：每个偏好键一个"已发出、未收到回传"的写入序号队列，写入失败的出队；别的窗口的修改照常应用。T084 留下的闪动修掉了。
6. **查询**：
   - 不按时间过期、不重试，由事件失效；
   - 键的约定（全局 `[method, …]`，会话 `["session", id, method, …]`）让一个事件能使一个会话的全部查询失效；
   - 重新连上后全部失效。
7. **测试不用 Node 模块**：主动避开了 T136 的 import 规则。

## 交接

- **T081、T082、T083、T086**：
  - 会话页用 `useStreams().session(id)` 拿 store，索引用 `useStreams().index`；
  - 条目用 `useEntry(view, key)`；
  - 草稿、附件、滚动位置用 `useUi()`；
  - 新的查询按键约定写，需要失效时在 `staleKeys` 里加一条。
