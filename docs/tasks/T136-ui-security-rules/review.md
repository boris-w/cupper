# T136 评审

- 评审人：编排者
- 日期：2026-10-20
- 结论：**通过，合并**

## 核对

- **规模**：新增 346 行、删除 2 行（不含 lock），在约 450 的线内。
- **CI**：执行者的摘要 `pr-975c09c63061.json` 通过。合并 main（含 T080 的状态层、T128 的组件、T133）时，`ui-core/package.json` 的 devDependencies 和 lockfile 冲突，编排者取两边并集、重新解析 lockfile；合并后的代码不改就通过全部规则。之后在本 worktree 重跑 `cargo xtask ci pr --record`，`pr-d5e2f36a454b.json` 通过（rust 1821、protocol 185、ts 410、license、testdata-scan、ui 7）。

## 逐项意见

1. **`policies.ts`**：
   - 全仓库只在这里建 Trusted Types 策略，只有 `cupper-worker`；`createScriptURL` 解析 URL 后核对协议、主机、路径前缀，`..` 跳出被拒；
   - 没有 `default`，也没有 `'allow-duplicates'`，与主文档 CSP 对应；
   - 对外只暴露 `workerScriptUrl(url)`。
2. **ESLint 规则**：只用内置规则，分安全、Trusted Types、样式、import、文案五组，覆盖了任务书列的全部 API，还多拦了 `el["innerHTML"]`、`replaceSync`、`adoptedStyleSheets`、`cssText`、`setAttribute('style', …)`、计算出来的 style 键。`no-restricted-syntax` 按文件组拼好再给出，避免后一段配置覆盖前一段，这个坑处理对了。
3. **UNIT-ui-lint-rules**：每条规则都有只触发它自己的负面样例，也有不该触发的正面样例；用仓库根的配置按假想路径检查，测的是真实生效的配置。
4. **文案白名单**：不含文字字母的文本，以及产品名、agent 名。理由成立。
5. **没加 `no-implied-eval`**：配置没声明浏览器全局变量，这条规则触发不了；主文档 CSP 没有 `'unsafe-eval'`，字符串形式的定时器在运行时本来就会被拦下。认可。
6. **静态检查拦不下的写法**写进了 `notes.md`，由评审把关：`style` 的值、拼进 `className` 的 agent 文本、从别处取到的 `<style>` 元素、`window` 的别名、自定义组件用别的属性名接收文案。以后评审界面任务时照这张清单看。

## 交接

- **第一个用 `new Worker` 的任务**（审查文档的 Worker、Shiki 等）：Worker 的脚本 URL 必须经 `workerScriptUrl` 取。
- **所有界面任务**：规则已对合并后的代码生效，JSX 文案和属性字面量走 `t()`；专有名词以外的例外要带理由的行内禁用注释。
