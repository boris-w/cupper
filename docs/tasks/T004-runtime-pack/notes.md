# T004 工作笔记

- 执行者：子 agent（工程师，构建与发布工具）

## 开工前的规模估计

行数指 rustfmt、Prettier 展开后的行，含文档注释和空行，不含 `Cargo.lock`、锁文件、许可证原文和 `runtime-pack` 写回清单的计算字段（生成物）。清单里手工维护的部分（锁定版本、哈希、环境变量分类）是数据，单列一项。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `runtime_pack/mod.rs` | 参数（`--verify`）；缓存与暂存目录；按组件取文件、重签、算哈希；写回清单或逐项比对；许可证文本写入或核对 | 180 |
| `manifest.rs` | 清单的类型（键有序）、读写、`--verify` 的逐项差异 | 90 |
| `fetch.rs` | curl 下载（不读 `.curlrc`、只走 https、UA `cupper-research`、只允许官方地址）；按 sha-256 或 SRI 校验；缓存复用前重新核对 | 70 |
| `unpack.rs` | tar.gz 读取：只取普通文件，逐段检查路径不越界，去掉首层目录 | 60 |
| `sign.rs` | 本机 Node 副本 ad-hoc 重签（hardened runtime、只有 allow-jit、identifier `dev.cupper.runtime.node`）、校验、取 cdhash | 40 |
| `claude.rs` | 解析锁文件（跳过 optional）、在隔离目录里按 SRI 下载解包、用重签后的 Node 和 esbuild-wasm 跑打包脚本、算哈希、扫描环境变量、删除隔离目录 | 70 |
| `envscan.rs` | 从产物源码里提取环境变量读取点（成员访问、`in`、解构），按白名单、禁用、无害三类归类，未归类即失败 | 110 |
| `runtime/bundle-claude-acp.mjs`、`runtime/node.entitlements.plist` | 本机打包脚本（与 SP-2 的选项一致，产物逐字节可复现）；Node 的 entitlements | 45 |
| `main.rs`、`Cargo.toml`、`.prettierignore` | 子命令入口；`tar`、`flate2`、`base64`、`serde` 依赖；清单和锁文件不过 Prettier | 15 |
| **实现小计** | | **680** |

按计划 1.1 的经验，实现常比草估多出不少；xtask 没有宿主那样字段多的结构体字面量，按 1.3 倍计约 **880**。

**测试**

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-runtime-pack-envscan | 各种读取写法的提取；未归类的名字让命令失败；前缀规则；动态读取计数 | 120 |
| `unpack` | `..` 和绝对路径被拒；去首层目录；指定成员缺失时报错 | 50 |
| `fetch` | sha-256、SRI 不符时失败（本地文件，不联网）；非官方地址被拒 | 30 |
| `manifest` | 读写往返；逐项差异列出变化的键 | 40 |
| `claude` 的锁文件解析 | 跳过 optional、dev、link；非 npm 官方地址被拒 | 30 |
| **测试小计** | | **270** |

**数据**：清单里手工维护的部分，主要是环境变量分类。分类条目数要等第一次扫描出产物后才知道，预估 150–300 行。

**合计**：代码约 1,150 行（实现 880 + 测试 270），加数据约 1,300–1,450 行，接近停止线。每完成一个模块用 `git diff --numstat main...HEAD` 统计一次；推算总数超过约 1,500 行时，停下来汇报。

**范围上的取舍（开工前）**：codex-acp 内联的 22 个依赖的许可证声明（ADR-11）按设计 5.1.2 属于 `tools/license-gate` 的"THIRD_PARTY_NOTICES 生成"，本任务不做，见文末的交接。

## 实际行数（逐模块）

