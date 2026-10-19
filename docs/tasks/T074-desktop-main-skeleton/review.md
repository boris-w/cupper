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
