# T225 汇报

- 执行者：子 agent（桌面主进程工程师；T160 的执行者接手）
- 分支：`fix/T225-quit-during-reconnect`（PR #47：https://github.com/boris-w/cupper/pull/47 ，提交 e44ceb2）
- 完成日期：2026-11-03

**结论**：完成。原因已查清，在本机能确定地复现。修复和回归用例都在 runner 上验证过：修前对照抓到了卡住，修后连跑 150 次全过，PR 的 8 个检查全过。

## 做了什么

1. **原因**（证据见下文"验证方式"和 `notes.md`）：
   - SIGTERM 和菜单的 ⌘Q 走的是 Electron 原生的 `Browser::Quit()`，它执行 `is_quitting_ = HandleBeforeQuit()`：先派发 `before-quit`，等事件返回后才写 `is_quitting_`。
   - 控制连接不在 ready 时（正在重连，或已经失败），QuitGuard 的 `host/status` 立即以"连接丢失"结束，退出判定在同一次派发的微任务里就完成了。于是 `app.quit()` 嵌套在外层那次 `Browser::Quit()` 里执行：内层把 `is_quitting_` 设为 true 并开始关窗，外层随后用"被 preventDefault"的结果把它写回 false。
   - 窗口关完后，Electron 以为没在退出，只发 `window-all-closed`，不发 `will-quit`。我们的 `window-all-closed` 处理什么都不做（macOS 上关掉最后一个窗口时 App 照常保留），App 就留了下来，gpu-process 和 network service 也都还在。
   - 控制连接 ready 时，判定要等宿主应答，`app.quit()` 落到后面的回合里，不会嵌套。
   - **换包不是原因**，只是让重连变慢或失败。不换包、杀掉宿主 2 ms 后退出，同样卡住。T160 的三次运行里，SIGTERM 时控制连接都不在 ready。
   - 从 JS 里调用 `app.quit()`（现有 e2e 用例用的 `app.evaluate`）时，栈上已有 JS，微任务不会在派发中途运行，所以 E2E-quit-confirm 碰不到这个问题。
2. **修复**（`apps/desktop/src/main`）：
   - `index.ts`：
     - 判定为 quit 后，用 `setImmediate` 在下一个回合再调 `app.quit()`；
     - main.log 记 `before-quit`（带 `settled`）、`will-quit`、`quit` 三行；
     - 一旦 App 进入退出，`second-instance` 和 `activate` 不再建窗口。
   - `host/control.ts`：判定退出之前已经在进行的重连，如果在 `end()` 之后才完成，就关掉它，不再接上。修前在本机看到过：判定之后 main.log 还出现了一次 `host connection ready`。
   - `host/launcher.ts`：拨号途中被关闭时，不再接着拉起宿主。
   - `HostClient.attach()` 对已经 `close()` 的客户端不改状态：main 上已经是这样，没有改。
3. **回归用例 E2E-quit-during-reconnect**（`tools/e2e/src/quit.spec.ts`）：
   - 用 e2e 构建，Playwright 启动 App，退出用 SIGTERM。
   - 涉及换包的情形，用每个用例自己根目录下的 App 副本；fixture 的 `launch` 为此加了 `bundle` 参数，`paths.ts` 加了 `IN_APP`。
   - 五例：
     - (a) 正在重连：副本里的宿主换成先 `sleep 5` 再 exec 真宿主的脚本，杀掉宿主，等到 `reconnecting` 再退出；
     - (a) 重连已失败：删掉副本里的宿主，杀掉宿主，等到 `failed` 再退出；
     - (b) 杀掉宿主、包没换，等重连上之后退出；
     - (c) 换包（宿主重签，cdhash 不同），宿主还在运行时退出；
     - (d) 换包后用 CLI 连一次，旧宿主因待升级退出后立刻退出。
   - 每例断言四件事：
     - 10 s 内退出；
     - main.log 依次是 `before-quit`、`before-quit`、`will-quit`、`quit`；
     - 判定退出之后没有 `ready`；
     - 10 s 内本用例的数据目录上没有宿主进程。
   - 单测 UNIT-launcher 加了两例：退出判定之后才完成的连接被关掉、不被接上；拨号途中关闭不拉起宿主。

## 关键取舍

