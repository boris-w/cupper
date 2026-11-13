# T259 notes

## 本机运行（darwin-arm64，2026-11-13）

- `fetch`：npm 的 `latest` 是 0.168.0（11-13 发布），基线是 0.163.0。codex 平台包和 codex-acp 2.2.2 的 tarball 都对上了 integrity。
- `check`（记录之前）：退出码 1，21 个方法的参数变了，提示 "not reviewed yet"。没有方法消失，也没有方法变成 experimental。
- **0.167.0 之后 npm 上的版本**：

| 版本 | 发布时间 |
|---|---|
| 0.167.1 | 11-10 |
| 0.167.2 | 11-11 |
| 0.168.0 | 11-13 |

  三个都没有记录。nightly 从 11-11 起报 "not reviewed yet"，是因为记录按版本号查，`latest` 先后是 0.167.1、0.167.2、0.168.0。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有读写 `~/.codex`，没有登录，没有发 prompt。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包各放在一个新的目录里，脚本放在另一个目录，都在临时目录，不进仓库。

## 哪些是新出现的

和 0.167.0 那条记录逐行比，0.168.0 多出 3 处：

```
item/completed: params changed
  ItemCompletedNotification changed +property contentTokenCount
thread/items/list: params changed
  ThreadItemOutputDeferral added +property commandExecution
  ThreadItemsListParams changed +property deferOutput
thread/list: params changed
  ThreadListParams changed +property attachedToThreadId +property currentAccountOnly   （0.167.0 记录里只有 attachedToThreadId）
```

照 T246 的做法，我从 npm 取了 0.167.0、0.167.1、0.167.2 的平台包，核对了 integrity，同样只运行 `generate-json-schema`，再用 `drift.ts` 的 `summarize`、`compare` 两两比较：
- 0.167.0 → 0.167.1：没有变化；
- 0.167.1 → 0.167.2：没有变化；
- 0.167.2 → 0.168.0：正好是上面 3 处；
- 0.167.0 → 0.168.0：也是这 3 处。

摘要对每个定义取哈希，所以嵌套里的变化也会体现为"changed"，只是描述不细。两两比较没有变化，就说明 0.167.1、0.167.2 涉及的定义和 0.167.0 完全相同。

## 逐条审阅（codex-acp 2.2.2，`package/dist/index.js`，只读）

| 变化 | 涉及的方法 | 适配器怎么用 | 影响 |
|---|---|---|---|
| `ItemCompletedNotification` 加了 `contentTokenCount`，可选、可为 null，非必填 | `item/completed` | 通知经 `onUnhandledNotification` 直接按对象使用，不按 schema 校验；源码里没有 `contentTokenCount` | 无 |
| `ThreadItemsListParams` 加了可选的 `deferOutput`（新定义 `ThreadItemOutputDeferral`，只有 `commandExecution` 一个布尔项，"把成功命令的非空输出另外取"） | `thread/items/list` | `threadItemPages` 的两次调用只发 `threadId`、`turnId`、`cursor`、`limit`、`sortDirection`，不发 `deferOutput`，所以输出照旧随条目返回 | 无 |
| `ThreadListParams` 加了可选的 `currentAccountOnly`（默认 false） | `thread/list` | 三处调用（`listSessions`、`sessionIndexThreadListParams`、扫描）都不发这一项 | 无 |

## 补充：响应 schema（工具不比）

把 `exp` 下的 `*Response.json` 和 `v2/*Response.json` 去掉 description、title 后，比较 0.167.0 和 0.168.0：

- **变了 3 个**：
  - `v2/ThreadItemsListResponse.json`、`v2/ThreadItemsReadResponse.json`：`ThreadItemEntry` 加了可选的 `outputDeferred`（"保存的命令输出被省略了，用 `thread/items/read` 取完整条目"）。它只在请求了 `deferOutput` 时出现，适配器不请求，也不读这个字段；`thread/items/read` 适配器也不调用。
  - `v2/ConfigReadResponse.json`：`Config` 加了可选的 `client_search_provider`（新的 `WebSearchProvider`，`standalone` 或 `search_mcp`）。适配器调用 `config/read`，但只读模型提供方等字段。
- **新增 2 个**：`ModelProviderConfigWriteResponse`、`ModelProviderListResponse`，属于新方法，2.2.2 不用。
- 没有删除的。

## 其余 18 处

与 0.167.0 的记录逐行相同，影响同那条结论：
- `imageGeneration` 遇到 fileId 形式时，`imageResult` 抛 TypeError：实时通知里丢图，`session/load` 回放时整个加载失败；
- `session/load` 回放 fileId 形式的音频时，显示成 `[@audio](undefined)`；
- 其他变化无影响。

## 参考

npm 上最新的 codex-acp 仍是 2.6.0（依赖 `@openai/codex ^0.166.0`），另有 2.6.1 的几个 preview。没有升级，给 T113 参考。

## 规模

只改了 `testdata/codex-drift/reviewed.json`（记录数据）和这份笔记，都不计入规模，代码改动为 0。
