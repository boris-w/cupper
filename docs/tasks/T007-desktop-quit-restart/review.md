# T007 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：由 T075 拆出（计划 v1.3.17），另加 T075 交回后编排者定下的两处改动（启动等待、白名单只留一份）。合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-ca6bdef696f4.json`；这次运行在 T098 合并之前，两者改的模块不重叠。执行者那次的 `pr-35d720bf4cc8.json` 一并保留。
- 核对：没有新依赖；新的 `unsafe` 都在 `cupper-sys`（`pid_path`、`pid_cdhash`），带 `SAFETY:` 注释；只向自己拉起的宿主发信号，打包后的 e2e App 只用临时目录，偏好设置域已清空；没有个人信息。
- 审阅：
  - **"本构建的宿主"的判定**：路径（realpath）和 cdhash 必须同时相符才发 SIGTERM；路径相同但 cdhash 不同（重新构建过）也不发；两次读取之间 pid 被复用只会导致"不发"，不会误杀。符合安全 #9。
  - **启动等待**：500 ms 用完、自己拉起的宿主还活着时继续等，最长 10 s；退出码 3 后仍按 500 ms。把 bundle 复制到新位置实测冷启动约 1.9 s，靠等待成功，T075 加的预热已去掉。
  - **白名单只留一份**：`vite.config.ts` 构建期读 `runtime/manifest.json` 的 `env.allow`，经 `define` 交给主进程，执行者核对过构建产物里带着这份名单。
  - **`lifecycle`**：挂在 `before-quit`；查不到 `host/status` 时直接退出；确认后发 `host/exit`（`force: true`）；同时两次退出只弹一个框；关机和更新跳过确认。
  - **空闲时退出 App 不发 `host/exit`**：执行者请编排者确认。编排者认可：按设计宿主独立于界面存活、按空闲规则自行退出；App 退出时发 `host/exit` 会断掉同时连着的 CLI。
- 规模：新增 569 行、删除 87 行（测试约 210），在约 800 的汇报线以内；逐模块统计。
- 转交事项已写进 `handoff-notes.md`：T090（E2E-quit-confirm 的触发方式）、更新流程任务（确认 `before-quit-for-update` 真的经 `autoUpdater` 发出）、界面任务（"需要重启宿主"页面改走 ShellAPI 触发 `restart`）。
