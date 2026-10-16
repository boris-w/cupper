# 5.10 构建与发布

- 所属：`docs/02-design.md` 第 5 节
- 依据：ADR-01（helper bundle、升级时机、自检）、ADR-03（打包工具待定、Electron 版本、只做官网下载）、ADR-08（Node 运行时与清单）、ADR-11（再分发红线、许可证门禁）；SP-2（重签 Node、单文件适配器）、SP-4b（替换整个 .app 的实测）；R04、R05（同类产品的打包工具）
- 外部事实的核对记录见 `docs/tasks/D01-detailed-design/notes.md`

## 5.10.1 包结构

```
Cupper.app/Contents/
├── Info.plist                  <前缀>.app；LSMinimumSystemVersion 14.0；cupper:// URL scheme
├── MacOS/Cupper                Electron 主程序
├── Frameworks/                 Electron Framework、4 个 Electron helper、Squirrel.Mac 及其依赖
├── Helpers/
│   └── CupperHost.app/         会话宿主（ADR-01）
│       └── Contents/
│           ├── Info.plist      <前缀>.app.host；LSUIElement
│           ├── MacOS/cupper-host        运行时清单在编译期内嵌
│           └── Resources/runtime/
│               ├── node-v24.22.0-darwin-arm64.tar.xz     其中的 node 已用 cupper 身份重签，只保留 allow-jit
│               ├── codex-acp-2.2.2/       index.mjs、LICENSE、内联依赖的许可证声明
│               ├── esbuild-wasm-<版本>/   打包 claude-agent-acp 用，附 LICENSE
│               ├── bundle-claude-acp.mjs  本机打包脚本
│               └── locks/claude-agent-acp-0.91.0.package-lock.json   只是元数据
├── Resources/
│   ├── app.asar                界面代码（asar 完整性校验）
│   ├── app.asar.unpacked/…/cupper_native.node
│   ├── bin/cupper              cupper-cli
│   └── THIRD_PARTY_NOTICES.txt
└── Library/LaunchAgents/       M2 的后台常驻（ADR-01），M1 没有
```

- 只出 arm64 构建（待定问题 Q6）。
- **App 里不出现** `@anthropic-ai/claude-agent-sdk` 的任何部分和 Claude Code 二进制（ADR-11 红线）；claude-agent-acp 在用户机器上从 npm 安装并打包（ADR-08）。
- codex-acp 是 Apache-2.0，按 ADR-11 可以随 App 分发，所以直接随包携带，Codex 不需要联网安装（对 ADR-08 的细化，见待定问题 Q4）。

## 5.10.2 签名与公证

签名从内到外进行。

| 对象 | identifier | entitlements | 由谁签、何时签 |
|---|---|---|---|
| runtime 里的 `node` | `<前缀>.runtime.node` | 只有 `com.apple.security.cs.allow-jit`（SP-2：没有它一 JIT 就崩溃；去掉官方二进制自带的其余 5 项） | `cargo xtask runtime-pack`，打进 tar.xz 之前 |
| `CupperHost.app`（含 `cupper-host`） | `<前缀>.app.host` | 无 | `cargo xtask host-bundle`；electron-builder 用 `signIgnore` 跳过它，不重签 |
| `cupper`（CLI） | `<前缀>.cli` | 无 | CI 单独签；同样 `signIgnore` |
| `cupper_native.node` | 随 App 递归签名 | 无 | electron-builder |
| Electron 的 4 个 helper | `<前缀>.app.helper`、`.helper.Renderer`、`.helper.GPU`、`.helper.Plugin` | `entitlementsInherit`：只有 `allow-jit` | electron-builder |
| `Cupper.app` 主程序 | `<前缀>.app` | 只有 `allow-jit`（V8 需要）；**不带** `get-task-allow`、`disable-library-validation`、`allow-dyld-environment-variables` | electron-builder，最后签 |

