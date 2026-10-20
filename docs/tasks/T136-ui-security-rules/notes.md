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

## 实现中定下的几点

- **作用范围**：
  - 安全、样式、`createPolicy` 三组规则作用于 `ui-core`、渲染进程入口、preload，测试文件也在内（负面样例写成字符串，不会触发）；
  - 禁止 import 的规则只作用于 `ui-core` 的非测试文件：测试在 Node 里跑，preload 本来就要 import `electron`；
  - 文案规则只作用于 `ui-core` 的 `.tsx` 非测试文件：渲染进程入口和 preload 没有 JSX。
- **Node 内置模块的裸名**：取自 `node:module` 的 `builtinModules`，跟着 Node 版本自动更新。
- **`createPolicy`**：
  - `policies.ts` 以外一律禁止；
  - `policies.ts` 里只允许第一个参数是常量 `WORKER_POLICY`。它的值是 `cupper-worker`，由用例核对，写 `'default'` 或任何其他名字都会触发。
- **没有加 `no-implied-eval`**：
  - 内置规则要先知道 `setTimeout` 是全局变量，而配置里没有声明浏览器全局变量，样例触发不了；
  - 主文档的 CSP 没有 `'unsafe-eval'`，字符串形式的定时器在运行时本来就会被拦下。
- **文案白名单**：
  - 不含字母（任何文字的字母，`\p{L}`）的文本放行：标点、数字、符号在各语言里写法相同；
  - 产品名和 agent 名（Cupper、Claude、Codex）放行：专有名词，不翻译。
  - 受检的属性：`aria-label`、`aria-description`、`aria-roledescription`、`aria-placeholder`、`aria-valuetext`、`title`、`alt`、`placeholder`、`label`；
  - 子节点里的字符串字面量和模板字面量同样受检。
- **现有代码**：main 上已合并的 `ui-core`、渲染进程入口、preload 全部通过，没有需要修正或例外的地方。

## 静态检查做不全的部分（交评审把关）

agent 的字符串不能进入样式（设计 5.7.8"样式"一行）。规则拦下的写法：
- `<style>` 元素（JSX 和 `createElement('style')`）；
- `insertRule`、`replaceSync`、`adoptedStyleSheets`；
- `setProperty` 的第一个参数是 `--` 开头的字面量，或不是字面量；
- `style` 对象里 `--` 开头的键和计算出来的键；
- `cssText`；
- `setAttribute('style', …)`。

拦不下的，要靠评审：
1. **样式值**：`style={{ width: x }}` 里的 `x`，或拼进 `var(...)`、`url(...)` 的字符串。规则只看键不看值，样式值必须来自计算（数字、布局结果），不能来自 agent 文本。
2. **class 名**：`className` 里拼 agent 文本不会生成新的 CSS（Tailwind 在构建时生成），但可能套上意料之外的现成 class。class 名只能来自固定的映射，例如 ANSI 颜色映射到固定的 class。
3. **间接引用**：别处取到的 `<style>` 元素（例如 `querySelector('style')`）再写它的 `textContent`，规则看不出元素的类型。
4. **别名**：`const w = window; w.open(...)` 这类别名，`no-restricted-properties` 看不出来。
5. **自定义组件的属性**：文案规则只认上面那几个属性名。自定义组件用别的属性名接收文案（例如 `heading="..."`）时查不到，组件的属性要么接受 `t()` 的结果，要么在评审时看。

## 实际规模

| 模块 | 新增 / 删除 |
|---|---|
| `policies.ts` | 47 / 0 |
| `eslint.config.js` | 约 135 / 2 |
| `ui-core` 的 `package.json`（加 `eslint`） | 1 / 0 |
| UNIT-ui-lint-rules（规则样例约 125，策略约 40） | 约 165 / 0 |
| **合计**（不含 `pnpm-lock.yaml`） | **346 / 2** |
