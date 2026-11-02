# T212 CI 上 e2e 失败时留下附件

- 派发日期：2026-11-02
- 执行者：子 agent（工程师）
- 分支：`ci/T212-e2e-ci-attachments`
- 依赖：无（T208 发现）。T099 依赖本任务：退出验收时 e2e 有失败，要能拿到证据

## 背景

`cargo xtask ci e2e` 分两轮跑：先跑 `test-e2e`（不需要焦点的用例，并行），再跑 `test-e2e-focus`（带 `@focus` 的用例，单 worker）。两轮共用 Playwright 的输出目录 `tools/e2e/dist/test-results`，Playwright 每轮开始时会清空它，所以第一轮失败用例的附件（main.log、host.log、截图、trace、`error-context.md`）被第二轮删掉了。T208 查的两次 CI 失败都只剩 JSON 报告，第二处因此一直没有证据。

## 范围

**做**：
1. 两轮各用自己的输出目录（例如 `dist/test-results/rest` 和 `dist/test-results/focus`），由 xtask 传给 Playwright。
2. workflow 失败时把两轮的附件都作为构件上传，路径照新的目录改。`e2e-repeat` 也照此核对。
3. 本机 `--record` 的摘要照常记下附件的位置，路径跟着改。
4. 验证：在分支上造一个必然失败的临时用例，非 focus、focus 各一个，推到 GitHub 上看两轮的附件都留在构件里；验证完 revert，最终 diff 里不留。要推送时告诉 `main`。

**不做**：改 e2e 用例本身；改 Playwright 的报告格式。

## 验收标准

- [ ] 两轮的附件都留在构件里，附临时失败用例那次运行的链接。
- [ ] 最终 diff 里没有临时失败用例。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：60；硬停止线：90

## 参考资料

- 设计：5.1.6；5.9.4（e2e）。
- 卷宗：T208 的 `notes.md` 和中期汇报（发现经过）；T189、T195（`@focus` 的两轮）；T197（workflow 的构件上传）。

## 注意

- 本机 e2e 只单份跑；带 `@focus` 的用例不在本机跑。
- `gh` 只用只读命令；要推送时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
