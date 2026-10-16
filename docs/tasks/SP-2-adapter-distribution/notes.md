# SP-2 过程记录与原始结论

结论汇总以编排者存档的 report 为准。这里记录方法、环境、逐项数据和试错过程。路径一律写成 `$HOME`、`<tmp>`。

## 环境

- macOS 26.4（arm64），Xcode 26.6。本机只有 Apple Development 证书，**没有 Developer ID**，所以公证无法实测。所有重签都用 ad-hoc 签名，但 hardened runtime 照常生效（见 `data/sign-matrix.txt`：没有 entitlement 的 node 一 JIT 就被 SIGTRAP 杀掉）。
- 本机 CLI：Claude Code 2.1.296（Homebrew cask），codex-cli 0.159.2（npm 全局安装：`/usr/local/bin/codex` 是 `#!/usr/bin/env node` 的 shim，原生二进制在包里的 `vendor/aarch64-apple-darwin/bin/codex`）。系统 Node v24.13.0。
- 被测版本：
  - `@agentclientprotocol/claude-agent-acp` 0.91.0，依赖 `@anthropic-ai/claude-agent-sdk` 0.3.300，其 `package.json` 声明 `claudeCodeVersion: 2.1.300`；
  - `@agentclientprotocol/codex-acp` 2.2.2，依赖 `@openai/codex` `^0.160.1`，解析结果为 0.160.1；
  - Node 24.22.0 LTS（与 Electron 44 自带的版本相同）；bun 1.4.3；esbuild 和 esbuild-wasm 0.28.2；postject 1.0.0-alpha.6。
- 下载校验：
  - Node：tarball 的 sha256 与 `SHASUMS256.txt` 一致；`SHASUMS256.txt.sig` 用 nodejs/release-keys 的公钥环验证为 Good signature。
  - bun：zip 的 sha256 与 `SHASUMS256.txt` 一致；`SHASUMS256.txt.asc` 用 keys.openpgp.org 上的 bun 发布机器人公钥验证为 Good signature（指纹没有从第二个独立来源核对）。
  - codex 0.163.0：另用 curl 下载 tarball 计算 SRI，与 registry 的 `dist.integrity` 一致。其余 npm 包由 npm 按 lockfile 的 SRI 校验。
- 真实会话：Claude 6 个回合（全部 haiku），Codex 8 个回合（默认模型，推理强度 low，read-only 模式，其中一个回合因为选了不支持的模型而失败）。没有出现权限请求，也没有遇到 HTTP 429。

## 1. claude-agent-acp + 本机 claude

- `npm install --omit=optional` 之后，不会下载 SDK 的平台包 `@anthropic-ai/claude-agent-sdk-darwin-arm64`（原生 Claude Code 2.1.300，249.6 MB）。依赖树从 306 MB 降到 67 MB（共 107 个包）。
- 不设置 `CLAUDE_CODE_EXECUTABLE` 时，`initialize` 能成功，到 `session/new` 时失败：`-32603 Internal error`，`data.details` = "Claude native binary not found for darwin-arm64. Reinstall @anthropic-ai/claude-agent-sdk without --omit=optional, or set CLAUDE_CODE_EXECUTABLE."。也就是说，没有指定 CLI 时**不会悄悄用到别的程序**。
- 设置 `CLAUDE_CODE_EXECUTABLE=<本机 claude 的绝对路径>` 并使用 GUI 环境的 PATH：`session/new` 约 0.8 s（其中 SDK initialize 约 0.75 s），haiku 一个回合约 1.9 s，`end_turn`，返回 "pong"。
- **SDK 对 CLI 版本的要求**：
  - 没有显式检查。`sdk.mjs` 里找不到版本比较或"版本过旧"一类的提示。
  - SDK 只做两件事：通过环境变量 `CLAUDE_AGENT_SDK_VERSION=0.3.300` 把自己的版本告诉 CLI；按需拼出命令行参数（源码中共有 44 个不同的参数名）（`--output-format stream-json --input-format stream-json --verbose` 必传，其余如 `--permission-prompt-tool`、`--resume-session-at`、`--thinking-display` 等按选项决定）。
  - 所以兼容性只取决于两点：CLI 能不能识别这些参数，能不能听懂 stream-json 里的控制消息。
  - 实测：
    - 2.1.296（比 SDK 绑定的 2.1.300 旧）可以正常完成回合；
    - 2.1.200（2026-07-03，旧三个半月，放在临时目录，用隔离的 `CLAUDE_CONFIG_DIR`，未登录）握手和 `session/new` 都成功，prompt 返回 `-32000 Authentication required`，因为没有登录。
  - **推测**：CLI 太旧、不认识某个参数时，CLI 会直接以退出码 1 结束，SDK 再报告"进程退出"。这次没有用到会触发这种情况的选项，所以没有复现。
