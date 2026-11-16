# T182 笔记

## 开工前的估算（2026-11-16）

### 现状

- 第一层（T006，`tools/license-gate`）：`check` 按 vite 的 metafile 求每个 JS 入口的生产依赖闭包，再核对许可证；runtime 组件的许可证文本放在 `runtime/licenses/`，codex-acp 内联依赖的声明由 `codex-notices` 下载、生成。
- `release-config`（T161，nightly，macos-15）：只出 `.app`（`-c.mac.target=dir`，不出 zip 和 DMG），跑 `test:release` 和 `runtime-pack --verify`。
- `host-bundle` 已经按路径名拒绝 `anthropic`、`claude-agent-sdk`；这只看路径，不看内容。
- electron-builder 已经把 `LICENSE.electron.txt`、`LICENSES.chromium.html` 放进 `Contents/Resources`（本机的 e2e 包里能看到）。
- 宿主内嵌的运行时清单里有 SDK 读取的环境变量名（如 `CLAUDE_AGENT_SDK_VERSION`）；宿主定位用户的 Claude Code 时含有 `@anthropic-ai/claude-code`。所以这两类字符串都不能当特征串。
- PR 上的 `license` 和 `ts` 作业都在 ubuntu 上跑，所以两个 UNIT 用例不能依赖 `codesign`、`hdiutil`、`ditto`。

### 指纹清单的来源和内容（单列）

清单放在 `tools/license-gate/fingerprints.json`。`tools/` 不进任何产物；清单也不能放进宿主内嵌的运行时清单，否则特征串会进宿主二进制，扫描会把宿主自己报出来。

| 内容 | 来源 |
|---|---|
| `sdk`：包名、版本、tarball 的 integrity，以及包内每个文件（≥ 512 字节）的 sha-256 | 新命令 `license-gate fingerprints`。它按 `runtime/locks/claude-agent-acp-*.package-lock.json` 里 SDK 那一项的 `resolved` 和 `integrity`，把 tarball 下载到临时目录，核对 integrity 后解开、算哈希、删除，只把哈希写进清单。下载和校验沿用 `codex-notices` 的代码，抽成共用函数。 |
| `strings`：特征串，手工挑选 | 候选：`@anthropic-ai/claude-agent-sdk`、`Anthropic PBC. All rights reserved`。`fingerprints` 命令会核对：每个特征串都要在 SDK 的文件里出现；至少一个在它的 JS 代码里出现。打包脚本不压缩代码，字符串字面量打包后还在，所以 SDK 被打进我们的 JS 时也能发现。是否误报，由 nightly 对真实产物的扫描验证。 |
| `identifiers`：`com.anthropic.claude-code` | 本机已安装的 Claude Code（2.1.296）用 `codesign -dv` 读出的签名 identifier。只读签名元数据，不下载。 |
| `teams`：`Q6L2SF6YDW` | 同上读出的 TeamIdentifier。任何由 Anthropic 签名的 Mach-O 都会被报出，SDK 平台包里的二进制也一样。那个二进制不下载，所以没核对它的 identifier。 |
| SDK 各包 tarball 的 integrity（共 10 个，含各平台包） | 扫描时直接从锁文件读，不另外提交 |
| 锁文件的允许清单 | 运行时清单 `runtime/manifest.json` 的 `files` 里已经有 `Resources/runtime/locks/*` 的 sha-256（`host-bundle` 也按它核对），直接用，不再另记 |

- **首次生成在 runner 上跑**，用 spike-run 执行 `license-gate fingerprints` 并打印清单，不在本机下载 SDK。日志里只有哈希，以及每个特征串"找到 / 没找到"。
- 以后 claude-agent-acp 升级、锁文件里 SDK 的版本或 integrity 变了，PR 上的 `check` 会发现与清单不符而失败，提醒维护者重新生成。
- **备选**：在 `runtime-pack` 已有的隔离目录里算哈希、写清单，nightly 的 `runtime-pack --verify` 每晚核对。好处是不新增下载路径；代价是要写 Rust，约多 50 行。我倾向前一种。

