# T076 工作笔记

## 开工前的核对

- `v1.json` 的 `x-methods` 有 56 个请求、2 个客户端通知、6 个宿主通知。请求的 scope：`read` 19、`write` 20、`approve` 1、`ui_local` 3、`privileged` 7、`test` 3、`none` 3（`initialize`、`ping` 等）。白名单只放行 `read`、`write`、`approve`、`ui_local`（5.7.8），客户端通知的两个都是 `read`。
- T071 的合同（`packages/host-client/src/contract.ts`）要求：重复键的请求以重新序列化后的形式到达宿主；`initialize` 回 `forbidden/already_initialized`；批量数组、非对象、坏 JSON、没有 `id` 的 `initialize` 丢弃；`sendBlob` 是一个 `0x02` 帧；重连时旧数据、`reconnecting`、`ready`（带握手）依次到达；`close` 之后不再重连。
- 验收里"重新序列化后与原文不同的情形都被拒"与合同（重复键照样转发重新序列化的结果）有出入。我的理解：桥从不转发原始字节；解析后数值会变的情形（超出 ±(2^53−1) 的整数）直接拒绝，协议本来就不允许这样的整数；重复键按合同转发重新序列化的结果。
- 主进程的 `HostPort`（`socket-port.ts`）的 `ack` 是空操作；`Connected` 里有 `socket`，暂停和恢复读用它。
- 预加载脚本必须打包成一个只 require `electron` 的 CommonJS 文件；端口一侧的逻辑写成不依赖 Electron 的模块，合同测试在 Node 里用 `worker_threads` 的 `MessageChannel` 跑。

## 规模估计（开工前）

上限约 1,500 行（含测试，不含生成物和 lock 文件）。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `tools/gen-protocol-ts` | 从 `x-methods` 输出方法目录（scope、幂等）；生成物不计 | 35 | — |
| `main/bridge/window-bridge.ts` | 打开数据连接与代发 `initialize`、入方向按 16 ms 批量、出方向解析与白名单与重新序列化与本地拒绝、`sendBlob` 与上传限额、8 MB 流控、重连顺序（先等 control 就绪）、关闭、按窗口断连 | 300 | — |
| `main/bridge/whitelist.ts` | 按方法目录判定 | 25 | — |
| `main/bridge/ipc.ts` 与 `index.ts` 的接线 | `ipcMain` 处理器（`shell:init`、`host:open`、`prefs:set`、打开外链、选目录）、绑定文档实例（导航、`render-process-gone`、`closed`）、主题和焦点的推送、prefs 的广播、e2e 的断连钩子 | 200 | — |
| `preload/index.ts`、`preload/host.ts` | `contextBridge` 暴露完整的 ShellAPI；端口一侧的 `ShellAPI.host`（状态和数据同一条有序通道）；`flavor` 加 `local` | 200 | — |
| UNIT-bridge-whitelist | 遍历全部方法、畸形帧、批量、数值会变的消息、未知的端口消息类型 | — | 110 |
| UNIT-bridge-flow | 8 MB 暂停与恢复、批量 | — | 80 |
| UNIT-bridge-lifecycle | 绑定文档实例、重连顺序、按窗口断连 | — | 120 |
| UNIT-hostport-contract-bridge | 合同的 setup：Node 的 `MessageChannel` 适配、用 MockHost 的启动器 | — | 70 |
| 预测量脚本 | 判定负载（索引流加 4 条会话流、5 MB blob、5,000 事件重同步），任务目录下的物料 | 110 | — |
| 合计 | | 870 | 380 |

合计约 **1,250 行**。按前几个任务的偏差（T095 实际比估计多约 30%，T052 持平），实际可能在 1,250–1,450 行，贴着 1,500 的上限。按 brief，贴线也要开工前停下来汇报。

### 拆分建议

brief 建议"windowBridge 与合同测试"和"preload 的 ShellAPI 与预测量"分开。合同测试要走端口一侧的 `ShellAPI.host`（在 preload 里），所以这一块跟 windowBridge 放在一起：

| 部分 | 内容 | 估计 |
|---|---|---|
| A（T077、T078 依赖） | 方法目录、windowBridge、`host:open` 的接线与文档绑定、preload 里的 `ShellAPI.host` 和端口一侧、e2e 的断连钩子；UNIT-bridge-whitelist、UNIT-bridge-flow、UNIT-bridge-lifecycle、UNIT-hostport-contract-bridge | 约 900 |
| B | preload 的其余 ShellAPI（`init`、prefs、`shell:command`、`flavor` 的 `local`、主题、焦点、外链、选目录）与主进程接线；预测量 | 约 450 |

不拆的话按 A、B 的顺序提交。

## 编排者裁定（2026-10-20）

不拆，上限仍约 1,500 行，硬停止线 1,600；先 A 后 B 分步提交。上面 6 项取舍全部认可。预测量用任务目录下的一次性 vitest 配置运行，数字进本笔记，不进 CI。

## 进度统计