- 会话仍然写在用户的 `~/.claude/projects/<cwd 编码>/` 下，和直接用 CLI 时一样。

## 2. codex-acp + 本机 codex

- `--omit=optional` 会去掉 codex-acp 自带的 `@openai/codex-darwin-arm64`（333 MB）。依赖树降到 18 MB。
- **CODEX_PATH 必须指向原生二进制**。指向 npm shim（`/usr/local/bin/codex`）时，GUI 环境的 PATH 里没有 node，`initialize` 直接失败：`code 1001 "Codex process has exited with code 127: env: node: No such file or directory"`，`data.restartable: true`。
- 各版本的真实回合结果（codex-acp 2.2.2 锁定的版本是 0.160.1）：

| codex | 与锁定版本的关系 | 结果 |
|---|---|---|
| 0.163.0（2026-10-16，npm，放在临时目录） | 新三个小版本 | 回合正常 |
| 0.159.2（本机） | 旧一个小版本 | 回合正常 |
| 0.150.1（2026-08-27） | 旧约 7 周 | 回合正常 |
| 0.140.0（2026-06-15） | 旧约 4 个月 | `session/new` 返回 `-32000 Authentication required`（**误导性**，详见下文） |

- 0.140.0 失败的原因（从 app-server 日志确认）：
  - 旧版 codex 解析不了 `~/.codex/config.toml` 中按新版本格式写的配置项（报缺少字段）；
  - 它发出 `configWarning: "Invalid configuration; using defaults."`，然后按默认配置运行；
  - 按默认配置找不到账号，`account/read` 返回 `account: null`；
  - codex-acp 据此报告"需要认证"。

  所以 **CLI 太旧时看起来像是没登录**。宿主应该先比较版本，并把 `configWarning` 显示出来。
- **协议层的差异**（`data/codex-schema-drift.json`）：
  - codex-acp 2.2.2 在源码里引用了 127 个 app-server 方法和通知，其中 3 个是 experimental：`thread/settings/update`（切换 Plan 协作模式时会用到）、`thread/backgroundTerminals/list`、`thread/backgroundTerminals/terminate`。
  - 0.159.2 与 0.160.1：这 127 个方法的参数 schema **完全一致**。
  - 0.163.0 相对 0.160.1：只有增量变化。新增的可选字段有 `turn/start.parentTurnId/rootTurnId`、`Turn.rootTurnId`、`thread/list.excludedThreadIds` 等；新增的枚举值有 `MessagePhase.partial_answer` 等。没有删除，也没有新增必填项。
  - 0.150.1：少了 9 个通知，请求参数少了一些字段。但 app-server 会忽略多出来的字段，codex-acp 也能容忍收不到的通知，所以基本回合正常。
  - 0.140.0：缺少 `thread/items/list` 请求；`thread/start` 里的 `DynamicToolSpec` 结构不兼容（新增了必填的 `description`、`inputSchema`）。
- **不匹配时的错误形态**（`data/appserver-mismatch-errors.txt`）：
  - 方法不存在：`-32600 "Invalid request: unknown variant \`thread/items/list\`, expected one of ..."`；
  - 调用 experimental 方法但没有声明能力：`-32600 "<method> requires experimentalApi capability"`；
  - 先检查参数，再检查能力，所以参数不合法时只会看到 `missing field`。
  - codex-acp 一直声明 `experimentalApi: true`，所以第二种只会出现在"新版本把某个方法重新标为 experimental 或删除"的时候。
