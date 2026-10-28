# T114 notes

## 开工前的估计

停止线 300，硬停止线 380；基线数据（`testdata/`）不计入。

| 模块 | 行数 |
|---|---|
| `tools/codex-drift/src/drift.ts`：纯核心，方法表、参数的定义闭包、摘要、和基线比对 | 70 |
| `tools/codex-drift/src/main.ts`：命令行，`fetch`（npm）、`check`（离线比对）、`write`（重写基线）；curl、integrity、tar、运行 codex | 80 |
| `package.json`、`tsconfig.json`、`vitest.config.ts` | 33 |
| `crates/xtask/src/ci/jobs/codex_drift.rs` 和登记 | 35 |
| `nightly.yml`、`.prettierignore` | 6 |
| 测试：`drift.test.ts`（UNIT-codex-drift） | 65 |
| 合计 | 约 290 |

## 摸底（2026-10-28）

- npm 上 `@openai/codex` 的主包只有一个 node shim，原生二进制在平台包里。平台包是同一个包名下的版本，版本号带平台后缀：`@openai/codex@0.165.0-linux-x64`、`@openai/codex@0.165.0-darwin-arm64`，各有自己的 `dist.integrity`。tarball 里二进制的位置是 `package/vendor/<triple>/bin/codex`（linux-x64 是 `x86_64-unknown-linux-musl`，darwin-arm64 是 `aarch64-apple-darwin`，与 shim `bin/codex.js` 的选法一致）。tarball 约 140 MB，解开 340–450 MB，只解出二进制一个文件。
- `latest` 当天是 0.165.0，超出基线（已验证上限）0.163.0。
- codex-acp 2.2.2 的 tarball 的 SRI 与 `runtime/manifest.json` 一致；`package/dist/index.js` 是单文件。
- `generate-json-schema` 一次约 50 ms，不需要 PATH。**它会写 `CODEX_HOME`**：从第一次运行起就用 `env -i` 只给临时的 `HOME` 和 `CODEX_HOME`，跑完临时 `CODEX_HOME` 下多了 `tmp/arg0-macos` 目录。所以工具运行 codex 时环境里只有 `HOME` 和 `CODEX_HOME` 两项，都指向工作目录里的空目录，碰不到 `~/.codex`。
- 原型漏了一类参数：16 个请求的 `params` 不是 `$ref`，而是 `{"type": "null"}` 或 `anyOf: [{$ref}, null]`（如 `account/rateLimits/read`）。原型只取 `params.$ref`，这些方法的参数定义没有进闭包。迁移时改为从整个 `params` schema 出发找 `$ref`，并且对 `params` schema 本身也取哈希。
- 三个文件（`ClientRequest.json`、`ServerNotification.json`、`ServerRequest.json`）里同名定义的内容一致，方法名也没有跨文件重复，所以摘要里定义按名字放一张表，方法按名字放一张表。
- 方法的响应 schema 不在 `ClientRequest.json` 里（是 `v2/<Name>Response.json` 这类单独的文件，没有方法到响应的映射），照原型和任务书只比参数。

## 规模超线（2026-10-28）

工具写完时实测 492 行（`drift.ts` 174、`main.ts` 169、`drift.test.ts` 115、配置 34），已经超过硬停止线 380；加上 xtask 作业和 workflow 预计约 530。开工前的估计照原型的密度算，漏了 TS 的类型、prettier 的 100 列折行、三个子命令的下载核对与解包，以及原型漏掉的 null/anyOf 参数。已压过一轮（530 → 492），停下报给编排者裁定。编排者选 A：停止线改为 540，硬停止线 600（任务书"补充裁定"）。

## 本机实测（darwin-arm64，2026-10-28）

`write`：codex-acp 2.2.2 引用 127 个方法（与 SP-2 一致），其中 3 个只在 experimental 里：`thread/backgroundTerminals/list`、`thread/backgroundTerminals/terminate`、`thread/settings/update`；参数闭包共 332 个定义。基线约 130 KB，`tools/sanitize scan` 干净。下载 codex 平台包约 140 MB，整个 `write` 约 6 秒。

