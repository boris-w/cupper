# T076 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,250 行（可能到 1,450），贴近上限，执行者停下汇报；编排者裁定不拆、按 A、B 两部分顺序提交，6 项取舍全部认可。最终 +1,385 −18 行。
  - 合并 main 时与刚合并的 T007 在 `apps/desktop/src/main/host/index.ts`、`main/index.ts` 上有实质冲突，编排者保留两边：函数名用本分支的 `startHost`，返回 `control`、`openWindow`、`restart`；白名单副本按 T007 去掉（构建期从运行时清单读）；`main/index.ts` 合并了 T007 的退出确认、重启钩子和本分支的 window bridges。`tsc --build` 通过。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-54becbca2af5.json`。执行者那次的 `pr-83592eb2524c.json` 一并保留。
- 核对：没有新的外部依赖；渲染进程只拿到 `window.shell`，通往宿主的唯一路径是 windowBridge 的端口（只认 `send`、`blob`、`ack`、`close`）；e2e 的 `__cupperE2E` 只在 e2e 构建的主进程里；没有个人信息。
- 审阅：
  - **出方向从不转发原始字节**：解析、白名单、重新序列化后再发；白名单由协议生成的方法目录得出，只放行 `read`、`write`、`approve`、`ui_local`。
  - **端口一侧不依赖 Electron**：合同测试和桥的测试能在 Node 里用 `worker_threads` 的 `MessageChannel` 跑，走同一段生产代码。好的设计。
  - **顺带修了 T074 的 vite 配置**：preload 原来会把 npm 上的 `electron` 包打进产物，现在设为外部依赖。
  - **预测量**（mock 宿主、同一个 Node 线程）：事件循环延迟 p99 约 1.4 ms，5 MB blob 约 25 ms，5,000 事件重同步约 16 ms，没有触发流控暂停。局限写明了（不含 socketpair 和 Electron 的 `MessagePortMain`），留给 T078 实测。
- 规模：+1,385 −18 行（不含生成物、notes、CI 摘要），在约 1,500 的上限以内；逐模块统计。
- 转交事项已写进 `handoff-notes.md`：T077（真实输入检查的位置、`requestPrivileged`）、T078（预测量脚本与断连钩子）、自用构建的 vite 模式名冲突（交给 T160）、深链接与更新器的任务（`onNavigate`、`updates` 的通道）。
