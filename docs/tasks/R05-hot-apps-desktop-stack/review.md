# R05 Review

## 第 1 轮（2026-10-16）

- 结论：通过
- 审阅范围：`git diff main...research/R05-hot-apps-desktop-stack`（sources.md，187 条来源）

### 存档说明

报告正文按子 agent 的回复原样存档，只改了一句：原文逐一列出了用户本机装了哪几个 App，与 brief 的约束冲突，改成了概括写法。合并前扫描了分支的 diff，只有一处误报：Discord 博客 URL 里的 "-users-"。

### 抽查

- 新版 Teams 用的是 WebView2：编排者在本机只读核实，`/Applications/Microsoft Teams.app/Contents/Frameworks` 下有 `MSWebView2.framework` 和 `libSlimCoreWebview2.dylib`，与报告一致。

### 验收标准核对

| 验收标准 | 结论 |
|---|---|
| 每个 App 一行的技术栈表 | 达标，33 行，覆盖 brief 列出的全部 App，包括 Manus |
| 按框架分组的统计，以及与 R04 合并后的分布 | 达标，合并后共 73 个样本 |
| 3–5 条补充规律 | 达标，共 5 条 |
| 来源注明日期和获取方式 | 达标 |

### 意见

| # | 位置 | 问题 | 处理 |
|---|---|---|---|
| 1 | 外发请求 | 和 R04 一样，用本机已登录的 `gh` 做了只读查询 | 接受 |
| 2 | 遗留问题第 5 条 | Electron 在 macOS 26 上有已知的卡顿，Qoder 为此升级了 Electron | 写进 ADR-03：最低 Electron 版本要避开这个问题，具体版本在详细设计时核实 |
| 3 | 遗留问题第 6 条 | agent 类产品普遍不上 Mac App Store | 写进 ADR-03：只做官网直接分发，不上 Mac App Store，因为需要操作本地文件和进程 |

### 结论对 ADR-03 的影响

R05 支持 ADR-03 的结论（Electron + React），没有反例：
- 32 个热门 App 中，有 25 个用 Web 技术渲染主界面；
- 大厂"超出 Electron"以后，换的是自己掌握的 Chromium 运行时，没有一家改用 WKWebView（元宝除外）；
- 老牌 Electron 产品没有一个离开 Electron。

### 合并

- 合并提交：`9d92a4a`
