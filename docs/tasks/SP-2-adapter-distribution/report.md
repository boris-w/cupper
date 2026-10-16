# SP-2 汇报

- 执行者：子 agent（构建与发布工程师）
- 分支：`spike/SP-2-adapter-distribution`
- worktree：`$HOME/.../cupper/.claude/worktrees/agent-a8faee5e2915ab881`（编排者存档时去掉了本机用户名）
- 完成日期：2026-10-16

> 编排者存档说明：正文按子 agent 的回复原样保留，只有"本机痕迹"一节改为摘要。那一节原本列出了会话 scratchpad 的完整路径和会话 ID，完整清单存在不入库的 `.private/cleanup/SP-2-leftovers.md`。

## 做了什么

brief 的第 1 至 6 项都已完成，每一项都有真实回合或实测数据作证据。

规模和安全措施：
- 真实回合：Claude 6 个（全部用 haiku）；Codex 8 个（默认模型，推理强度 low，read-only 模式）。提示词都是 "Reply with exactly one word: pong"。
- 所有安装都放在 worktree 外的临时目录里。npm 使用空的 userconfig 和独立的缓存，带 `--ignore-scripts`，不读 `~/.npmrc`。
- 适配器在模拟 launchd 的环境里运行：`PATH=/usr/bin:/bin:/usr/sbin:/sbin`，不带开发环境变量。
- 没有出现权限请求，也没有遇到 HTTP 429。
- 外发请求都不带认证。User-Agent 只有 npm 或 curl 的默认值，或者 `cupper-runtime-installer/0`。

物料：
- 脚本：`spikes/SP-2-adapter-distribution/`，带 README。
- 数据：`docs/tasks/SP-2-adapter-distribution/`，包括 `data/`、`locks/`、`runtime-manifest.json`、`licenses.md`、`notes.md`。

### 1. claude-agent-acp 0.91.0 配合本机 claude：能用

- 用 `npm install --omit=optional` 后，不再下载 SDK 自带的原生 Claude Code 2.1.300（249.6 MB），依赖树从 306 MB 降到 67 MB。
- 没有设置 `CLAUDE_CODE_EXECUTABLE` 时，`session/new` 会明确报错：`-32603`，details 为 "Claude native binary not found … set CLAUDE_CODE_EXECUTABLE"。适配器不会悄悄改用别的程序。
- 指向本机的 claude 2.1.296（SDK 绑定的是 2.1.300）后，回合正常：`session/new` 约 0.8 s，一个回合约 1.9 s，`end_turn`。
- SDK 对 CLI 版本的要求：**没有显式检查**。它只通过环境变量 `CLAUDE_AGENT_SDK_VERSION` 告诉 CLI 自己的版本，并按需拼出命令行参数（源码里共 44 个参数名）和 stream-json 控制消息。
  - 用 2.1.200（旧三个半月，放在临时目录，配置隔离，未登录）测试，握手和 `session/new` 都成功；prompt 返回 `-32000 Authentication required`，原因是没有登录。
  - CLI 太旧、不认识某个参数时会怎样，这次没有复现（推测是 CLI 以退出码 1 结束）。

### 2. codex-acp 2.2.2 配合本机 codex：在 0.150 到 0.163 之间没有出现实验 API 不匹配

| codex | 与锁定版本 0.160.1 的关系 | 真实回合 |
|---|---|---|
| 0.163.0（npm，放在临时目录，SRI 已核对） | 新 | 正常 |
| 0.159.2（本机） | 旧 | 正常 |
| 0.150.1 | 旧约 7 周 | 正常 |
| 0.140.0 | 旧约 4 个月 | `session/new` 返回 **`-32000 Authentication required`**，这是误导性的报错 |

- **0.140.0 失败的根因**（来自 app-server 日志）：旧版 codex 解析不了 `~/.codex` 中按新版本格式写的配置，发出 `configWarning "Invalid configuration; using defaults"`，`account/read` 因此返回 `null`，codex-acp 就报"需要认证"。也就是说，**CLI 太旧时看起来像没登录**。
- **协议差异分析**（`codex-schema-drift.mjs`）：
  - codex-acp 引用了 127 个 app-server 方法和通知，其中 3 个是 experimental。
  - 0.159.2 和 0.160.1 的这 127 个方法 schema 完全相同。
  - 0.163 只有增量变化：新增可选字段和枚举值，没有删除，也没有新增必填项。
  - 0.140 缺少 `thread/items/list`，`thread/start` 里的 `DynamicToolSpec` 也不兼容。
