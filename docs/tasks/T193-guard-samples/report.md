# T193 汇报

- 执行者：子 agent（工程师）
- 分支：`test/T193-guard-samples`（T052 worktree），PR #7
- 完成日期：2026-10-27

覆盖面从 135/220 提到 220/220，其中 216 个可选属性在去掉时也检查过；PR #7 的 8 个检查全绿。另外修了守卫走法的一个漏洞：`ToolCall`、`Plan`、`LateToolOutput`、`ErrorItem` 的属性原来从没被检查过。

## 做了什么

**1. 先修守卫的走法（e0a9905）**
- **漏洞**：带标签的联合里，每个分支写成"`$ref` 加旁边的标签常量"，比如 `ItemStarted` 的各种条目。`fits` 只看 `$ref` 指向的定义，不看旁边的标签，结果第一个分支 `user_message` 对什么值都"合适"。`tool_call`、`plan`、`late_tool_output`、`error` 条目都被当成 `user_message` 来走，所以 `ToolCall`、`Plan`、`LateToolOutput`、`ErrorItem` 的属性从没被检查过。
- **改法**：分支要 `$ref` 和旁边的标签都对得上才算合适。走法和覆盖面统计也补上了 `oneOf`，`DiffScope`、`Entry`、`GitTarget`、`WorkspaceView` 用的是它。
- **效果**：只修这一处，现有样例的覆盖面就从 135 到了 161/220，而且没有出 `null`。

**2. 全填实例（b03b351，新文件 `crates/cupper-protocol/tests/fixtures/full.rs`）**
- **怎么生成**：沿导出的 schema 生成实例，每个属性都填上：
  - `$ref`、`allOf` 合并；
  - `anyOf`/`oneOf` 的每个分支各出一个实例，`Unknown…` 和 `null` 分支除外；
  - 数组每个实例放一个元素，不同实例从不同的元素开始；`minItems` 要求更多时放够；
  - `const`、`enum` 取第一个值，整数取 1（夹在上下限内）。
- **ID 怎么给值**：标了 `x-id` 的字符串、或者定义名以 `Id` 结尾的字符串，都按 ID 处理：
  - 取说明里第一个反引号内容中到 `_` 为止的部分当前缀，比如 "Session: `s_` + UUIDv7" 得到 `s_01`；
  - 说明里没有反引号、提到 hex 的，给 64 位十六进制；
  - `StreamId` 给 `index`，路径给 `/w/a`。
- **根**：
  - `Event`，严格解码；
  - 方法目录三张表里每个方法的参数和结果，复用 `each_method!` 类型表；
  - `RpcError`。

  所有生成的实例都能被 Rust 类型读进来，没有一个读不进。

**3. 守卫改成先全填、再逐个去掉**
- **为什么不能只全填**：全填时每个可选字段都是 `Some`，写出来永远不会是 `null`。漏写了 `skip_serializing_if`，只有在字段为 `None` 时才会暴露。
- **做法**：守卫（`guard`）先读入、写出、走一遍。走的时候记下每个有值的可选属性的 JSON 指针，然后逐个去掉，各自再读入、写出、走一遍。现有样例也这样处理。读不进的实例和出 `null` 的位置一起收集，断言为空。
- **覆盖面统计**：照旧输出"出现过有值"，另加一项"去掉时检查过"（上一层走到了、这个属性不在）。"全部 `Option` 属性都出现过"现在由测试断言，以后新加的属性生成不到，测试会直接失败。
- **顺带的提速**：方法目录改成在 `Walk::new` 里读一次，不再每个样例都重新导出 schema。这个测试从约 19 s 降到 1 s 以内（调试构建）。

## 覆盖面

| | 原来（T190） | 只修走法 | 完成后 |
|---|---|---|---|
| 出现过有值 | 135 / 220 | 161 / 220 | **220 / 220** |
| 去掉时检查过 | — | — | **216 / 220** |

**没能在去掉时检查的 4 个属性**：`IndexView.agents`、`IndexView.clients`、`IndexView.host`、`RequestView.resolved`，都属于视图。原因是视图不在生成范围内：视图里的映射以各种 ID 为键（`projects` 以 `ProjectId` 为键，`sessions` 以 `SessionId`，`items` 以 `ItemId`，`requests` 以 `RequestId`），而 schema 只写了值的类型（`additionalProperties`），没写键是哪种 ID，生成不出 Rust 能读进的键。按值去猜键太脆弱，我没有做。这 4 个属性在 `views.json` 的样例里都有值，所以算"出现过"，但没有在 `None` 时检查过。任务书只要求方法和事件；视图是宿主自己写出的状态，不在契约的收发里。

