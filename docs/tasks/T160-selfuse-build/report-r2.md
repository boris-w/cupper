# T160 汇报（第 2 轮：runner 上第一次失败的排查与修复）

- 执行者：子 agent（构建与桌面工程师）
- 分支：`build/T160-selfuse-build`（PR #40：https://github.com/boris-w/cupper/pull/40 ）
- 远端已推送到 f3fff60；本地还多一个只改 notes 的提交 ba481bb，没有推送
- 日期：2026-11-03

## 当前状态（交回时）

- 第二次运行还没有结果，三个用例能否通过要看下面两个运行；要等它们跑完验收才算完成。
- **第一次** nightly（b767fd4，job=selfuse）：https://github.com/boris-w/cupper/actions/runs/41581457853 。四步构建全部成功（build 273 s）；用例 10 过 3 败。
- **第二次** nightly（f3fff60，job=selfuse）：https://github.com/boris-w/cupper/actions/runs/41586879127 ，交回时排队中。
- PR 的 ci：
  - b767fd4 上 8 项都通过：https://github.com/boris-w/cupper/actions/runs/41581412488
  - f3fff60 上的那次排队中：https://github.com/boris-w/cupper/actions/runs/41586882919

## 三处失败的原因、证据与处理

构件 `selfuse-macos-15-logs` 下载到本机一个新的空目录，内容只当数据读。

### 1. IT-selfuse-build 报"有测试入口"

- **现象**：asar 里查到 `__cupperE2E`、`CUPPER_E2E_DIR`、`CUPPER_E2E_BACKGROUND`。
- **原因**：这三个名字只出现在 source map 里，代码里没有。`dist/main/index.js.map` 的 sourcesContent 带着 `main/index.ts` 的源码，其中有 e2e 专用的几段，Vite 构建时已把它们从代码里去掉。
- **证据**：
  - 本机用 `vite build --mode self-use` 构建到临时目录后扫描：.js、.cjs、.html 都没有命中，只有 `main/index.js.map` 命中，命中的正是这三个名字。
  - `flavors.test.ts` 只扫代码文件，所以一直没发现。
- **修复（3b32c99）**：发布配置和自用配置的 `files` 加上 `!**/*.map`。App 里没有任何地方读 source map（没有 `setSourceMapsEnabled`，也不带 `--enable-source-maps`），这两份配置又是本任务新建的，不影响运行时行为。T161 扫描发布产物时也会遇到同样的问题，这样一并解决了。

### 2. E2E-selfuse-coexist 的 helper 列表不对

- **现象**：实际起来的是主程序、Renderer helper、**两个**普通 helper 和宿主，没有 GPU helper。
- **原因**：用例的期望写错了。Electron 新版本把 GPU 进程放在普通 helper 里跑，GPU helper 和 Plugin helper 一样不会启动。
- **证据**：
  - runner 上的进程列表就是上面这五个。
  - 本机 VS Code 用的是 Electron 42.10.0，它的 `--type=gpu-process` 进程是 "Code Helper"，不是 "Code Helper (GPU)"。
- **修复（f3fff60）**：
  - 改为断言主程序、普通 helper、Renderer helper、宿主都在运行；
  - 另按 `ps` 里的 `--type` 断言有 gpu-process、renderer、utility 三类进程；
  - 两个不会启动的 helper 只由 IT-selfuse-build 检查签名。

### 3. E2E-selfuse-replace 中旧 App 退出超时（原因未查清）

- **已经确认的部分（host.log、main.log）**：
  1. 换上"下一版"后，新 CLI 一连上旧宿主（`conn.rejected peer_exited`：CLI 校验 cdhash 不符后自己断开），旧宿主就写下 `host.pending_upgrade`；0.45 s 后以 reason `upgrade` 有序退出。这一段符合设计。
  2. 旧界面随即拉起了两次新宿主，control 连接、窗口连接各一次。两次都因为旧宿主还占着锁，以退出码 3 退出，前后相差 10 ms。之后 500 ms 的预算用完，界面停在 `failed: host_unreachable`。所以这一次**没有**出现"旧界面接上新宿主"（汇报遗留第 7 条的情形）。不过旧界面确实试过拉起磁盘上的新宿主，只是这次时机不巧没成功。
  3. 测试对旧 App 发出第一次 SIGTERM 后，30 s 内 App 里一直有进程在跑；finally 里第二次 SIGTERM 让它立刻结束，整个用例共 39 s。
     - 我的理解（没有核对源码）：Electron 只把第一个 SIGTERM 当作优雅退出，处理完就恢复系统默认处理，第二个会直接杀掉进程。
     - 照这个理解：**被替换掉的旧 App 卡在了优雅退出（相当于 ⌘Q）的过程里**。
     - 对照：正常情况下同一种退出没有问题。同一轮的 coexist 用例里，Cupper Local 收到 SIGTERM 后正常退出，宿主约 10 s 后作为孤立宿主退出；那 10 s 是因为它正在从 npm 安装 claude-agent-acp。
- **还不知道卡在哪里**：用例在等待时只能看到进程是否存在。
- **这次的处理（f3fff60），只加诊断，没有改产品代码**：
  - 等待超时时，在报错里列出 App 内还在运行的进程（pid、ppid、运行时长、参数），并附上 Chromium stderr 的末尾；
  - 第一次退出改成 `expect.soft` 检查，随后再退出一次（第二次 SIGTERM 会结束它），这样后半段"新 App 跑新宿主"照样执行、照样出结果。
- **它是测试步骤的问题还是产品的问题，要看第二次运行的诊断才能判断。**
  - 如果确实是"App 包运行中被替换后，⌘Q 退不出去"，那属于产品行为（Electron 或主进程在这种状态下的退出流程），要修就改主进程，我会先报给你，不动手。
  - 在此之前，安装文档本来就要求先退出再替换。
  - 用户忘了先退出的情况，文档目前写的是"退出再打开"。如果确认 ⌘Q 在这种状态下会卡住，这段说明要改成强制退出（Activity Monitor 或 `kill`）。

## 规模

- `cargo xtask size`：+772 −67，离停止线 800 只剩 28 行。
- 这一轮新增约 32 行：两份配置各 3 行，`app.ts` 的诊断和进程类型约 25 行，两个用例约 5 行。
- 如果第 3 处要改测试，28 行大概够用；要改产品代码的话，会先报给你。

## 提交（相对上一轮汇报新增）

```
ba481bb docs(task): record T160's first runner run and what it found   （本地，未推送）
f3fff60 test(desktop): expect the helpers Electron 44 starts, and say why a wait ran out
3b32c99 build(desktop): leave source maps out of the release and self-use apps
```

## 待办

1. 看第二次 nightly 41586879127：
   - 通过：验收完成。
   - 失败：按诊断信息给出第 3 处的结论。是测试步骤的问题就在余量内修；是产品行为的问题就先报给你。
2. PR 的 8 项检查以 41586882919 为准。
3. 下次推送时，本地的 ba481bb（只有 notes）可以一起推。

上一轮汇报里的设计和计划建议（5.1.5、5.1.6、5.7.8、5.10.x）、P-4 的缺口、遗留事项都不变。另外补充一条：5.7.8 说"发布产物扫描测试钩子标记串"，建议注明发布产物不带 source map。
