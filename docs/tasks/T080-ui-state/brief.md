# T080 ui-core I：状态层与安全规则

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师，安全）。T054、T060、T098、T044、T109、T038 的执行者接手
- 分支：`feat/T080-ui-state`
- 依赖：T072（已合并）。T081、T082、T083、T086 依赖本任务。**本任务在剩余关键路径上（松弛为 0）**：T080 → T081 → T090 → T091 → T099

## 背景

`packages/ui-core` 已由 T084 建立：token、三栏外框、Providers、小状态路由、页面骨架与插槽、一个只管布局的 store。截图设施（T118）已合并；基础组件与行类型注册表（T128）在并行进行。
- host-client 那边，连接状态机（T071）、TS 版视图 reducer 与 `versions`（T072）都已就位。
- 界面还没有把流接成 store，没有订阅策略，渲染进程也还没有 ESLint 安全规则。

本任务做状态层和安全规则，之后侧边栏（T081）、消息流（T082）、请求面板（T083）、输入框（T086）都建在这上面。

## 范围

**做**（计划 T080 一行；设计 5.7.6、5.7.8 的 Trusted Types、样式与代码规则）：
1. **流 store**：
   - 每条已订阅的流一个 store：游标、视图（host-client 的 `IndexView`、`SessionView` reducer）；
   - **消息到达即应用、应用后确认**，只有"通知组件重新渲染"合并到动画帧：每帧调用一次 `versions.flush()`；
   - 组件经 `useSyncExternalStore` 按条目订阅：`versions.subscribe(key, cb)`、`versions.of(key)`，键如 `item:<id>`、`turn:<id>`、`tail`、`session`；
   - `complete` 显示"加载中"，`error` 显示订阅失败（T072 的交接）。
2. **订阅策略**：
   - 索引流常驻；当前会话加最近 3 个会话（LRU）保持订阅；
   - 其余会话退订并丢弃视图，但保留草稿和滚动位置（界面状态）；
   - 缓存未命中的路径（从通知或"下一个需要处理的会话"跳过去）。
3. **界面状态**（zustand）：选中的会话、面板开合与尺寸、草稿、附件、滚动位置。
   - 接管 T084 的小状态路由和布局 store，形状照 T084 的 `notes.md`，不改对外的形状；
   - 修掉"快速连按手柄键时宽度闪一下"：按序号丢弃过期的 prefs 回传。
4. **查询**：TanStack Query 管请求-响应类数据（`git/*`、`files/search`、`agents/capabilities`、`session/list_archived`），收到相关事件时失效。本任务只搭好客户端和失效机制，配一个示例查询；具体查询由各界面任务加。
5. **校验**：开发和测试构建对每条消息做 zod 校验；生产构建只校验非 delta 的事件（T071、T087 的交接）。
6. **`HostClient.notify`**：加上发通知的方法（例如 `client/presence`），供在场上报使用。`sendBlob` 留给需要它的任务（T145）。
7. **`ui-core/src/security/policies.ts`**：
   - 唯一能创建 Trusted Types 策略的地方；
   - 只有一个具名策略 `cupper-worker`，`createScriptURL` 只放行 `app://cupper/assets/` 下的同源脚本；
   - 禁止名为 `default` 的策略。
8. **ESLint 安全与文案规则**（渲染进程、preload、ui-core）：
   - 禁止 `innerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`eval`、`new Function`、`window.open`；
   - `ui-core` 禁止 import `electron`、`node:*`；
   - `trustedTypes.createPolicy` 只能出现在 `policies.ts`，且名字不能是 `default`；
   - agent 给出的字符串不能进入 `<style>` 文本、`insertRule`、CSS 自定义属性：能静态检查的部分做成规则，不能的写进 `notes.md`，由评审把关；
   - 界面文案一律走 `@cupper/i18n`：例如禁止 JSX 里的裸字面量文本，按需设白名单。
   - 每条规则都要有负面样例，证明它会触发（UNIT-ui-lint-rules）。

**不做**：
- 外框与页面（T084，已合并）、基础组件（T128）；
- 侧边栏、消息流、请求面板、输入框的组件（T081、T082、T083、T086）；
- `sendBlob`（T145）。

## 验收标准

- [ ] UNIT-ui-store：
  - 到达即应用、按条目通知、每帧一次 flush；
  - LRU 3 的订阅与退订，退订后草稿和滚动位置仍在；
  - 缓存未命中；重连后按游标重新订阅（经 host-client 的 mock）；
  - 过期 prefs 回传被丢弃；
  - 生产构建只校验非 delta 事件。
- [ ] UNIT-ui-lint-rules：每条规则的负面样例都触发；现有代码全部通过（必要的例外逐条写明理由）。
- [ ] 截图回归（T118 的设施）照常通过；状态层接入后外框的基线不应变化，变了要说明原因。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T080 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T080-ui-state/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T080 的行；开工前逐行读一遍）

- T074：渲染进程和 preload 的 ESLint 安全规则和 ui-core 一起加。
- T071、T087：生产构建只校验非 delta 事件；`client.counters` 是全部流的计数之和；`StreamSink` 的约定见 T072 的任务书。
- T071：`HostClient` 还没有 `notify`、`sendBlob`。
- T072：`versions.flush()`、`subscribe`、`of`、`complete`、`error`。
- T084：路由与布局 store 的形状；过期 prefs 回传；import 限制规则。

## 注意

- 代码约束：`ui-core` 不 import `electron`、`node:*`；渲染进程不用 `innerHTML` 一类的 API；只有 `policies.ts` 可以创建 Trusted Types 策略。
- 新依赖锁定精确版本（zustand、TanStack Query 等，设计 5.1.3 的清单）。
- 不运行真实的 Claude Code、Codex CLI；用例用 host-client 的 mock 宿主。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 900 行（包括贴线）就开工前停下汇报，并给出拆分建议。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`07-desktop.md` 5.7.6、5.7.8；`01-repo-build-ci.md` 5.1.3、5.1.7；`03-protocol-v1.md` 5.3.3（流与同步）。
- `docs/tasks/` 下 T071、T072、T074、T084、T087、T118 的 `notes.md` 和 `review.md`。
- `packages/host-client/src/`（`client.ts`、`views/`）、`packages/ui-core/src/`。

## 工作方式

在原来的 T054 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T080-ui-state main`。T038 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