## 负面样例（不提交，改回后通过）

| 临时去掉 `skip_serializing_if` 的字段 | 本任务的守卫 | T190 的守卫（main 上的测试） |
|---|---|---|
| `ToolCall.cwd`（`item.rs`，原来没覆盖到） | 失败，8 处 `null on optional ToolCall.cwd`：6 处来自现有样例（走法修正后才走得到），2 处来自全填实例 | 通过，没抓到 |
| `ErrorData.retry_after_ms`（`error.rs`） | 失败，只有全填实例抓到：`full error /data/retry_after_ms: null on optional ErrorData.retry_after_ms` | 通过，没抓到 |
| `ToolDiff.hunks` | 失败，`item.started`、`item.updated`、`item.completed` 的全填实例都抓到 | — |

## 规模

- `cargo xtask size`：**+293 −55（净 +238）**，停止线 300，没有越过。
- 分模块：走法修正 +12 −7；生成器和守卫 +281 −48。
- **比估计（约 195）多出约 100 行，原因有三**：
  1. 走法的漏洞是开工后才发现的，估计里没有它（+12）；
  2. 守卫要记下 JSON 指针才能逐个去掉，`Walk` 还要多记"去掉时检查过"，比预想的多；
  3. 新文件 `full.rs` 在写完第一个模块时还没加进 git，那次 `cargo xtask size` 没把它算进去（显示 +147），直到用 `git add -N` 加进去才看到 +295。这是我的疏忽，以后新文件一创建就先 `git add -N` 再统计。

  发现后压缩了几处 rustfmt 展开的写法，回到 293。

## 验证方式

**本机**
- `cargo test -p cupper-protocol` 全部通过。
- `cargo clippy -p cupper-protocol --tests -- -D warnings`、`cargo fmt --check` 都通过。
- 本机负载过高（load 约 100），没有跑 `cargo xtask ci protocol`，以 GitHub CI 为准。

**GitHub（守则第 9 条）**
- PR：https://github.com/boris-w/cupper/pull/7
- 最终运行：https://github.com/boris-w/cupper/actions/runs/40445383846 ，提交 e04f2a4。第 2 次运行（attempt 2）8 个检查全部通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。
- 中途的失败都是已记下的偶发用例，与本任务无关：
  - 上一轮 run 40441106791：
    - ts 连续 3 次挂在 `bridge/contract.test.ts` 的 `settle()` 固定等待上（T198）。T198 合进 main 后再推送，ts 通过。
    - rust 有一次挂在 `it_gw_sync_reconnect_replays_without_gaps`（T199 第 1 条），重跑通过。
  - 最终这一轮 run 40445383846：rust 第 1 次挂在 `it_nfr9_crash_under_load_kill9_again_and_again`（T199 第 3 条），重跑通过。

## 提交列表

```
e04f2a4 Merge branch 'main' into test/T193-guard-samples
b66d327 Merge branch 'main' into test/T193-guard-samples
8bae8a5 docs(T193): record the walk fix, the coverage and the negative checks
b03b351 test(protocol): the null guard checks every type with every field filled
e0a9905 fix(protocol): the null guard walks tagged branches and oneOf
30b3f3a docs(T193): record the checks, the plan and the estimate
```

两个合并提交是编排者合 main 时做的。工作区干净。

## 遗留问题 / 范围外发现

- **T190 的覆盖面数字有两处偏差**，以后引用 T190 的 135/220 时要注意：
  - 偏低：它只算"出现过有值"，一个属性只要上一层走到了、它自己不在，其实也已经检查过了，这部分没算进去；
  - 偏高：带标签联合的分支一律按第一个分支走，`ToolCall` 这几个类型实际上没被检查。
- **视图的 4 个属性没能在去掉时检查**：如果以后要补，得让视图映射的键类型进入 schema（例如导出 `propertyNames`）。这属于改导出，不在本任务范围内。
- **ID 前缀从定义说明里读**：生成器取说明里第一个反引号内容中到 `_` 为止的部分。以后新加的 ID 类型如果说明不按这个格式写，生成的实例会读不进，测试会明确失败并报出是哪个实例，不会悄悄漏过。
