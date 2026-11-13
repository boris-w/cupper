# T107 汇报

- 执行者：子 agent（工程师，桌面主进程与构建）
- 分支：build/T107-electron-45
- 完成日期：2026-11-13

## 做了什么

1. **升级版本**：electron 从 44.9.0 改为 45.2.3（2026-11-04 发布，45 的最新补丁），apps/desktop 和 tools/e2e 两处都是精确版本，pnpm 的供应链策略检查通过。45 的底层是 Chromium 156、Node 24.22.0、V8 15.6；vite 的构建目标从 chrome152 改为 chrome156，Node 仍是 24。
2. **配套依赖都不用升**：electron-builder 26.17.0 能正常打包 45。它和我们用的 @electron/fuses 都是 1.8.0；fuse 的读写按序号进行，45 也没有新增 fuse。
3. **fuses**：45.2.3 的 `build/fuses/fuses.json5` 仍是 9 个，名字和出厂值都与 44 相同。所以 5.7.9 的两列和 T161 扫描的期望值都不用改。本机用 45 不签名构建了一次发布配置，release-config 的 3 条用例全部通过。写 fuse 后要给 Electron Framework 补 ad-hoc 签名，在 45 上照旧。
4. **breaking changes 逐条对照**（来源：breaking-changes.md 的 45.0 一节，以及 v45.0.0 到 v45.2.3 的发布说明）：共 12 条，都不影响我们。表在 notes.md 第 3 节，其中：
   - 沙箱 preload 去掉的垫片我们没用，构建出的 preload 只 require electron；
   - display-capture 照旧被权限处理器拒绝；
   - asar 里的 fd 我们没交给原生代码或子进程；
   - window.open 一律 deny；
   - safeStorage、contentTracing、fork 都不用。

   任务书点名的 webUtils.getPathForFile、sendInputEvent、will-frame-navigate、utilityProcess、窗口和菜单 API，在 45 上都没有破坏性改动。
5. **Q26 复测**：T077 的探针复制到了本目录的 q26/，只改成不抢焦点（accessory 加 showInactive，与 e2e 的后台模式相同）。在 45.2.3 上跑的 result.json 和 T077 在 44.9.0 上的逐字相同，五条结论都成立：
   - Playwright 的键盘输入不触发 before-input-event；
   - sendInputEvent 会触发；
   - input-event 的键盘事件运行时带键码；
   - page.click() 产生 mouseDown 和 mouseUp；
   - focusedFrame 可靠。
6. **MAN-keyboard**：用户在 7eff08b4 的 e2e 构建上用实体键盘做了一遍，7 项全部通过。结果写进了 `docs/tasks/T107-electron-45/man-keyboard.md`，证据追加在 tests/ids.toml 末尾：date 2026-11-13，electron = "45.2.3"。
   - 发请求的延时用的是 45 秒，不是 15 秒：编排者回消息要几秒，15 秒时请求会在用户切回窗口之前到达。
   - 第 1 项的前两次属于步骤问题，不算数：用户把面板第 4 行的空输入框当成了消息输入框。
   - 第 2 项用的是清掉那次请求时按的 Esc：焦点在第 4 行的空输入框里，请求带特权选项，Esc 立即拒绝，没有确认框。用户认为这已经验证，没有在焦点位于选项列表时重做；两种焦点下 Esc 走的是同一个 close 回答。
   - 其余各项的现象和用时都记在表里。
7. **coexist.test 的注释**：第一次改成了 "Electron 45 runs the GPU process in the plain helper"，但断言用的是 arrayContaining，证实不了这一点。按你的决定，改成只写断言实际核对的内容（The app, its plain and Renderer helpers and its host are running），不写版本，断言不动。

## 这期间分支上的其他事

