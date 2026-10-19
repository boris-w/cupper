# T070 shell-api（与协议无关的部分）与 i18n

- 派发日期：2026-10-19
- 执行者：子 agent（TypeScript 工程师，桌面端）
- 分支：`feat/T070-shell-api-i18n`
- 依赖：T001（已合并）。M1a 第 1 波，是桌面链的起点。

## 背景

界面代码 `@cupper/ui-core` 不能 import Electron，外壳能力一律通过 `ShellAPI` 调用（设计 5.7.3）。ShellAPI 另有一个浏览器 mock，供截图测试使用。本任务实现 ShellAPI 里与协议无关的部分，以及 `@cupper/i18n`。与协议类型相关的部分（`host.*`、`HostPort`、`requestPrivileged`）要等 `packages/protocol` 生成之后，由 T071 完成。

这是仓库里第一个 TS 包。

## 范围

**做**（计划 T070 一行，设计 5.7.3）：
- `packages/shell-api`（`@cupper/shell-api`）：
  - 接口：`init(): Promise<ShellInfo>` 与 `ShellInfo`、`onCommand`（命令 ID 是开放枚举）、`onNavigate`、`openExternal`、`pickDirectory`、`system`、`window`、`prefs`、`updates`；
  - 浏览器 mock 实现。
- `packages/i18n`（`@cupper/i18n`）：
  - 基于 i18next，以语义码为键；
  - 文案按功能分文件（`locales/<语言>/<功能>.json`），先只做一种语言（M1a）；
  - 不可见字符的转义工具：把双向控制符、零宽字符、其他控制字符显示成可见的转义。渲染进程和原生确认框共用这个工具（设计 5.7.8）。

**不做**：`host.*`、`HostPort`、`requestPrivileged` 及其类型（T071）；界面组件（T080 起）。

## 验收标准

- [ ] 新增 UNIT-shell-mock：浏览器 mock 实现了全部成员；`onCommand` 遇到未知的命令 ID 时忽略，不报错。
- [ ] 新增 UNIT-i18n-escape：双向控制符（例如 U+202E）、零宽字符、控制字符都被转义成可见文本；普通的中英文原样保留。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T070-shell-api-i18n/ci/` 通过，其中 ts 作业要真正跑到 tsc 和测试。摘要随任务一起提交。
- [ ] 测试的标题或名称以用例编号开头（计划 1.6）。

## T001 留下的两点（要在本任务处理）

1. `tsc --build` 会把测试文件编译进 `dist/`，而 Vitest 默认不排除 `dist`，同一个用例会跑两遍。解决办法二选一：在 Vitest 配置里排除 `dist`，或者让测试文件不进 tsc 的产出。
2. Vitest 目前整步只算一项，没有逐用例的编号结果。请用 JSON reporter 把逐用例结果接入 `xtask ci` 的摘要：xtask 里有解析测试输出、按编号归类的逻辑（`crates/xtask/src/ci/ctx.rs`），这里要补上 Vitest 的解析。

## 注意

- 遵守 `CLAUDE.md` 的 TypeScript 约束：`ui-core` 不 import `electron` 或 `node:*`，界面文案一律走 `@cupper/i18n`。
- 遵守 pnpm 的约定：工作区成员用通配，新包不改根清单；依赖脚本白名单保持为空，确实需要时在汇报里说明。
- 规模上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`07-desktop.md` 5.7.3、5.7.7（本地化）、5.7.8（不可见字符）；`01-repo-build-ci.md` 5.1.2、5.1.3。
- `docs/tasks/T001-repo-skeleton/report.md`：遗留问题第 5 条；xtask 的结构。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
