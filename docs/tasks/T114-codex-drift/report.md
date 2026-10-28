# T114 汇报

- 执行者：子 agent（工程师）
- 分支：`ci/T114-codex-drift`
- 完成日期：2026-10-28

**CI 状态（交接时）**：nightly 运行 https://github.com/boris-w/cupper/actions/runs/40640601300 里的 `codex-drift (ubuntu-24.04)` 已通过（job 122657595505：`fetch ok`、`drift ok`、`2 passed, 0 failed`）。在 linux-x64 上，0.165.0 的漂移和在 darwin-arm64 上记录的逐行一致，说明这些方法的 schema 与平台无关。PR #19 https://github.com/boris-w/cupper/pull/19 的 8 个检查里，4 个 ubuntu 作业（license、protocol、testdata-scan、ts）已通过；4 个 macOS 作业（rust、ui、e2e、compat-framework）交接时还在跑，结果请看 PR。

## 做了什么

1. **`tools/codex-drift`**：由 SP-2 的 `codex-schema-drift.mjs` 迁移而来，写法照 `tools/` 下其他 TS 工具：Node 直接运行 `.ts`，包的 `test` 跑 Vitest。
   - `drift.ts` 是纯核心，不做 I/O：
     - 从 codex 的 `ClientRequest.json`、`ServerNotification.json`、`ServerRequest.json` 取出方法表；
     - 只保留适配器源码里以字符串字面量出现的方法；
     - 每个方法记下种类、是否 stable、`params` schema 的哈希、参数经 `$ref` 能到达的定义；
     - 每个定义记下哈希（键排序、去掉 description）和"特征"：属性名、必填项、枚举值，tagged union 的变体带上标签，例如 `imageGeneration: property result`；
     - 和基线比对；`render` 生成逐行报告，`reviewed` 判断漂移是否已审阅。
   - `main.ts` 是命令行，四个子命令：
     - `fetch <dir>`：问 npm 要 `latest`。如果它不是基线的版本，就下载 manifest 锁定的 codex-acp tarball，以及本平台的 `@openai/codex@<版本>-<平台>` tarball。已有且 integrity 相符的文件不重下。
     - `check <dir>`：离线运行。先核对 integrity，只解出 `dist/index.js` 和 codex 二进制，运行两次 `app-server generate-json-schema`（带和不带 `--experimental`），然后比对。
     - `write <dir>`：按 manifest 的已验证上限重写基线。
     - `review <dir> <task> <verdict>`：把 `check` 发现的漂移记为已审阅。
   - curl 的参数和 `xtask runtime-pack` 相同：`-q`、只走 https、User-Agent 用 `cupper-research`、带重试，只访问 `registry.npmjs.org`。
   - codex 运行时环境里只有 `HOME` 和 `CODEX_HOME` 两项，都指向工作目录里的空目录。
2. **基线** `testdata/codex-drift/baseline.json`（由 `write` 生成）：codex-acp 2.2.2 引用的 127 个方法（其中 3 个只在 experimental 里），在 codex 0.163.0 里的参数摘要，共 332 个定义，并记下这两个版本号。如果 manifest 的 codex-acp 版本或已验证上限和基线对不上，`check` 判失败，提示用 `write` 重写。
3. **已审阅的漂移**（编排者补充的设计）`testdata/codex-drift/reviewed.json`：每个新版本只报警一次。
   - 每条记录：版本号、对比的基线、审阅日期、跟进的任务、结论，以及当时的漂移原文。
   - 最新版本有记录，而且基线和漂移都逐行相同：判通过，并列出漂移和结论。
   - 没有记录，或者漂移变了：判失败。
   - 0.165.0 已经记录，跟进任务是 T113，结论里写了 `imageGeneration` 的发现。
4. **接进 `xtask ci nightly`**：
   - 新增 `crates/xtask/src/ci/jobs/codex_drift.rs`，作业登记从 `None` 改为实现；`nightly.yml` 加一行 `codex-drift`，runner 是 `ubuntu-24.04`，超时 30 分钟。
   - `fetch` 单独成一步，重试规则复用 license 作业的 `retry` 和 `FETCHES`，最多 3 次。3 次都失败时，`fetch` 和 `drift` 都记为跳过，原因写"npm could not be reached in 3 tries: the drift is undetermined"，不判为漂移。
   - `drift` 不联网，未审阅的漂移会让作业失败。
   - 权限仍是 `contents: read`，没有新增密钥，没有开 issue。
5. **UNIT-codex-drift**（`tools/codex-drift/src/drift.test.ts`，8 个用例，用合成的 schema 和适配器片段，不联网）：
   - 摘要的内容；
   - 没有变化；
   - 方法消失；
   - stable 变成 experimental；
   - 参数加字段、删字段、新增必填项（包括定义整个离开闭包）；
   - tagged union 的某个变体少了字段；
   - 已审阅的漂移：相同时通过，基线或漂移不同时不通过；
   - 适配器用了基线里没有的方法。

## 关键取舍

- **拉取和比对分两步**，照 license 作业的做法：网络失败只会出在 `fetch`，可以重试，也可以判为"不确定"；integrity 不符、漂移这类真问题只在离线的 `drift` 里出现，不会被吞掉。代价是 `fetch` 里非网络的 bug 也会被当成"不确定"跳过，好在跳过的步骤每晚都会写进摘要。
- **3 次都失败时不回退**：license 作业可以退回缓存的公告库，这里没有对应的东西，因为不联网就不知道最新版本是什么。
- **下载用 curl，不用 Node 的 fetch**：参数和 runtime-pack 一致；140 MB 的 tarball 直接写盘；本机走代理时只要设 `HTTPS_PROXY`。
- **摘要只存哈希和特征，不存完整 schema**：基线约 130 KB，PR 的 diff 里能直接看出字段增删。特征看不出的变化（比如只改了类型）由哈希发现，报告里显示为 `changed`。
- **比原型多做的两处**：
  - 16 个请求的 `params` 是 `null`，或者是 `anyOf` 里一个 `$ref` 加 `null`，原型漏掉了这些参数的定义。现在从整个 `params` 出发找 `$ref`。
  - tagged union 的变体带上标签。否则 0.165.0 里 `imageGeneration` 去掉 `result` 这一条，会被其他变体的 `result` 盖住。