- 附带发现（与 SP-1 相关）：
  - 选了 ChatGPT 账号不支持的模型（`gpt-5.4-mini`）时，错误以 **agent 消息文本**的形式返回，`stopReason` 仍然是 `end_turn`，不是 JSON-RPC 错误。
  - `session/new` 返回的 model 列表来自用户自己的 codex 配置，不一定只有 OpenAI 的模型，宿主不应假设列表内容。

## 3. Node 运行时来源

签名实测（`data/sign-matrix.txt`，ad-hoc 签名 + `--options runtime`）：

| 运行时 | 不加 entitlement | 只加 allow-jit | 说明 |
|---|---|---|---|
| 官方 node 24.22.0 | 一 JIT 就 SIGTRAP（退出码 133）；`--jitless` 可以运行，但没有 WebAssembly | 全部正常 | 官方二进制自带 6 项 entitlement：allow-jit、allow-unsigned-executable-memory、disable-executable-page-protection、disable-library-validation、allow-dyld-environment-variables、**get-task-allow** |
| Node SEA | 同上（SIGTRAP） | 正常 | 注入 blob 前要先去掉签名，注入后重签 |
| bun --compile | JSC 回落到解释器，但 `SharedArrayBuffer` 不可用，claude SDK 因此 ReferenceError | 正常 | bun 本身自带 5 项 entitlement（没有 get-task-allow） |

适配器在每种运行时下的真实回合都通过了（见 `data/probe-runs.md` 中的 c3-*）。

启动耗时（`data/startup-bench.json`，10 次的中位数/p90，单位 ms；codex 的 initialize 包含拉起 app-server）：

| 方案 | claude `--version` | claude initialize | codex `--version` | codex initialize |
|---|---|---|---|---|
| 系统 node 24.13 + npm 树（多文件） | 140 / 170 | 150 / 164 | — | — |
| 系统 node 24.13 + 单文件 | 103 / 114 | 115 / 141 | 54 / 59 | 76 / 83 |
| 官方 node 24.22 重签（allow-jit）+ 单文件 | 105 / 111 | 111 / 130 | 57 / 62 | 115 / 223 |
| 同上，零 entitlement + `--jitless` | 115 / 125 | 128 / 142 | 94 / 110 | 93 / 107 |
| Node SEA（allow-jit） | 97 / 101 | 107 / 125 | 60 / 63 | 83 / 237 |
| bun --compile（allow-jit） | 69 / 78 | 82 / 83 | 37 / 40 | 62 / 86 |

- 基线：`node -e 0` 约 23 ms。
- 新签名或新下载的二进制第一次执行会更慢：重签后的二进制首次多 100～150 ms；从 nodejs.org 下载、安装到 runtime 的 node 第一次执行，initialize 用了 2.6 s，之后恢复正常。原因推测是系统对新文件做首次评估。

体积（`data/sizes.txt`，原始 / xz -9）：

| 产物 | 原始 | xz |
|---|---|---|
| node 24.22 `bin/node`（重签后） | 121.8 MB | 24.2 MB |
| claude 单文件（esbuild） | 3.4 MB | 0.59 MB |
| codex 单文件（上游 dist） | 1.7 MB | 0.25 MB |
| claude SEA / codex SEA | 125.0 / 123.3 MB | 24.9 / 24.5 MB |
| claude bun / codex bun | 73.1 / 72.1 MB | 19.7 / 19.5 MB |
| claude npm 树（不含可选依赖） | 70.3 MB，8215 个文件 | 4.7 MB |
| codex npm 树（不含可选依赖） | 18.6 MB，1305 个文件 | 1.3 MB |
| （不用）SDK 自带的 claude / codex-acp 自带的 codex | 249.6 / 333 MB | 73.0 / 75.2 MB |

方案对比：

