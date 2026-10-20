# T076 桌面主进程 III：windowBridge 与 preload

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T052、T053、T095 的执行者接手
- 分支：`feat/T076-desktop-window-bridge`
- 依赖：T075（已合并）。T077（真实输入与特权）、T078（测量闸门）依赖本任务；离剩余关键路径有 2 个单位的松弛

## 背景

桌面主进程已经能拉起宿主并建立 control 连接（T075）。每个窗口还需要一条数据连接：渲染进程经 preload 暴露的 ShellAPI 收发，主进程的 `windowBridge` 在中间做白名单、流控和重连（设计 5.7.1、5.7.3、5.7.8）。渲染进程从不直接碰宿主的套接字。

## 范围

**做**（计划 T076 一行；设计 5.7.1、5.7.3、5.7.6 末尾"T071 开工前的裁定"、5.7.8 的命令白名单）：
1. **每窗口 `windowBridge`**：
   - 数据连接绑定文档实例；用 T075 的同一个 launcher 打开（`open({role: "window", features: …})`），代发 `initialize`；
   - 入方向：字节块批量转给渲染进程；
   - 出方向：解析、按白名单检查、重新序列化后再发给宿主；白名单按协议常量生成——生成的 `@cupper/protocol` 目前没有方法目录（方法名 → scope、幂等），扩展 `tools/gen-protocol-ts` 从 `v1.json` 的 `x-methods` 输出它（交接，T071）；
   - `sendBlob`；
   - 8 MB 流控（`SocketPort.ack` 现在是空操作，按窗口的流控由本任务做，交接 T075）；
   - 重连顺序与本地拒绝按"T071 开工前的裁定"；状态和数据走同一条有序通道；
   - 按窗口断连的钩子（供 e2e）。
2. **preload 的 ShellAPI 实现**：preload 目前是空模块；prefs 还没接到 ShellAPI（`init` 的快照、`set`、`onChange`）；菜单命令走 `shell:command` 通道（`apps/desktop/src/shared/ipc.ts`）；ShellInfo 的 `flavor` 没有自用构建的 `local`——冻结前以附加方式加上，或映射为 release，写明选了哪种（交接，T074）。
3. **合同测试**：对 windowBridge 运行 `hostPortContract`（setup 提供 `shell`、`host`（一个 `MockHost`）和 `settle`；请求用例用 `host/status`；握手结果来自 host-client 的 `handshake()`，去掉 `host_proof`）（交接，T071）。
4. **预测量**：用 TS mock 宿主按判定负载（索引流加 4 条会话流、5 MB blob、5,000 事件重同步）做一次预测量，数字写进 `notes.md`，供 T078 的测量闸门参考。原生扩展经 socketpair 转发、默认缓冲区只有 8 KB，转发会分成很多小块；数字不理想时记下来，是否加 `setsockopt` 由 T078 定（交接，T073）。

**不做**：真实输入证据与特权命令（T077）；测量闸门本身（T078）；界面（ui-core）。

## 验收标准

- [ ] 新增 UNIT-bridge-whitelist（不在白名单里的方法、畸形帧、重新序列化后与原文不同的情形都被拒）、UNIT-bridge-flow（8 MB 流控）、UNIT-bridge-lifecycle（绑定文档实例、重连顺序、按窗口断连）、UNIT-hostport-contract-bridge（与 mock 用同一套合同）。
- [ ] 渲染进程不能绕过 windowBridge 直接发出任何帧。
- [ ] 代码约束：`ui-core` 不 import `electron`、`node:*`；渲染进程不用 `innerHTML` 等；主进程在调用时才取 `dialog.showMessageBox` 这类函数。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T076 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T076-desktop-window-bridge/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`，第一列含 T076 的行）

T074（prefs、preload、`shell:command`、`flavor`）、T071（方法目录、本地拒绝和重连顺序、合同测试的 setup）、T073（socketpair 的缓冲区）、T075（同一个 launcher、`SocketPort.ack`）。

## 注意

- 用例里的数据目录都用临时目录；不碰真实的 `~/Library` 目录。
- 不运行真实的 Claude Code、Codex CLI。
- 依赖锁精确版本，许可证在 ADR-11 的 allowlist 内。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超线（包括贴线）就开工前停下汇报，并给出拆分建议**（例如"windowBridge 与合同测试"和"preload 的 ShellAPI 与预测量"分开）。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`07-desktop.md` 5.7.1、5.7.3、5.7.6（"T071 开工前的裁定"）、5.7.8；`03-protocol-v1.md`（方法的 scope、幂等）；`09-testing.md`（测量闸门的判定负载）。
- `docs/tasks/T071-host-client-conn/`、`T073-desktop-native/`、`T074-desktop-main-skeleton/`、`T075-desktop-host-launch/`。
- `apps/desktop/`、`packages/host-client/`、`packages/shell-api/`、`tools/gen-protocol-ts/`。

## 工作方式

在原来的 T052 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T076-desktop-window-bridge main`。T095 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,250 行（按偏差可能到 1,450），贴近上限，停下汇报。

**裁定：不拆，上限仍是约 1,500 行，硬停止线 1,600。** 估计在上限之内，而且 A 部分本来就要先做。按执行者提的顺序分步提交：
- **A**（T077、T078 依赖）：方法目录、windowBridge、`host:open` 的接线与文档绑定、preload 里的 `ShellAPI.host`、e2e 断连钩子，四个 UNIT 编号的用例；
- **B**：preload 的其余 ShellAPI（`init`、prefs、`shell:command`、`flavor`、主题、焦点、外链、选目录）及其主进程接线；预测量。
每完成一个模块统计一次；推算超过 1,500 行时停下汇报。

执行者的取舍全部认可：
1. **"重新序列化后与原文不同的被拒"按 T071 的合同解释**：桥从不转发原始字节；超出 ±(2^53−1) 的整数直接拒绝（解析后数值会变，协议本来就不允许）；重复键按合同处理，转发重新序列化的结果。验收标准里的这一句以此为准。
2. **白名单**：放行 scope 为 `read`、`write`、`approve`、`ui_local` 的方法和两个客户端通知；本地拒绝 `privileged`、`test`、scope 为 `none` 的方法和未知方法；带 `id` 的请求被拒时回 `forbidden`（`initialize` 回 `forbidden/already_initialized`），没有 `id` 的消息直接丢弃；`interaction/respond`、`git/revert` 的真实输入检查归 T077。
3. **重连顺序**：先把旧连接已收到的数据发完再推 `reconnecting`；等 control 连接就绪再开新的数据连接；新连接 `ready` 之后才转发新数据；状态和数据走同一个 MessagePort。
4. **上传限额**：按每个窗口统计还没有对应 `blob/put` 的 `0x02` 字节数，超过握手结果里的 `max_pending_upload_bytes` 就拒绝。
5. **`flavor`**：以附加方式加上 `local`。
6. **还没有实现方的成员**：`requestPrivileged` 先返回 `{status: "failed"}`，T077 接上；`updates`、`onNavigate` 先只注册监听，等更新器和深链接的任务接上。已写进交接。

预测量写成任务目录下的用例文件，用一次性的 vitest 配置运行，数字写进 `notes.md`，不进 CI：认可。
