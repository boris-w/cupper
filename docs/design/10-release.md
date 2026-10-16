# 5.10 构建与发布

- 所属：`docs/02-design.md` 第 5 节
- 依据：ADR-01（helper bundle、升级时机、自检）、ADR-03（打包工具待定、Electron 版本、只做官网下载）、ADR-08（Node 运行时与清单）、ADR-11（再分发红线、许可证门禁）；SP-2（重签 Node、单文件适配器）、SP-4b（替换整个 .app 的实测）；R04、R05（同类产品的打包工具）；设计评审会（`docs/meetings/2026-10-16-design-review/`）
- 外部事实的核对记录见 `docs/tasks/D01-detailed-design/notes.md`

## 5.10.1 包结构

```
Cupper.app/Contents/
├── Info.plist                  dev.cupper.app；LSMinimumSystemVersion 14.0；cupper:// URL scheme；不写辅助功能、自动化、屏幕录制的用途说明
├── MacOS/Cupper                Electron 主程序
├── Frameworks/                 Electron Framework、4 个 Electron helper、Squirrel.Mac 及其依赖
├── Helpers/
│   ├── cupper                  cupper-cli（dev.cupper.cli）
│   └── CupperHost.app/         会话宿主（ADR-01）
│       └── Contents/
│           ├── Info.plist      dev.cupper.app.host；LSUIElement；同样不写上述用途说明
│           ├── MacOS/cupper-host        运行时清单在编译期内嵌
│           ├── Helpers/node             已用 cupper 身份重签的 Node（dev.cupper.runtime.node），随 App 一起公证
│           └── Resources/runtime/
│               ├── codex-acp-2.2.2/       index.mjs、LICENSE、内联依赖的许可证声明
│               ├── esbuild-wasm-<版本>/   打包 claude-agent-acp 用，附 LICENSE
│               ├── bundle-claude-acp.mjs  本机打包脚本
│               ├── node-LICENSE
│               └── locks/claude-agent-acp-0.91.0.package-lock.json   只是元数据
├── Resources/
│   ├── app.asar                界面代码（asar 完整性校验）
│   ├── app.asar.unpacked/…/cupper_native.node
│   └── THIRD_PARTY_NOTICES.txt
└── Library/LaunchAgents/       M2 的后台常驻（ADR-01），M1 没有
```

