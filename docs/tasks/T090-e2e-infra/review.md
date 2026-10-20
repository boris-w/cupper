# T090 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 963 行、删除 15 行（不含 lock 和文档），在收窄后的上限约 1,000 以内。开工前估约 1,545，执行者在写代码前停下，按方案一拆出 T167（计划 v1.3.36）。写到 1,066 行时执行者自己删掉了还没有用例用到的 CLI `repl`、`watch`、`cli` 身份和 `showMessageBoxSync` 替换，留给 T167，做法符合"没人用的代码不进 main"。
- **CI**：执行者的摘要 `pr-3135501eea51.json` 通过，e2e 作业第一次进了 `ci pr`（`@smoke` 4 条）。合并 main（含 T138、T082、T083 等）只在 `i18n.ts` 的 JSON 导入上冲突：main 新增了 `request` 命名空间，本分支给导入加了 `with { type: "json" }`，编排者取并集并给新导入也加上属性。合并后重跑 `ci pr --record` 通过：`pr-b84ca730dea6.json`（rust 1876、protocol 185、ts 451、license、testdata-scan、ui 21、e2e 4）。第一次重跑的 protocol 失败是编排者在 CI 运行中往工作区写了评审文件，`pnpm gen` 之后的"工作区未变"检查因此失败，与代码无关。之后又合入了 T166（只动 ui-core 的消息行，与本分支不重叠），在合并结果上补跑 `ci ts` 和 `ci e2e`（全集）通过。
- **验收**：E2E-single-instance、E2E-quit-confirm（三条）在每晚的全集；连跑 10 次全绿、无残留进程，执行者修了清理路径后又连跑了 10 次。

## 逐项意见

1. **生产代码没有测试分支**：`apps/desktop` 没有改动。主进程里的确认框替换经 `app.evaluate` 注入，依赖的是 e2e 构建的 `enableNodeCliInspectArguments: true`（`electron-builder.e2e.yml`，T074）。按设计 5.7.8 的 Fuses 一行，发布构建要关掉它，并由 CI 用 `@electron/fuses read` 断言；发布构建的配置还没建，到时这条断言就是这扇门的保障。
2. **确认框挂起等用例回答**：比预先排好答案灵活，"先取消、再确认"就是这样测出来的；确认框从不真的弹出、不抢焦点。主进程在调用时才取 `dialog.showMessageBox`（代码约束），所以替换能拿到每一次调用。
3. **进程守卫**：进程按 pid 加启动时间识别，宿主还要求命令行正好是本用例的 `--data-dir`；被杀的运行由下一次运行按属主 worker 清掉，含子孙进程。只发信号给本用例记下且仍是同一个进程的 pid，符合 T008 修复 1 的做法。
4. **e2e CLI**：用 `host-bundle --kind e2e` 构建、宿主旁边的那个，以 `test_harness` 身份经 fd 3 传 key，不落在命令行和环境里。
5. **`ci e2e` 的产物摘要**：文件记 sha-256、符号链接记指向且不跟随，按路径排序；`JobReport` 加了 `artifacts`。机器级锁与 `ui` 作业共用。
6. **临时 git 仓库**：固定作者信息，不读本机 git 配置，没有个人信息进入产物。
7. **范围外的小改动**（JSON 导入属性）：理由成立，Vite、Vitest、`tsc` 和 Node 的 ESM 加载器都接受。合并后 main 的新导入已统一。

## 记下的点

- **交给 T167**：CLI 包装的 `repl`、`watch`、`cli` 身份连接；`showMessageBoxSync` 的替换随用到它的辅助一起加。
- **交给 T093**：添加项目时的 `dialog.showOpenDialog` 没有替换。
- **磁盘**：`ci e2e` 有意保留 `target/host-bundle`（约 3.7 GB）等构建产物，供下次复用；清理 worktree 的 `target/` 时一并清掉。
- **E2E 编号只在每晚跑**：`ci pr` 只跑一条夹具自检，PR 上看不到 E2E-quit-confirm 的回归，要靠 `ci nightly`。
