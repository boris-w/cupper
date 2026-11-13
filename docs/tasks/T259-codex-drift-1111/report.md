# T259 汇报

- 执行者：子 agent（CI 与工具）
- 分支：`chore/T259-codex-drift`
- 完成日期：2026-11-13
- PR：https://github.com/boris-w/cupper/pull/109
- CI：https://github.com/boris-w/cupper/actions/runs/43379643893 。`e33dcd1d` 上 8 个检查全部通过。
- 分支上的 nightly：https://github.com/boris-w/cupper/actions/runs/43379690389 ，结论 success。其中 `codex-drift (ubuntu-24.04)`（job 131734312060）是 `fetch ok`、`drift ok`、`2 passed, 0 failed`。判通过要求漂移和记录逐行相同，所以 linux-x64 上的漂移和本机 darwin-arm64 上记下的一致。

## 做了什么

1. **确认 npm 上的新版本**：
   - npm 的 `latest` 是 0.168.0（11-13），基线是 0.163.0。
   - 0.167.0 之后有三个版本没有记录：0.167.1（11-10）、0.167.2（11-11）、0.168.0。
   - nightly 从 11-11 起报 "not reviewed yet"，是因为记录按版本号查，`latest` 先后换成了这三个版本。工具只记录 `latest`，所以记的是 0.168.0。
2. **分清新出现的变化**：
   - 本机跑 `fetch`、`check`（darwin-arm64），记录之前 `check` 退出码 1：21 个方法的参数变了，没有方法消失，也没有方法变成 experimental。
   - 和 0.167.0 那条记录逐行比，多出 3 处。
   - 照 T246 的做法，我从 npm 取了 0.167.0、0.167.1、0.167.2 的平台包，核对了 integrity，同样只跑 `generate-json-schema`，再用 `drift.ts` 两两比较：
     - 0.167.0 → 0.167.1：没有变化；
     - 0.167.1 → 0.167.2：没有变化；
     - 0.167.2 → 0.168.0：正好是这 3 处；0.167.0 → 0.168.0 也是这 3 处。
3. **逐条查对 codex-acp 2.2.2 的源码**（只读）：

| 0.168.0 的新变化 | 涉及的方法 | 适配器的处理 | 影响 |
|---|---|---|---|
| `ItemCompletedNotification` 加了 `contentTokenCount`，可选、可为 null | `item/completed` | 通知经 `onUnhandledNotification` 直接按对象使用，不按 schema 校验；源码里没有这个字段 | 无 |
| `ThreadItemsListParams` 加了可选的 `deferOutput`（新定义 `ThreadItemOutputDeferral`，只有 `commandExecution` 一个布尔项） | `thread/items/list` | `threadItemPages` 只发 threadId、turnId、cursor、limit、sortDirection，不发 `deferOutput`，输出照旧随条目返回 | 无 |
| `ThreadListParams` 加了可选的 `currentAccountOnly`（默认 false） | `thread/list` | 三处调用（`listSessions`、`sessionIndexThreadListParams`、扫描）都不发这一项 | 无 |

4. **补比响应 schema**（工具不比；去掉 description、title 后比较 0.167.0 和 0.168.0）：
   - `thread/items/list`、`thread/items/read` 的 `ThreadItemEntry` 加了可选的 `outputDeferred`：只在请求了 `deferOutput` 时出现，适配器不读，也不调用 `thread/items/read`。
   - `config/read` 的 `Config` 加了可选的 `client_search_provider`：适配器调用 `config/read`，但只读模型提供方等字段。
   - 新增 `ModelProviderConfigWrite`、`ModelProviderList` 的响应：属于新方法，2.2.2 不用。
5. **写入记录**：用 `review <dir> T113 <结论>` 写进 `testdata/codex-drift/reviewed.json` 的 0.168.0 条目。结论写明了：
   - 0.167.1、0.167.2 和 0.167.0 相同；
   - 新出现的 3 处和响应的变化，以及各自为什么没有影响；
   - 其余 18 处与 0.167.0 的记录逐行相同，影响同那条结论。
   - 0.165.0–0.167.0 的记录、基线和已验证范围都没有动。
6. **`notes.md`**：写了版本表、两两比较的结果、逐条审阅表（附源码里的函数名）、响应比较，以及 npm 上最新的 codex-acp（仍是 2.6.0，依赖 `@openai/codex ^0.166.0`，另有 2.6.1 的几个 preview）。

## 关键取舍

- **另取三个旧版本两两比较**：工具只拿最新版本和基线比，看不出变化出自哪个版本。逐版比较后可以确认：0.167.1、0.167.2 没有协议变化，新变化全部来自 0.168.0。这样记录里能写准，不用靠对比两份文字报告去猜。
- **只记录 0.168.0**：工具每晚只查 `latest`。0.167.1、0.167.2 已经不是 `latest`，它们的漂移和已审的 0.167.0 相同，结论里写明了这一点，不另加条目。
- **没有需要单列给 T113 的新问题**：3 处新变化都是可选字段，适配器既不发也不读。

## 验证方式

- **本机**：
  - 记录之后 `check` 退出码 0，并打印审阅结论。
  - `cargo xtask ci codex-drift --in nightly` 通过（fetch ok、drift ok，2 passed）。
  - `cargo xtask ci testdata-scan` 5 项全部通过；prettier 检查 `reviewed.json` 和 `notes.md` 都通过。
  - `cargo xtask size` 是 +0 −0，代码改动为 0。
- **隔离**：
  - codex 只运行了 `app-server generate-json-schema`，每次的环境里只有 `HOME`、`CODEX_HOME`，都指向各自工作目录里的空目录，跑完只多了 `tmp/`。
  - 没有登录，没有发 prompt，没有读写 `~/.codex`。
  - 外发请求只到 npm，User-Agent 是 `cupper-research`。
  - 下载的包各放在新目录里，脚本另放，都在临时目录，用完已删除。
- **对照验收标准**：
  - [x] 记录已写入 `reviewed.json`，结论逐条说明了对 codex-acp 2.2.2 的影响。
  - [x] 分支上的 nightly `codex-drift` 判通过：run 43379690389，job 131734312060。
  - [x] PR #109 上 8 个检查全部通过，链接见上。

## 提交列表

```
e33dcd1d docs(task): T259 notes: codex 0.168.0's drift, item by item
64bfdee8 test(codex-drift): the drift of codex 0.168.0, reviewed for T113
```

## 遗留问题 / 范围外发现

1. **给 T113**：没有新问题。旧记录里的两处影响仍然成立：
   - `imageGeneration` 遇到 fileId 形式时，`imageResult` 抛 TypeError；
   - `session/load` 回放 fileId 形式的音频时显示不对。
   - 用 `write` 重写基线后，0.165.0–0.168.0 四条记录都会失效。
2. **工具的行为**：记录按版本号查，所以没有协议变化的补丁版本（这次的 0.167.1、0.167.2）也会让 nightly 报未审，直到有人记录。要不要在漂移和上一条记录逐行相同时自动视为已审，另开任务定；我没有改工具（任务书不做）。
3. worktree 保留着。
