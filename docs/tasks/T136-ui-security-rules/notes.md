# T136 工作笔记

- 执行者：子 agent（工程师；T077、T134 的执行者接手）

## 开工前核对现状

- `eslint.config.js` 只有基础规则，加上 host-client 禁止 import `node:*`、`electron` 的一段；渲染进程、preload、`ui-core` 还没有安全规则。
- 现有的 `ui-core`（约 900 行）、渲染进程入口、preload 用 grep 查过：没有 `innerHTML`、`insertRule`、`setProperty`、`createElement`、`trustedTypes`。界面文案已经走 `t()`。JSX 裸字面量和属性字面量要等规则跑过才知道。
- 扁平配置里，同一条规则在几段配置里出现时，后一段整体覆盖前一段的选项。所以 `no-restricted-syntax` 的选择器要在 `eslint.config.js` 里按文件组拼好，再一次给出。
- UNIT-ui-lint-rules 放在 `ui-core/src/security/`，用 ESLint 的 Node API 的 `lintText`，按仓库根的配置检查假想路径下的样例代码。`ui-core` 要在 devDependencies 里加上 `eslint`（与根同一版本）。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| `policies.ts` | 唯一的 `cupper-worker` 策略，`createScriptURL` 只放行 `app://cupper/assets/` | 35 |
| `eslint.config.js` | 安全、import、`createPolicy`、样式、文案五组规则，按文件组拼选择器；白名单与理由 | 100 |
| 修正现有代码 | 按规则跑出来的结果 | 10 |
| **实现小计** | | **145**，乘 1.3 约 **190** |
| UNIT-ui-lint-rules | 每条规则的负面样例和正面样例 | 130 |
| 策略的用例 | 放行与拒绝的地址、`default` 不存在 | 35 |
| **合计** | | 约 **310–355** |

在约 450 行的线内。
