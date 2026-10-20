# T128 工作笔记

## 开工前的核对

- `docs/tasks/handoff-notes.md` 里第一列含 T128 的行：没有。T084 的 notes 里与本任务有关的：只有 `tokens.css` 里定义过的 token 能生成工具类；颜色是 `light-dark()`。
- `tests/ids.toml` 的 T128 段已有 UNIT-ui-components、UNIT-ui-row-registry。
- 规格：ui-spec 2.1（工具栏按钮：高 28、左右 8、`--radius-lg`、tertiary 色、悬停 `--color-hover`、打开状态 `--color-btn-secondary`、tooltip 延迟出现并带快捷键）、3.4（菜单外边距 4，条目 5 × 8）、3.5（菜单与条目 `--radius-xl`，tooltip `--radius-2xl`）、3.6（`--shadow-menu`）、4.3（警告框：`--radius-lg`，1px 警告色 30% 的边框，内边距 12 × 8，最大宽 77%）、4.7（开关 32 × 19，滑块 13，开启时强调色）。
- 行类型：设计 5.7.5 的消息流行（回合标题、用户消息、活动组、工具行、助手正文、思考、计划、提示、错误、迟到的工具输出、请求记录、回合改动），具体组件由 T082、T083 等注册。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 依赖与测试环境 | Radix 三个包、jsdom 与 Testing Library；Vitest 收 `.tsx` 用例；jsdom 缺的 `ResizeObserver` 等 | 20 | — |
| 基础组件 | 工具栏按钮（含 tooltip）、菜单、tooltip、开关、警告框、两个自绘图标 | 160 | — |
| 替换顶栏占位 | 五个按钮改用工具栏按钮，"打开"和"⋯"挂空菜单；`App` 加 tooltip 的 Provider | 35 | — |
| 行类型注册表 | 注册、查找、重复注册报错、未知类型的占位 | 45 | — |
| 文案与导出 | i18n 键、`index.ts` | 15 | — |
| 测试 | UNIT-ui-components、UNIT-ui-row-registry | — | 165 |
| 合计 | | 275 | 165 |

合计约 **440 行**。按以往约 15% 的偏差推算约 500，贴近停止线：写得从紧，每个模块完成后统计，推算到 500 就停下汇报。
