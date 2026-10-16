# ADR-08 Agent 程序来源、适配器分发与版本锁定

- 状态：已采纳（2026-10-16，`gate/design`）
- 日期：2026-10-16
- 来源：SP-2 实测（`docs/tasks/SP-2-adapter-distribution/report.md`）、SP-1、SP-4b、ADR-01

## 背景

PRD 的 AG-1 和 NFR-5 要求：
- agent 程序使用用户本机已登录的官方 CLI；
- 适配器由 cupper 锁定版本；
- 不随 App 再分发受 Anthropic 商业条款约束的组件。

另有几项约束：
- claude-agent-acp 需要 Node 22 及以上；
- 不能借用 Electron 自带的 Node（那需要打开 `runAsNode` fuse，与 ADR-03 冲突）；
- 官方 Node 二进制自带 `get-task-allow` 等 entitlement，会削弱注入防护（SP-4b）。

## 决策

**1. agent 程序：只用用户本机的 CLI**
- 通过 `CLAUDE_CODE_EXECUTABLE` 和 `CODEX_PATH` 传入原生二进制的**绝对路径**。npm 的 shim（`#!/usr/bin/env node`）要先解析到它包里的平台二进制，不能直接传 shim，因为 GUI 进程的 PATH 里没有 node。
- 永远不下载适配器自带的 agent 程序，省下约 580 MB。
- 查找 CLI 的顺序：
  1. 用户在设置里指定的路径；
  2. 从用户的登录 shell 读取的 PATH；
  3. 常见安装位置：Homebrew、原生安装器、npm 全局目录。
- 找到后显示路径和版本（AG-5）。具体实现在详细设计中定。

**2. Node 运行时：随 App 带一份官方 LTS，用 cupper 的身份重新签名**
- 开启 hardened runtime，**只保留 `com.apple.security.cs.allow-jit`**，去掉官方二进制自带的其余 5 项 entitlement。
- 作为普通 Mach-O 放在 `CupperHost.app/Contents/Helpers/`，随 App 一起公证；安装时复制到 `runtime/node/<版本>/` 并核对 cdhash，两个适配器共用这一份（设计评审修订，U-R3；原写法为首次运行时从随包的 tar.xz 复制）。
- 这份 Node 只用来运行适配器，**永远不跑可信代码**，也不持有任何秘密。

**3. 适配器的形态：每个都是一个 ESM 单文件，由 cupper 自己的 Node 运行**
- **codex-acp**：直接用上游发布的 `dist/index.js`，按 npm tarball 的 SRI 校验。
- **claude-agent-acp**：首次运行时，按锁文件从 npm 下载依赖，逐包校验 SRI，不运行任何包脚本，然后在本机用 esbuild-wasm 打成单文件，并核对一个固定的产物哈希。这个哈希可以在 CI 里预先算好，因为不同机器上打出的产物逐字节相同。这样 Anthropic 的 SDK 始终不经过 cupper 分发（见 ADR-11）。
- 单文件在启动时就把全部代码载入内存，所以升级时替换磁盘上的文件也不会影响正在运行的进程（SP-2 实测）。拉起前的校验耗时也从 270 ms 降到 3 ms。
- esbuild-wasm（MIT，约 14 MB）随 App 一起带，保证可以离线安装。

**4. 安装与完整性**
- `runtime-manifest.json` 随 App 一起签名，记录 Node tarball 的 sha256、codex-acp 的 SRI、claude-agent-acp 的锁文件，以及本机打包产物的固定哈希。
- 安装步骤：
  1. 清理上次留下的 staging 目录；
  2. 边下载边计算哈希，核对通过后才解压；
  3. 解包时检查路径，防止写到目标目录之外；
  4. 写入 `.installed.json`；
  5. 用 `rename(2)` 原子提交。
- 失败时的处理：哈希不一致立即失败；网络错误退避后重试 3 次；中途被杀，下次启动时重装。
- 版本目录一旦写入就不再改动。旧版本只在没有进程使用时回收。
- 每次拉起适配器之前，都核对适配器文件的 sha256 和 Node 的 cdhash（与 ADR-01 一致）。
- 安装器由 Rust 宿主实现。SP-2 里用 Node 写的原型只用来验证流程。

**5. 版本锁定与兼容性**
- 每个锁定的适配器版本都附一个"已验证的 CLI 版本范围"。实测通过的范围：
  - claude：2.1.296 跑完了完整回合，2.1.200 完成了握手；
  - codex：0.150.1 到 0.163.0。

  完整的范围用 SP-1 录制的剧本回归测试来确定。
- 拉起适配器之前先运行 `--version`。版本超出范围时给出提示，但不阻止使用（AG-5）。
- Codex 版本过旧时，会误报"Authentication required"，宿主要把它识别成"CLI 版本过旧"，并显示 `configWarning`。
- 每次 codex 发布新版本，CI 都检查一遍 codex-acp 引用的 127 个 app-server 方法有没有变化（schema drift）。
- 适配器的 `clientInfo.name` 会出现在 codex 发往 OpenAI 的 User-Agent 里，所以固定写成 `cupper`，不带任何用户信息。

## 后果

- **好处**：
  - 不再分发 Anthropic 的 SDK 和二进制；
  - App 增加 Node（不压缩，随 App 公证；下载包增加十几 MB，安装后约 100 MB，估计值）加 14 MB（esbuild-wasm）（设计评审修订，原写法为"约 24 MB（Node xz 压缩后）"）；
  - entitlement 最少；
  - 安装可以复现，也可以验证。
- **代价**：
  - 首次运行需要联网安装，耗时约 6–8 秒；
  - Node 的安全更新跟着 App 一起发版；以后要不要单独为 runtime 开一个更新通道，等有需要时再定；
  - 依赖用户本机 CLI 的版本，需要维护兼容矩阵。
- **待验证**：
  - 在有 Developer ID 的 CI 上，只带 `allow-jit` 的 Node 随 App 公证的结果（U-R3 已改为随 App 公证）；
  - 公证过的 App 里，新文件首次执行的耗时。

## 设计评审修订（2026-10-16）

来源：设计评审会（`docs/meetings/2026-10-16-design-review/minutes.md`）、用户决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）、详细设计 v0.7 的 5.13.2。本节是细化，不推翻上文；上文中被替换的写法已就地改正并注明。

- **codex-acp 随包**（Q4）：CI 按 npm 的 SRI 校验上游文件，清单记录 sha-256，附带它内联的依赖的许可证声明，每晚执行 `runtime-pack --verify`。
- **清单**在编译期内嵌进宿主二进制，覆盖所有会被执行或加载的文件（包括 `bundle-claude-acp.mjs`）；安装时按锁文件直接下载 tarball，不调用 npm，不运行生命周期脚本。
- **已验证的 CLI 范围**由兼容报告生成并写进清单，不手填。
- **未信任的项目**：驱动层经 `_meta.claudeCode.options` 传 `settingSources: ["user"]`（U-R1）。
- `runtime-pack` 从锁定的适配器产物里提取环境变量的读取点，出现未分类的新变量就失败（ADR-05 的环境变量禁用清单）。
- 安装器的解压依赖由 xz 改为 gzip（npm tarball），Node 不再需要解 xz。
- **CLI 发现**只在 PATH 的可信条目里查找：绝对路径，并且不在任何项目、工作区、临时目录下（5.6.6）。