`git diff --numstat`，不含 `Cargo.lock`、`runtime/locks/`、`runtime/licenses/`。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） |
|---|---|---|---|
| 规模估计（`docs(task)`） | 41 | — | — |
| 打包脚本、entitlements、`.prettierignore`（`build(runtime)`） | 47 | — | 45 |
| 下载、解包、重签、隔离打包、清单与 `--verify`（`build(xtask)`） | 1,012 / 3（其中清单 91 行：锁定值约 25 行，其余是计算字段） | 240 | 525 + 150 |
| 环境变量扫描与归类（`build(xtask)`） | 324 / 20（Rust）+ 清单 299 | 75 + 测试辅助的改动 | 110 + 120 |

估计一列是开工前的草估（未乘 1.3）。

**合计**：代码 1,272 行（实现约 957，测试约 315），比估计（1,150）多 11%；清单里手工维护的数据约 170 行（锁定的版本和哈希约 25 行，环境变量分类约 145 行），代码加数据约 1,440 行，在停止线以内。清单其余约 220 行（各适配器读取的变量、每个文件的哈希）由 `runtime-pack` 写回，是生成物。

## 实现时的取舍

**下载用系统的 curl，不引入 HTTP 库**

- 走 rustls 的 HTTP 库要带根证书包，`webpki-roots` 的许可证是 CDLA-Permissive-2.0，不在 ADR-11 的白名单里；macOS 自带 curl。xtask 本来就以子进程调用 cargo、git、pnpm。
- 参数：`-q` 放在最前（不读 `.curlrc`，那里可能有请求头或凭据）、`--proto =https --proto-redir =https`、`--user-agent cupper-research`、`--parallel`；地址只允许 `https://nodejs.org/dist/` 和 `https://registry.npmjs.org/` 两个前缀，锁文件里的 `resolved` 也一样。
- 实测遇到一次 `SSL_ERROR_SYSCALL`（退出码 35）。curl 的 `--retry` 默认不重试连接错误，加了 `--retry-all-errors`。哈希不符不会重试：下载完才统一核对。

**Node 用 .tar.gz，不用 .tar.xz**

- SP-2 锁的是 .tar.xz 的 sha-256。改用同一版本的 .tar.gz，解包用 `tar` + `flate2`（宿主的安装器 T056 按设计 5.2.12 也用这两个 crate），不需要 xz。
- 新的 sha-256 `6a9724d9…` 取自 `SHASUMS256.txt`。2026-10-19 用 Node 官方的 `release-keys` 公钥环、`gpgv` 核对了 `SHASUMS256.txt.sig`：Good signature（RSA 密钥 `C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C`）。同一文件里 .tar.xz 的哈希与 SP-2 锁的值一致。
- 设计 5.10.7 第 2 步写着 `runtime-pack --verify` 要"核对 sha-256 和 GPG 签名"。本任务没有把 GPG 做进 xtask：哈希锁定在入库的清单里，经过评审，GPG 只在更新锁定值时有意义。见汇报的遗留问题。

**解包**：只取普通文件，跳过链接；每个路径去掉首层目录后必须全是普通名字（没有 `..`、根、盘符），否则整个命令失败；可执行位按 tar 头取 0755 或 0644。

**重签**：`codesign --force --sign - --options runtime --timestamp=none --identifier dev.cupper.runtime.node --entitlements runtime/node.entitlements.plist`，再 `--verify --strict`，从 `--display --verbose=3` 取 `CDHash`。开工前在复制的系统二进制上实测：ad-hoc 签名不带时间戳，两次签出的文件 sha-256 相同，所以清单能记重签后 Node 的 sha-256 和 cdhash，`--verify` 重算后比对。只动 `target/runtime-pack/` 下的副本。

**暂存目录按 `CupperHost.app/Contents` 布局**：`Helpers/node`、`Resources/runtime/{node-LICENSE, codex-acp-2.2.2/{index.mjs, LICENSE}, esbuild-wasm-0.28.2/…, bundle-claude-acp.mjs, locks/…}`，与设计 5.10.1 一致；清单的 `files` 用相对 `Contents` 的路径。T005 直接把 `target/runtime-pack/Contents/` 合并进 `CupperHost.app/Contents/` 即可。esbuild-wasm 整包保留（含浏览器版文件），每个文件都记了哈希。

