# T104 汇报

- 执行者：子 agent（前端工程师，桌面与渲染进程安全）
- 分支：`spike/T104-review-iframe`（PR https://github.com/boris-w/cupper/pull/68 ，头提交 024ea972）
- 完成日期：2026-11-04

**结论：建议 K9 通过，做 T131。** 功能项和桌面确认 8 点里的硬性项都通过；场景 9 在 `macos-15` runner 上做了 30 次冷启动，数据已记录。PR #68 在 024ea972 上 8 项检查全部通过。

## 做了什么

1. **`spike-run.yml`**（分支上的第一个提交 ed759a8，经 PR #65 先合进 main）：通用的 `workflow_dispatch` workflow，T103、T108 之后也能用。
   - 三个输入：
     - 原型目录：必须以 `spikes/` 开头、不含 `..`、目录存在；
     - 命令：经 env 传入，`bash -c` 执行；
     - 分钟数：1–360，`timeout-minutes` 取它且封顶 360。
   - 上传 `<目录>/results/` 为 `spike-results`，只有输入校验通过时才上传。
   - `permissions: contents: read`，不用 secret；最后照 e2e-repeat 清理 runtime-pack 的 stage。
2. **双源原型**（`spikes/T104-review-iframe/`）：
   - 直接 import `apps/desktop` 的四份代码，没有复制：
     - `app-protocol`：`standard`、`secure`，每个主机一个根目录和一份 CSP；
     - `permissions`；
     - `mainWindowOptions` / `webPreferences`：preload 只在主框架；
     - `watchInput`（inputGuard）。
   - 主文档的 CSP 用 `MAIN_DOCUMENT_CSP`；审查文档的 CSP 照 5.7.8 的表，不启用 Trusted Types；iframe 带 `sandbox="allow-scripts allow-same-origin"`、`allow="clipboard-write"`。
   - 导航：`will-frame-navigate` 只放行审查文档的首次加载，之后一律拦截。
   - 审查文档里原样使用 `@pierre/diffs` 1.6.4：`WorkerPoolContextProvider` 的 `workerFactory`，`shiki-js` 高亮，`Virtualizer` 里放 `MultiFileDiff`。
   - 原型自带 `package.json` 和 lockfile，依赖锁精确版本，不进 pnpm 工作区；Electron 用 `apps/desktop` 的 44.9.0。
3. **一次运行**（`src/main.ts`）：场景 9 冷启动一次，接着做全部检查项，写 JSON 后退出。`measure.mjs` 负责循环冷启动和汇总。截图、剪贴板、真实按键这三步只在 runner 上做。
4. **数据**：runner 运行 41769669515 的产物已放进 `docs/tasks/T104-review-iframe/results/`（024ea972）：run-1 到 run-30、summary.json、亮暗两张截图。
5. **判定报告**：全文见本目录的 `report-q29.md`。

## 关键取舍

- **许可证**：开工前核对过，52 个包全部在 ADR-11 的允许清单里（Apache-2.0 4 个、MIT 46 个、ISC 1 个、BSD-3-Clause 1 个）。
- **Virtualizer 就是滚动容器**：一开始在外面另套了一个可滚动的 div，pierre 把 6,000 行全部放进 DOM，渲染进程约 800 MB。改成由 Virtualizer 自己滚动之后，DOM 里约 400 行，约 300 MB。T131 要照这个写。
- **两条输入路径分开用**：
  - `sendInputEvent` 走 `before-input-event`，用来验证焦点在 iframe 时 inputGuard 不记证据；
  - DevTools 的 `Input.dispatchKeyEvent` 能送进进程外 iframe，用来验证 `key_forward` 和 Esc 留在 iframe；
  - 菜单加速键只能用真实按键验证，用 runner 上的 `osascript`。
- **注入检查放在顶部做**：diff 是虚拟化渲染的，滚走以后带标记的那一行不在 DOM 里，会误报失败（0151aa5d 修正）。

## 验证方式

- **本机**：构建后单跑了几次，看检查项的 JSON，功能项都通过。没有跑测量循环。
- **runner**：https://github.com/boris-w/cupper/actions/runs/41769669515 ，冷启动 30 次全部成功，各次之间没有不一致的检查项，违规计数合计 0。数字见 `report-q29.md` 第 5 节。
- **PR CI**：https://github.com/boris-w/cupper/actions/runs/41771482510 ，PR #68 在 024ea972 上 8 项检查全部通过。
- **规模**：597 行，不含原型自己的 `package-lock.json` 和 `docs/`；其中 workflow 76 行，原型 521 行，停止线 600。`cargo xtask size` 会把 npm 的 lockfile（1,197 行）算进去，这个缺口你已记进交接事项。

## 提交列表

```
024ea972 docs(task): T104 scenario 9 data from the runner
0151aa5d fix(spike): check the markup line while it is rendered
bec617b3 docs(task): T104 notes, so far
7a6b0422 test(spike): the review document in a cross-origin iframe (T104)
ed759a8c ci: spike-run, a spike's command on a macOS runner
```

## 遗留问题 / 范围外发现

1. **我在本机的失误**：本机试跑 `measure.mjs 2` 时，第一次运行误开了真实按键和剪贴板两步。
   - `osascript` 把原型抢到前台约 1 秒，发了一次 ⌘⇧P；
   - 剪贴板写入失败（`NotAllowedError`），用户的剪贴板没有被改；
   - 之后改成只在 `GITHUB_ACTIONS=true` 时执行。
   - 另外要说明：本机每次单跑都会经 `window.show()` 把窗口带到前台片刻。按你的要求，今后在本机跑任何东西之前，先确认不会抢焦点、不会发按键、不会动剪贴板。
2. **剪贴板没有对上**：在 runner 上，iframe 里的写入成功，但主进程读回的值不对。现象、怀疑的原因和 T131 要核实什么，见 `report-q29.md` 第 4 节第 6 条。
3. **`navigation.ts` 要加一个例外**：放行审查文档的首次加载。原型里有现成的写法，交给 T131。
4. **冷启动的高亮要 2.5–3 s**（runner）。T131 应在空闲时预加载 iframe、预热 Worker，并只加载 diff 里用到的语言。
5. **notes 和数据**：notes.md 已在分支上提交，位置 `docs/tasks/T104-review-iframe/notes.md`；runner 数据在同目录的 `results/`。

（原文分两部分，第二部分即本目录的 `report-q29.md`，由编排者按执行者的说明存档：运行环境不允许子 agent 创建报告类文件。）
