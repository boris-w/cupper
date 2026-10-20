# T167 工作笔记

- 执行者：子 agent（测试工程师；T090 的执行者接着做）

## 开工前读过的

- 任务书；T090 的 `notes.md`（模块 6–14）和 `review.md`；`handoff-notes.md` 第一列含 T090、T167、e2e 的行。
- 请求面板（T083）：`RequestPanel.tsx`，它的行为决定了几个辅助的写法：
  - 待命 500 ms 后设 `data-ready`；
  - 获得焦点时激活，`data-activation` 变为 `auto`；
  - 激活前除 Tab 外的按键都被吞掉。
- 输入守卫：`main/input/index.ts`、`guard.ts`。按键只在主框架有焦点时算数；窗口获得焦点后 1 s 内的按键不算。
- 侧边栏：
  - 会话行是按钮，名字是会话标题；
  - 提权横幅里的按钮文案是 `sidebar:review`，点了经 `privileged({kind: "elevation"})` 弹原生确认框。
- 添加项目：`main/shell.ts` 调 `dialog.showOpenDialog`。
- `nettop -L 1 -n -x` 的输出：每个进程一行（`名字.pid`），下面跟它的连接行（`tcp4 本地<->远端`）；`-p` 可以重复给。

## 开工前的规模估计

行数指格式化后的行，含注释。实现按 1.15 倍、测试按 1.1 倍放大。

**实现**

| # | 模块 | 内容 | 草估 |
|---|---|---|---|
| 1 | `__cupperTest` | ui-core 的 `mount` 加可选的 `onClient`，经 `App` 传到 `HostProvider`；渲染进程入口只在 e2e 构建里传入，挂上 `window.__cupperTest`（`counters()`、`state()`）；用例侧的读取 | 40 |
| 2 | CLI `clock-advance <ms>` | 参数解析和执行，`#[cfg(feature = "test-harness")]` 门控 | 25 |
| 3 | CLI 包装补齐 | `repl`（按 `repl.exit` 分条）；`watch` 和 `until(谓词)`；以 `cli` 身份连接 | 65 |
| 4 | 按窗口断连 | 经主进程的 `__cupperE2E.disconnectWindow` 断开，等窗口重新连上 | 20 |
| 5 | 事件屏障 | `clock-advance`；在会话流上等事件；在 `sessions --watch` 上等会话出现 | 40 |
| 6 | `kill -9` 宿主 | 取 pid，按进程表确认是本用例的宿主，SIGKILL，等它消失，等新宿主连上（pid 变了） | 35 |
| 7 | CLI 提权 | `cli` 身份在 `repl` 里 `elevate`；点横幅；替换后的确认框批准并勾选 approve；等授权返回 | 45 |
| 8 | 外发采样 | App 和宿主的进程树；`nettop -L 1 -n -x -p …` 采样并解析成（pid、协议、本地、远端、状态） | 50 |
| 9 | 按键 | `sendInputEvent` 发 keyDown、char、keyUp；修饰键和键名；先让主框架获得焦点，并过了 1 s 冷却 | 45 |
| 10 | 请求面板 | 等 `data-ready`；按 Tab 直到 `data-activation=auto`；按数字键 | 30 |
| 11 | 原生对话框补齐 | `showMessageBoxSync`（立即答 `cancelId`，并记录）；`showOpenDialog` 挂起到用例给出路径或取消，并记录 | 30 |
| | 实现小计 | | **约 425**，×1.15 ≈ **490** |

**测试**

| 用例 | 草估 |
|---|---|
| 非 e2e 构建不含测试入口：Vitest 用例，以 `release`、`dev` 两种 flavor 把 main、preload、renderer 构建到临时目录，扫 `__cupperTest`、`__cupperE2E`；`e2e` flavor 作对照，必须扫到 | 50 |
| `__cupperTest` 与断连：断开后重新连上，缺口、重复、重置计数都为 0 | 30 |
| 屏障与 `clock-advance`：手动时钟下推进后会话出现在索引里；在会话流上等到 `turn.completed` | 30 |
| `kill -9`：宿主被杀后 UI 拉起新宿主 | 25 |
| CLI 提权：经横幅和确认框批准，之后 `repl` 里写成功 | 30 |
| 外发采样：能取到本用例进程树的样本 | 15 |
| 按键：经 `sendInputEvent` 在输入框里打字 | 20 |
| 请求面板：假 agent 的权限请求；打开会话，等 `data-ready`，按 Tab 激活，按 1；假 agent 回显所选的选项 | 40 |
| 对话框：添加项目时的 `showOpenDialog` 拿到路径，项目加进来；`showMessageBoxSync` 的替换 | 33 |
| CLI 单元测试补 `clock-advance` 的解析 | 5 |
| 测试小计 | **约 280**，×1.1 ≈ **305** |

**合计约 795 行**，在约 850 的线以内，开工。
