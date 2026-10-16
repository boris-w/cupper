# D01 工作笔记

- 执行者：子 agent（架构师）
- 分支：`docs/D01-detailed-design`
- 日期：2026-10-16

## 1. 外部事实核对

所有请求都在 2026-10-16 发出，User-Agent 为 `cupper-research`（npm 安装时用 `--user-agent cupper-research` 和空的 userconfig），不带任何认证和个人信息。

| # | 来源 | 结论 | 用在哪里 |
|---|---|---|---|
| E1 | `registry.npmjs.org/electron` 的 dist-tags；`releases.electronjs.org/releases.json` | 最新稳定版 44.8.0（2026-10-16），43.8.0，42.11.15；45.0.0-beta.5。各大版本首个稳定版：42.0.0（2026-05-05）、43.0.0（2026-06-30）、44.0.0（2026-08-24）。44 自带 Node 24.22.0 | 5.10.6 Electron 版本 |
| E2 | `registry.npmjs.org/electron-builder`、`electron-updater` | electron-builder：`latest` 26.15.3，`v26` 26.17.0（2026-09-26），`next` 27.0.0-alpha.10。electron-updater：`latest` 6.8.9，`v26` 6.8.10，`next` 7.0.0-alpha.9 | 5.1.4、5.10.4 |
| E3 | `registry.npmjs.org/@electron-forge/cli` | 8.0.0 于 2026-09-29 正式发布，`latest` 为 8.0.1；7.11.2（2026-05-20） | 5.10.4 |
| E4 | `registry.npmjs.org/@electron/fuses` | 2.1.3 | — |
| E5 | electron-builder 仓库 tag `electron-updater@6.8.10` 的 `packages/electron-updater/src/MacUpdater.ts` | 下载完成后，只有 `autoInstallOnAppQuit` 为真时才调用 `nativeUpdater.checkForUpdates()` 把更新交给 Squirrel.Mac；为假时直到 `quitAndInstall()` 才交（源码注释："If this is not triggered before quitAndInstall then the downloaded update will be installed only after the next app quit"）。交给 Squirrel 时在 `127.0.0.1` 的随机端口上开一个临时代理 | 5.10.4 的决定性依据；5.8.5 进程卫生 |
| E6 | 同仓库 master 的 `AppUpdater.ts`、`MacUpdater.ts` | 7.x 用 `autoInstallEvent`（`onQuit`、`onNextLaunch`、`manual`）取代 `autoInstallOnAppQuit`；把旧属性设为 false 会映射成 `manual` 并给出弃用警告 | 5.10.5 |
| E7 | 同仓库 tag `electron-builder@26.15.3` 的 `packages/app-builder-lib/scheme.json` | 顶层有 `electronFuses`（`FuseOptionsV1`：`runAsNode`、`enableCookieEncryption`、`enableNodeOptionsEnvironmentVariable`、`enableNodeCliInspectArguments`、`enableEmbeddedAsarIntegrityValidation`、`onlyLoadAppFromAsar`、`loadBrowserProcessSpecificV8Snapshot`、`grantFileProtocolExtraPrivileges`、`resetAdHocDarwinSignature`）；`MacConfiguration` 有 `binaries`、`signIgnore`、`entitlements`、`entitlementsInherit`、`notarize`、`hardenedRuntime`、`extraFiles` | 5.7.8、5.10.2、5.10.4 |
| E8 | GitHub 搜索 `cornerMask repo:electron/electron` | #48311 "Electron-based apps cause a huge system-wide lag on macOS 26"（2025-09-26 关闭）；修复 #48376（2025-09-26）及回移 #48398–#48401（2025-09-27）："stop overriding private cornerMask API to fix WindowServer GPU load" | 5.10.6 |

此外，用 mermaid 11.4.1 加 jsdom 25.0.1（装在会话临时目录，不在仓库里）对 `docs/design/` 下全部 mermaid 图做了语法解析，全部通过；对一个故意写错的图做了反向验证，能报错。

## 2. 设计中的主要取舍

1. **桌面主进程到宿主的连接**：ADR-03 写的是"主进程是唯一连接，所有窗口共用"。但主进程按 ADR-03 不解析入方向的 JSON，就没法把事件分给不同窗口，也没法认出自己发出的特权请求的响应。改为"主进程持有全部连接：一条自用的 control 连接 + 每个窗口一条数据连接"。安全意图（套接字和凭据不进渲染进程）不变。记为待定问题 Q1。
2. **命令的"受理"语义与幂等**：SP-4a 在命令执行完后单独保存结果，宿主在两者之间崩溃会导致重试时重复执行。改为命令结果和它产生的第一批事件同一事务落盘；改动用户仓库的 git 命令先写 `pending_external`。principal 改用稳定 ID，UI 重连后重试同一命令仍能命中。
3. **会话命令的顺序**：SP-4a 每个请求一个 task，同一连接先后发出的 `send` 和 `interrupt` 可能乱序。改为由连接的读循环直接按顺序投递到会话 actor。
4. **历史的存放**：ADR-06 要求日志设保留窗口、历史以宿主日志为准。两者同时成立需要一个"压实"后的历史：每个回合结束时写回合快照（`turn_snapshots`），日志只保留断线续传所需的部分，`history/page` 从回合快照读取。
5. **会话 actor 不 await 长操作**：SP-4a 的 actor 会 await 适配器启动，期间处理不了中断。改为另起 task、用完成消息驱动状态机。
6. **Codex 的 `threadStatus`**：SP-1 s05 显示计划确认请求发出时线程状态已是 `idle`。如果像 ADR-06 写的那样"优先采用 threadStatus"界定回合，会提前结束还在等确认的回合。细化为：有 prompt 在途时以 prompt 响应为准，`threadStatus` 只用于显示运行状态和界定自发回合。记为待定问题 Q11。
7. **线路分接层**：原始帧要保存（ADR-06），SDK 的封闭枚举要容错（ADR-02），扩展消息（`_auth/status_update`、`$/cancel_request`）SDK 不一定处理。三件事都放在 stdio 和 SDK 之间的一层里，避免分叉 SDK。
8. **选项生效范围**：从 SP-1 的录制里抽出全部 `optionId`，发现 Claude 的 `allow-with-updates` 在 Write 上只对本会话有效、在 Bash 上会写入持久规则，两者的 ID 相同，必须按工具区分。
9. **打包工具**：决定 electron-builder，依据 E5。详见 5.10.4。
10. **codex-acp 随包**：ADR-08 没写明它是随包还是首次运行下载，ADR-11 允许随包分发。选随包，Codex 不需要联网安装，也少一类失败。记为待定问题 Q4。