| 方案 | 能否用 | 体积 | 启动 | 签名与公证 | 所需 entitlement | 与 Electron 的 Node 分离 |
|---|---|---|---|---|---|---|
| **A 随 App 打包官方 Node，用 cupper 的身份重签** | 能 | App 增加约 24 MB（xz）；两个适配器共用一个 node | 约 105～115 ms | 能重签；去掉 get-task-allow 等 5 项 entitlement，只保留 allow-jit。公证需要 Developer ID，**未实测**；按 Apple 的公证要求（hardened runtime、时间戳、不含 get-task-allow），预期可以通过 | allow-jit | 完全分离，有独立的签名和 cdhash |
| A' 首次运行时从 nodejs.org 下载官方 Node，不重签 | 能（安装器试跑过） | App 不增加；下载 27.5 MB | 首次 2.6 s，之后与 A 相同 | 使用上游的 Developer ID 签名，不经过 cupper 公证；我们自己写入的文件不带 quarantine 属性，不会触发 Gatekeeper | 继承上游的 6 项，包括 get-task-allow、allow-dyld-environment-variables、disable-library-validation，攻击面更大 | 分离 |
| B Node SEA | 能 | 每个适配器约 125 MB，等于每个适配器带一份完整的 node | 约 100 ms | 同 A | allow-jit | 分离 |
| C bun --compile | 能 | 每个适配器约 73 MB | 最快，约 70～80 ms | 能重签 | allow-jit | 分离 |
| D 系统已装的 Node | 本机能用；但无法保证一定装了、版本 ≥ 22；GUI 进程的 PATH 也找不到 Homebrew、nvm 装的 node | 0 | 与 A 相近 | 不是我们的二进制，没法锁定 cdhash | 与我们无关 | 分离 |
| E Electron 自带的 Node | 不评估：需要打开 runAsNode fuse，与 ADR-03 的加固清单冲突 | — | — | — | — | 否 |

B 和 C 还有一个硬伤：claude 的 SEA 或 bun 产物包含 SDK，不能由 cupper 分发（见 `licenses.md`）。在用户机器上构建也不现实，因为需要 postject 或 bun。

## 4. 单文件打包

- claude-agent-acp 0.91.0 发布的是 124 个独立模块（dist 目录）。用 esbuild 打成一个 ESM 文件后是 3.4 MB，有两处需要处理：
  - 加一段 `createRequire` banner，让其中的 CJS 依赖能 require 内置模块；
  - 剩下唯一的外部解析是 `import.meta.resolve("@anthropic-ai/claude-agent-sdk")`。它只在没有设置 `CLAUDE_CODE_EXECUTABLE` 时才会走到，作用是去找 SDK 自带的原生程序。打包后这条路径必然失败，正好**防止误用自带程序**。
  - ajv 里的 `require("ajv/dist/runtime/...")` 只是代码生成用的字符串，不是真正的 require。
- codex-acp 发布的 `dist/index.js` **本来就是**单文件（1.7 MB，只依赖 Node 内置模块）。`require.resolve("@openai/codex/bin/codex.js")` 只在没有设置 `CODEX_PATH` 时才会用到。
- Node 24 的 SEA 只支持 CJS 主脚本。`mainFormat: "module"` 会被忽略，导致 ESM 报语法错误。上游入口用了顶层 await，所以 SEA 版本需要 `sea-entry-claude.mjs` 这个入口改写，CJS 打包时还要用 `import.meta.url` 垫片。codex-acp 直接打成 CJS 就行。bun 直接编译上游入口即可。
- **升级替换实测**：在 `session/new` 之后，用 stub 文件替换磁盘上的单文件，然后再发 prompt，claude 和 codex 都正常完成回合。作为对照，把 claude 多文件版的整个 `node_modules` 挪走后，基本回合**也能**完成，因为它的 ESM 静态图在启动时已经全部载入。但多文件树里确实有延迟 require，例如 iconv-lite 的 `getCodec` 里有 `require("../encodings")`，会出现在 MCP HTTP 请求体解析等不常走的路径上，所以多文件不能保证不受影响。
- 单文件的另一个好处是校验快：校验单文件 sha256 约 3 ms，校验整棵 npm 树（8215 个文件）约 270 ms。

