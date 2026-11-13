# T255 e2e-repeat 一条用例都没跑时判通过

- 派发日期：2026-11-10
- 执行者：子 agent（CI；T151、T248、T253 的执行者）
- 分支：`ci/T255-e2e-repeat-empty`
- 依赖：无

## 范围

**做**（计划 T255 一行）：

1. **问题**：
   - `.github/workflows/e2e-repeat.yml` 分两组运行：不带 `@focus` 的 `rest`，和带 `@focus` 的一组。两组都带 `--pass-with-no-tests`，因为两组里本来可能有一组是空的。
   - `test` 输入有默认值 `E2E-SEC-real-input`。`spec` 换了、`test` 没改时，一条用例都不跑，结果仍判为成功。
   - 实例：T149 的 run 42764334381，`spec=src/composer.spec.ts` 而 `test` 用了默认值。
2. **修法**：
   - 两组运行之后，从 `repeat-rest.json`、`repeat-focus.json` 读出各自实际跑过的用例数。合计为 0 时作业失败，并说明是没有匹配到用例。
   - 每组跑了几条写进作业摘要。
   - 判定写在 workflow 里，或者放进 `tools/` 下的小脚本，你定；照 T247 给 test-repeat 加判定的写法。
3. **确认**（由 `main` 触发）：
   - 一组匹配不到用例的参数：应判失败；
   - 一组正常参数，例如 `spec=src/composer.spec.ts`、`test=E2E-slash-commands`、`times=2`：应判成功，摘要里有条数。

**不做**：
- 改 `test` 的默认值或其他输入；
- 改 test-repeat（T247 已修过它的判定）。

## 验收标准

- [ ] 两次确认运行的结果如上，汇报里写上运行的链接。
- [ ] workflow 的输入只经环境变量进 shell（现有写法），不新增注入面。
- [ ] PR 上 GitHub CI 的 8 个检查都通过。

## 规模

- 停止线：40；硬停止线：60。

## 参考资料

- 设计：5.1.6（CI 与手动 workflow）。
- 卷宗：T247（test-repeat 的判定）、T212（e2e-repeat 每次运行各用一个文件夹）。

## 注意

- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T052`，编排者已切到新分支 `ci/T255-e2e-repeat-empty`，从 main 开出（含本任务书）。T147 合并后你接着做 T252，到时另开分支。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
