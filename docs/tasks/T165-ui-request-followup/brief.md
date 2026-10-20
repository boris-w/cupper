# T165 ui-core V-b：拒绝并附消息、请求记录行与多请求

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师）。T083 的执行者接着做
- 分支：`feat/T165-ui-request-followup`
- 依赖：T083（合并中）；请求记录行要登记进 T082 建的共用注册表实例（T082 合并中）。T091、T093 依赖本任务

## 背景

T083 开工前拆出了三块（计划 v1.3.34）：RP-12"拒绝并附一条消息"、请求记录行、多请求的"1 / 3"。T083 的汇报里写好了这三块的接入点。T082 建了共用的行注册表实例：`rows/stream.ts` 的 `streamRows`，组件用 `registerRow(kind, Component)` 登记；请求条目在拍平时留作 `request` 行。

## 范围

**做**（计划 T165 一行；设计 5.7.4 的请求面板、5.7.8 的 RP-12）：
1. **RP-12"拒绝并附一条消息"**：
   - 自由输入行加在 `OptionsKind` 的选项列表之后，编号为"选项数 + 1"，数字键聚焦它；
   - 先发 `interaction/respond`（不持久的拒绝选项），**收到成功之后**才发跟进消息，带 `reply_to`（被拒绝的 `request_id`）；
   - 收到 `conflict/already_resolved` 或 `expired` 时不发，文字留在输入框里，提示"已在别处处理"；
   - 遵守"跟进消息默认排队"，M1a 没有 Steer，一律排队；
   - 消息流里这条用户消息标明"附于对 … 的拒绝"（用户气泡是 T082 做的，在它上面加标注）；
   - 自由输入行里刚打完字 1 秒内按的 Enter 不算真实输入：不提示"再按一次"，只在提交按钮旁提示"点击提交"（RP-5）。
2. **请求记录行**："已批准""已询问 n 个问题""未回答"，登记为 `request` 行。
3. **多请求的"1 / 3"**：`ComposerArea` 已算出待处理请求的列表，把长度和序号传给面板的标题行。
4. **"展开看全文"**（T083 的交接，放得下就做）：
   - host-client 加 `getBlob`：用 `onBlobChunk` 按偏移收集分片，收齐 `blob/get` 结果里的 `chunks` 数为止；
   - `Terminal` 展开时按 `BlobRef` 下载全文，经 `escapeInvisible` 转义后显示；
   - 全部内容都看过之后，`Body` 改传 `truncated={false}`，允许类选项才可选。
   - 估计约 140 行。放不下就在汇报里提出，留给后续任务；在那之前保持"只能拒绝"。

**不做**：插话（T140）；审查面板（M1b）。

## 验收标准

- [ ] UNIT-request-followup：
  - RP-12 的顺序（先拒绝成功、再发跟进）；
  - `already_resolved`、`expired` 时文字留下并提示；
  - `reply_to`；
  - 自由输入行的编号与数字键；
  - 刚打完字的 Enter 只提示"点击提交"；
  - "1 / 3"。
  - 做了"展开看全文"的话：`getBlob` 的分片收集，看完全文前允许类选项不可选。
- [ ] UI-request-records：请求记录行的三种状态；多请求的面板标题；两种主题。
- [ ] UI-csp-zero 为 0；ESLint 规则通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T165 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T165-ui-request-followup/ci/` 通过，摘要随任务提交。

## 交接事项

- T083 的汇报"遗留问题 / 给 T165 的接入点"：`OptionsKind`、`Body`、`isEnabled(option, truncated)`、`Terminal`、`pending(view)`。
- T082：`streamRows`、`registerRow`；`StreamRow` 的形状；用户气泡。
- T029、T164：blob 分片的约定（`getBlob` 与 `putBlob` 对称）。
- T070：测试里需要不可见字符时用 `String.fromCodePoint` 生成。

## 规模

S（不含"展开看全文"约 250 行，含则约 390）。**开工前先在 `notes.md` 里按模块写行数估计；推算超过约 450 行（包括贴线）就先停下汇报**；硬停止线 520。

## 工作方式

T083、T082 都合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T165-ui-request-followup main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
