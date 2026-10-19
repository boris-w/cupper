# T074 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并
- 核对：
  - 在任务 worktree 里重跑 `cargo xtask ci pr`，结果通过：ts 64 项，UNIT-main-hardening 为 pass；
  - diff 中命中的路径都是测试夹具里的假路径（`/Users/u`）；
  - `~/Library/Preferences` 下没有遗留文件；
  - 规模：代码、测试、配置约 1,380 行，在上限以内。
- 认可的地方：
  - 在真实的 e2e 构建产物上逐项实测了加固，包括 Trusted Types、路径穿越、导航、权限、开关；
  - 只在 `index.ts` 里 import `electron`，单元测试不需要 Electron。
- 接受的取舍：
  - `allowBuilds` 不放行 electron（Electron 44 没有安装脚本）；
  - e2e 的目录布局为 `<root>/data`、`<root>/logs`；
  - 菜单顶层用 Electron 的角色。
- 转交（记入 `docs/tasks/handoff-notes.md`）：交给 T075、T076、T080、T090、T152、T153、T154、T160、T161、T180 的事项。
- 设计更新（编排者）：5.1.5 中"electron 的下载脚本需要放行"的说法已过时，已改正。

## 修复第 1 轮：测试的临时目录（2026-10-19，编排者）

- 结论：通过，合并 `fix/T074-test-tmpdir-cleanup`。
- 背景：编排者在排查 T021 修复时，发现用户临时目录里有大量 `cupper-app-protocol-*`。
- 执行者查明的范围比任务书写的大：每跑一次桌面端测试会留下 7 个目录。
  - `hardening.test.ts` 留 1 个；
  - `log.test.ts` 留 2 个；
  - `prefs.test.ts` 留 4 个。
  - 修复前，三类目录分别累积到 52、92、184 个。
- 修法：
  - 整组共用的目录，用 `afterAll` 删除；
  - 辅助函数每次调用建的目录，用 `onTestFinished` 删除，用例失败时也会删。
  - 测试语义没有改。
- 核对：
  - 用一个专用的空 `TMPDIR` 验证：修复前每次运行留下 7 个目录，修复后 0 个；三个文件里各故意弄坏一个用例，临时目录同样不留。
  - Rust 测试的临时目录都由 `Drop` 清理，或者放在 `CARGO_TARGET_TMPDIR` 下，不需要改。
  - CI 摘要为 `ci/pr-b7a041eeed34.json`：rust 530、ts 64、license 1，全部通过。
- 合并后由编排者清理已有的残留，清单见执行者汇报的"遗留问题"。