- bundle identifier 的前缀是 `dev.cupper`（Q5，已定（2026-10-16），见 `docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）；M1b 首次签名发布前可以无成本修改，之后不再改。开发构建和 e2e 构建的 identifier 见 [5.7.9](07-desktop.md#579-构建种类)。
- 只出 arm64 构建（Q6，已定（2026-10-16））。以后需要 x64 时只多一个构建目标，不涉及兼容代码。
- **CLI 放在 `Contents/Helpers/cupper`**：按 Apple 的 bundle 布局，可执行代码不放 `Resources`（桌面建议 #16）。设置页提供"安装命令行工具"，在用户选择的目录里建符号链接。
- **Node 作为普通 Mach-O 放在 `CupperHost.app/Contents/Helpers/node`**（U-R3，已定（2026-10-16），见 `docs/meetings/2026-10-16-gate-design-decisions/minutes.md`；即原待定问题 Q3 的后备方案）：随 App 一起公证，公证覆盖到 Node；安装时复制到运行时目录并核对 cdhash，不再需要解 tar.xz。代价是下载包大一些，安装后多占约 100 MB（估计，未核实）。
- **App 里不出现** `@anthropic-ai/claude-agent-sdk` 的任何部分和 Claude Code 二进制（ADR-11 红线）；claude-agent-acp 在用户机器上从 npm 安装并打包（ADR-08）。
- codex-acp 是 Apache-2.0，按 ADR-11 可以随 App 分发，所以直接随包携带，Codex 不需要联网安装（Q4 定案，纪要 D2）。

## 5.10.2 签名与公证

签名从内到外进行。

| 对象 | identifier | entitlements | 由谁签、何时签 |
|---|---|---|---|
| `CupperHost.app/Contents/Helpers/node` | `dev.cupper.runtime.node` | 只有 `com.apple.security.cs.allow-jit`（SP-2：没有它一 JIT 就崩溃；去掉官方二进制自带的其余 5 项） | `cargo xtask runtime-pack` |
| `CupperHost.app`（含 `cupper-host`） | `dev.cupper.app.host` | 无 | `cargo xtask host-bundle`；electron-builder 用 `signIgnore` 跳过它，不重签 |
| `cupper`（CLI） | `dev.cupper.cli` | 无 | CI 单独签；同样 `signIgnore` |
| `cupper_native.node` | 随 App 递归签名 | 无 | electron-builder |
| Electron 的 4 个 helper | `dev.cupper.app.helper`、`.helper.Renderer`、`.helper.GPU`、`.helper.Plugin` | `entitlementsInherit`：只有 `allow-jit` | electron-builder |
| `Cupper.app` 主程序 | `dev.cupper.app` | 只有 `allow-jit`（V8 需要）；**不带** `get-task-allow`、`disable-library-validation`、`allow-dyld-environment-variables`、`automation.apple-events` | electron-builder，最后签 |

- 全部开启 hardened runtime，带时间戳，用 Developer ID Application 证书。
- 公证：electron-builder 的 `mac.notarize` 用 notarytool（App Store Connect API key）公证 `.app` 并 staple；DMG 再单独公证、staple。
- CI 验证：`codesign --verify --deep --strict`、`spctl --assess --type execute`；逐个 Mach-O 导出 entitlements，与上表的白名单比对，多一项就失败；两个 Info.plist 都不含 `NSAppleEventsUsageDescription` 等用途说明（安全 #13）。
- 随 App 公证的 Node 首次执行的耗时，在第一次有 Developer ID 的发布构建上人工记录（MAN-node-notarize）。

## 5.10.3 Node 运行时与清单

在 ADR-08 的基础上细化：

- **清单**：`runtime/manifest.json`，由 SP-2 的试用格式扩充而来，覆盖**所有会被执行或加载的文件**（安全建议 #20）：

  | 组件 | 来源 | 校验 |
  |---|---|---|
  | `node` | 随包（`CupperHost.app/Contents/Helpers/node`） | 文件的 sha-256 和 cdhash（复制时核对，拉起前再核对 cdhash） |
  | `codex-acp` | 随包 | `index.mjs` 的 sha-256（CI 构建时先按 npm tarball 的 SRI 校验上游文件） |
  | `esbuild-wasm` | 随包 | 每个文件的 sha-256 |
  | `bundle-claude-acp.mjs` | 随包 | sha-256 |
  | `claude-agent-acp` | 用户机器上从 npm 下载 | 锁文件里每个包的 SRI；本机打包产物的固定 sha-256（esbuild-wasm 的产物逐字节确定，CI 预先算好） |
  | 已验证 CLI 范围 | 由兼容报告生成（[5.9.5](09-testing.md#595-兼容矩阵)） | 每个适配器版本一份，供 [5.6.6](06-agent-driver.md#566-cli-的发现与版本检查) 使用 |
  | 适配器读取的环境变量 | `runtime-pack` 从锁定的适配器产物里提取 | 新出现的名字要人工归类（[5.6.6](06-agent-driver.md#566-cli-的发现与版本检查)） |

- **内嵌**：`cupper-host` 的 `build.rs` 在编译期读取清单，宿主运行中不读包内的清单文件。清单的可信性就来自宿主二进制的签名（ADR-08 的"清单随 App 签名"）。
- **安装与校验**：见 [5.2.12](02-host.md#5212-运行时安装器)。按锁文件直接下载 tarball，不调用 npm，不运行任何生命周期脚本；先核对哈希，再解包。
- **版本**：Node 24 LTS（起始 24.22.0，与 Electron 44 自带的 Node 版本相同，SP-2 实测通过）。Node 的安全更新跟随 App 发版（ADR-08）。
- CI 计算 claude-agent-acp 本机打包产物的哈希时，需要在一个隔离的临时目录里下载它的依赖（含 Anthropic SDK）；这个目录不属于任何打包输入，算完即删，许可证门禁会检查打包输入里没有它（ADR-11）。

## 5.10.4 打包工具：electron-builder 还是 Electron Forge

**决定：用 electron-builder（26.x）配合 electron-updater（6.8.x），通过 GitHub Releases 分发。**

| 考量 | electron-builder + electron-updater | Electron Forge |
|---|---|---|
| **安装时机由我们控制**（ADR-01） | `autoInstallOnAppQuit = false` 时，下载好的更新**不会**交给 Squirrel.Mac，直到我们调用 `quitAndInstall()`（已核对 electron-updater 6.8.10 的 `MacUpdater` 源码；7.x 改名为 `autoInstallEvent: "manual"`，语义相同）。正好实现"宿主先退出再安装" | Forge 默认的更新路径是 Electron 内置的 `autoUpdater`（Squirrel.Mac），文档写明"下载好的更新总会在下次启动时应用"，无法暂缓。要控制时机只有三条路：等宿主退出后才开始检查和下载（几百 MB，用户要等）；在 Forge 下自己生成 electron-updater 需要的 `latest-mac.yml`（补丁代码，违反工程原则）；像 Codex 那样接 Sparkle（要写原生扩展） |
| Fuses | 26.x 的配置 schema 有 `electronFuses`（已核对），覆盖 ADR-03 要求的全部 fuse | `@electron-forge/plugin-fuses` |
| 嵌套 helper 和按文件的 entitlements | `extraFiles` 直接放进 `Contents/Helpers`；预先签好的宿主和 CLI 用 `signIgnore` 跳过 | `osxSign.optionsForFile` 能按文件指定 entitlements，更顺手；但把 bundle 放进 `Contents/Helpers` 要写打包钩子 |
| 签名与公证 | `mac.notarize` 内置 notarytool | `osxNotarize`（@electron/notarize） |
| helper 的 identifier | 给 Renderer、GPU、Plugin helper 加后缀，审计日志里能分清 | @electron/packager 让它们和基础 helper 的 identifier 相同（技术选型讨论会第二轮已核实）。对 ADR-05 无影响（helper 都会被拒），只影响可读性 |
| Vite | 自己写 Vite 配置构建 main、preload、两个渲染入口，一次性工作 | 有官方 Vite 插件（Codex、Claude 用的就是 Forge + Vite） |
| 同类产品（R04、R05） | Paseo、Lody、Superhuman、Linear、Manus、Granola、千问、智谱、Notion、DeepSeek Harness | Codex（配 Sparkle，没用 Squirrel）、Claude（Forge 8 alpha + fuses）、Goose |
| 成熟度 | 26.x 是稳定线；27.x 在 alpha | Forge 8.0.0 在 2026-09-29 才正式发布，Forge 7.11 仍在维护 |
| 与现有 ADR 的一致性 | ADR-01、ADR-03 已写明 electron-builder + electron-updater，不需要改 ADR | 需要改 ADR-01 的升级流程 |

**理由**：决定性的一条是安装时机。ADR-01 和 SP-4b 证明"宿主运行时替换 App 包"会让新 UI 无法校验旧宿主、旧宿主读到新资源，所以必须"宿主先退出、再安装"。electron-updater 原生支持暂缓安装，不需要任何补丁代码；Forge 要么做不到，要么要额外写原生扩展或自造更新元数据。Codex 用 Forge 却配了 Sparkle，正说明 Forge 默认的更新路径满足不了这类需求。其余各项两者相当，Forge 的 Vite 集成优势是一次性的小成本。

**以后什么情况下重新评估**：需要 EdDSA 签名的更新源、增量更新，或者 electron-updater 不再维护时，再评估 Sparkle；那时打包工具的选择与更新方案可以分开考虑。

**需要实测确认**：首次正式发布前做一次升级演练（M1b 发布任务的验收项，MAN-upgrade）：用真实签名的两个版本，确认 Squirrel.Mac 的 ShipIt 是"新包移入、旧包移走"，而不是原地覆盖正在运行的可执行文件（SP-4b 的 S4 会导致 SIGKILL）；同时验证 5.5.7 的整个流程和下文的安装空窗处理。

## 5.10.5 更新流程

- **配置**：GitHub provider（公开仓库的 Releases）；`autoDownload = true`；`autoInstallOnAppQuit = false`（升级到 electron-updater 7 时改为 `autoInstallEvent = "manual"`）；`allowPrerelease = false`；`allowDowngrade = false`。
- **检查**：启动 30 秒后检查一次，之后每 4 小时一次；离线时跳过。只在后台下载，不打断用户。
- **完整性**：`latest-mac.yml` 经 HTTPS 获取，带每个文件的 sha-512；Squirrel.Mac 安装前会核对新包的签名满足当前 App 的 designated requirement（同一 Team ID 和 identifier）。Squirrel 不比较版本号，防降级只靠 electron-updater 的版本比较。
- **本地代理端口**（安全建议 #14）：electron-updater 在下载完成时就在 UI 进程里开一个只监听 127.0.0.1 的代理端口，把 zip 交给 Squirrel.Mac；它一直开到调用 `quitAndInstall` 才关闭，可能持续数天。根路径要求随机生成的 Basic 认证，zip 在 128 位随机路径下。写进安全说明（[5.8.5](08-security.md#585-进程卫生)）。升级演练加两项负面测试：其他 Team 签名的包被拒、被篡改的 zip 被拒。
- **安装**：用户点"重启以更新"，或者退出 App 时有已下载的更新，按 [5.5.7](05-flows.md#557-升级) 执行：`host/prepare_upgrade` → 宿主等回合结束（用户可以选择立即中断）后退出 → 主进程确认宿主已退出 → `quitAndInstall()`。M2 的后台模式下，关闭窗口不安装更新（ADR-01）。
- **安装空窗**（桌面 #15）：`quitAndInstall()` 之后，Squirrel 要经本地代理取 zip、解包、校验签名，全部完成后 App 才退出。这段时间宿主已经退出，界面还开着：
  - `hostLauncher` 不得自动重新拉起宿主，界面显示"正在安装更新"；
  - Squirrel 报错时（签名不符、磁盘满），重新拉起宿主、恢复界面并提示；
  - 走 `before-quit-for-update` 时，`lifecycle` 跳过 NFR-8 的退出检查（control 连接已经断开）；
  - M2 的 LaunchAgent 如果配置了 KeepAlive，要先停掉，否则 launchd 会把宿主拉回来。
- **安装位置**：首次启动时用 `app.isInApplicationsFolder()` 检查；不在"应用程序"里就提示，确认后调用 `moveToApplicationsFolder()`。从"下载"文件夹或 App Translocation 路径运行时，Squirrel 无法替换包。
- **运行时随之更新**：新版本的宿主内嵌新的清单，启动后安装新版本的 Node 和适配器目录；旧版本目录在没有进程使用后回收。
- **不支持降级**（NFR-10）：旧版本的宿主遇到更新的数据库会以退出码 4 退出，界面给出说明。

## 5.10.6 Electron 版本

- **起始版本：Electron 44.8.0**（2026-10-16 时的最新稳定版；官方支持的是 42、43、44 三个大版本，45 在 beta）。
- **策略**：只用一个大版本构建，始终在官方支持的最新三个大版本之内；新的大版本正式发布后，在一个发布周期（约 8 周）内跟进，每次升级跑完整的 CI、e2e 和性能回归（ADR-03），并按人工验证清单用实体键盘确认真实输入的规则（MAN-keyboard）。M1a 开工时如果 45 已经正式发布两周以上，就直接从 45 开始。
- **macOS 26 的卡顿问题**（ADR-03 提到 Qoder 为此升级）：根因是 Electron 覆盖了私有的 `_cornerMask` 接口，导致 WindowServer 的 GPU 负载过高，修复已于 2025-09 合入并回移到当时的各个支持版本（electron/electron #48376 等）。42 及以后的版本都已包含，44 不受影响。
- Electron 的最低系统要求（macOS 12）低于我们的 macOS 14，没有冲突。

## 5.10.7 发布流水线

由推送受保护的 `release/vX.Y.Z` tag 触发（`release.yml`）；同一条流水线也用于 `release/vX.Y.Z-rc.N`，在正式发布之前跑通签名路径。作业放在需要人工批准的 protected environment 里，签名证书和 App Store Connect API key 只对它可见；在 `macos-15` runner 上运行（安全 #10）。

1. 检查版本号一致：tag、`Cargo.toml`、`apps/desktop/package.json`。最近一次 `security-macos14` 的每晚校验必须是通过的（Q24）。
2. `cargo xtask runtime-pack --verify`：下载 Node（核对 sha-256 和 GPG 签名）、重签 `node`；按 SRI 校验 codex-acp 和 esbuild-wasm；在隔离目录里按锁文件重算 claude-agent-acp 本机打包产物的哈希，与清单比对；检查适配器读取的环境变量没有未归类的新名字。
3. **对每个二进制单独构建**：`cargo build -p cupper-host --release --locked`、`cargo build -p cupper-cli --release --locked`、`cargo build -p cupper-desktop-native --release --locked`，避免 cargo 合并特性把测试特性带进发布二进制（测试 #14）。
4. `cargo xtask host-bundle`：组装并签名 `CupperHost.app`（含 `Helpers/node`）；单独签名 `cupper`。
5. `pnpm build` 后运行 electron-builder（`electron-builder.release.yml`）：Vite 产物进 asar → `extraFiles` 放入宿主和 CLI → 烧录 fuses → 签名 → 公证 → staple → 生成 DMG 和 zip → 生成 `latest-mac.yml` 和 blockmap。
6. **产物检查**（测试 #14、安全 #11、#12）：
   - 签名、`spctl`、entitlements 白名单、Info.plist 不含用途说明；
   - `@electron/fuses read` 逐项断言；Info.plist 里有 `ElectronAsarIntegrity`；`asarUnpack` 只有 `cupper_native.node`；asar 文件清单与预期一致；
   - `cupper-host --build-info` 和 `cupper --build-info` 不含 `dev-auth`、`test-harness`；发布版 CLI 没有 `--harness-key-fd`；
   - 产物里扫描测试钩子的标记串（`__cupperTest`、e2e 专用的主进程钩子），命中就失败；
   - **许可证门禁第二层**：对最终的 `.app`、zip、DMG 按内容指纹扫描（`@anthropic-ai/claude-agent-sdk` 发布文件的哈希和特征串、Claude Code 的 Mach-O 签名 identifier）；`locks/` 下的锁文件按哈希放进允许清单；
   - `THIRD_PARTY_NOTICES.txt` 存在且包含 Node、codex-acp 及其内联依赖、esbuild-wasm、Electron 与前端依赖、Rust 依赖（cargo-about 生成）；
   - `cupper-host --self-test`：校验内嵌清单与随包组件一致，然后退出。
7. **签名后冒烟**（测试 #15、安全 #12；编号 SMOKE-SIGNED-*）：在 runner 上启动签名后的 App 和 CLI，断言：
   - 宿主认 UI 走的是 `codesign` 认证，`weak_auth` 为 false；
   - CLI 默认只读；
   - 杀掉 UI 主进程后，新 UI 能接上旧宿主（NFR-8；退化模式下做不到，这里补上覆盖，Q22）；
   - 握手矩阵：UI 主进程被接受；4 个 Electron helper、CLI、node、e2e 版 UI、复制后用 ad-hoc 重签的 UI 都被拒绝；
   - UI 和 CLI 都拒绝假宿主；发布版 UI 遇到 `weak_auth: true` 的宿主拒绝连接。
8. 任何一步失败，都不上传；全部通过后上传到 GitHub Release 的草稿，由人工发布。

"用项目的 Apple Development 证书构建一个 CI 签名要求变体、每晚跑握手矩阵"暂不做：它要把开发证书放进 CI。RC 标签和签名后冒烟已经覆盖发布路径；等有了 Developer ID 之后，在 M1b 的发布任务里再评估。