- **出错时的形态**：
  - 方法不存在：`-32600 "Invalid request: unknown variant \`X\`, expected one of …"`；
  - 缺少 experimental 能力：`-32600 "X requires experimentalApi capability"`。
- **`CODEX_PATH` 必须指向原生二进制**。指向 npm 的 shim（`#!/usr/bin/env node`）时，GUI 环境的 PATH 里没有 node，`initialize` 直接失败：`code 1001 "…exited with code 127: env: node: No such file or directory"`。

### 3. Node 运行时来源

签名实测：ad-hoc 签名，hardened runtime 照常生效。

| 方案 | 能否用 | 体积 | 启动（claude initialize，中位数） | 签名与公证 | 所需 entitlement | 与 Electron 的 Node 分离 |
|---|---|---|---|---|---|---|
| **A 随 App 打包官方 Node 24.22，用 cupper 的身份重签** | 能（两个适配器都完成了真实回合） | node 121.8 MB，xz 后 24.2 MB；两个适配器共用一份 | 111 ms | 能重签。官方二进制自带 6 项 entitlement，其中包括 `get-task-allow`，重签时全部去掉。公证未实测（本机没有 Developer ID），预期可以通过 | **只要 allow-jit**。不加任何 entitlement 时，一 JIT 就 SIGTRAP；`--jitless` 可以运行，但没有 WebAssembly | 完全分离 |
| A' 首次运行时从 nodejs.org 下载，不重签 | 能（安装器试跑过） | App 不增加；下载 27.5 MB | 首次 2.6 s，之后 136 ms | 使用上游签名；我们写入的文件不带 quarantine | 继承上游的 6 项，包括 get-task-allow、允许 DYLD 环境变量、关闭库验证 | 分离 |
| B Node SEA | 能 | 每个适配器约 125 MB | 107 ms | 注入 blob 后重签 | allow-jit | 分离 |
| C bun --compile | 能 | 每个适配器约 73 MB | 82 ms，最快 | 能重签 | allow-jit（没有它时 `SharedArrayBuffer` 不可用，SDK 起不来） | 分离 |
| D 系统已装的 Node | 本机能用，但无法保证存在、版本 ≥ 22；GUI 进程的 PATH 找不到 Homebrew 或 nvm 装的 node | 0 | 115 ms | 不是我们的二进制，没法锁定 | 与我们无关 | 分离 |
| E Electron 自带的 Node | 不评估：需要打开 runAsNode fuse，与 ADR-03 冲突 | — | — | — | — | 否 |

**推荐方案 A**：随 App 打包官方 Node LTS，用 cupper 的身份重签，只保留 allow-jit；首次运行时拷贝到 `runtime/node/<ver>/`；两个适配器各自是一个 JS 单文件，共用这份 Node。
- B 和 C 相对 A 没有实质优势：启动差距在 30 ms 以内，体积反而翻倍。
- 更关键的是，claude 的 SEA 或 bun 产物里包含 SDK，cupper 不能分发。
- bun 还会带上 LGPL 义务。

### 4. 单文件打包：可行，并推荐使用

- claude-agent-acp 发布的是 124 个独立模块。用 esbuild 打成一个 ESM 文件后是 3.4 MB，产物里只剩 Node 内置模块的 import。唯一残留的外部解析是寻找 SDK 自带程序的那条路径，打包后必然失败，正好防止误用。
- codex-acp 发布的 `dist/index.js` 本来就是单文件（1.7 MB）。
- 做 SEA 时需要改写入口：Node 24 的 SEA 只支持 CJS，而上游入口用了顶层 await。bun 可以直接编译上游入口。
- **升级替换实测**：在 `session/new` 之后，把磁盘上的单文件替换掉，再发 prompt，两个适配器都正常完成回合。
- 对照组：多文件版整棵 `node_modules` 被挪走后，基本回合也能完成，因为 ESM 静态图在启动时已全部载入。但依赖树里确实有延迟 require（例如 iconv-lite 的 `require("../encodings")`），不常走的路径仍然可能出问题。
- 单文件的另一个好处是校验快：单文件约 3 ms，整棵树（8215 个文件）约 270 ms。

### 5. 首次运行安装：草案已写好，并用 `install-runtime.mjs` 试跑

