# T168 请求面板与用户气泡的全文

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T165 的执行者接着做
- 分支：`feat/T168-blob-fulltext`
- 依赖：T164、T165（合并中）。T099 依赖本任务

## 背景

T165 的"展开看全文"放不下，拆成本任务（计划 v1.3.37）。现在请求面板里标了 `content_truncated` 的内容只能拒绝：`blob` 类内容的 `text()` 返回 `undefined`、不显示，允许类选项一直不可选。用户气泡里的长文本块（T164 经 blob 发出的）也只显示"长文本"（T082 的交接）。两处都缺 host-client 的 `getBlob`。

## 范围

**做**（计划 T168 一行；设计 5.3.6 的"要用户批准的内容不截断"、5.7.4 的请求面板与用户消息、5.3.4 的 blob）：
1. **host-client 的 `getBlob`**（T029 的交接）：
   - 调 `blob/get`，用 `onBlobChunk` 按 `offset` 收集 `0x02` 分片，收齐结果里的 `chunks` 数为止。分片一定先于结果到达；空范围也有一个"最后一片"；
   - 与 T164 的 `putBlob` 对称，放在同一处；
   - 收到的内容按 `BlobRef` 的哈希核对，不符就当失败。
2. **请求面板的全文**（安全 #6）：
   - 标了 `content_truncated` 的内容（命令、路径列表、详情，T083 的 `Terminal` 等）展开时按 `BlobRef` 下载全文，经 `escapeInvisible` 转义后显示；
   - **全文显示出来之后**，`Body` 才改传 `truncated={false}`，允许类选项才可选。展开且全文载入完成就算看过，不要求滚到底；
   - 下载失败、哈希不符、收起后再展开之前，一律保持"只能拒绝"，并显示可以重试的失败状态；
   - 请求结束（被别处回答、过期）时丢掉进行中的下载。
3. **用户气泡里的长文本块**：
   - 按需下载（例如点"显示更多"时），下载之前显示预览和大小；
   - 显示照设计 5.7.4 的用户消息：长消息按行数折叠、"显示更多""收起"切换；文字是纯文本，不当 Markdown。

**不做**：工具输出、diff 里的 blob（T145 等）；录制导出（T064）。

## 验收标准

- [ ] UNIT-blob-fulltext：
  - `getBlob` 的分片收集：乱序到达、空范围、分片数不足时报错、哈希不符时报错；
  - 请求面板：全文载入前允许类选项不可选，载入后可选；失败时仍不可选、可重试；不可见字符显示为转义；
  - 用户气泡的长文本块：点开前不下载，点开后显示全文。
- [ ] 改了截图里能看到的东西时，补或更新对应的 UI 用例基线，并逐张看过。
- [ ] UI-csp-zero 为 0；ESLint 规则通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T168 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T168-blob-fulltext/ci/` 通过，摘要随任务提交。

## 交接事项

- T165 的汇报"遗留问题"：`request/RequestPanel.tsx` 的 `Body`、`request/logic.ts` 的 `isEnabled(option, truncated)`、`request/kinds.tsx` 的 `Terminal`。
- T029：blob 分片的约定（`handoff-notes.md` 里"第一个在界面里读 blob 的任务"一行）。
- T164：`putBlob` 的位置和写法。
- T082：用户气泡（`rows/basic.tsx`）；长文本块现在只显示"长文本"。
- T070：测试里需要不可见字符时用 `String.fromCodePoint` 生成。

## 规模

S，估约 250 行（`getBlob` 约 50、请求面板约 40、用户气泡约 40、用例约 120）。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 350 行（包括贴线）就先停下汇报**；硬停止线 400。

## 工作方式

T165 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T168-blob-fulltext main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