- 编排者把 main（9c43c6d1）合进了分支，合并提交 3f37112c，没有冲突；本机 `pnpm install --frozen-lockfile`、`cargo xtask ci ts` 608 条通过。
- 另一位执行者在这个 worktree 里提交了 7eff08b4：删掉 T260 为 Electron 44 加的 `dropVisibilityListenersOnClose`。
  - 上游 #53971 经 electron/electron#54048 回合，已经进了 45.2.3；45.2.3 的框架里 `visibilityChanged` 0 处，`-window-visibility-change` 0 处。
  - 改动的三个文件（window.ts、index.ts、hardening.test.ts）与 T260 之前逐字相同。记录在 notes.md 第 7 节。

## 关键取舍

- **配套依赖不升**：只升"必须跟着升的"。@electron/fuses 2.x 只是认得第 9 个 fuse 的名字，读写不受影响。
- **Q26 的探针改成不抢焦点再跑**：本机不跑抢焦点的用例（守则第 10 条）。结果与 T077 逐字相同，说明这个改动不影响结论。
- **MAN-keyboard 的步骤直接引用 T099 的记录**，只写三处差异：构建的提交、Electron 版本、45 秒的延时。T099 记录里的两处缺陷已由 T217 修好，原文的写法照样适用。
- **注释只写能证实的**：见"做了什么"第 7 条。

## 验证方式

- **本机**：
  - desktop 和 e2e 的 tsc、全仓库的 eslint 和 prettier、desktop 的单元测试 109 个、trace-ids check，都通过。
  - 用 45 不签名构建发布配置一次，release-config 的 3 条用例通过。
  - Q26 探针跑一次，结果与 T077 相同。
- **500fc45b 上**：
  - PR #94 的 8 个检查全过：https://github.com/boris-w/cupper/actions/runs/42606101827 ，其中 ui 70、e2e 33、rust 2703、ts 557、protocol 190、license 6、testdata-scan 4 个通过；compat-framework 按规则跳过。
  - nightly 全集 https://github.com/boris-w/cupper/actions/runs/42606099450 ：e2e 70、release-config 8、selfuse 12、security-peer（macos-15、macos-26）各 18、differential 3、compat-framework 15、perf 6 个通过（perf 只记录）。codex-drift 失败，是 codex 0.167.0 的问题，与本任务无关，T246 已修。
- **7eff08b4 上（编排者告知）**：PR #94 的 8 个检查全过（ci 43388518666）；nightly 全集 43388524911 九个作业全过，含 e2e、selfuse、release-config、perf、differential。
- **截图基线**：ui 作业没有报差异，基线不用更新。
- **4b35f977、2809d1c1**：只改了一句注释和文档、证据，本机 prettier 和 trace-ids 通过，还没有跑过 CI。
- **验收标准**：
  - [x] PR 的 8 个检查通过，nightly 全集在分支上通过；
  - [x] 截图基线没有变化；
  - [x] MAN-keyboard 的检查步骤写好，用户已做完，7 项通过；
  - [x] 汇报里写了 PR 和运行的链接。
- **规模**：cargo xtask size 为 +13 −49（含 7eff08b4 删掉的代码；lock、docs 不计），在停止线 200 以内。

## 提交列表

```
2809d1c1 docs(T107): MAN-keyboard on Electron 45.2.3, all seven pass
4b35f977 test(desktop): the coexist comment says what the check checks
7eff08b4 refactor(desktop): drop the visibility listener workaround on Electron 45
3f37112c Merge branch 'main' into build/T107-electron-45
500fc45b docs(T107): 45's breaking changes, Q26 on 45, and MAN-keyboard's steps
72306e6a build(desktop): Electron 45.2.3
```

## 遗留问题 / 范围外发现

1. 设计和 CLAUDE.md 里写着 "Electron 44" 的地方，由你在合并时更新，清单在 notes.md 第 6 节：CLAUDE.md 的技术栈表、5.1.x、5.10.6、5.7.8、5.7.9 里的几句，以及 13 的 Q26（可补"45.2.3 上复测相同"）。
2. MAN-keyboard 的发请求延时：T099 的记录里写的是 15 秒，实测要 45 秒才稳。以后照 T099 重做时，请按 T107 这份记录里的写法。