- **布局**：`runtime/<名称>/<版本>/`，里面有一个 `.installed.json`，记录每个文件的 sha256，最后写入。
- **清单**（`runtime-manifest.json`，随 App 签名）：
  - Node：tarball 的 sha256；
  - codex-acp：npm tarball 的 SRI，只取出 `dist/index.js`；
  - claude-agent-acp：锁文件（115 个条目，跳过 8 个可选的平台二进制），外加**本机打包产物的固定 sha256**。esbuild-wasm 在不同的根目录下打出的产物逐字节相同，所以这个哈希可以在 CI 中预先算好。
- **步骤**：
  1. 清理上次中断留下的 staging 目录；
  2. 已安装的版本先校验，不一致就挪到一边；
  3. 边下载边算哈希，**核对通过后才解压**；
  4. 按锁文件逐包校验 SRI 后解包，不运行任何包脚本，也不需要 npm 本身；
  5. 在本机打包并核对哈希；
  6. 写入标记文件后，用 `rename(2)` 原子提交；
  7. 每次拉起前核对 adapter.mjs 的 sha256 和 node 的 cdhash；
  8. 旧版本目录只在没有进程使用时才回收。
- **耗时**：全新安装 5.6～7.9 s。
- **失败处理实测**：

| 情况 | 行为 |
|---|---|
| 哈希不一致 | 立即失败，不重试 |
| 网络失败 | 退避重试 3 次后失败 |
| 提交前被杀 | 正式目录不存在，下次运行时清理并重装 |
| 安装后文件被篡改 | verify 能发现；install 把目录挪到一边后重装 |
| 重复运行 | 什么都不做 |

以上每种情况下，staging 都被删除，旧版本都不受影响。

### 6. 许可证（`licenses.md` 和 `data/licenses-*.json`）

- **不能再分发**：
  - `@anthropic-ai/claude-agent-sdk` 0.3.300：LICENSE.md 写着 "© Anthropic PBC. All rights reserved. Use is subject to …legal-and-compliance"，没有任何开源授权；
  - 它的 darwin-arm64 平台包，也就是原生 Claude Code。法律页面写明，在产品中预装 Claude Code 需要接受 Commercial Terms，并且不得修改二进制、不得移除内置认证方式。
- **其余全是宽松许可证**：
  - claude-agent-acp 的 105 个依赖：MIT 91、ISC 7、BSD-3 3、Apache-2.0 2、BSD-2 1、Unlicense 1；
  - codex-acp 树的其余 24 个包：MIT 18、Apache-2.0 3（`@agentclientprotocol/sdk`、`@openai/codex` 及其平台包）、ISC 2、BSD-3 1。
  - 没有任何包带 NOTICE 文件。
- **codex-acp 的 "Other"**：核对原文，就是 Apache-2.0。文件开头是 JetBrains 的版权样板，后面的条款与 apache.org 的标准文本一致，只是删掉了附录。npm 和 GitHub 上的 LICENSE 逐字节相同。但它的单文件内联了 22 个包，没有保留这些包的许可证声明。
- **`bun --compile` 或 SEA 算不算再分发：算。** 产物里包含完整的运行时和所有被打包的包。所以：
  - claude 的任何打包产物都不能由 cupper 分发；
  - 在用户机器上、用从 npm 下载的包打包，推测不算 cupper 的再分发，需法务确认；
  - bun 静态链接了 JavaScriptCore/WebKit（LGPL-2）和 tinycc（LGPL-2.1），分发 bun 产物有 LGPL 义务；
  - Node 二进制本身只涉及宽松许可证，分发时要附带它的 LICENSE 全文。

## 对 ADR-08 的建议

1. **agent 程序一律使用用户本机的 CLI。** 用 `CLAUDE_CODE_EXECUTABLE` / `CODEX_PATH` 传原生二进制的**绝对路径**。npm 的 shim 要先解析到它包里的平台二进制，不能直接传 shim。永远不下载适配器自带的 agent 程序，这样能省下约 580 MB。
2. **适配器的形态**：每个适配器是一个 ESM 单文件，用 cupper 自己的 Node 运行。
   - codex-acp：直接取上游发布的 `dist/index.js`，按 SRI 校验。
   - claude-agent-acp：首次运行时按锁文件从 npm 安装，再在本机用 esbuild-wasm 打包，并核对固定的产物哈希。
