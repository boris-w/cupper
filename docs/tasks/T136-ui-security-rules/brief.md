# T136 ui-core I-b：安全规则

- 派发日期：2026-10-20
- 执行者：子 agent（前端工程师，安全）。T077、T134 的执行者接手
- 分支：`feat/T136-ui-security-rules`
- 依赖：T084（已合并）。与 T080（状态层）并行；T099（M1a 退出）依赖本任务

## 背景

T080 开工前拆成两份（计划 v1.3.28，见 `docs/tasks/T080-ui-state/brief.md` 末尾的补充裁定）：
- T080 做状态层与查询；
- 本任务做 Trusted Types 策略和 ESLint 安全、import、文案规则。

渲染进程、preload、`ui-core` 至今还没有这些规则（T074、T084 的交接）。规则文件不碰状态层：本任务只改 `eslint.config.js` 和 `ui-core/src/security/`，T080 不碰这两处。

## 范围

**做**（计划 T136 一行；设计 5.7.8 的"主文档的 CSP""Trusted Types""样式""代码规则"几行，5.1.7 的代码约束）：
1. **`packages/ui-core/src/security/policies.ts`**：
   - 唯一能创建 Trusted Types 策略的地方；
   - 只有一个具名策略 `cupper-worker`，它的 `createScriptURL` 只放行 `app://cupper/assets/` 下的同源脚本；
   - 不写 `'allow-duplicates'`；禁止名为 `default` 的策略。
   - 与主文档 CSP（`@cupper/shell-api` 的 `csp.ts`）里的 `trusted-types cupper-worker` 对应。
2. **ESLint 规则**（作用于渲染进程、preload、`ui-core`；T080 执行者核对过，内置的 `no-restricted-syntax`、`no-restricted-properties`、`no-restricted-imports`、`no-eval`、`no-new-func` 就够，不需要新插件）：
   - 禁止 `innerHTML`、`outerHTML`、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`document.write`、`eval`、`new Function`、`window.open`；
   - `ui-core` 禁止 import `electron`、`node:*`（以及 Node 内置模块的裸名）；
   - `trustedTypes.createPolicy` 只能出现在 `policies.ts`，且名字不能是 `default`；
   - 样式：禁止 `insertRule`、写 `<style>` 的文本、`style.setProperty('--…')` 一类写 CSS 自定义属性的调用。agent 字符串能否进入样式，静态检查做不全的部分写进 `notes.md`，由评审把关；
   - 文案：界面文案一律走 `@cupper/i18n`。例如禁止 JSX 里的裸字面量文本和 `aria-label`、`title` 等属性里的字面量，按需设白名单（标点、数字、产品名等），白名单写明理由。
3. **负面样例**：每条规则都有会触发它的样例，在 Vitest 里用 ESLint 的 Node API（`lintText`）跑，即 UNIT-ui-lint-rules。
4. **现有代码全部通过**：
   - 对 main 上已合并的 `ui-core`、渲染进程、preload 代码跑一遍；
   - 违反规则的地方逐条修正；属于例外的，用带理由的行内禁用注释；
   - 修正的范围只限于为满足规则所做的最小改动。

**不做**：状态层与查询（T080）；CSP 本身（已在 `@cupper/shell-api`）；审查文档（M1b）。

## 验收标准

- [ ] UNIT-ui-lint-rules：每条规则的负面样例都触发；正面样例不触发。
- [ ] `pnpm lint` 在整个仓库通过；例外逐条带理由。
- [ ] 截图回归（T118）照常通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T136 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T136-ui-security-rules/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T080 的行里与规则有关的部分）

- T074：渲染进程和 preload 的 ESLint 安全规则和 ui-core 一起加。
- T084：`ui-core` 禁止 import `electron`、`node:*` 的规则还没加。

## 注意

- 合并顺序：T080、T128 等并行的界面任务可能先合并。本任务合并前，由编排者把 main 合进来，规则要对合并后的代码全部通过。
- 不改 `CLAUDE.md`；规则与 `CLAUDE.md` 的代码约束、设计 5.1.7 一致即可。

## 规模

计划估为 S，T080 的执行者估约 290 行（实现约 160、测试约 100，另有修正现有代码的量）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 450 行（包括贴线）时，必须先停下来汇报**；硬停止线 520。

## 工作方式

T134 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T136-ui-security-rules main`。T134 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