### 做法

1. **扫描**（`license-gate scan <产物>...`）：
   - 产物可以是 `.app`；`.zip` 用 `unzip` 解到临时目录；`.dmg` 用 `hdiutil attach -readonly -nobrowse` 挂载，结束时卸载。
   - 逐个普通文件：
     - 位于 `Resources/runtime/locks/` 且 sha-256 与运行时清单一致的，放行；
     - sha-256 命中 SDK 的文件，或 sha-512 命中 SDK 某个 tarball 的，报出；
     - 内容含特征串的，报出（app.asar 作为一个文件整体搜索）；
     - Mach-O（含 fat）：解析 `LC_CODE_SIGNATURE` → SuperBlob → CodeDirectory，取 identifier 和 team，命中清单的报出。
   - 每个 `.app` 的 `Contents/Resources/THIRD_PARTY_NOTICES.txt` 必须存在，并且与按当前树重新生成的声明逐字节相同。声明是否齐全，由生成器保证、由单元测试覆盖。
2. **声明生成**（`license-gate notices`，写到 `target/notices/THIRD_PARTY_NOTICES.txt`），分节：
   - Node、codex-acp（LICENSE 加内联依赖的声明）、esbuild-wasm：取 `runtime/licenses/`；
   - Electron：electron 包的 LICENSE，并注明 Chromium 的声明见同目录的 `LICENSES.chromium.html`；
   - 前端依赖：各入口打包进去的 npm 包的并集（与 `check` 同一份 metafile，本机现在是 168 个），取包里的许可证文件。缺许可证文件的按 ADR-11 从上游补全文本，放在 `tools/license-gate/licenses/`，现在有 2 个：`react-remove-scroll-bar`、`lru_map`。既没有文件也没有补全的，生成失败。
   - Rust 依赖：用 cargo-about 0.9.3（MIT OR Apache-2.0，在允许清单里）。对三个随包的 crate（`cupper-host`、`cupper-cli`、`cupper-desktop-native`）各跑一次 `--format json`，按许可证文本合并后输出。`about.toml` 写允许清单、`aarch64-apple-darwin`、忽略 dev 依赖，并设 `no-clearly-defined`，保证离线、结果确定。
3. **接入**：
   - `build:release` 在 `vite build` 之后、electron-builder 之前生成声明；`electron-builder.release.yml` 的 `extraResources` 把它放进 `Contents/Resources`。
   - `release-config`：改为同时出 zip 和 DMG（去掉 `-c.mac.target=dir`），三种产物都扫描。
   - PR 上的 `license`：在 `check` 里核对清单与锁文件，并生成一次声明（新 npm 依赖缺许可证文本时，PR 上就会失败）。为此 `license` 和 `release-config` 都要装 cargo-about（照 cargo-deny 的写法 `cargo install --locked`，每次多 1–2 分钟）。
4. **用例**：两个用例都用注入的指纹清单和临时目录里的夹具，不依赖 SDK，也不依赖 macOS 工具。
   - **UNIT-license-gate-l2**：
     - 干净的产物通过；
     - 含特征串的文件被报出；
     - 哈希命中 SDK 文件的被报出；
     - 用代码拼出的、identifier 为 `com.anthropic.claude-code` 的 Mach-O 被报出，team 为 `Q6L2SF6YDW` 的也被报出；
     - 原样的锁文件放行；改动一个字节后被报出；挪到别的路径后被报出；
     - zip 里含特征串的被报出；
     - 声明缺失或不同的被报出。
   - **UNIT-third-party-notices**：
     - 夹具含 runtime 许可证、带 LICENSE 的 npm 包、靠补全文本的 npm 包、cargo-about 的 JSON，生成结果每一节都在；
     - 缺许可证文本时生成失败；
     - 输出是确定的。

### 估算（行，含测试；按 `cargo xtask size` 的口径）

