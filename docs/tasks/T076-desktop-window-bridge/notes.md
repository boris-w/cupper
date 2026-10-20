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
