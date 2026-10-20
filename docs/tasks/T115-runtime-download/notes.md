# T115 工作笔记

- 执行者：子 agent（工程师；T004、T056 的执行者接手）

## 开工前核对现状

- **`runtime/`（T056）**：
  - 驱动是同步的：`Installer::run(publish)`，核心不实现 `pure::Core`；
  - 组件只有 node 和 esbuild-wasm；
  - 计划 `ComponentPlan` 的文件表是"版本目录内路径 → 包内路径和哈希"，只能表示"从包内复制"。claude-agent-acp 的版本目录里只有本机打包的产物，它的哈希是清单里的 `bundle_sha256`，计划的形状要改。
- **`launch.rs`（T055）**：
  - `production` 从 `<数据目录>/runtime/node` 和 `<数据目录>/runtime/bundle-claude-acp.mjs` 拉起。
  - 拉起前按 `manifest::CLAUDE_BUNDLE`（`Resources/runtime/bundle-claude-acp.mjs`）的哈希核对适配器。**这个文件是打包脚本，不是适配器**：适配器是打包脚本的产物，哈希应是 `claude_agent_acp.bundle_sha256`。照现状拉起，会用 Node 去跑打包脚本。第 5 项衔接时一并改正，`CLAUDE_BUNDLE` 这个常量随之删掉。
- **`manifest.rs`**：`claude_agent_acp` 只解析了 `version`，还要加 `lockfile`、`entry`、`bundle_sha256`。
- **锁文件**：116 个条目，去掉 optional、dev、link 后要下载 107 个包。8 个 optional 包都是 `@anthropic-ai/claude-agent-sdk-<平台>`。
- **HTTP 客户端可行性**（试加依赖后已还原，没有提交）：
  - 依赖写法：`reqwest = { version = "=0.12.28", default-features = false, features = ["rustls-tls-native-roots"] }`；
  - 加密实现走 `ring`，根证书走 `rustls-native-certs`；依赖树里没有 `aws-lc-rs`、`webpki-roots`、`openssl`；
  - `cargo deny check licenses bans` 通过；
  - `Cargo.lock` 新增 69 个包（hyper、rustls、url/idna/icu 等）。
  - 关掉默认特性后，也就没有读 macOS 系统代理的 `macos-system-configuration`。代理只按登录 shell 的 `HTTPS_PROXY` 显式设置，并关掉 reqwest 读本进程环境变量的代理。
- **本机打包**：可以复用 `driver::probe::run::run`（经 `cupper_sys::spawn` 拉起，用 `Clock` 限时，超时杀进程组），不必另写。

## 开工前的规模估计

行数指 rustfmt 展开后的新增行，含注释和空行，不含 `Cargo.lock`。"草估"是逐模块的估计。T056 的实现实际是草估的 1.42 倍，这里仍按 1.3 算。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `manifest.rs` | `claude_agent_acp` 的锁文件、入口、打包哈希；删掉错的 `CLAUDE_BUNDLE` | 15 |
| 计划（`plan.rs`） | 计划分成两部分："版本目录里有什么、各自的哈希"和"从哪来"（包内复制，或按锁文件下载加打包）；claude-agent-acp 的计划（产物 ← `bundle_sha256`，锁文件和打包脚本作为包内输入） | 45 |
| 核心（`machine.rs`） | 改为实现 `pure::Core`；claude-agent-acp 等 node 和 esbuild-wasm 都 `ready` 再开始，依赖失败时随之失败；`network` 按定时器退避重试 3 次，`hash_mismatch` 不重试；记住"挪开重装" | 90 |
| 驱动（`mod.rs`） | 新组件和两个失败原因；驱动改为异步、带 `Clock`，用 `TimerTable` 等定时器，文件操作放进 `spawn_blocking`；配置（registry、代理、时长）；导出 `verify` 和 Claude 适配器的两个已装文件 | 100 |
| 版本目录（`files.rs`） | 按来源填充 staging；锁文件和打包脚本经 `Bundle` 读取、核对后放进 staging；打包后、提交前删掉 npm 树 | 30 |
| 下载（`npm.rs`） | 解析锁文件（跳过 optional、dev、devOptional、link；路径检查；`resolved` 必须在 `https://registry.npmjs.org/` 下，换成注入的 registry）；reqwest 客户端（UA、代理；限时经 `Clock`）；并发 8 个下载，SRI（sha-512）核对；解包只取普通文件、路径不越界 | 170 |
| 打包 | 经 `probe::run` 用已装的 Node 跑打包脚本，环境只有 `PATH`，用 `Clock` 限时；产物 sha-256 核对 | 35 |
| `launch.rs` | `production` 改到 `runtime/<组件>/<版本>/`；拉起前用 `runtime::verify`；删掉自己那份 `verify` | 10（净增，改动约 40） |
| 时长、依赖 | `runtime_retry`、`runtime_download`、`runtime_bundle` 进时长注册表；`reqwest`、`tar`、`flate2`、`base64` | 15 |
| **实现小计** | | **510**；乘 1.3 约 **660** |