不含生成物 `methods.gen.ts` 和本笔记。

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| A1 | `gen-protocol-ts` 输出方法目录 | +45 −1 | 45 |
| A2 | `window-bridge.ts` 272、preload 的 `host.ts` 113、端口消息类型；测试：whitelist、flow、lifecycle（桥一级）243、合同 12、测试辅助 66 | +730 −0 | 775 |
| A3 | `bridges.ts`（`host:open` 与文档绑定）、control 的 `ready()`、`openWindow`、`index.ts` 接线、preload 暴露 `window.shell`、vite 的 preload 外置 `electron`、`SHELL_GLOBAL`；lifecycle（文档绑定）74 | +315 −15 | 1,090 |
| B1 | `shell.ts`（`init`、prefs、外链、选目录、主题、焦点）、preload 的其余 ShellAPI、`flavor` 加 `local` | +184 −6 | 1,274 |
| B2 | 预测量（任务目录下的配置和用例） | +115 | 1,389 |

合计约 +1,389 −24，在 1,500 行以内。A 部分比估计（约 900）多约 190 行，主要是测试辅助（Node `MessageChannel` 的两端适配、MockHost 前的数据连接）和 lifecycle 的文档绑定用例；B 部分比估计少（约 300 对 450），因为 B 的接线没有单独的用例（验收里没有对应的编号）。

## 实现里的取舍

- **preload 打包修正**：T074 的 vite 配置在 preload 环境里只写了 `resolve.builtins: ["electron"]`，客户端环境下它不生效，`import "electron"` 会把 npm 上那个查找 Electron 二进制的包打进 preload（T074 时 preload 是空模块，没暴露出来）。加上 `rolldownOptions.external: ["electron"]` 后，产物只 `require("electron")`。
- **渲染进程只有端口**：preload 用 `contextBridge` 只暴露 `window.shell`；它的 `host` 只能经 windowBridge 的 MessagePort 收发，端口上只认 `send`、`blob`、`ack`、`close` 四种消息，其余一律丢弃；`send` 的 JSON 解析、白名单、重新序列化之后才写到套接字，`blob` 由 `HostPort.sendBlob` 组帧（头部不合规、超过帧上限、超过待上传限额都不发）。
- **重连**：断开时先把批里的旧数据发出，再推 `reconnecting`；等 control 连接 `ready()`（control 失败或关闭时以它的原因推 `failed`），再经同一个 launcher 开新的数据连接；新连接的 `ready`（带握手）之后才转发它的数据。
- **文档绑定**：`host:open` 只接受窗口主框架、`app://cupper` 下的文档；同一个 webContents 再次 `open` 时先关掉旧的桥；主框架的跨文档导航、`render-process-gone`、`destroyed` 都关闭桥。
- **e2e 钩子**：e2e 构建在主进程的 `globalThis.__cupperE2E.disconnectWindow(webContentsId)`，供 Playwright 的 `electronApp.evaluate` 调用；release 和 dev 的产物里没有它（已用 vite 构建核对）。local 构建核对不了：vite 拒绝 `--mode local`（"local" 与 `.env.local` 的后缀冲突），这是 T074 留下的范围外问题，已在汇报里提出。
- **只回答主文档**：`shell:init`、`prefs:set`、选目录、外链都检查发送方是窗口主框架上的 `app://cupper` 文档。
- **占位**：`requestPrivileged` 返回 `{status: "failed", error: "unsupported"}`（T077）；`onNavigate`、`updates.onState` 只注册监听，`installNow` 什么都不做（深链接、更新器的任务）。

## 预测量

判定负载按 5.7.1：索引流每秒 5 帧、每帧约 1 KB；4 条会话流每秒各 25 帧、每帧约 2 KB；第 3 秒一个 5 MB 的 blob（82 个 64,000 字节的帧，一次性写出）；第 6 秒一次 5,000 个事件的重同步（50 帧、每帧 100 个事件）；共 10 秒。消费端在另一头切帧、解析 JSON、收到即确认。宿主是 TS 的 MockHost，桥和消费端用 Node 的 `MessageChannel` 相连。

**注意**：mock 宿主的编码、桥、消费端都在同一个 Node 线程里，所以延迟是桥本身的上限；没有原生扩展，socketpair 转发（默认缓冲区 8 KB、每条连接两个线程）不在这组数字里；也不是 Electron 的 `MessagePortMain` 和渲染进程。这些由 T078 的测量闸门在真实环境里测。

本机（Apple Silicon 笔记本，Node 24.19）连跑 3 次：

| 次 | 帧数 | 字节 | 5 MB blob 送达 | 5,000 事件重同步送达 | 事件循环延迟 p50 / p99 / max |
|---|---|---|---|---|---|
| 1 | 1,173 | 8.7 MB | 23.0 ms | 15.5 ms | 1.15 / 1.37 / 7.06 ms |
| 2 | 1,173 | 8.7 MB | 25.1 ms | 17.1 ms | 1.15 / 1.37 / 8.92 ms |
| 3 | 1,174 | 8.7 MB | 27.7 ms | 14.9 ms | 1.15 / 1.36 / 5.94 ms |

- p99 约 1.4 ms，低于判定线 5 ms；p50 的 1.15 ms 是 `resolution: 1` 的底数。
- 最大值 6–9 ms 出现在 blob 一次性写出的那一下（82 帧在同一个任务里编码和转发），没有发生流控暂停。
- 原生扩展的小缓冲区会把转发拆成很多小块，每块都触发一次 `data` 事件并进入 16 ms 的批；按这组数字看桥本身有余量，是否加 `setsockopt` 留给 T078 按真实测量定。
