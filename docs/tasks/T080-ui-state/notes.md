# T080 工作笔记

- 执行者：子 agent（前端工程师，安全）
- 日期：2026-10-20

## 读到的现状（开工前）

- **流与视图**：`HostClient.subscribe(stream, sink)` 已经按游标重连、处理缺口与快照；`IndexView`、`SessionView` 就是 `StreamSink`，`apply` 当场改视图并 `versions.touch`。帧解码后 `HostClient` 才 `port.ack`，所以"到达即应用、应用后确认"已经由 host-client 做到，store 要做的只是：把 sink 包一层，在变化后安排一次动画帧，在帧里对每个变过的视图调一次 `versions.flush()`。
- **校验**：现在每个事件被 zod 解析三次：`StreamSync.events` 的 `zEvent`（信封加数据）、`#apply` 里为记日志的 `isKnownEvent`、视图 `apply` 里的 `isKnownEvent`。"生产只校验非 delta"要在这三处对 `item.delta` 换成轻量的信封检查（`seq`、`from` 是整数），所以要改 host-client，并由 ui-core 在生产构建打开（`HostClientOptions` 加一项，传到 `StreamSync` 和视图）。
- **索引流名** `index`，会话流 `session/<id>`。mock 宿主对 `stream/subscribe` 回固定样例，截图页面不会因为订阅索引而多画东西，外框基线应当不变。
- **布局 store**：`LayoutStore` 是手写的 subscribe/getState；`prefs.set` 在主进程写完后先广播 `onChange`（也发给自己），再回 invoke 的结果。连按手柄键时，自己较早那次写入的回传晚于本地的新值到达，宽度跳回去一下。修法：每个键记下"已发出、还没听到回传"的写入序号队列；回传按写入顺序到达，队列里还有更新的写入时丢弃，失败的写入出队。
- **路由**：`RouterProvider` 用 `useState` 持有 `Route`；接管后放进 zustand，`useRouter()` 的 `{route, navigate}` 不变。
- **ESLint**：根目录的 `eslint.config.js` 只给 host-client 加了 import 限制。渲染进程、preload、ui-core 的安全与文案规则都还没有。规则可以全部用内置的 `no-restricted-syntax`、`no-restricted-properties`、`no-restricted-imports`、`no-eval`、`no-new-func` 写，不需要新的 ESLint 插件；负面样例用 ESLint 的 Node API（`lintText`）在 Vitest 里跑。
- **依赖**：要加 `zustand`、`@tanstack/react-query`（设计 5.1.3 的清单里有），锁精确版本。

## 开工前的规模估计

实现按以往的实际偏差乘 1.3，测试按写出来的估。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| host-client | `notify`；校验模式（`all` / `non_delta`）穿过 `HostClient`、`StreamSync`、`StreamView`；delta 的轻量信封检查 | 65 | 50 |
| 流 store | 包装 sink、按动画帧合并的 flush 调度（可注入，测试用手动帧）、`useEntry` 一类的 hook、加载中与失败状态 | 130 | — |
| 订阅策略 | 索引常驻；当前会话加 LRU 3；退订丢视图；缓存未命中路径；随路由切换 | 105 | — |
| 界面状态（zustand） | 路由、草稿、附件、滚动位置；`LayoutStore` 改由 zustand 承载、对外形状不变，加过期回传的丢弃；`RouterProvider`、`App` 与 Provider 接线 | 210 | — |
| 查询 | `QueryClient`、按事件失效的表（例如 `session.removed` 使归档列表失效、`turn.completed` 使该会话的 git 查询失效）、一个示例查询、Provider | 80 | 30 |
| `security/policies.ts` | 唯一的 `cupper-worker` 策略，`createScriptURL` 只放行 `app://cupper/assets/` | 50 | 20 |
| ESLint 规则 | 危险 API、`window.open`、ui-core 与渲染进程的 import 限制（含无前缀的 Node 内置模块）、`createPolicy` 只在 `policies.ts` 且名字只能是 `cupper-worker`、`<style>`/`insertRule`/`setProperty`/`cssText`/style 对象里的 `--` 键、JSX 裸文案与 `aria-label`/`title`/`placeholder`/`alt` 的字面量 | 110 | — |
| UNIT-ui-store | 到达即应用与按帧 flush、按条目通知、LRU 与退订后保留草稿和滚动位置、缓存未命中、重连按游标（mock 宿主）、过期 prefs 回传、生产只校验非 delta | — | 220 |
| UNIT-ui-lint-rules | `lintText` 的小夹具，每条规则一个会触发的负面样例，外加一个合规样例 | — | 80 |
| 依赖与导出 | `package.json`、`index.ts` | 10 | — |
| **合计** | | **约 760** | **约 400** |

合计约 **1,160 行**，超过约 900 的停止线。按任务书和守则第 8 条，开工前停下汇报，没有写代码。

## 拆分建议

| 拆分 | 内容 | 验收 | 估计 |
|---|---|---|---|
| **T080a 状态层**（留在关键路径上） | host-client 的 `notify` 与校验模式；流 store 与按帧 flush；订阅策略；zustand 界面状态（接管路由与布局 store，修过期回传）；Provider 接线；截图基线不变 | UNIT-ui-store | 约 760 |
| **T080b 安全规则与查询**（不在关键路径上，可与 T081 并行） | `security/policies.ts`；ESLint 安全、import、文案规则；TanStack Query 的客户端、按事件失效与示例查询 | UNIT-ui-lint-rules（查询的失效另加一个单元编号，或并进 UNIT-ui-store 由 b 补用例） | 约 400 |

- 依赖：T081、T082、T083 只需要 a；需要请求-响应类数据的界面任务（例如输入框的 `files/search`，T086）还要 b 的查询客户端。b 的规则落地后会检查已合并的界面代码，所以 b 最好在 T081 合并前后尽快合并；界面任务按 `CLAUDE.md` 里已有的代码约束写，规则落地时应当不需要返工。
- 两半改的文件基本不重叠：a 动 `packages/host-client/src/`、`packages/ui-core/src/state/`、`layout/`、`app/`；b 动 `eslint.config.js`、`packages/ui-core/src/security/`、`state/query.ts`。b 的 Provider 接线只在 `providers.tsx` 加一层 `QueryClientProvider`。
- 另一种：不拆，上限提到约 1,250。