3. **Node 的来源选方案 A**：官方 LTS 二进制，用 cupper 的身份重签，开启 hardened runtime，只保留 `allow-jit`，放在 `runtime/node/<ver>/`。不用系统 Node、Electron 自带的 Node，也不用 bun。不建议 `--jitless`，因为没有 WebAssembly，esbuild-wasm 就跑不了。
4. **兼容矩阵和检测**：
   - 每个锁定的适配器版本都附一个"已验证的 CLI 版本范围"。本次只验证了基本回合：claude 2.1.200（握手）和 2.1.296（回合）；codex 0.150.1 至 0.163.0。完整范围要用 SP-1 录制的剧本去跑。
   - 拉起适配器之前先运行 `--version`，超出范围就给提示，不阻止使用。
   - codex 版本低于下限时，要把"Authentication required"改为提示"CLI 版本过旧"，同时显示 `configWarning`。
   - CI 对每个新发布的 codex 版本跑一次 schema drift，盯住 codex-acp 引用的这 127 个方法。
5. **完整性**：清单随 App 签名下发。安装时核对 sha256 或 SRI，以及本机打包产物的哈希。每次拉起前核对 adapter 的 sha256 和 node 的 cdhash，这与 ADR-01 的要求一致。版本目录一旦写入就不再改动，在没有进程使用时才回收。

## 对 ADR-11 的建议

1. **红线**：仓库、.app、发布页、自动更新源、镜像，都不能出现 `@anthropic-ai/claude-agent-sdk*`、Claude Code 二进制，或任何包含它们的打包产物。CI 在打包输入中发现 `@anthropic-ai/claude-agent-sdk` 时直接失败。
2. claude-agent-acp 一律在用户机器上从 npm 安装和打包，App 里只放锁文件这类元数据。
3. 可以随 App 分发的：重签后的 Node（附 LICENSE）、codex-acp 单文件（附 Apache-2.0 和 22 个内联依赖的声明，由锁文件自动生成）、esbuild-wasm（MIT）。不建议分发 bun 产物。
4. CI 许可证门禁：白名单为 MIT、ISC、BSD、Apache-2.0、0BSD、Unlicense、CC0、BlueOak。npm 包里缺少 LICENSE 文件的（`standardwebhooks`、`@openai/codex`），从上游补全文本。
5. cupper 自身建议用 Apache-2.0，与 ACP 生态一致，并带专利授权。
6. 需法务确认两点：一是"由 cupper 触发、在用户机器上自动下载 SDK"是否属于用户自己的使用；二是政策问题，即 Agent SDK 与订阅登录的关系（R02 核心结论 6；法律页面另写明"不妨碍终端用户用自己的订阅登录未修改的 Claude Code 二进制"）。

## 关键取舍

- **单文件 + 共用 Node（A）**，没有选 SEA 或 bun。启动只差 30 ms 以内，体积小一半；更重要的是，claude 的单文件可以在本机生成并核对固定哈希，从而绕开 SDK 的再分发问题。SEA 和 bun 都需要在 CI 中构建，也就必须分发 SDK。
- **重签 Node（A）**，没有选直接下载上游签名的 Node（A'）。A 的 entitlement 最少（只有 allow-jit，没有 get-task-allow 和 DYLD 注入），cdhash 由我们自己掌握，也能离线安装。代价是 App 增加约 24 MB，Node 的安全更新要跟着 App 一起发布。
- **在本机打包**，没有选直接运行 npm 树。原因是拉起前的校验成本从 270 ms 降到 3 ms，也不用担心延迟 require。备选方案是在不可变的版本目录里直接运行 npm 树。
- 安装器原型用 Node 写，是为了快速验证流程。正式实现应放在 Rust 宿主中；原型里调用的 tar 也应换成带路径检查的 tar 解析库。

## 验证方式

- 第 1、2 项：用 `acp-probe.mjs` 跑了 25 次运行，汇总在 `data/probe-runs.md`。版本不一致时的错误形态见本汇报第 2 项和 `data/appserver-mismatch-errors.txt`，协议差异见 `data/codex-schema-drift.json`。✅
- 第 3 项：
  - `sign-matrix.sh` 测了 node、SEA、bun 三种运行时 × 4 组 entitlement（`data/sign-matrix.txt`）；
  - `bench-startup.mjs` 测了 13 种配置，每种 10 次加首次（`data/startup-bench.json`）；
  - `sizes.sh` 测了体积（`data/sizes.txt`）；
  - 每种运行时都完成了真实回合。✅