- **用例放在 e2e 作业里，不放进 selfuse**：根因与包的签名和 hardened runtime 无关，e2e 构建就能确定地复现，而且用的是临时数据目录。这样每晚随 e2e 全集运行，也能用现成的 e2e-repeat 连跑。没有打 @smoke 标签，所以不进 PR 的冒烟子集。
- **修在调用 `app.quit()` 的地方**：问题出在"判定在 `before-quit` 派发里同步完成"。改用 `setImmediate` 延后是最小的改动，QuitGuard 的判定规则不变。

## 验证方式

- **本机确定性复现**（e2e 构建，单份跑，没有启动 Cupper Local）：
  - 修前：杀宿主后 2 ms 退出卡住 2/2；换包并杀宿主后 50 ms 退出卡住 1/1。其余情形（不动宿主；只换同一个包；杀宿主后等重连上再退出）都在 1.5 s 内退出。
  - 加临时日志对比：正常时是窗口 closed 之后 `will-quit`、`quit`；卡住时是窗口 closed 之后 `window-all-closed`，没有 `will-quit`。
  - 加上 `setImmediate` 后，同样的三次都在 0.5 s 内退出。
- **本机用例**：
  - 修前的构建上，(a) 两例和 (d) 卡住；
  - 修后 5 例全过，`app.spec.ts` 的 E2E-single-instance 和 4 例 E2E-quit-confirm 也全过；
  - apps/desktop 的 vitest、tsc、eslint、prettier 都通过。
- **runner 修前对照**：e2e-repeat，times=3，https://github.com/boris-w/cupper/actions/runs/41599945199 。用的是临时分支 `test/T225-before` 上的 24fc3f2，即 main 1f972d2 加上用例、不含修复，分支已删。从构件 `e2e-repeat` 核实，失败原因与预期一致：
  - (a) 正在重连、(a) 重连已失败、(d)：各 3/3 失败，原因都是 `the app quit within 10 s`，即卡住。
  - (b)、(c)：各 3/3 失败，但都是在第 58 行日志断言上失败：修前的 main.log 没有 `before-quit`、`will-quit`、`quit` 这几行，收到的是空数组。能走到这一行，说明 App 已经退出，与卡住无关。
- **runner 修后连跑**：e2e-repeat，times=30，workers=1，https://github.com/boris-w/cupper/actions/runs/41599662900 。5 例各 30 次，共 150/150 通过。
- **PR CI**：https://github.com/boris-w/cupper/actions/runs/41599672590 ，8 个检查全部通过。
- **规模**：`cargo xtask size` 为 +199 −15，停止线 300、硬停止线 380 以内。其中 apps/desktop +52（含单测），tools/e2e +147。

## 提交列表

```
e44ceb2 docs(task): record how T225's hang was reproduced and why it happened
92e527d test(e2e): E2E-quit-during-reconnect, SIGTERM in four states of the host
5808185 fix(desktop): take no connection and start no host once quitting is decided
ebf7ace fix(desktop): quit even when the decision comes within before-quit
```

## 遗留问题 / 范围外发现

1. **这不只是升级场景的问题**：用户按 ⌘Q 时，只要控制连接恰好不在 ready（宿主刚崩、正在重启，或重连失败停在 failed），修前 App 都会卡住；控制连接停在 failed 时更是每次必现。所以 T225 合并前，开发构建、e2e 构建也都受影响。
2. **T160 的 E2E-selfuse-replace 现在可以重跑了**：T225 合并后，请把 T160 分支（本机在 f1973f2 / c8bbe18）推上去，并触发 `nightly job=selfuse`。T160 的用例原本就是在等旧界面重连出结果之后才退出；有了这个修复，即使不等也不会卡住。要不要去掉那段等待，由你决定，我倾向保留，不过它的注释要改成引用"已修"。
3. **退出判定之前就已拉起的宿主**：在退出判定之前就开始的重连所拉起的宿主，连接会被关掉，但进程本身仍要靠宿主自己的规则退出：开发构建、自用构建按"孤立"规则，约 1 s 后退出；发布构建没有这条规则，要等 10 分钟空闲退出（T214 已记录）。没有在本任务处理。
4. **`second-instance` 不再建窗口没有专门的用例**：时机很难构造，修后退出在 0.5 s 内完成。只在代码里用 `quitting` 标记守住，没有加测试。
5. **5.7.2 / 5.5 可以补一句**：退出判定可能在 `before-quit` 的派发里同步完成，`app.quit()` 必须延到下一个回合，并注明原因。建议编排者补进设计。
