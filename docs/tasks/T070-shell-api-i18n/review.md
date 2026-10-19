# T070 Review

## 第 1 轮（2026-10-19，编排者）

- 结论：通过，已合并
- 核对：
  - 编排者在任务 worktree 里重跑 `cargo xtask ci pr`，结果通过：rust 16 项、ts 26 项；
  - diff 里没有双向控制符和零宽字符；
  - 个人信息为 0；
  - 规模约 1,120 行。
- 接受的取舍：
  - TS 包直接导出源码，类型检查走 project references（第一个 TS 包定下的惯例）；
  - Vitest 只收 `src/**/*.test.ts`；
  - `UpdateState` 的取值；
  - 拆出 `KnownNavigateTarget` 和守卫，以绕开 TS 开放枚举的收窄问题；
  - M1a 先做 zh-CN。
- 转交（记入 `docs/tasks/handoff-notes.md`）：
  - `pathForFile`、`saveFile` 由对应任务以附加方式加入；
  - `lightningcss`（MPL-2.0）只是开发依赖，交 T006 按生产依赖闭包判断；
  - ts 作业的 `test` 脚本约定；
  - 文件写入工具会把 `\uXXXX` 解码成真实字符，写不可见字符的测试时要用脚本生成，并扫描确认。
- 编排者修正：设计 07-desktop.md 里示例中混入的真实 U+202E 字符，改写为 `\u{202E}`。