- 第 4 项：用 `bundle.mjs` 打包并做 import 检查；clobber 实测见 c4-*。✅
- 第 5 项：用 `install-runtime.mjs` 做了全新安装、5 种故障、可复现性和 verify，结果在 `data/install-trial.txt`；装好的 runtime 能拉起两个适配器（c5-*）。✅
- 第 6 项：`licenses.mjs` 清点了 133 个包；LICENSE 原文与 apache.org 做了 diff；核对了 Anthropic 法律页面和 bun 的 LICENSE.md。✅
- 下载校验：Node 的 sha256 加 GPG（Good signature），bun 的 sha256 加 GPG（Good signature，公钥来自 keys.openpgp.org，指纹没有从第二个来源核对），codex 0.163.0 另用 curl 下载核对了 SRI。
- 所有脚本都通过了 `node --check` 或 `bash -n`。提交前用 grep 扫了用户名、绝对路径、主机名、身份证书和 Team ID，没有命中。
- 公证：**未验证**（没有 Developer ID）。

## 提交列表

```
ef1ea38 docs(task): add SP-2 measurements, runtime manifest and license inventory
6425e4e chore(spike): add SP-2 adapter distribution test scripts
<编排者存档提交>
```

## 对用户配置的改动

- `~/.codex/config.toml`：**这次没有写入任何信任项。** 每一批 Codex 运行之后，以及最后一次，都只读检查过，没有任何条目包含我的临时目录（`<tmp>/sp2/...`）。这个文件在实测期间确实被改过：里面有 2 个 `[projects."<var-folders>/..."]` 条目，不是我的路径，推测来自同时运行的其他 agent。我没有编辑这个文件。
- `~/.claude.json`：没有我的测试仓库的项目条目。
- `~/.npmrc`、`~/.gnupg` 没有改动；`~/.bun` 不存在。我的 npm 用的是独立缓存。`~/.npm/_logs` 和 `_npx` 在这段时间有变化，但时间都在我最后一次调用 npm 之后，来自其他进程。

## 本机痕迹（编排者脱敏摘要）

- Claude Code：一个测试仓库对应的 project 目录，里面有 6 个会话和一个空的 memory 目录；`session-env/` 下有 9 个目录。
- Codex：8 个 rollout 文件，另有 3 个只建了会话的线程，没有留下文件。
- 会话 scratchpad 下的 `sp2/`，约 5.1 GB：npm 缓存、下载文件、依赖树、构建产物、测试 runtime、日志。日志里的原始 JSONL 和 app-server 日志含主机名和配置细节。另有若干辅助脚本。
- 没有遗留的适配器或 app-server 进程。

具体路径和 ID 见不入库的 `.private/cleanup/SP-2-leftovers.md`。

## 遗留问题 / 范围外发现

1. 公证没有实测。需要在有 Developer ID 的 CI 上验证：把只带 allow-jit 的重签 Node 嵌进 App 后能否通过公证；以及在已公证的 App 里，新文件首次执行有多慢（这次在下载的 Node 上看到了 2.6 s）。
2. 已验证的 CLI 版本范围目前只覆盖基本回合，要靠 SP-1 的录制剧本补全。claude 因为参数不认识而失败的路径没有复现。
3. 宿主怎么找到用户的 CLI 还要设计：Homebrew cask、原生安装器、npm shim 解析到真实二进制，以及其他桌面 App 内置的 codex。GUI 进程的 PATH 不可用。
4. （与 SP-1 和 ADR-06 相关）选了 ChatGPT 账号不支持的模型时，错误以 agent 消息文本返回，`stopReason` 仍是 `end_turn`。另外，codex-acp 会把客户端的 `clientInfo.name` 传给 codex，codex 发往 OpenAI 的 User-Agent 里会带上它，所以 cupper 的 clientInfo 要按"会外发"来命名。
5. Node 的安全更新是否走独立于 App 发布的 runtime 更新通道，留给 ADR-08 决定。esbuild-wasm（14 MB）随 App 带还是按 SRI 下载，也留给 ADR-08。
6. 在共享 IP 上，GitHub API 未认证的配额已经用完。以后要查 release 元数据，建议用 release 下载地址的跳转，或者带项目自己的令牌。
