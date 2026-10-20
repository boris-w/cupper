# T164 输入框的长文本

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T029 的执行者接手（blob 上传与下载是它写的）
- 分支：`feat/T164-composer-long-text`
- 依赖：T029、T086（合并中）。T099 依赖本任务

## 背景

- T086 做完了输入框的其余部分（计划 v1.3.33），超过帧预算的长文本拆给本任务；
- T029 实现了宿主端的 `blob/put`、`blob/get` 和 CLI 的分片收发，但 host-client（TS）的 `putBlob` 还没有（T029 的遗留第 1 条）。

## 范围

**做**（计划 T164 一行；设计 5.7.4 的输入框、5.3.4 的 blob）：
1. **host-client 的 `putBlob`**（T029 给出的形状）：
   - 用 `crypto.subtle.digest` 算 sha-256；
   - 按 `MAX_BLOB_CHUNK_BYTES` 切片，经已有的 `HostPort.sendBlob` 发出；
   - 再请求 `blob/put`；错误按语义码给出（`conflict`、`too_large`）。
2. **输入框的长文本**：
   - 超过帧预算的文本先经 `putBlob` 上传，再以 `text_blob` 发送；
   - 上传失败时草稿保留，经 T081 的 Toaster 给出反馈；
   - 接入点见 T086 的汇报（输入框在哪里判断"超过帧预算"、发送时在哪里换成 `text_blob`）。
3. `getBlob` 不做，留给第一个在界面里读 blob 的任务。

**不做**：图片附件（M1b）；`getBlob`。

## 验收标准

- [ ] UNIT-composer-long-text：超过帧预算的文本走上传和 `text_blob`；没超过的照常走文本；上传失败时草稿保留并有反馈。
- [ ] `putBlob` 有单元测试（经 host-client 的 mock 宿主，覆盖切片、哈希、错误码）。
- [ ] UI-csp-zero 为 0；ESLint 规则通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T164 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T164-composer-long-text/ci/` 通过，摘要随任务提交。

## 交接事项

- T029：分片的上限、`blob/put` 的错误码；宿主端按 `blob/put` 的 `write` scope 检查分片。
- T086：输入框的接入点（见它的汇报和 `notes.md`）。
- T071：`HostClient` 还没有 `sendBlob`（`HostPort.sendBlob` 已有）。

## 注意

- 渲染进程里算哈希用 Web Crypto，不引入新依赖。
- 单帧不超过 64,000 字节（ADR-04）。

## 规模

S，T086 和 T029 的执行者合估约 200 行。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 300 行（包括贴线）就先停下汇报**。

## 工作方式

T029、T086 都合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T164-composer-long-text main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