- 全部开启 hardened runtime，带时间戳，用 Developer ID Application 证书。
- 公证：electron-builder 的 `mac.notarize` 用 notarytool（App Store Connect API key）公证 `.app` 并 staple；DMG 再单独公证、staple。
- CI 验证：`codesign --verify --deep --strict`、`spctl --assess --type execute`；逐个 Mach-O 导出 entitlements，与上表的白名单比对，多一项就失败。
- Node 在 tar.xz 里，公证服务不一定检查到它；运行时安装器解出来的文件不带隔离属性，Gatekeeper 不会再评估。只带 `allow-jit` 的 Node 能否通过公证、首次执行有多慢，仍是 ADR-08 的待验证项（待定问题 Q3，附后备方案）。

## 5.10.3 Node 运行时与清单

在 ADR-08 的基础上细化：

- **清单**：`runtime/manifest.json`，由 SP-2 的试用格式扩充而来：

  | 组件 | 来源 | 校验 |
  |---|---|---|
  | `node` | 随包（`node-<版本>.tar.xz`） | tar.xz 的 sha-256；解出的 `bin/node` 的 cdhash（拉起前核对） |
  | `codex-acp` | 随包 | `index.mjs` 的 sha-256（CI 构建时先按 npm tarball 的 SRI 校验上游文件） |
  | `esbuild-wasm` | 随包 | 每个文件的 sha-256 |
  | `claude-agent-acp` | 用户机器上从 npm 下载 | 锁文件里每个包的 SRI；本机打包产物的固定 sha-256（esbuild-wasm 的产物逐字节确定，CI 预先算好） |
  | 已验证 CLI 范围 | — | 每个适配器版本一份，供 [5.6.6](06-agent-driver.md#566-cli-的发现与版本检查) 使用 |

- **内嵌**：`cupper-host` 的 `build.rs` 在编译期读取清单，宿主运行中不读包内的清单文件。清单的可信性就来自宿主二进制的签名（ADR-08 的"清单随 App 签名"）。
- **安装与校验**：见 [5.2.12](02-host.md#5212-运行时安装器)。
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
| Vite | 自己写 Vite 配置构建 main、preload、renderer 三个入口，一次性工作 | 有官方 Vite 插件（Codex、Claude 用的就是 Forge + Vite） |
| 同类产品（R04、R05） | Paseo、Lody、Superhuman、Linear、Manus、Granola、千问、智谱、Notion、DeepSeek Harness | Codex（配 Sparkle，没用 Squirrel）、Claude（Forge 8 alpha + fuses）、Goose |
| 成熟度 | 26.x 是稳定线；27.x 在 alpha | Forge 8.0.0 在 2026-09-29 才正式发布，Forge 7.11 仍在维护 |
| 与现有 ADR 的一致性 | ADR-01、ADR-03 已写明 electron-builder + electron-updater，不需要改 ADR | 需要改 ADR-01 的升级流程 |

**理由**：决定性的一条是安装时机。ADR-01 和 SP-4b 证明"宿主运行时替换 App 包"会让新 UI 无法校验旧宿主、旧宿主读到新资源，所以必须"宿主先退出、再安装"。electron-updater 原生支持暂缓安装，不需要任何补丁代码；Forge 要么做不到，要么要额外写原生扩展或自造更新元数据。Codex 用 Forge 却配了 Sparkle，正说明 Forge 默认的更新路径满足不了这类需求。其余各项两者相当，Forge 的 Vite 集成优势是一次性的小成本。

**以后什么情况下重新评估**：需要 EdDSA 签名的更新源、增量更新，或者 electron-updater 不再维护时，再评估 Sparkle；那时打包工具的选择与更新方案可以分开考虑。

**需要实测确认**：首次正式发布前做一次升级演练（M1b 发布任务的验收项）：用真实签名的两个版本，确认 Squirrel.Mac 的 ShipIt 是"新包移入、旧包移走"，而不是原地覆盖正在运行的可执行文件（SP-4b 的 S4 会导致 SIGKILL）；同时验证 5.5.7 的整个流程。

## 5.10.5 更新流程

- **配置**：GitHub provider（公开仓库的 Releases）；`autoDownload = true`；`autoInstallOnAppQuit = false`（升级到 electron-updater 7 时改为 `autoInstallEvent = "manual"`）；`allowPrerelease = false`；`allowDowngrade = false`。
- **检查**：启动 30 秒后检查一次，之后每 4 小时一次；离线时跳过。只在后台下载，不打断用户。
- **完整性**：`latest-mac.yml` 经 HTTPS 获取，带每个文件的 sha-512；Squirrel.Mac 安装前会核对新包的签名满足当前 App 的 designated requirement（同一 Team ID 和 identifier）。
- **安装**：用户点"重启以更新"，或者退出 App 时有已下载的更新，按 [5.5.7](05-flows.md#557-升级) 执行：`host/prepare_upgrade` → 宿主等回合结束（用户可以选择立即中断）后退出 → 主进程确认宿主已退出 → `quitAndInstall()`。M2 的后台模式下，关闭窗口不安装更新（ADR-01）。
- **运行时随之更新**：新版本的宿主内嵌新的清单，启动后安装新版本的 Node 和适配器目录；旧版本目录在没有进程使用后回收。
- **不支持降级**（NFR-10）：旧版本的宿主遇到更新的数据库会以退出码 4 退出，界面给出说明。

## 5.10.6 Electron 版本

- **起始版本：Electron 44.8.0**（2026-10-16 时的最新稳定版；官方支持的是 42、43、44 三个大版本，45 在 beta）。
- **策略**：只用一个大版本构建，始终在官方支持的最新三个大版本之内；新的大版本正式发布后，在一个发布周期（约 8 周）内跟进，每次升级跑完整的 CI、e2e 和性能回归（ADR-03）。M1a 开工时如果 45 已经正式发布两周以上，就直接从 45 开始。
- **macOS 26 的卡顿问题**（ADR-03 提到 Qoder 为此升级）：根因是 Electron 覆盖了私有的 `_cornerMask` 接口，导致 WindowServer 的 GPU 负载过高，修复已于 2025-09 合入并回移到当时的各个支持版本（electron/electron #48376 等）。42 及以后的版本都已包含，44 不受影响。
- Electron 的最低系统要求（macOS 12）低于我们的 macOS 14，没有冲突。

## 5.10.7 发布流水线

由推送 `release/vX.Y.Z` tag 触发（`release.yml`），在 macOS arm64 runner 上运行，签名证书和 App Store Connect API key 放在 CI 的加密变量里。

1. 检查版本号一致：tag、`Cargo.toml`、`apps/desktop/package.json`。
2. `cargo xtask runtime-pack --verify`：下载 Node（核对 sha-256 和 GPG 签名）、重签 `node`、打成 tar.xz；按 SRI 校验 codex-acp 和 esbuild-wasm；在隔离目录里按锁文件重算 claude-agent-acp 本机打包产物的哈希，与清单比对。
3. `cargo build --release`：`cupper-host`（内嵌清单）、`cupper`、`cupper-desktop-native`。
4. `cargo xtask host-bundle`：组装并签名 `CupperHost.app`；单独签名 `cupper`。
5. `pnpm build` 后运行 electron-builder（`electron-builder.release.yml`）：Vite 产物进 asar → `extraFiles` 放入宿主和 CLI → 烧录 fuses → 签名 → 公证 → staple → 生成 DMG 和 zip → 生成 `latest-mac.yml` 和 blockmap。
6. 验证：签名、`spctl`、entitlements 白名单、`@electron/fuses read`、asar 完整性；许可证门禁对最终的 `.app` 再查一遍（不得出现 `@anthropic-ai/claude-agent-sdk` 和 Claude Code 二进制）；`THIRD_PARTY_NOTICES.txt` 存在且包含 Node、codex-acp 及其内联依赖、esbuild-wasm、Electron 与前端依赖、Rust 依赖（cargo-about 生成）；运行 `cupper-host --self-test`（校验内嵌清单与随包组件一致，然后退出）。
7. 上传到 GitHub Release 的草稿，由人工发布。