**claude-agent-acp 的打包产物**

- 用 SP-2 `install-runtime.mjs` 的 esbuild 选项（`absWorkingDir` 设为隔离根目录等），算出的 sha-256 是 `a8f29655…`，**与 SP-2 三天前在另一个目录下算出的值逐字节相同**，可复现性得到第二次确认。
- esbuild-wasm 的 `lib/main.js` 以 `node` 这个名字从 PATH 拉起服务进程，不用 `process.execPath`。第一次运行时环境变量全清，报 `spawn node ENOENT`。现在只给 `PATH=<暂存的 Helpers 目录>`，保证用的是重签后的 Node。**T056 的安装器同样要这样设 PATH**，否则用户机器上要么找不到 node，要么用到别的 node。
- 锁文件里的包不进下载缓存：每次都下载到 `target/runtime-pack/claude-agent-acp.isolated/`，校验 SRI、解包、打包、算哈希、扫描环境变量后整个目录删除（`Drop` 守卫；上次被杀留下的目录在下次开始时先删）。optional 的 8 个平台包（含 Claude Code 二进制）从不下载。全程约 20 秒。
- 跑打包脚本时用仓库里那份（与暂存的那份字节相同），工作目录是 `runtime/`，不在任何下载目录里执行解释器。

**许可证文本**：从校验过的 tarball 取出 Node 的 `LICENSE`、codex-acp 的 `LICENSE`、esbuild-wasm 的 `LICENSE.md`，写到 `runtime/licenses/`；`--verify` 逐字节比对。codex-acp 内联的 22 个依赖的声明按设计 5.1.2 属于 `tools/license-gate` 的"THIRD_PARTY_NOTICES 生成"，没有做，见交接。

**CI**：`runtime-pack` 要联网，不进 `pr` 组合作业。设计 5.1.6 把 `runtime-pack --verify` 放在每晚的 `release-config`（T161）。UNIT-runtime-pack-envscan 的测试离线运行，在 `rust` 作业的 `cargo test` 里执行；其中一条检查入库的清单是规范格式、全部读取都已归类。

## 环境变量扫描

**规则**：凡是名为 `env` 或以 `Env` 结尾的标识符（`process.env`、esbuild 改名后的 `process2.env`、`options.env`、`spawnEnv`），取它后面的成员读取：`.NAME`、`?.NAME`（后面紧跟 `(` 的方法调用除外）、`["NAME"]`，也接受中间隔一个 `)`（`(r ?? process.env).NAME`）。键是表达式的只计数（`dynamic`）。不是 `process.env` 的对象上，只收看起来像变量名的（含下划线或大写开头），排除了 ajv 的 `schemaEnv.root`、codex-acp 里 `env.name`、`env.value` 这类误报。

**试过又删掉的**：`"NAME" in env` 和 `{ NAME } = process.env` 两种写法，在两个锁定的产物里都没有出现，为了规模删掉，在模块文档里写明没覆盖。

**覆盖不到的（重要）**：Claude Agent SDK 0.3.300 自带一张环境变量登记表：8 个对象，约 1,160 个名字，每个配一个 zod 解析器，经 `process.env[name]` 动态读取。静态扫描只看到 SDK 直接写死的约 100 个读取点，登记表里的名字它看不到（只体现在 `dynamic` 计数里）。另外适配器会把自己的整个环境传给 Claude Code CLI 子进程，CLI 读的变量更多。宿主对适配器的环境是白名单制（5.6.6），这些名字只有在用户放行时才会出现，所以禁用清单是纵深防御；但"扫描能发现新增的控制变量"这一前提对 claude 侧只部分成立。见汇报的遗留问题。

**扫描结果**：claude-agent-acp 产物 113 个名字、8 处动态读取；codex-acp 21 个名字、4 处动态读取。

**归类**（`allow` 18 项、`deny` 34 项、`harmless` 93 项）：

