# T131 notes

## 开工前的估算（2026-11-04，报给编排者，等回复后再写代码）

已有、可以直接用的：
- ShellInfo 里冻结了的 `reviewDocumentUrl`（浏览器 mock 现在给的是 `about:blank`）；
- T130 的视图接口（`review/view.ts`）、还原确认（`RevertConfirm`，来源为 `view` 时不抢焦点，同一时间只显示一个）、`act` 与 `comment` 的接线；
- `app-protocol.ts` 的多主机支持；
- T104 原型的写法：导航例外、Worker、Virtualizer、两条测试输入路径。

开工前发现的一个问题：许可证门禁按 `allow.has(text)` 精确比较，`@pierre/*` 的 `package.json` 写的是小写 `apache-2.0`，会被判为不在清单里。门禁要改成不区分大小写，或者按 SPDX 规范化后比较。

| 模块 | 实现 | 测试 |
|---|---|---|
| A. 入口与 CSP、构建：`REVIEW_DOCUMENT_CSP`、vite 第二个 client 环境与入口 HTML、主进程加 `cupper-review` 主机、`reviewDocumentUrl`、许可证门禁登记新环境并修大小写、浏览器 mock 与截图页的第二个源 | 120 | 10 |
| B. 导航：`navigation.ts` 只放行一次审查文档的首次加载 | 25 | 20 |
| C. 协议与 zod：两个方向的消息、版本、解析（主文档与审查文档共用一份 schema） | 90 | 50 |
| D. 主文档一侧组件：iframe（sandbox、allow）、空闲时预加载、常驻、端口握手；props 译成 `init`/`render`/`scroll_to`/主题语言缩放；`need_file` 时取 `git/file_diff`（必要时取 blob）；按取到的 hunk 校验 `hunk_action`，译回 `act`；`draft_comment` 译回 `comment`；`key_forward` | 220 | 100 |
| E. 审查文档：入口、只认第一个端口、无状态视图、pierre（Worker、`shiki-js`、只加载用到的语言、Virtualizer 就是滚动容器）、按文件的头部与折叠、hunk 渲染（hunk、mode、whole_file）、`scroll_to`、懒加载、主题与缩放 | 260 | — |
| F. 审查文档里的操作：按范围的 hunk 按钮与文件按钮；行内评论（悬停行号出现"+"、评论框、Esc、⌘Enter、已有草稿显示在行上）；其余按键转发 | 130 | — |
| G. 文案 | 25 | — |
| H. 用例：UNIT-review-injection；E2E-SEC-review-iso；E2E-review-comment；E2E-SEC-revert-confirm；截图用例的改动（基线由 ui-baseline 生成，不计行数） | — | 270 |
| 合计 | ~870 | ~450 |

合计约 1,320，超过硬停止线 1,300；按前几个 L 任务的偏差上浮 20–30%，会到 1,600 左右。需要拆分。

### 拆分方案

- **T131（本任务）审查文档 I：入口、协议与只读视图**
  - 包括 A、B、C、D、E、G，以及 F 里的按键转发；
  - D 里 `hunk_action`、`draft_comment` 的接收与校验也在这里（E2E-SEC-review-iso 要用：iframe 发来的 `revert` 不直接执行）；
  - 用例：UNIT-review-injection、E2E-SEC-review-iso、review 与文件树的截图基线；
  - 约 960，上浮后约 1,150–1,250。
- **新任务 审查文档 II：hunk 按钮与行内评论**
  - 包括 F（审查文档里的按钮、评论交互）；
  - 用例：E2E-review-comment、E2E-SEC-revert-confirm、剪贴板的 e2e 核实；
  - 约 360，上浮后约 450。

## 实现记录

## 依赖

- `@pierre/diffs` 1.6.4，2026-11-04 00:39 UTC 发布，当天加入，没过 pnpm 12 默认的最短发布时间（1 天）。`pnpm add` 自动在 `pnpm-workspace.yaml` 加了 `minimumReleaseAgeExclude: ['@pierre/diffs@1.6.4']`。编排者裁定（选项 (a)）：开发期间保留这条临时例外，最终推送的提交里去掉；合并以 2026-11-05 00:39 UTC 之后、去掉例外的那次 CI 为准。
- **1.6.3 → 1.6.4 的核对**：两个 tarball 各自 `npm pack` 到空目录里解包比较。文件清单相同，内容只有 `package.json` 不同，`dist/` 的代码逐字节一致：
  - 1.6.3 发布时依赖还是 `workspace:*`、`catalog:`，装不上，并带一个 `prepublishOnly` 脚本；
  - 1.6.4 把依赖写成精确版本（`diff` 9.0.0、`hast-util-to-html` 9.0.5、`lru_map` 0.4.1、`@pierre/highlights` 0.4.0、`@pierre/theme` 2.0.0、`@pierre/theming` 1.0.2），`shiki` 仍是 `^3.0.0 || ^4.0.0`，没有任何 `scripts`。
  - 所以 1.6.4 只是修正了 1.6.3 的发布元数据，没有新增安装脚本，代码没有变化，谈不上新增的网络或文件访问。
- `zod` 4.6.7，与 `@cupper/protocol`、`@cupper/host-client` 相同。