`fetch` + `check`，npm `latest` = 0.165.0，对 0.163.0 的基线（照实记录，未改基线），14 个方法的参数变了，退出码 1：

```
codex 0.165.0 against 0.163.0, the 127 methods codex-acp 2.2.2 uses:
  account/rateLimits/updated: params changed
    PlanType changed +enum sci +enum sci_plus
  account/updated: params changed
    PlanType changed +enum sci +enum sci_plus
  error: params changed
    CodexErrorInfo changed +enum modelNotFound
  hook/completed: params changed
    HookEventName changed +enum postModelRequest +enum preModelRequest
  hook/started: params changed
    HookEventName changed +enum postModelRequest +enum preModelRequest
  item/completed: params changed
    ImageGenerationInputImage added +property path +property sha256 +required path +required sha256
    ThreadItem changed +imageGeneration: property inputImages -imageGeneration: property result -imageGeneration: required result
  item/started: params changed
    ImageGenerationInputImage added +property path +property sha256 +required path +required sha256
    ThreadItem changed +imageGeneration: property inputImages -imageGeneration: property result -imageGeneration: required result
  thread/fork: params changed
    ThreadForkParams changed +property attachToSource +property experimentalDisablePredictionMcp
  thread/list: params changed
    ThreadListParams changed +property attachedToThreadId
  thread/started: params changed
    CodexErrorInfo changed +enum modelNotFound
    ImageGenerationInputImage added +property path +property sha256 +required path +required sha256
    ThreadItem changed +imageGeneration: property inputImages -imageGeneration: property result -imageGeneration: required result
  turn/completed: params changed
    CodexErrorInfo changed +enum modelNotFound
    ImageGenerationInputImage added +property path +property sha256 +required path +required sha256
    ThreadItem changed +imageGeneration: property inputImages -imageGeneration: property result -imageGeneration: required result
  turn/start: params changed
    TurnStartParams changed +property responsesapiClientMetadataPatch
  turn/started: params changed
    CodexErrorInfo changed +enum modelNotFound
    ImageGenerationInputImage added +property path +property sha256 +required path +required sha256
    ThreadItem changed +imageGeneration: property inputImages -imageGeneration: property result -imageGeneration: required result
  turn/steer: params changed
    TurnSteerParams changed +property responsesapiClientMetadataPatch
```

没有方法消失，也没有方法从 stable 变成 experimental。其余是新增的可选字段和枚举值，只有一条值得 T113 注意：`ThreadItem` 的 `imageGeneration` 变体去掉了 `result` 字段，改成了 `inputImages`。codex-acp 2.2.2 的 `ImageGenerationReporter` 还在读 `item.result`，放进 `rawOutput.result`，所以在 0.165.0 上这一项会变成 `undefined`。不会崩溃，但图像生成的结果会丢失。

第一版摘要把 tagged union 各个变体的属性合并在一起，这一条只显示成 `-required result`：其他变体（如 `mcpToolCall`）也有 `result`，掩盖了"属性被删掉"。现在变体内的属性和必填项都带上变体的标签，例如 `imageGeneration: property result`，并补了一个用例。

工作目录里的 `HOME`/`CODEX_HOME` 跑完只多了 `tmp/arg0-macos`，没有别的文件。

## `cargo xtask ci codex-drift --in nightly`（本机，提交 318ff70）

- 当天 npm 的真实版本：`fetch` 通过（latest 0.165.0），`drift` 判失败，输出与上面相同，`codex-drift FAIL 1 passed, 1 failed (3.5s)`。基线没有改。
- 用不通的代理 `HTTPS_PROXY=http://127.0.0.1:9` 模拟断网（照 T006）：`fetch` 试 3 次都失败，`fetch` 和 `drift` 都记为跳过，原因写的是 "npm could not be reached in 3 tries: the drift is undetermined"，作业通过，`skipped steps: codex-drift / fetch, drift`，约 21 秒。

## 提交后的规模

`cargo xtask size`：+551 −5（净 +546）。`tools/codex-drift` 507（其中测试 127），`crates/xtask` +38 −3，`.github` +4 −2，`.prettierignore` +2。比裁定后的停止线 540 多 6，低于硬停止线 600。