- **只比参数，不比响应**：方法到响应 schema 没有映射，这点照原型和任务书。
- **已审阅漂移按"基线版本 + 漂移逐行原文"匹配**：有任何变化都要重新审阅；基线一旦重写，旧记录自然失效。

## 验证方式

- 本机：`tools/codex-drift` 的 tsc、ESLint、Prettier、Vitest（8 个用例）都通过。`cargo clippy -p xtask --all-targets -D warnings`、`cargo fmt --check` 通过，`cargo test -p xtask` 通过。`trace-ids check` 一致，`tools/sanitize scan testdata` 干净。
- 对照验收标准：
  - [x] UNIT-codex-drift 通过，已登记在 `tests/ids.toml`。T114 段原本就有这条，suite 是 `pr`，在 `ts` 作业里跑。
  - [x] 本机用当天 npm 的真实版本跑了 `cargo xtask ci codex-drift --in nightly`，结果写在 `notes.md`。0.165.0 超出了基线 0.163.0：
    - 记录之前判失败：14 个方法的参数变了，没有方法消失，也没有方法变成 experimental；
    - 记录之后判通过，并列出漂移和结论（be077e1）；
    - 基线没有为了通过而改动；
    - 用不通的代理模拟断网，两步都跳过，作业通过。
  - [x] GitHub 上手动触发的 nightly：https://github.com/boris-w/cupper/actions/runs/40640601300 ，`codex-drift (ubuntu-24.04)` 通过。
  - [x] 没有新增 action，已有的 action 都按 SHA 锁定，权限只读，没有新增密钥。
  - [ ] PR 上的 8 个检查（https://github.com/boris-w/cupper/pull/19 ）：交接时 ubuntu 的 4 个已通过，macOS 的 4 个还在跑。
- 隔离：codex 从第一次运行起就在隔离的 `HOME` 和 `CODEX_HOME` 下运行。手动运行时用 `env -i`，工具里显式传 `env`。`generate-json-schema` 会往 `CODEX_HOME` 里写 `tmp/arg0-macos`，这只发生在隔离目录里。没有读写 `~/.codex`，没有登录，没有发 prompt。外发请求只有 npm，不带个人信息。
- 规模：`cargo xtask size` 是 +598 −5（净 +593），按裁定后的线（停止线 540，硬停止线 600）在硬线以内。`tools/codex-drift` 553 行，其中测试 141 行；`crates/xtask` +39 −3；`.github` +4 −2；`.prettierignore` +2。开工前估计约 290，第一次超线时已停下报告；编排者改了线，之后又加了"已审阅的漂移"。

## 提交列表

```
6fc1bec docs(T114): reviewed drifts, the local run with them, and the size
be077e1 refactor(codex-drift): plainer messages, a shorter gone-method case
612b6a0 test(codex-drift): the drift of codex 0.165.0, reviewed for T113
607b26b feat(tools): codex-drift fails once per codex version
5c873d6 docs(T114): the nightly step run locally, and the size
318ff70 ci(xtask): codex-drift runs every night on ubuntu
1f1dbb3 test(codex-drift): the baseline, codex-acp 2.2.2 on codex 0.163.0
dc06837 feat(tools): codex-drift compares the app-server methods codex-acp uses
6d6ef76 docs(T114): record the estimate, what npm and codex give, and the local run
```

## 遗留问题 / 范围外发现

1. **给 T113**：0.165.0 里 `ThreadItem` 的 `imageGeneration` 变体去掉了 `result`，改成了 `inputImages`。codex-acp 2.2.2 的 `ImageGenerationReporter` 还在读 `item.result`，所以图像生成的结果会丢失，但不会崩溃。其余 13 个方法的变化都是新增的可选字段和枚举值，比如 `PlanType` 的 sci、`CodexErrorInfo.modelNotFound`、`HookEventName`。
2. **要不要开 issue（涉及仓库权限，请编排者定）**：建议暂时只靠 nightly 的失败通知。现在每个版本只报警一次，噪声已经不大。以后真要开 issue，做法是：
   - 把 `codex-drift` 移出矩阵，单独成一个作业；
   - 另加一个 `needs: codex-drift`、`if: failure()` 的报告作业，只给它 `issues: write`，用固定的标题或标签查重、追加评论；
   - 不要把写权限给运行 codex 二进制的那个作业，因为它运行的是从 npm 下载的第三方二进制。
3. **基线过期检查没有单元测试**：这段逻辑在 `main.ts` 里，只在 scratchpad 的副本里手动验证过：退出码 1，提示用 `write` 重写。
4. **用字符串字面量判断"用到"偏保守**：像 `error` 这样的通用方法名会被算作用到。和原型一样，只会多报，不会漏报。
5. **没有比对的部分**：响应 schema，以及 `ServerNotification.json` 顶层的信封字段（比如 `emittedAtMs`）。
6. **关于 `~/.codex`**：所有运行都在隔离目录里。codex 有没有绕开 HOME、经 getpwuid 往别处写，我没法检查；按编排者的要求，没有读也没有删 `~/.codex`。