**测试**

| 项 | 内容 | 草估 |
|---|---|---|
| 下载的单元测试 | 锁文件的跳过、路径检查、registry 替换；解包的越界路径和链接 | 50 |
| 夹具 | 用 tokio 写的最小 HTTP registry（路径 → 内容、按次数注入失败、记录请求和 UA）；造 tarball；假 Node（shell 脚本：核对环境只有 `PATH`，把入口文件抄成产物） | 100 |
| IT-runtime-download | 经本地 registry 的完整安装（optional 包没有被请求、UA、npm 树已删、版本目录只有产物）；网络失败在 `ManualClock` 上退避重试 3 次后失败，以及重试 2 次后成功；SRI 不符（不重试）；越界的 tarball；打包产物哈希不符；依赖失败随之失败 | 160 |
| 已有用例的调整 | T056 的用例改用异步驱动；`launch` 的用例 | 20 |
| **测试小计** | | **330** |

**合计约 840 行**（草估）到 **990 行**（实现乘 1.3），超过约 800 行的线。按任务书，开工前停下汇报，没有写代码。

## 拆分建议

| 拆分 | 模块 | 用例 | 估计 |
|---|---|---|---|
| **T115a 按锁文件下载** | `manifest.rs` 加字段；`npm.rs`（锁文件、reqwest 客户端、并发下载与 SRI、解包）；依赖 | 下载的单元测试；本地 registry 夹具；IT-runtime-download 的 SRI 不符、越界的 tarball、网络错误归为 `network`、optional 包不下载、UA | 实现约 200，乘 1.3 约 260；测试约 180；合计约 **440** |
| **T115b claude-agent-acp 组件与本机打包** | 计划的两种来源；核心实现 `Core`（依赖等待、退避重试）；异步驱动；打包与产物核对；两处衔接（`launch` 改布局、核对只留一份，修正 `CLAUDE_BUNDLE`） | 完整安装、退避重试 3 次、打包产物不符、依赖失败；T056 用例调整 | 实现约 310，乘 1.3 约 400；测试约 160；合计约 **560** |

T115a 结束时，下载模块只有测试在用，产品代码里还没有调用方，T115b 接上。T056 的 `Installer` 合并时也是这种情况。

T115b 的规模偏大。如果要更均衡，第 5 项的两处衔接（约 50 行，含测试）可以再单独拆出去。但它要用 claude-agent-acp 的版本目录布局，最好在 T115b 之后做。

## 需要编排者定的几点

1. 是否按上表拆分。
2. tarball 已通过 SRI、却含越界路径时，失败原因建议算 `hash_mismatch`："内容不可接受"，不重试。设计 5.2.12 只列了 `network`、`hash_mismatch`、`disk`、`bundle_changed` 四种。
3. 重试粒度建议按组件整体重试，每次重下全部包：核心只看组件，staging 每次重建。不做按包重试和已下载包的缓存。
4. 时长建议：
   - 退避从 `runtime_retry` = 2 秒起，每次翻倍（2、4、8 秒）；
   - 打包限时 `runtime_bundle` = 120 秒；
   - 下载并发 8 个；每个包的下载限时 `runtime_download` = 60 秒，用 `Clock` 的定时器和下载 `select!`，不用 reqwest 自带的超时（那是 tokio 计时，绕过 `Clock`）。
