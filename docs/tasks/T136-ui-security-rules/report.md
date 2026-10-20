# T136 汇报

- 执行者：子 agent（工程师；T077、T134 的执行者接手）
- 分支：`feat/T136-ui-security-rules`
- 完成日期：2026-10-20

## 做了什么

`policies.ts` 和 ESLint 规则都已加上，main 上已合并的代码不用改就全部通过，`cargo xtask ci pr` 含截图回归全部通过。规模新增 346 行、删除 2 行，在约 450 行的线内。

**1. `packages/ui-core/src/security/policies.ts`**
- 全仓库只有这里创建 Trusted Types 策略，而且只建一个，名叫 `cupper-worker`。
- 它的 `createScriptURL` 只放行 `app://cupper/assets/` 下的脚本：解析后协议必须是 `app:`、主机 `cupper`、路径以 `/assets/` 开头，`..` 跳出去的会被拒绝。
- 没有 `default` 策略，也没写 `'allow-duplicates'`，与主文档 CSP 里的 `trusted-types cupper-worker` 对应。
- 对外是 `workerScriptUrl(url)`：第一次调用时建策略；没有 Trusted Types 的环境下，对普通字符串做同样的检查。

**2. `eslint.config.js` 的规则**（只用内置规则；`no-restricted-syntax` 的选择器按文件组拼好再给出，因为后一段配置会整体覆盖前一段）

| 组 | 作用范围 | 内容 |
|---|---|---|
| 安全 | ui-core、渲染进程入口、preload | `innerHTML`、`outerHTML`（包括写成 `el["innerHTML"]` 的）、`dangerouslySetInnerHTML`、`insertAdjacentHTML`、`document.write`/`writeln`、`window.open` 和全局的 `open`、`eval`、`new Function` |
| Trusted Types | 同上 | `createPolicy` 只能出现在 `policies.ts`，在那里第一个参数只能是常量 `WORKER_POLICY` |
| 样式 | 同上 | `insertRule`、`replaceSync`、`adoptedStyleSheets`；`<style>`（JSX 和 `createElement`）；`setProperty` 写 `--` 开头的属性名或非字面量属性名；`style` 对象里 `--` 开头的键和计算出来的键；`cssText`；`setAttribute('style', …)` |
| import | ui-core 的非测试文件 | 不能 import `electron`、`electron/*`、`node:*`，以及 Node 内置模块的裸名（取自 `builtinModules`） |
| 文案 | ui-core 的 `.tsx` 非测试文件 | JSX 文本；子节点里的字符串字面量和模板字面量；`aria-label`、`title`、`alt`、`placeholder`、`label` 等属性里的字面量 |

文案白名单和理由：
- 不含任何文字字母的文本放行，因为标点、数字、符号在各语言里写法相同；
- 产品名和 agent 名（Cupper、Claude、Codex）放行，因为是专有名词，不翻译。

**3. UNIT-ui-lint-rules**
- 规则的用例在 `ui-core/src/security/lint-rules.test.ts`：用 ESLint 的 Node API（`lintText`），按仓库根的配置去检查放在假想路径下的样例代码。
  - 每条规则都有触发它的负面样例，并断言只触发这一条；
  - 也有不该触发的正面样例：`textContent`、`setProperty('width')`、`style={{ width }}`、`t()` 取的文案、纯标点和数字、产品名、`<svg width="16">`，以及 preload 里 import `electron`。
- 策略的用例在 `policies.test.ts`：放行与拒绝哪些地址、只建 `cupper-worker`。
- 为跑规则用例，`ui-core` 的 devDependencies 加了 `eslint`，版本与根目录相同。

## 关键取舍

- **没有加 `no-implied-eval`**：这条内置规则要先知道 `setTimeout` 是全局变量，而配置没有声明浏览器全局变量，所以样例触发不了。另外，主文档的 CSP 没有 `'unsafe-eval'`，字符串形式的定时器在运行时本来就会被拦下。任务书列出的 `eval`、`new Function` 都已覆盖。
- **哪些规则管测试文件**：import 规则不管 ui-core 的测试，因为测试在 Node 里跑；安全和样式规则照样管测试文件，负面样例写成字符串，不会误报。
- **静态检查拦不下的样式写法**写在 `notes.md`，交评审把关：
  - `style` 的**值**；
  - 拼进 `className` 的 agent 文本；
  - 从别处取到的 `<style>` 元素；
  - `const w = window` 这类别名；
  - 自定义组件用别的属性名接收文案。

## 验证方式

- `pnpm lint` 用到的 `eslint --max-warnings=0 .` 和 `prettier --check .` 在整个仓库通过，例外一条也没有。
- `tsc --build packages/ui-core` 通过：用例里没有用 Node 的 API，所以 ui-core 不必加 `@types/node`。
- `cargo xtask ci pr --record docs/tasks/T136-ui-security-rules/ci/` 通过，摘要 `pr-975c09c63061.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1795 通过 |
  | protocol | 185 通过 |
  | ts | 394 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui（T118 截图回归） | 7 通过 |

- 规模：新增 346 行（`eslint.config.js` 133、`policies.ts` 47、用例 165、`package.json` 1），删除 2 行，不含 `pnpm-lock.yaml`。

## 提交列表

```
fecacff docs(task): record the T136 pr CI summary
975c09c feat(lint): security, import, style and text rules for the main document
94875a5 feat(ui-core): the one Trusted Types policy, cupper-worker
a09e212 docs(task): estimate T136
```

## 遗留问题 / 范围外发现

1. **合并 T080、T081 之后**：您合并 main 进来后，需要再跑一次 `eslint .`。状态层和侧边栏如果有 JSX 裸文案或属性字面量，会被文案规则拦下，按规则改成 `t()`；专有名词以外的例外要带理由的行内禁用注释。
2. **`workerScriptUrl` 还没有调用方**：ui-core 目前不建 Worker。哪个任务第一次用 `new Worker`，就该经它取 URL。
3. 分支基于 ad92a3b，没有同步 main。