- `allow`：照抄设计 5.6.6 的固定白名单，代理变量展开成大小写两组（`HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`、`NO_PROXY` 及其小写）。
- `deny`：设计 5.6.6 的全部条目，另加本任务扫出来的 15 项。归入禁用的原则：改变适配器提供哪些登录方式、用哪个程序或配置目录、把会话或日志写到哪里、以什么模式运行，或者是适配器自己的开关。
  - `CODEX_ACP_*`：codex-acp 自己的开关（`CODEX_ACP_APP_SERVER_CRASH_LIMIT`、`…_CRASH_WINDOW_MS`，app-server 崩溃重启的上限），与 `CLAUDE_AGENT_ACP_*` 同理。设计的控制变量清单漏了它。
  - `NO_BROWSER`、`SSH_CLIENT`、`SSH_CONNECTION`、`SSH_TTY`、`CLAUDE_CODE_REMOTE`：两个适配器据此决定提供哪些登录方式（codex-acp 设了 `NO_BROWSER` 就不提供 ChatGPT 登录；claude-agent-acp 把这五个之一视为远程会话）。
  - `CLAUDE_CODE_REMOTE_SESSION_ID`、`CLAUDE_CODE_SESSION_KIND`：CLI 的远程会话、后台会话模式。
  - `CLAUDE_CODE_DEBUG_LOGS_DIR`、`CLAUDE_CODE_DEBUG_LOG_LEVEL`：调试日志的位置和级别，与已禁用的 `APP_SERVER_LOGS`、`CLAUDE_AGENT_LOGS` 同理。
  - `CLAUDE_SECURESTORAGE_CONFIG_DIR`、`CLAUDE_CODE_PROJECT_DIR_NAME`：凭据存储配置和会话存放目录，与宿主要校验的 `CLAUDE_CONFIG_DIR` 同理。
  - `CLAUDE_CODE_CUSTOM_OAUTH_URL`、`CLAUDE_CODE_OAUTH_CLIENT_ID`、`CLAUDE_LOCAL_OAUTH_*`：OAuth 端点和客户端的覆盖（开发用），会把登录引到别的服务器；用户没有正当用途。
- `harmless`：用户可以按需放行的认证、端点、模型和平台选择（`ANTHROPIC_API_KEY`、`ANTHROPIC_BASE_URL`、`CLAUDE_CODE_USE_BEDROCK` 等，与 5.6.6 举的例子同类）、隐私开关（`DISABLE_TELEMETRY`、`DO_NOT_TRACK` 等）、调试输出（`DEBUG`、`DEBUG_SDK`）、SDK 自己写入的（`CLAUDE_AGENT_SDK_VERSION`、`NoDefaultCurrentDirectoryInExePath`），以及终端、IDE、云平台、CI、Windows 和 WSL 的探测变量。`harmless` 里不用前缀，新名字一律要人看过。

## 交接

- **T005**：把 `target/runtime-pack/Contents/` 合并进 `CupperHost.app/Contents/`。清单里 Node 的 sha-256 和 cdhash 是 ad-hoc 签名的值。
- **T006 / license-gate**：codex-acp 内联依赖的声明应按**上游 v2.2.2 标签的 `package-lock.json`** 生成（`dev` 以外、去掉 `@openai/codex`），不要用 SP-2 的锁文件：后者是 SP-2 自己按版本范围解析出来的，与上游打包时内联的版本不同（`@agentclientprotocol/sdk` 1.5.0 对 1.8.0、`zod` 4.6.5 对 4.6.7、`vscode-jsonrpc` 9.0.2 对 9.0.3、`bundle-name` 4.1.0 对 4.1.1、`wsl-utils` 1.0.0 对 1.0.1，另有嵌套的 `powershell-utils` 0.1.0）。
- **T056**：拉起打包脚本时 PATH 只放随包 Node 所在的目录（见上）；打包脚本的用法写在脚本开头。
