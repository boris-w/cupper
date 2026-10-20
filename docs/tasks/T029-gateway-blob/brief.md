# T029 网关 IV：blob 上传与下载

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T077、T134、T136、T135 的执行者接手
- 分支：`feat/T029-gateway-blob`
- 依赖：T033（已合并）。T090（e2e 基础设施）依赖本任务；本任务离剩余关键路径有 2 个单位的松弛

## 背景

网关的分发（T033）、订阅与历史（T019）都已就位，`Target::Blobs` 还空着（T019 的交接）。宿主那边的 `BlobStore` 已经有 `put`，T116 的录制导出也已在用。

但客户端至今不能经网关上传或下载 blob：
- T116 的 IT 只能直接读数据目录里的 blob 文件；
- 输入框的长文本和附件（T086）、录制导出的读取都要用到 blob 上传与下载。

## 范围

**做**（计划 T029 一行；设计 5.3.1、5.3.4 的 blob 部分、5.3.9）：
1. **`0x02` 分片的接收与缓冲上限**：上传缓冲放进每条连接的 `Dispatch`，断开时自动丢弃（T033 的交接）；超过上限按协议报错，不再增长。
2. **`blob/put`**：核对声明的 `size` 与哈希。`Mismatch` → `conflict`，`TooLarge` → `too_large`。成功后返回 `BlobRef`。
3. **`blob/get`**：按 `offset`、`length` 截取，经 outbox 的 `push_frame` 分片下发（T019 的交接：blob 分片仍走 `push_frame`）。不存在的 blob 回 `not_found`。
4. **`HOST_FEATURES`**：按 T033 的交接，补上 blob 相关的特性。
5. **host-client**：如果 TS 端还没有上传、下载的辅助函数，补上最小可用的一组（例如 `HostClient.sendBlob`，T071 的交接）。是否放在本任务做，取决于工作量；放不下就在汇报里提出，留给 T086、T145。

**不做**：图片按 agent 的 10 MiB 限制（T043 已做）；输入框里的长文本转 blob（T086）。

## 验收标准

- [ ] **IT-gw-blob**（真实宿主）：
  - 上传后再下载，内容一致；`offset`、`length` 截取正确；
  - `size` 或哈希不符回 `conflict`；超过上限回 `too_large`；
  - 上传中途断开，缓冲被丢弃（例如之后用同一个 ID 上传不受影响）；
  - 不存在的 blob 回 `not_found`。
- [ ] 新增的 IT 连跑 10 次全绿。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T029 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T029-gateway-blob/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T029 的行）

- T033："给 T019、T029 的接入点"（`docs/tasks/T033-gateway-dispatch/notes.md`）；`blob/*` 是 `Target::Blobs`；上传缓冲放进 `Dispatch`；`HOST_FEATURES`。
- T019：outbox 的项是 `Item`，blob 分片仍走 `push_frame`；`Target::Blobs` 由本任务接上。
- T071：`HostClient` 还没有 `sendBlob`。
- T116：录制导出的 blob 要能经 `blob/get` 取到（T064 会用）。

## 注意

- 帧大小：单帧不超过 64,000 字节（ADR-04）；分片按这个上限切。
- 代码约束：时间只经 `Clock`；错误用 thiserror；每条事件流只有一个写者（blob 不是事件流，但写库仍经写线程）。

## 规模

S，计划估为 S。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 450 行（包括贴线）就先停下汇报**；硬停止线 520。

## 工作方式

T135 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T029-gateway-blob main`。T135 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