## 5. 首次运行安装流程（草案，已用 `install-runtime.mjs` 试跑）

目录结构：`runtime/<名称>/<版本>/`，例如 `runtime/node/24.22.0/bin/node`、`runtime/claude-agent-acp/0.91.0/adapter.mjs`、`runtime/codex-acp/2.2.2/adapter.mjs`。每个目录里都有一个 `.installed.json`，记录每个文件的 sha256。

清单（`runtime-manifest.json`）随 App 签名下发，每个组件都写死版本和哈希：
- Node：tarball 的 sha256。如果采用方案 A，由 App 内的副本拷贝，改为核对 cdhash。
- codex-acp：npm tarball 的 SRI，只取出 `dist/index.js`。
- claude-agent-acp：一份锁文件（`locks/claude-agent-acp-0.91.0.package-lock.json`，115 个条目，其中 8 个平台二进制标为 optional，跳过），外加**本机打包产物的固定 sha256**。esbuild-wasm 0.28.2 在两个不同的 runtime 根目录下打出的产物逐字节相同，sha256 都是 `a8f29655…`，所以可以在 CI 中预先算好。

步骤：
1. 加锁，防止两个安装器同时运行（原型没有实现，宿主可以复用 ADR-01 的 flock）。
2. 清理上次中断留下的 `.staging-*` 目录。
3. 如果目标版本的目录已经存在：重新算哈希，与 `.installed.json` 一致就跳过；不一致就改名为 `.broken-*`，然后重装。
4. 下载到 staging 目录，边下载边算哈希，**核对通过后才解压**。npm 树按锁文件逐包下载、校验 SRI 后解包（并发 8 个），**不运行任何包脚本**，也不需要 npm 本身。
5. 只保留需要的文件。对 claude，在本机用 esbuild-wasm 打包，核对固定的 sha256，然后删掉整棵 npm 树。
6. 计算所有文件的哈希，写入 `.installed.json`，再把 staging 目录 `rename(2)` 成正式目录（同一个卷上是原子操作）。
7. 每次拉起适配器之前：核对 adapter.mjs 的 sha256（约 3 ms），以及 node 的 cdhash（用 codesign API；直接对 117 MB 的文件算 sha256 要 50～90 ms）。
8. 旧版本目录只在没有进程使用时才回收，所以运行中的适配器不会因为升级而丢文件。

失败处理（`data/install-trial.txt`）：

| 情况 | 实测行为 |
|---|---|
| 哈希不一致 | 立即失败，不重试；删除 staging；已安装的 2.2.2 不受影响；报告为 integrity 错误，可能是被篡改或镜像有问题 |
| 网络失败 | 退避重试 3 次后失败；删除 staging；旧版本不受影响 |
| 安装器在提交前被杀 | 只留下 `.staging-*`，正式目录不存在；verify 报告 "no marker"；下次运行时先清理再重装 |
| 安装之后文件被改 | verify 报告 "hash mismatch adapter.mjs"；install 把目录改名为 `.broken-*` 后重装 |
| 重复运行 | 什么都不做（present） |

耗时：全新安装 5.6～7.9 s，其中 Node 2.1～2.4 s，codex-acp 0.3～0.5 s，claude 107 个包 1.8～2.8 s，本机打包 1.0～1.3 s。我们自己的进程写入的 node 不带 `com.apple.quarantine`，只有系统自动加的 `com.apple.provenance`。

## 6. 许可证

见 `licenses.md`。

## 试错记录

- Node 24.22 的 SEA 不支持 `mainFormat: "module"`，所以改成"CJS 入口改写 + `import.meta.url` 垫片"。
- 用 esbuild 本机打包时，产物里的路径注释取决于工作目录，导致不同根目录下的产物不一致。设置 `absWorkingDir: staging` 后变成逐字节可复现。
- GitHub API 在共享 IP 上未认证的限额（每小时 60 次）已经用完，因此改为从 release 下载地址的跳转确认 bun 的最新版本。
- 一开始用 `gpt-5.4-mini` 想省额度，被 ChatGPT 账号拒绝，于是改为默认模型加 low 推理强度。
