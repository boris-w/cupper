# T118 工作笔记

## 开工前的核对

- `docs/tasks/handoff-notes.md` 里第一列含 T118 的行：没有。T084 的 notes 和评审里与本任务有关的：主题经 `data-theme` 固定；zod 的 Trusted Types 违规在本任务修。
- `tests/ids.toml` 的 T118 段已有 UI-shell、UI-shell-narrow、UI-csp-zero、UNIT-ui-baseline-meta。
- xtask：`ui` 作业已经登记为持锁作业（`locked("ui", None)`），`pr` 组合里也有它；还没有按路径过滤的机制，也没有 Playwright 报告的解析。
- `MAIN_DOCUMENT_CSP` 在 `apps/desktop/src/main/security/app-protocol.ts`。截图设施若放在独立的包里，直接引用桌面端的源文件会让 TS 工程互相引用，所以把它挪到 `@cupper/shell-api`（两边都已依赖）。
- `@cupper/protocol` 的 `src/index.ts` 是手写的（生成器只写 `gen/`），可以在那里设置 zod。

## 规模估计（开工前）

按格式化后的写法估。字体、字符表、截图基线这类夹具不计。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| CSP 挪到 shell-api | `csp.ts`、导出、桌面端改为引用 | 25 | — |
| zod jitless | `@cupper/protocol` 入口设置，附理由 | 5 | — |
| 包骨架 | `tools/ui-screenshots` 的 `package.json`、`tsconfig`、Vitest、Playwright、截图页的 Vite 配置 | 85 | — |
| 截图页 | `index.html`、`main.ts`（browser-mock + mock 宿主 + 夹具）、字体的 `@font-face` | 70 | — |
| 运行设施 | 构建截图页、带 CSP 提供页面、违规计数、基线元数据的读取比对与写入 | 100 | — |
| 截图用例 | UI-shell（亮、暗）、UI-shell-narrow（浮层、自动收起与恢复）、UI-csp-zero | — | 90 |
| 单元测试 | UNIT-ui-baseline-meta、字体子集覆盖文案的检查 | — | 55 |
| xtask | `ui` 作业、Playwright JSON 报告解析、`ci pr` 的路径过滤 | 110 | 60 |
| 其他 | 根 `package.json` 的脚本等 | 10 | — |
| 合计 | | 405 | 205 |

合计约 **610 行**。实现常比估计多 15% 左右，推算约 700，离 750 不远：每个模块完成后统计，推算到 750 就停下汇报。