| 模块 | 行数 |
|---|---|
| 指纹清单：`fingerprints.json`（数据，约 30）、`fingerprints` 命令和共用下载（约 45）、`check` 核对清单与锁文件（约 10） | 85 |
| 扫描 `scan.ts`：打开产物 25；遍历、放行、匹配 40；Mach-O 签名 40；声明核对和汇报 25 | 130 |
| 声明生成：`third-party.ts` 95、`about.toml` 10、两份补全的许可证文本约 45（数据） | 150 |
| `main.ts`：三个新命令，`check` 和 `notices` 共用读 metafile 的代码 | 30 |
| 接入：`build:release`、electron-builder、`release_config.rs`、`license.rs`、`action.yml` | 35 |
| 用例：UNIT-license-gate-l2 约 90，UNIT-third-party-notices 约 50 | 140 |
| **合计** | **约 570** |

超过停止线 500，没有超过硬线 600。其中约 75 行是数据（指纹清单和补全的许可证文本）。可以压下来的地方：

- 指纹清单和补全的许可证文本按数据算，不计入行数（像 `runtime/licenses/` 那样）：约 −75，降到 495 左右。
- nightly 仍然只出 `.app`，不做 DMG 的挂载：约 −10。DMG 留给 T181 在发布流水线里接，但那样这条路径在 T181 之前没有跑过。
- Mach-O 不解析 CodeDirectory，只在签名区里搜 identifier 和 team 的字节：约 −10，代价是报错信息不够具体。

## 实现后的行数（2026-11-16，超过硬线，停下报告）

编排者批复：指纹清单用 TS 命令、在 runner 上生成；release-config 不改，只扫 `.app`，zip 由扫描支持、由单元测试覆盖，DMG 留给 T181；数据不计入行数；`license` 装 cargo-about 并生成一次声明；规模按停止线 550、硬线 650 算。

写完以后按 `cargo xtask size` 的口径统计：代码 842 行，另有补全的许可证文本 42 行（数据）。超过硬线 650。原因是每个模块都低估了，而且没有在每写完一个模块时就统计（守则第 8 条）。

| 模块 | 估算 | 实际 |
|---|---|---|
| 指纹清单：`fingerprints.ts` 104、`notices.ts` 中共用的下载和校验 +32、`main.ts` 中的部分约 15 | 55 | 约 150 |
| 扫描：`scan.ts` 176、`main.ts` 中的部分约 10 | 130 | 约 186 |
| 声明生成：`third-party.ts` 146、`main.ts` 中的部分约 35 | 105 | 约 181 |
| `main.ts` 共用部分：用法说明、命令分派、读 metafile 的代码抽成 `entries()` | 30 | 约 47 |
| 接入：xtask、action、build:release、electron-builder、.prettierignore | 35 | 26 |
| 用例：`scan.test.ts` 163、`third-party.test.ts` 88 | 140 | 251 |

### 本机核对过的事实（只跑单元测试和冒烟，不是测量）

- 两个用例和原有用例共 12 个，全部通过；eslint、tsc、prettier、xtask 的 clippy、trace-ids 都通过。
- 签名解析：
  - 本机已安装的 Claude Code（2.1.296）读出 `com.anthropic.claude-code`、`Q6L2SF6YDW`，与 `codesign -dv` 一致；
  - `/usr/bin/true` 是 universal 二进制，两个切片都读出 `com.apple.true`；
  - 把它的副本用 `codesign -s - -i com.anthropic.claude-code` 重签后，读出伪造的 identifier；
  - 本机 e2e 包里的主程序、Electron Framework、重签过的 Node 都读出各自的 identifier。
- 用本机 e2e 包冒烟扫描，SDK 哈希清单临时置空：只报出"没有 THIRD_PARTY_NOTICES.txt"，没有误报。宿主带的锁文件按哈希放行。
- `notices` 在本机生成约 620 KB、12,339 行。cargo-about 0.9.3 要加 `--features cli` 才会装出命令。离线（`--frozen`）对三个 crate 各跑一次，每次约 2.4 秒。
- 两份补全的许可证文本取自上游仓库默认分支：`theKashey/react-remove-scroll-bar` 的 `LICENSE`，`rsms/js-lru` 的 `LICENSE.txt`。
