# Codex 桌面版界面规格（R06 一手资料）

- 对象：ChatGPT 桌面 App（macOS）里的 Codex 视图，版本 26.1013.40934
- 编写日期：2026-10-19
- 用途：给 D03（带界面的 PRD）和 T106（视觉规格）使用
- 写法：全部用自己的话描述，只记录事实性数值和简短的界面文案。没有复制对方的代码、样式表、图标、图片或字体

## 0. 来源标注

每条结论后面用方括号标来源：

| 标记 | 含义 |
|---|---|
| 〔本机〕 | 静态读取本机 App 包里的资源（CSS、JS、主进程脚本、本地化文案）得到 |
| 〔截图〕 | 在作者 10-16 截的主界面亮色截图上逐像素测量得到（2 倍屏，已折算成 pt）。截图本身不入库，见 `docs/meetings/2026-10-16-prd-interview-2/codex-screenshot-notes.md` |
| 〔公开〕 | 官方文档、发布说明、官方截图或第三方评测，URL 见第 1.3 节 |
| 〔推测〕 | 由上面几类间接推出，没有直接证据 |

"本机"的数值是代码里的默认值。用户改过的设置（侧边栏宽度、字号、主题）会让实际界面偏离默认值，截图就是一例。

## 1. 来源与版本

### 1.1 App 包

| 项 | 值 | 来源 |
|---|---|---|
| 版本 | 26.1013.40934（CFBundleVersion 20770），包标识 `com.openai.codex` | 〔本机〕`Contents/Info.plist` |
| 包内产品名 | `package.json` 的 name 是 `openai-codex-electron`，productName 是 `Codex`。ChatGPT 桌面 App 就是改名后的 Codex Electron 外壳 | 〔本机〕`app.asar` 里的 `package.json` |
| 运行时 | Electron，Chromium 155（`Chrome/155.0.8059.80`）。框架改名为 `Codex Framework.framework` | 〔本机〕`Contents/Frameworks/` |
| 最低系统 | `LSMinimumSystemVersion` 13.0 | 〔本机〕 |
| 渲染进程 | React + Tailwind v4 + CSS Modules，构建工具是 Vite（rolldown）；颜色在 CSS 里用 lightningcss 的 light/dark 写法，运行时再由 JS 按主题写入一批 CSS 变量 | 〔本机〕 |
| 本地化 | 每种语言一个 JS 文件（简体中文是 `zh-CN-*.js`），英文原文在组件代码的 `defaultMessage` 里 | 〔本机〕 |

### 1.2 读过的包内资源（相对 `Contents/Resources/`）

| 位置 | 内容 |
|---|---|
| `app.asar` → `webview/index.html` | 启动页、CSP、启动 logo 动画 |
| `app.asar` → `webview/assets/app-shared-*.css` | 主样式表：Tailwind 主题、全部颜色和尺寸变量、工具类 |
| `app.asar` → `webview/assets/app-initial-*.css`、`user-message-*.css`、`terminal-panel-*.css` 等 | 组件级 CSS Module（Markdown、输入框、用户气泡、标签条、终端） |
| `app.asar` → `webview/assets/app-initial-*.js`、`app-primary-*.js`、`app-shared-*.js` | 外壳布局、侧边栏、主题推导、命令和快捷键注册表、设置 schema |
| `app.asar` → `webview/assets/` 下按功能拆分的块 | 消息流条目、回合折叠、用户消息、审批与提问面板、审查面板、文件树、diff、提交弹窗、设置各页、通知设置 |
| `app.asar` → `webview/assets/zh-CN-*.js` | 简体中文文案 |
| `app.asar` → `.vite/build/main-*.js` | 主进程：窗口尺寸、标题栏样式、交通灯位置、半透明、通知与提示音 |
| `app.asar` → `native-menu-locales/` | 原生菜单文案 |
| `codex-notification.wav`、`codex-classic.wav` | 两个内置提示音（只记名字） |

### 1.3 公开物料

R01 的 `sources.md` 已登记过的不再重复。本次新读、带出界面事实的：

| URL | 类型 | 用到的内容 |
|---|---|---|
| https://learn.chatgpt.com/docs/reference/commands | 官方文档 | 三平台快捷键表 |
| https://learn.chatgpt.com/docs/reference/slash-commands | 官方文档 | 28 个斜杠命令 |
| https://learn.chatgpt.com/docs/reference/settings | 官方文档 | 设置分区、General 和 Appearance 的选项 |
| https://learn.chatgpt.com/docs/code-review | 官方文档 | 审查面板的范围、暂存和还原、行内评论 |
| https://learn.chatgpt.com/docs/permission-modes 、https://learn.chatgpt.com/docs/sandboxing | 官方文档 | 权限模式和审批范围 |
| https://learn.chatgpt.com/docs/prompting | 官方文档 | Steer、Queue、排队消息的位置 |
| https://learn.chatgpt.com/docs/notifications | 官方文档 | 通知三档、Activity 视图 |
| https://learn.chatgpt.com/docs/integrated-terminal 、https://learn.chatgpt.com/docs/artifacts-viewer 、https://learn.chatgpt.com/docs/environments/modes | 官方文档 | 终端抽屉、任务侧栏、Work in 和 Worktree |
| https://learn.chatgpt.com/docs/changelog | 发布说明 | 最新的 App 条目是 26.1007，没有 26.1013 的条目 |
| https://web.archive.org/web/20260501022809/https://developers.openai.com/codex/app/features （以及同期的 /codex/app、/review、/settings、/worktrees 存档） | 官方文档存档 | 合并前的官方截图（亮色和暗色各一版） |
| https://simonwillison.net/2026/Feb/2/introducing-the-codex-app/ | 第三方 | 首发版界面 |
| https://www.codecademy.com/article/open-ai-codex-app-for-mac-os | 第三方 | 带截图的教程（截图说明文字） |

公开截图全部早于 7 月 9 日的合并，看不到左侧图标栏，权限按钮的位置也和现在不同。凡是公开物料和本机包冲突的地方，本文以本机包为准，冲突点列在第 7.2 节。

## 2. 布局

### 2.1 窗口与标题栏

| 项 | 值 | 来源 |
|---|---|---|
| 默认窗口 | 1280 × 820；Windows 上按工作区的 85% × 80% 收缩，但不小于 480 × 600 | 〔本机〕主进程 |
| 最小窗口 | 480 × 600 | 〔本机〕 |
| 标题栏样式 | macOS 用 `hiddenInset`，内容一直画到窗口顶部，没有原生工具栏 | 〔本机〕 |
| 交通灯位置 | x = 16，y = (44 × 缩放 − 14) / 2，默认缩放下 y = 15，也就是在 44px 高的顶栏里垂直居中 | 〔本机〕 |
| 顶栏高度 | 44px。代码里交通灯按 44 居中，`--height-titlebar` 是 44px；截图实测顶栏 43–44pt | 〔本机〕〔截图〕 |
| 其他工具栏高度 | `--height-toolbar` 46px（页头、标签条）；`--height-toolbar-pane` 40px（面板头）；`--height-toolbar-sm` 36px | 〔本机〕 |
| 窗口背景 | 主窗口透明，macOS 用 `vibrancy: menu` 材质。设置里关掉"半透明侧边栏"后改为不透明，背景亮色 #F9F9F9、暗色 #000000 | 〔本机〕 |
| 截图里的顶栏 | 横贯整个窗口的浅灰带（#E8E8E8 左右），和下面白色的会话区有明显分界。这个灰色应该是半透明材质透出来的【推测】 | 〔截图〕 |

顶栏从左到右：〔截图〕〔本机〕

1. 交通灯（每个约 13pt，间距约 23pt）。
2. 后退、前进（命令 `navigateBack/Forward`）。
3. 侧边栏开关（`sidebar-trigger`）。
4. 会话区上方：文件夹图标加会话标题。
5. 右侧一组按钮，按 order 从小到大：分享、底部面板开关（⌘J）、侧边面板开关、摘要卡片开关、"+" 新建标签页、"…" 会话菜单。另有一个带数字的"已打开标签页"按钮，tooltip 是"{count} open tabs"。

工具栏按钮的规格：〔本机〕

- 高 28px，左右内边距 8px，圆角 `--radius-lg`；
- 默认是透明底，文字和图标用 tertiary 色；
- 悬停时用 ghost-hover 背景；
- 处于打开状态时换成 secondary 底色（截图里高亮的任务面板按钮就是这个状态）；
- tooltip 延迟出现，带快捷键。

"…" 会话菜单的内容（`threadHeader.*`）：〔本机〕

- Copy：Copy as Markdown、Copy working directory、Copy deeplink、复制会话 ID；
- Fork：Fork chat、Fork chat in same worktree、Fork chat in new worktree；
- Continue in：a new chat、the same worktree、a new worktree；
- 其他：New side chat、Open in new window；
- 简化版只有 Rename / Pin / Archive / Copy；
- 归档前有确认框；会话正在运行时，按钮变为"Stop and archive"。

### 2.2 左栏：导航栏和侧边栏

左栏分成两块：最左边一条窄的**导航栏**（rail），右边是**侧边栏**（会话列表）。〔本机〕〔截图〕

| 项 | 值 | 来源 |
|---|---|---|
| 左栏总宽 | 默认 340px，存在 localStorage 的 `sidebar-width`。范围：不小于 240，不大于 520，并且给主区至少留 240px | 〔本机〕 |
| CSS 里的同名 token | `--spacing-token-sidebar` = clamp(240px, 首选宽度（缺省 275px）, min(520px, 100vw − 320px))。JS 会写入首选宽度，所以实际默认是 340 | 〔本机〕 |
| 截图实测 | 导航栏约 52pt，侧边栏约 238pt，合计约 290pt，在 240–520 之内。说明截图作者把左栏拖窄过；340 这个默认值包含导航栏 | 〔截图〕；"包含导航栏"属于〔推测〕 |
| 导航栏 | 纵向排列，左右内边距 8px，图标按钮约 36px 宽（`w-9`），按钮间距 8px，底部内边距 8px；Electron 下背景透明，透出半透明材质 | 〔本机〕〔截图〕 |
| 导航栏内容 | 截图里上半部分是首页（选中）、历史、一个圆形图标、"…"、Git 分支图标；下半部分是更新下载（蓝色）、设置。这些是 ChatGPT 全局的导航项，可以置顶和拖动排序，分隔线宽 24px。文案有 Home、Chats、Plugins、Settings、Explore 等 | 〔截图〕〔本机〕 |
| 侧边栏背景 | 默认是半透明（`vibrancy: menu`）。截图里侧边栏是 #F8F8F8，导航栏随壁纸偏暖（#EEEDEB 左右）。不透明模式下用 surface-under 色（亮色 #F6F6F6，暗色 #141414，见第 3 节） | 〔本机〕〔截图〕 |
| 侧边栏头 | 高 32px；左侧是"Codex ⌄"（产品模式切换：ChatGPT / Codex / Home），右侧是铃铛（Activity）和搜索 | 〔本机〕〔截图〕 |
| 列表行 | 行高 30px，圆角 10px，左右内边距 8px（这组值只在 `.sidebar-navigation` 里生效；在别处，行高按"字号 × 1.5 + 上下内边距"计算）。会话行文字 14px、行高 20px；分组标题和部分辅助行 13px | 〔本机〕；选中行实测 30pt〔截图〕 |
| 选中行背景 | ghost-hover 色（前景色约 5% 叠加），截图里是 #EBECEC | 〔本机〕〔截图〕 |
| 窄窗口 | 768px 是断点：宽度小于 768 时侧边栏浮在内容上面，大于等于 768 时与内容并排 | 〔本机〕媒体查询 `(min-width: 768px)`；行为细节属于〔推测〕 |

### 2.3 会话区

| 项 | 值 | 来源 |
|---|---|---|
| 内容列最大宽度 | `--thread-content-max-width` = 48rem（768px），两侧各留 16px 内边距（`--thread-gutter`），所以文字和输入框的实际宽度是 736px | 〔本机〕；截图实测输入框和用户气泡的外沿宽 736.5pt〔截图〕 |
| 输入框旁的宽度 | `--composer-adjacent-max-width` = 768 + 2 × 24 − 2 × 13 = 790px，用于输入框上方的提示、toast 等 | 〔本机〕 |
| 宽块 | Markdown 里的表格、Mermaid 图、媒体可以超出内容列，最宽 56rem（896px），高不超过 80vh | 〔本机〕 |
| 条目间距 | 相邻条目 16px（`--conversation-item-gap`），同一组内的条目 4px（`--conversation-grouped-item-gap`） | 〔本机〕 |
| 输入框到底边 | 16px（`--thread-composer-bottom-inset`），截图实测约 19pt（含阴影） | 〔本机〕〔截图〕 |
| 新建会话页 | 输入框垂直方向锚在视口高度的 42% 处（`home-composer-anchor`） | 〔本机〕 |
| 内容居中方式 | 会话列在"主区减去右侧卡片"后剩下的宽度里居中。截图里主区宽约 1108pt，右侧摘要卡片占 300pt，会话列整体左移约 154pt | 〔本机〕〔截图〕 |

### 2.4 右侧工作区

右侧有三种东西：贴满高度的**侧边面板**（标签容器，装审查、文件、浏览器、终端等标签）、浮起的**摘要卡片**（截图右上角那张），以及会话下方的**底部面板**（终端抽屉）。

**侧边面板**〔本机〕

- 外形：贴满整个高度，靠会话区的一侧有 1px 边线。背景默认等于 surface。不是浮起的卡片。
- 宽度存在 localStorage 的 `app-shell:right-panel-width:v3`，存的是最小值和最大值之间的比例（0–1），不存像素。
- 宽度范围：
  - 最小 320px；
  - 最大 = 主区宽 − 352px（保证会话区至少留 352px）；
  - "统一工作区"模式下最大 = 主区宽 − 320px；
  - 全宽（full view）模式下最大 = 主区宽。
- 默认宽度：max(320, min(窗口高 × 1.6, 主区宽 − 500), min(640, 主区宽 − 352))。按默认窗口估算约 500–640px【推测】。
- 拖拽手柄：
  - 是 role=separator，鼠标显示 col-resize；
  - 支持键盘：方向键按步长调整，Home 跳到最小，End 跳到最大；
  - 指示线是 1px 的 border-strong 色，平时透明，悬停、拖动或获得焦点时才显示；
  - 无障碍文案"Resize content panes"。
- 开合动画：spring，0.5s，bounce 0.1。
- 窗口变窄时自动收起，变宽后恢复。

**标签条**〔本机〕

- 作为标题栏时高 46px，作为面板头时高 40px；左内边距 8px，标签间距 4px。
- 单个标签：
  - 尺寸：高 32px，圆角 `--radius-lg`，最小宽 90px；
  - 内边距：左 10px、右 4px；文字 13px；
  - 选中：有边框，背景是 elevated-secondary；
  - 未选中：文字 tertiary，悬停时显示 ghost-hover 背景。
- 标签之间是 12px 高的竖线，相邻标签悬停时隐藏。
- 标签种类：Changes（审查）、Files、Browser、Terminal、Side chat、PR #n、New tab。
- 标签可以拖到左栏、右栏、底部或新窗口。

**摘要卡片**（输出内容 / 来源 / 计划）〔本机〕〔截图〕

- 宽 300px，和边缘相距 8px，合计占 308px。截图实测卡片宽约 301pt，距右边缘约 8pt。
- 显示方式随两侧余量变化。设 gutter =（主区宽 − 736）/ 2：
  - gutter < 180：收进弹出层；
  - 180 ≤ gutter < 400：会话列左移 154px，让出位置。截图属于这一档；
  - gutter ≥ 400：卡片直接放在右侧留白里。
- 卡片外形：圆角 `--radius-2xl`，1px 边框，带阴影。
- 节标题是 13px secondary 色，左右内边距 20px。节之间是 0.5px 的分隔线，左右各缩进 20px。
- 列表最高 208px，超出部分渐隐。
- 文案：
  - "Outputs / 输出内容"，空状态"Create a file or site / 创建文件或站点"；
  - "Sources / 来源"，"View all / 查看全部"；
  - "Plan / 计划"。

**底部面板**〔本机〕〔公开〕

- 高度存在 `app-shell:bottom-panel-height`：默认 280px，最小 160px，最大为主区高的 50%。
- 拖拽手柄是 row-resize，高 8px。
- 终端基于 xterm：背景等于 surface，字体跟随代码字体；顶部提示条高 24px，文字 12px；标签名"Terminal {n}"。

### 2.5 断点与可以收起的部分

| 部分 | 怎么收起 | 来源 |
|---|---|---|
| 左栏 | ⌘B 或 ⌘⇧S；宽度小于 768 时变成浮层 | 〔本机〕 |
| 侧边面板 | ⌘⌥B；窗口变窄时自动收起 | 〔本机〕 |
| 底部面板 | ⌘J | 〔本机〕 |
| 摘要卡片 | 按两侧余量自动切换浮层、左移、留白三种方式；可以固定（`app-shell-summary-panel-pinned`） | 〔本机〕 |
| 文件树 | ⌘⇧E，默认关闭（`app-shell-file-tree-open`） | 〔本机〕 |
| 全宽视图 | ⌘⇧F 进入或退出；⌘⇧B 在全宽、分屏、隐藏标签三种布局之间循环 | 〔本机〕〔公开〕 |

### 2.6 布局常量（建议 token）

建议的 token 名沿用 Tailwind v4 的命名空间（`--color-*`、`--radius-*`、`--text-*`、`--shadow-*`），布局尺寸用 `--layout-*`，最终名称由 T084/T106 确定。

| 建议 token | 值 | 对方原始变量或常量 | 来源 |
|---|---|---|---|
| `--layout-titlebar-h` | 44px | `--height-titlebar`；主进程交通灯常量 44 | 〔本机〕 |
| `--layout-toolbar-h` | 46px | `--height-toolbar` | 〔本机〕 |
| `--layout-pane-header-h` | 40px | `--height-toolbar-pane` | 〔本机〕 |
| `--layout-traffic-light-x` | 16px | 主进程常量 | 〔本机〕 |
| `--layout-left-w` | 默认 340px，范围 240–520 | localStorage `sidebar-width` | 〔本机〕 |
| `--layout-rail-w` | 约 52px | 导航栏的内边距加按钮宽 | 〔本机〕〔截图〕 |
| `--layout-row-h` | 30px | `--height-token-nav-row`（在侧边栏里） | 〔本机〕 |
| `--layout-row-radius` | 10px | `--radius-token-row` | 〔本机〕 |
| `--layout-row-px` | 8px | `--padding-row-x` | 〔本机〕 |
| `--layout-thread-max-w` | 768px（内容 736px） | `--thread-content-max-width` 48rem | 〔本机〕〔截图〕 |
| `--layout-thread-gutter` | 16px | `--thread-gutter` | 〔本机〕 |
| `--layout-item-gap` / `--layout-item-gap-grouped` | 16px / 4px | `--conversation-item-gap` / `--conversation-grouped-item-gap` | 〔本机〕 |
| `--layout-composer-bottom` | 16px | `--thread-composer-bottom-inset` | 〔本机〕 |
| `--layout-right-min-w` | 320px | 右面板常量 | 〔本机〕 |
| `--layout-chat-min-w` | 352px | 右面板最大宽度的扣除量 | 〔本机〕 |
| `--layout-summary-w` | 300px（另加 8px 间距） | 摘要卡片常量 | 〔本机〕 |
| `--layout-bottom-h` | 默认 280px，最小 160px，最大为主区高的 50% | 底部面板常量 | 〔本机〕 |
| `--layout-breakpoint-sidebar` | 768px | 媒体查询 | 〔本机〕 |
| `--layout-window-min` | 480 × 600 | 主进程 | 〔本机〕 |

## 3. 设计参数

### 3.1 主题机制

颜色不是写死的两套色板，而是由几个"种子值"推导出来的：〔本机〕

- 每个主题有亮色和暗色两组种子：accent（强调色）、ink（前景色）、surface（背景色）、contrast（对比度，0–100）、三个语义色（diffAdded、diffRemoved、skill），以及字体选项和"是否不透明窗口"。
- 渲染进程用这些种子按固定比例混色，算出约 60 个 `--app-color-*` 变量，写到 `<html>` 的行内样式上。CSS 再把它们映射成 `--color-*` 语义变量。
- 设置 → 外观可以改 accent、background、foreground、contrast，也可以从约 29 个内置主题里选：Codex、GitHub、Dracula、Nord、Solarized、Tokyo Night、Xcode、Linear、Notion 等。
- 跟随系统的切换靠 `<html data-theme>` 加上 lightningcss 的 light/dark 变量。

**Codex 默认种子**〔本机〕

| 种子 | 亮色 | 暗色 |
|---|---|---|
| accent | #339CFF | #339CFF |
| ink（前景） | #1A1C1F | #FFFFFF（默认主题下正文实际用 #DFDFDF，见下表） |
| surface（背景） | #FFFFFF | #181818 |
| contrast | 45 | 60 |
| diffAdded / diffRemoved | #00A240 / #BA2623 | #40C977 / #FA423E |
| skill（紫） | #924FF7 | #AD7BF9 |
| 半透明侧边栏 | 开 | 开 |

另一种情况：设置了 ChatGPT 强调色时，accent 换成 #3A83F7；发送按钮变成纯黑或纯白；用户气泡换成固定的灰（亮色 rgb(233 233 233 / 50%)，暗色 rgb(50 50 50 / 85%)）。〔本机〕

截图对得上 Codex 默认种子，没有走 ChatGPT 强调色这条路径：〔截图〕

- 正文色 #191C1F；
- 禁用态发送按钮 #8C8D8F，正好是 ink 的 50%；
- 用户气泡 #F3F3F4，正好是 ink 5% 叠在白底上。

所以下表按 Codex 默认种子给值。

公开截图（约 3 月）里亮色默认值是 Accent #0285FF、Foreground #0D0D0D，和本机包不一致，属于旧版本。〔公开〕

### 3.2 颜色表

下表的值由默认种子按本机包里的推导规则计算。表中"@n%"表示带透明度，括号里给出叠在 surface 上的近似实色。

| 用途 | 对方变量 | 建议 token | 亮色 | 暗色 |
|---|---|---|---|---|
| 主背景（会话区、面板） | `--app-color-background-surface` → `--color-surface` | `--color-surface` | #FFFFFF | #181818 |
| 侧边栏背景（不透明模式） | `--app-color-background-surface-under` → `--color-surface-secondary` | `--color-surface-sidebar` | #F6F6F6 | #141414 |
| 侧边栏背景（默认半透明） | macOS `vibrancy: menu` | 同上，加半透明材质 | 截图实测 #F8F8F8，导航栏约 #EEEDEB | 未实测 |
| 编辑器和终端底 | `--app-color-background-editor-opaque` → `--color-surface-tertiary` | `--color-surface-code` | #FFFFFF | #282828 |
| 卡片和面板底 | `--color-background-panel` | `--color-surface-panel` | #FFFFFF | #232323 |
| 菜单、下拉、tooltip（暗色）底 | `--color-background-control-opaque` → `--color-surface-elevated-secondary` | `--color-surface-elevated` | #FFFFFF | #2D2D2D |
| 输入框底 | 输入框布局根节点用 `--color-surface-elevated-secondary`；暗色主题下用 `--color-background-composer-surface` | `--color-surface-composer` | #FFFFFF | #2D2D2D 或 #363636【两条规则的生效条件没有完全核清】 |
| 正文 | `--app-color-text-foreground` → `--color-text` | `--color-fg` | #1A1C1F | #DFDFDF |
| 次要文字 | `--color-text-secondary`（前景 65%） | `--color-fg-secondary` | @65%（≈#6A6B6D） | @65%（≈#999999） |
| 三级文字、说明 | `--app-color-text-foreground-tertiary` → `--color-text-tertiary`、`--color-codex-description` | `--color-fg-tertiary` | ink @49.5%（≈#8E8F90） | 白 @49.8%（≈#8B8B8B） |
| 图标（主、次、三级） | `--app-color-icon-primary/secondary/tertiary` | `--color-icon` / `-secondary` / `-tertiary` | ink；@69.5%；@49.5% | 白 @90%；@71%；@51% |
| 边框 | `--app-color-border` → `--color-border` | `--color-border` | ink @7.8%（≈#EDEDEE） | 白 @8.4%（≈#2B2B2B） |
| 浅边框 | `--app-color-border-light` → `--color-border-subtle` | `--color-border-subtle` | ink @4.9%（≈#F4F4F4） | 白 @4.2%（≈#222222） |
| 重边框 | `--app-color-border-heavy` → `--color-border-strong` | `--color-border-strong` | ink @11.7%（≈#E4E4E5） | 白 @15.6%（≈#3C3C3C） |
| 焦点环 | `--app-color-border-focus` → `--color-ring` | `--color-ring` | #339CFF | #83C3FF @76% |
| 强调色文字和图标 | `--app-color-text-accent` / `--app-color-icon-accent` | `--color-accent` | #339CFF | #83C3FF |
| 强调色浅底 | `--app-color-background-accent` | `--color-accent-soft` | #E5F2FF | #0D273F |
| 次级按钮底 | `--app-color-background-button-secondary` | `--color-btn-secondary` | ink @4.9%（≈#F4F4F4） | 白 @5.2%（≈#242424） |
| 悬停底（ghost-hover，侧边栏选中行同色） | `--app-color-background-button-secondary-hover` → `--color-background-primary-ghost-hover` | `--color-hover` | ink @5.3%（截图选中行 #EBECEC） | 白 @7.8%（≈#2A2A2A） |
| 按下底 | `--app-color-background-button-tertiary-active` | `--color-active` | ink @19.6% | 白 @10% |
| 主按钮、发送按钮底 | `--color-background-primary-solid` → `--color-background-composer-primary` | `--color-btn-primary` | #1A1C1F | #DFDFDF |
| 主按钮文字 | `--color-text-primary-solid` | `--color-btn-primary-fg` | #FFFFFF | #2D2D2D |
| 禁用态发送按钮 | 主按钮色加透明度 | — | ≈#8C8D8F（截图实测） | 未实测 |
| 用户气泡底 | `--color-background-user-message`（前景 5%） | `--color-user-bubble` | ≈#F3F3F4（截图实测） | ≈#222222 |
| 输入框上方或下方的动作条底 | `--color-background-composer-action-bar` | `--color-composer-bar` | #F6F6F6（等于 surface-under） | #282828 |
| 遮罩（scrim） | `--app-color-simple-scrim` | `--color-scrim` | 黑 @9.8% | 白 @10.4% |
| 弹窗遮罩 | `--modal-backdrop-background` | `--color-backdrop` | #0000004D | #00000080 |
| tooltip | `--tooltip-background-color` / `--tooltip-text-color` | `--color-tooltip` / `-fg` | #1A1C1F 底、白字 | #2D2D2D 底、#DFDFDF 字 |
| 新增（git、diff 文字） | `--app-color-decoration-added` → `--color-codex-git-added` | `--color-diff-add` | #00A240 | #40C977 |
| 删除 | `--app-color-decoration-deleted` → `--color-codex-git-deleted` | `--color-diff-del` | #BA2623 | #FA423E |
| 修改 | `--app-color-decoration-modified` | `--color-diff-mod` | #923B0F | #FF8549 |
| 新增行底 / 删除行底 | `--app-color-editor-added` / `--app-color-editor-deleted` | `--color-diff-add-bg` / `-del-bg` | 对应色 @15% | 对应色 @23% |
| 错误色 | `--app-color-text-error`、`--app-color-accent-red` | `--color-danger` | #E02E2A | #FF6764 |
| 警告色 | `--app-color-text-warning` | `--color-warning` | #E25507 | #FF8549 |
| 成功色 | `--app-color-text-success`、`--app-color-accent-green` | `--color-success` | #00A240 | #40C977 |
| 黄色 | `--app-color-accent-yellow` | `--color-caution` | #FFC300 | #FFD240 |
| 紫色（技能） | `--app-color-accent-purple` | `--color-skill` | #924FF7 | #AD7BF9 |
| 警告浅底 / 错误浅底 | `--app-color-background-status-warning/error` | `--color-warning-soft` / `--color-danger-soft` | #FFE7D9 / #FFD9D9 | #4A2206 / #4D100E |
| 远程连接状态点 | `bg-chart-green`、`bg-codex-description`、`bg-red`，蓝色转圈 | `--color-status-*` | 绿=已连接、灰=已断开、红=出错、转圈=连接中 | 同左 |

几点说明：

- 灰阶和彩色色阶的完整表在 `app-shared-*.css` 的主题层（例如 gray-0 … gray-1000、blue-25 … blue-1000），这里只列 Codex 视图实际用到的语义值。〔本机〕
- 正文在暗色下是 #DFDFDF，不是纯白：默认主题下前景被固定成 `--gray-fixed-150`。〔本机〕
- 代码高亮的默认配色（Codex 代码主题）：〔本机〕

| 语法元素 | 亮色 | 暗色 |
|---|---|---|
| keyword | #AB4F7A | #F8A6C8 |
| string | #3A843F | #83D197 |
| literal | #AC4F23 | #F1A275 |
| variable | #643CAE | #B897F4 |
| attribute | #B8802B | #F9DC78 |
| name | #1F4E94 | #63A8F8 |
| comment | 灰 #4F4F4F | 灰 #B9B9B9 |

### 3.3 字体、字号、行高

| 项 | 值 | 来源 |
|---|---|---|
| 界面字体 | 系统字体 -apple-system（SF Pro，中文落到苹方），回退 BlinkMacSystemFont、Segoe UI | 〔本机〕〔截图〕 |
| 界面字重 | 正文 430（Electron 下的 `--font-ui-weight`），medium 500，semibold 600；控件标签 510；未读角标 590 | 〔本机〕 |
| 代码字体 | ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono | 〔本机〕 |
| 随包字体 | 包里带 OpenAI Sans（400/500/600），只给 ChatGPT 网页账号的部分界面用，Codex 视图默认不用。cupper 不需要，只记名字 | 〔本机〕 |
| 字体平滑 | 默认开（macOS 下 `-webkit-font-smoothing: antialiased`） | 〔本机〕 |
| 可调范围 | 界面字号 11–16px，默认 14；代码字号 8–24px，默认 12；正文、代码、界面三种字体都可以换 | 〔本机〕 |

字号阶梯（默认界面字号 14 时；界面字号变化时 base/sm/xs 跟着变，其余按比例缩放）〔本机〕：

| 建议 token | 对方变量 | 字号 | 行高 | 用在哪里 |
|---|---|---|---|---|
| `--text-xs` | `--text-xs` | 12px | 16px（×1.333） | 时间、徽标、辅助说明 |
| `--text-sm` | `--text-sm` | 13px | ≈18.6px（×1.429） | 按钮、菜单、tooltip、标签、设置说明 |
| `--text-base` | `--text-base`、`--font-ui-size` | 14px | 21px（×1.5） | 侧边栏行、正文控件 |
| `--text-chat` | `--codex-chat-font-size` | 14px | 22.75px（×1.625，`--leading-relaxed`） | 消息正文、用户气泡、活动行 |
| `--text-chat-sm` | `text-size-chat-sm` | 13px | — | 命令输出的标题条、次要链接 |
| `--text-code` | `--codex-chat-code-font-size`、`--font-code-size` | 12px | diff 里 ×1.8 = 21.6px | 代码块、命令输出、diff |
| `--text-lg` | `--text-lg` | 16px | ≈24.9px | 少量标题 |
| `--text-heading-sm/md/lg` | 同名 | 18 / 18（Electron 下）/ 24px | — | 页内标题 |
| `--text-heading-app` | 同名 | 28px | — | 新建会话页的大标题 |

Markdown 的排版：〔本机〕

- 正文字号 14px，行高 ×1.625；间距单位 = 字号 / 4 = 3.5px。
- 段落之间 14px（4 个单位）。
- 标题：h1 是正文的 1.5 倍，h2 是 1.25 倍，h3 是 1.125 倍；h1 上边距 14px、下边距 7px。
- 列表项缩进 5.25px，标记用 semibold。
- 小字和代码：取 max(代码字号, 正文 × 0.875)，默认约 12.25px。

### 3.4 间距

- 基本单位 4px（`--spacing: .25rem`），所有内外边距都是它的倍数。〔本机〕
- 常用值：〔本机〕

| 用途 | 值 | 对方变量 |
|---|---|---|
| 列表行左右内边距 | 8px | `--padding-row-x` |
| 列表行上下内边距 | 5px | `--padding-row-y`（Electron） |
| 面板内边距 | 20px | `--padding-panel`（Electron） |
| 工具栏内边距 | 16px | `--padding-toolbar` |
| 输入框内边距 | 左右 12px | `--padding-composer-input` |
| 输入框底栏到底边 | 8px | `--spacing-composer-footer-bottom` |
| 输入框里的按钮 | 28px，按钮间距 4px | `--spacing-token-button-composer`、`--spacing-token-button-composer-gap` |
| 菜单 | 外边距 4px；条目内边距 5px 8px | `--menu-gutter`、`--menu-item-padding` |
| 设置行 | 上下 12px、左右 16px，标签和控件间距 24px；紧凑版上下 8px | 设置行组件 |
| 设置分节间距 | 40px | 设置页 |

### 3.5 圆角

Chromium 139 以上支持 `corner-shape`，Codex 在这种情况下把圆角统一放大 1.25 倍，并改用 superellipse(1.5) 形状（更接近 macOS 的连续圆角）。本机的 Chromium 是 155，所以实际值是放大后的值。〔本机〕

| 建议 token | 对方变量 | 基准值 | 实际值（×1.25） | 用在哪里 |
|---|---|---|---|---|
| `--radius-xs` | `--radius-xs` | 4px | 5px | 小徽标 |
| `--radius-sm` | `--radius-sm` | 6px | 7.5px | — |
| `--radius-md` | `--radius-md` | 8px | 10px | 小按钮、文件树行 |
| `--radius-lg` | `--radius-lg` | 10px | 12.5px | 工具栏按钮、标签、命令输出框、diff 卡片 |
| `--radius-xl` | `--radius-xl` | 12px | 15px | 菜单、菜单条目、弹窗、popover |
| `--radius-2xl` | `--radius-2xl` | 16px | 20px | 摘要卡片、tooltip |
| `--radius-3xl` / `--radius-4xl` | 同名 | 20 / 24px | 25 / 30px | — |
| `--radius-row` | `--radius-token-row` | 10px | 10px（不放大） | 侧边栏和设置导航的行 |
| `--radius-composer` | `--composer-radius` | 22px | 22px（按间距单位算，不放大） | 输入框 |
| `--radius-bubble` | 用户气泡里重设的 `--radius-2xl` | 22px | 22px | 用户消息气泡 |
| `--radius-full` | `--radius-full` | 9999px | — | 发送按钮、权限按钮、悬浮药丸、回到底部按钮 |

### 3.6 阴影与边框

| 建议 token | 对方变量 | 值 | 来源 |
|---|---|---|---|
| `--shadow-composer` | `--elevation-composer` | 0 0 0 1px 黑 4%；0 2px 8px 黑 4%；0 4px 80px 8px 黑 2.4% | 〔本机〕 |
| `--shadow-composer-dark` | `--elevation-composer-dark` | 内阴影 0 0 1px 白 20% | 〔本机〕 |
| `--shadow-prominent` | `--elevation-prominent` | 0.5px 的 border-strong 描边；0 3px 7.5px 黑 4%；0 0 20px 黑 5%。Electron 里输入框改放在下方时用这一档 | 〔本机〕 |
| `--shadow-sidebar` | `--elevation-sidebar` | 0.5px 描边；0 3px 7.5px 黑 3%；0 0 16px 黑 2% | 〔本机〕 |
| `--shadow-card` | `--shadow-card` | 0 4px 16px 黑 5% | 〔本机〕 |
| `--shadow-menu` | `--shadow-xl-spread` | 0.5px 的 border 描边，加 0 8px 16px −4px 黑 12% | 〔本机〕 |
| `--shadow-100…400` | `--shadow-100…400` | 几何 0 1px 2px −1px / 0 2px 4px −1px / 0 4px 8px −2px / 0 8px 16px −4px；亮色透明度 .08/.08/.10/.12，暗色 .20/.20/.36/.30 | 〔本机〕 |
| `--shadow-hairline` | `--shadow-hairline` | 1px 描边，亮色黑 8%，暗色白 10% | 〔本机〕 |
| `--shadow-tooltip` | `--tooltip-box-shadow` | 0 8px 18px，#0F172A 20% | 〔本机〕 |
| 边框宽度 | — | 常规 1px；菜单和 popover 的描边环 0.5px（`--border-width-hairline`） | 〔本机〕 |

截图上的印证：〔截图〕

- 输入框外沿有一圈约 1px 的 #F0F0F0 描边，下方有约 15px 的浅阴影（#F6F6F6 渐变到 #FAFAFA）；
- 摘要卡片的边线是 #DADADB，左侧有约 12px 的阴影渐变。

### 3.7 动画

| 建议 token | 对方变量 | 值 | 来源 |
|---|---|---|---|
| `--duration-fast` | `--transition-duration-basic`、`--default-transition-duration` | 150ms | 〔本机〕 |
| `--duration-moderate` | `--transition-duration-moderate` | 200ms | 〔本机〕 |
| `--duration-relaxed` | `--transition-duration-relaxed` | 300ms（展开箭头旋转、hover 才出现的按钮） | 〔本机〕 |
| `--ease-default` | `--default-transition-timing-function` | cubic-bezier(.4, 0, .2, 1) | 〔本机〕 |
| `--ease-enter` | `--cubic-enter` | cubic-bezier(.19, 1, .22, 1) | 〔本机〕 |
| `--ease-exit` | `--cubic-exit` | cubic-bezier(.8, 0, .4, 1) | 〔本机〕 |
| `--ease-move` | `--cubic-move` | cubic-bezier(.65, 0, .35, 1) | 〔本机〕 |
| `--ease-snappy` | `--ease-enter-snappy` | cubic-bezier(.23, 1, .32, 1) | 〔本机〕 |
| 面板开合 | — | spring，0.5s，bounce 0.1 | 〔本机〕 |
| 流式文字淡入 | `--animation-duration-streaming-text` | 0.7s | 〔本机〕 |
| "进行中"文字的闪光 | `loading-shimmer` | 2s 一次，从左扫到右 | 〔本机〕 |
| 转圈 | — | 1.25s 一圈 | 〔本机〕 |
| 脉冲点 | `--animate-token-pulsing-dot` | 1.25s，缩放到 1.25 再回来 | 〔本机〕 |
| 提问选项入场 | — | 逐项淡入，spring 0.3s，相邻两项间隔 0.05s | 〔本机〕 |

系统开启"减少动态效果"时，闪光、转圈、宽度动画全部关闭。设置里也有 Reduce motion（Off / On / System）。〔本机〕

## 4. 组件清单

文案先写英文原文，再写本机的中文译文。

### 4.1 侧边栏

**结构**（从上到下）〔本机〕〔截图〕

1. **头部**（高 32px）：
   - "Codex ⌄"：产品模式切换，选项是 ChatGPT / Codex / Home，Codex 的描述是"Build, debug, and ship"；
   - 铃铛：Activity，"View activity / 查看活动"；
   - 搜索："Search / 搜索"。
2. **New chat / 新聊天**：带编辑图标的一行。
3. **Pinned / 置顶**分组：组标题是 13px、tertiary 色的小字。
4. **项目分组**：
   - 项目行：文件夹图标加项目名；worktree 显示特殊图标，tooltip 是"This is a git worktree"；
   - 会话行：缩进挂在项目下，没有图标，单行截断；
   - 每个项目默认显示若干条，其余用"Show more / 展开显示"展开，"Show less / 收起"收起。
5. **Chats / 聊天**分组：不属于任何项目的会话。
6. 底部：用量提醒卡片（"{remaining}% usage remaining"）、更新横幅等，按需出现。

**组织与排序**〔本机〕

- Organize sidebar：By project / In one list / By connection。
- Sort chats by：Created / Last updated / Manual order。
- 可以自建分区（"New section / 新建分区"），把会话和项目拖进去。

**会话行**〔本机〕〔公开〕

- 高 30px，选中时用 ghost-hover 背景。
- 状态：
  - 运行中：行首灰色转圈；
  - 未读：蓝色实心点，无障碍描述"Unread"；
  - 等待审批："Awaiting approval / 等待批准"；
  - 等待输入："Needs input / 需要用户输入"；
  - 出错："Chat encountered a system error"。
- 悬停时：行首图标淡出，换成操作按钮；行尾出现菜单按钮（20px 宽）。
- 右键菜单：
  - Pin chat / 置顶聊天，Unpin chat；
  - Rename chat / 重命名聊天（弹窗，副标题"Keep it short and recognizable"，占位"Add a title…"）；
  - Mark as read / unread；
  - Archive chat / 归档聊天；
  - Permanently delete / 永久删除（确认框"Delete chat?"）；
  - Change emoji。
- 行尾：相对时间（如"4d"）和 diff 统计（"+1 −9"）。这一条来自合并前的公开截图，本机包里没有核实。〔公开〕

**项目行**〔本机〕

- 菜单：
  - Pin / Unpin；
  - Edit project；
  - Reveal in Finder / 在 Finder 中显示；
  - Start new chat in {folder}；
  - Archive chats；
  - Mark all as read；
  - Remove project（确认框说明只是从 App 里移除，不删文件）。
- 悬停卡片：显示"{n} chats · {n} active · {n} unread · {n} waiting"。

**Activity 视图**〔本机〕〔公开〕

- 只列未读、运行中、等待回复的会话。
- 空状态"Nothing needs attention / 暂无需要关注的内容"。
- 过滤项：Chat / Work / Pinned / Scheduled。
- 可以"Mark all as read"、"Clear read chats"。

**远程连接分组**（按连接组织时）〔本机〕

- 每个连接一个状态点：8px 圆点，绿=已连接，灰=已断开，红=出错，连接中显示蓝色转圈。
- 这套样式可以直接用在 cupper 的"宿主状态"上。

**归档页**〔本机〕

- "Archive / 归档"，可以搜索、按项目和类型过滤。
- 空状态"No archived chats / 暂无已归档的聊天"。

### 4.2 新建会话页

**结构**〔本机〕〔公开〕

- 页面中部是大标题，下面紧跟输入框。输入框锚在视口高度的 42% 处。
- 大标题文案：
  - 有项目时："What should we build in {project}? / 你想让我们在 {project} 中构建什么？"，项目名本身就是项目选择器；
  - 没有项目时："What should we build? / 我们要构建什么？"；
  - 另一套措辞："Let's build / 开始构建"。
- 输入框占位："Ask Codex anything. Type @ to use plugins or mention files / 可向 Codex 询问任何问题。输入 @ 使用插件或提及文件"（本机中文）。合并前的公开截图是"Ask Codex anything, @ to add files, / for commands"。
- 输入框下方（或上方）一条动作条：
  - 运行位置"Work in / 工作位置"：This computer / Cloud / Remote；
  - "Worktree / 工作树"开关；
  - Environment / 环境；
  - 分支选择器"Switch branch"；
  - 项目选择器"Choose project / 选择项目"，可以选"Don't work in a project"。
- 动作条背景是 composer action bar 色，亮色和侧边栏同色。
- 标题下方还有首页建议卡片（"Homepage suggestions"，设置里可以关）。

### 4.3 消息流

**用户消息**〔本机〕〔截图〕

- 靠右的气泡，最大宽为内容列的 70%。
- 尺寸：内边距左右 16px、上下 10px；圆角 22px。
- 颜色：前景 5% 的底色，正文色文字。
- 长消息按行数折叠，底部渐隐（下边 20% 渐隐到透明），下面是"Show more / 显示更多"，展开后变成"Show less / 收起"。
- 悬停时，气泡下方出现操作按钮：Copy message、Edit message。
- 编辑时变成文本框，按钮是"Cancel"和"Send"。
- 特殊标记：
  - "Sent as goal / 设为目标"；
  - "Review mode / 审查模式"；
  - "Hook blocked this message / Hook 已阻止此消息"；
  - "{n} comments"（带审查评论时）。

**思考**（推理摘要）〔本机〕

- 运行中："Thinking / 正在思考"，文字带闪光动画。
- 结束后："Thought for {elapsed} / 已思考 {elapsed}"，可以折叠。

**工具活动的单行摘要**〔本机〕〔截图〕

- 每行：
  - 左侧是 16px 图标；
  - 后面是一句过去时摘要，13–14px，description 色，透明度 80%，单行截断；
  - 动词部分可以单独着色（文案里用 `<verb>` 标出）。
- 常见文案（英文 / 中文）：
  - `Read {path}` / `已读取 {path}`
  - `Listed files in {path}` / `已列出 {path} 中的文件`
  - `Searched for {query} in {path}` / `已在 {path} 中搜索“{query}”`
  - `Ran {command}` / `已运行 {command}`；带用时的是 `Ran {command} in {elapsed}`
  - `Edited {filename}` / `已编辑 {filename}`
  - `Searched the web` / `已搜索网页`
  - 被中断：`Stopped {command} after {elapsed}`
  - 被自动审查拒绝：`Command declined by auto-review`
- 运行中的写法：`Running {command}` / `正在运行 {command}`，`Reading`，`Searching for …`，带闪光动画。
- **分组**：
  - 连续的同类操作合并成一行汇总，例如 `Edited files` + `read files` + `ran commands`（中文"编辑了文件读取文件运行了命令"）；
  - 末尾带一个展开箭头（14px，悬停时才显示，旋转动画 300ms）；
  - 展开后的列表最高 224px，超出部分上下渐隐（渐隐距离 24px）。
- 同一组内的行间距 4px，组与组、组与正文之间 16px。

**命令输出**〔本机〕

- 展开某条命令后看到一个带边框的块：1px border，圆角 `--radius-lg`。
- 标题条：底色是半透明的 hover 色，内边距左右 10px、上下 2px，文字 13px description 色，有分隔线。
- 正文：surface-tertiary 底色，等宽字体，最高约 240px，可以滚动。

**回合折叠与用时**〔本机〕〔截图〕〔公开〕

- 回合结束后，过程收成一行：
  - 文案"Worked for {time} / 用时 {time}"，带展开箭头；
  - 箭头 14px，收起时朝右，展开时转 90°，动画 150ms；
  - 这一行是 13–14px 的 secondary 色文字，下面紧跟一条 1px 的边线（上方留 4px）。
- 运行中写"Working for {time} / 已处理 {time}"。
- 没有用时的旧会话写"{n} previous messages"。
- 有被自动审查拒绝的动作时，旁边带一个提示："Auto-review denied # action…"。
- 用户中途停止时显示"You stopped after {time} / 你在 {time} 后停止了"。
- 截图里"用时 8m 31s"是左对齐的；合并前的 Windows 公开截图是居中、两侧带横线的样式，以本机包为准。

**最终回答**〔本机〕

- 不加气泡，Markdown 排版，见第 3.3 节。
- 回答末尾可以带"产物卡片"：48px 圆角图标块，标题 14px medium，副标题 13px，点击打开。
- 本回合有文件改动时，显示改动卡片：
  - 标题行是"{n} files changed +a −b"，带 Undo；
  - 每个文件一行；
  - 卡片圆角 `--radius-lg`，悬停时变色。
  - 卡片的布局细节来自公开截图，本机包里只核实了卡片存在和圆角。

**计划与待办**〔本机〕〔公开〕

- 运行中的计划以药丸形式浮在输入框上方，文案"Step {n} / {total} / 第 n / m 步"。
- 计划内容：编号步骤，每步一个 16px 状态图标（未开始、进行中、完成）。
- 计划摘要条：
  - 标题"Plan / 计划"，生成中是"Writing plan / 编写计划"；
  - 可以展开或收起、在侧边面板打开、下载。
- 计划模式结束时问"Implement this plan? / 实施此计划？"，选项"Yes, implement this plan / 是，实施此计划"。

**错误与提示**〔本机〕

| 情况 | 文案 |
|---|---|
| 流断开重连 | "Reconnecting {n}/{max} / 正在重新连接"；服务端繁忙时是"Server is busy, reconnecting" |
| 服务端过载 | 按钮"Retry"，倒计时"Retry in {s}s" |
| 渲染失败 | "This turn couldn't render / 此轮内容无法显示"，按钮"Try again" |
| 用量用尽 | "You've hit your usage limit. … try again at {resetDate}." |
| 模型切换 | "Model changed from {from} to {to}."，并提示"Changing models mid-conversation will degrade performance." |
| 警告框 | 圆角 `--radius-lg`，1px 警告色 30% 透明度的边框，内边距左右 12px、上下 8px，最大宽 77% |

**回到底部**〔截图〕〔本机〕

- 圆形按钮，约 28pt，浮在输入框上方居中，带向下箭头。
- 无障碍文案"Scroll to bottom"。

### 4.4 审批与提问卡片

本机包里，审批和提问不是插在消息流中间的卡片，而是**替换输入框位置**的请求面板。外观和输入框一致（同样的圆角和阴影），处理完后输入框恢复。〔本机〕依据是组件带"输入框外观"参数，按钮用输入框尺寸，导航用的是输入框的键盘导航；实际渲染位置没有实机确认。

公开截图（合并前）里是消息流中的卡片：细边框、圆角，底部左侧有"Always allow"复选框，右侧是"Cancel Esc"和黑色胶囊"Allow ↵"。〔公开〕

**命令审批**（文案）〔本机〕

- 问句："Allow ChatGPT to run this command? / 允许 ChatGPT 运行此命令？"；指明执行者时是"Do you want {actor} to run this command?"。
- 标签"Terminal / 终端"，命令全文可以展开或收起（"Expand / Collapse"）。
- 选项：
  - Allow once / 允许一次
  - Allow this conversation / 允许此对话
  - Always allow / 始终允许
  - Allow similar commands / 允许类似命令（说明是"Allow commands that start with {command} for this conversation"）
  - Deny / 拒绝

**文件编辑审批**〔本机〕

- 问句："Allow ChatGPT to edit the following files?"。
- 文件列表，移动显示为"{source} → {target}"。
- 选项"Allow all edits / 允许所有修改"，说明是"Allow this and future file edits in this conversation without asking again"。

**网络与权限审批**〔本机〕

- 网络："Allow ChatGPT to connect to {destination}?"，原因"{host} isn't on the current network allowlist"。
- 权限：
  - 问句"Allow ChatGPT to view/edit the contents of {paths}?"；
  - 标签"Permissions / 权限"、"Internet access / 互联网访问"。

**提问**（agent 向用户提问，也用于选项式审批）〔本机〕

- 选项列表：
  - 多于一个选项时，每行前面显示编号 1、2、3…；
  - 行内：标题 13px medium，说明 13px description 色；
  - 推荐项带"Recommended / 推荐"徽标；
  - 选中行右侧显示上、下箭头的键盘提示；
  - 多选时改成复选框。
- 最后一项是自由输入："No, and tell ChatGPT what to do differently / 否，并告诉 ChatGPT 应该如何做得不同"，占位"Type here / 在此输入"。
- 底部按钮：
  - "Skip / 跳过"
  - "Submit / 提交"，旁边带 Enter 键帽
  - 多题时有"Next question"、"Previous question"
- 右上角"Dismiss"，带"ESC"键帽。有倒计时的请求显示"Dismiss, {n} seconds remaining"。
- 消息流里留下的记录："Asked {n} questions / 已询问 n 个问题"，没答的写"No answer provided"。

**键盘**〔本机〕〔公开〕

- Enter 批准，Esc 拒绝，命令名是 `approval.approve` 和 `approval.decline`。
- 合并前的发布说明里，⌘Enter 提交自定义反馈。

**自动审查（"Approve for me"）**〔本机〕

- 消息流里显示一行状态：
  - "Auto-reviewing / 自动审核中"
  - "Auto-review approved / 自动审核已批准"
  - "Auto-review denied / 自动审核已拒绝"
  - "Auto-review timed out / 自动审核超时"
- 可以展开看理由（"Why it was denied"），也可以手动批准（"Approve"）。
- 频繁审批时会弹出提示："Want fewer approval prompts?"，按钮是"Approve for me"和"Keep manual approvals"。

**通知侧**：审批还会发系统通知，带 Approve、Approve for session、Decline 按钮，见第 4.8 节。〔本机〕

### 4.5 输入框

**外形**〔本机〕〔截图〕

- 圆角 22px，背景见第 3.2 节；亮色用 `--shadow-composer`，暗色用内描边。
- 截图实测：宽 736.5pt；空输入框高约 99pt（含一行文字区和底栏）。
- 文字区：
  - 最小高 40px，最大高 min(25dvh, …)，超出后滚动；
  - 行高 = 聊天字号 × 1.625；左右内边距 12px；
  - 占位文字透明度 50%。
- 已有会话的占位："Work with Codex / 和 Codex 一起工作"（截图里是旧译"与 Codex 协作"）。运行中是"Working… / 处理中…"。

**底栏**（文字区下方一行）〔本机〕〔截图〕

- 按钮尺寸 28px，间距 4px，文字 13px，行高 18px。
- 左侧：
  - "+"：添加附件（图片、文件），也可以拖放，拖放时显示 drop overlay；
  - 权限按钮：图标加短名，下拉标题"How should ChatGPT actions be approved? / 应如何批准 Codex 操作？"。选项如下表。
- 右侧：
  - 环形指示：上下文用量。悬停显示"Context: {remaining}% left ({used} used / {total})"和额度信息；需要在设置里打开"Show context window usage"；
  - 模型和推理强度合在一个按钮里："{模型} {强度} ⌄"，截图里是模型名后跟"极高"；
  - 发送按钮：28px 圆形，主按钮色，白色向上箭头；输入为空时是 50% 透明的禁用态；运行中变成停止按钮（实心方块）。

权限选项：〔本机〕

| 选项 | 说明 |
|---|---|
| Ask for approval / 请求批准 | Always ask to edit external files and use the internet |
| Approve for me / 帮我批准 | Only ask for actions detected as potentially unsafe |
| Full access / 完全访问权限 | Unrestricted access to the internet and any file on your computer。打开后输入框上方出现警告"Full access is on" |
| Custom (config.toml) / 自定义 | Uses permissions defined in config.toml |

模型选择器：〔本机〕

- 标题"Select model / 选择模型"，另有"Select effort / 选择推理强度"；
- Fast 模式开关（"{n}x speed, more usage"）；
- "Reset to default"。

**输入框上方或下方的动作条**（`composer-utility-bar`）〔本机〕

- 内容：运行位置、项目、分支（"Switch branch"，未提交改动时显示"Uncommitted: {n} files"）、环境。
- 高度由行内边距决定；底色是 composer action bar 色。
- 新建会话页一定显示；已有会话按需显示。截图里看不到它。

**@ 提及**〔本机〕〔公开〕

- 输入 `@` 弹出列表，分标签页：文件、插件、技能等。
- 提示"按 Tab 查看更多"。
- 选中的引用在输入框里以 accent 色显示。
- `$` 用来调用技能。

**斜杠命令**〔本机〕〔公开〕

- 输入 `/` 弹出菜单，标题"Slash commands / 斜杠命令"，有搜索框，空结果"No commands"；技能单独成组"Skills"。
- 菜单最高 320px 或 10 行。
- 官方命令：`/approve /archive /chat /cloud /cloud-environment /compact /fast /feedback /fork /goal /ide-context /init /local /mcp /memories /model /new /pet /pin /plan /project /reasoning /rename /review /side /status /usage /worktree`

**排队与 Steer**〔本机〕〔公开〕

- 跟进行为在设置里选，默认 **Steer**：运行中按 Enter，消息直接插进当前回合，不打断模型；用反向键（默认 ⌘Enter）则排队。
- 排队的消息显示在输入框上方，每条一行，可以：
  - 预览；
  - 编辑；
  - 删除："Delete queued message"；
  - 立即插入："Steer / 引导"，tooltip"Submit without interrupting the model"。
- 用户中断回合后队列暂停："Queue paused because you interrupted"，按钮"Resume / 继续"。
- 队列暂停时又发新消息，会弹框"Send message?"，可以选择清空队列。

**其他**〔本机〕

- 输入框里可以开关规划模式；光标在开头时按 Backspace 退出规划模式。
- Esc 的处理顺序：
  1. 先关闭听写、语音等浮层；
  2. 运行中按第一次进入"确认停止"状态，再按一次才停止回合；
  3. 焦点不在输入框时，Esc 把焦点移回输入框。
- 输入框预测（Composer predictions）：按 Tab 接受下一条消息的预测，2026-10-09 起为 beta。〔公开〕

### 4.6 审查面板（"Changes / 变更"标签）

**头部**〔本机〕

- 左侧：
  - 仓库下拉：只在多于一个仓库时出现；
  - 范围下拉"Review source / 查阅来源"，选项分三组：

    | 组 | 选项 |
    |---|---|
    | ① | Last turn / 最近一轮 |
    | ② | This branch / 此分支；Uncommitted / 未提交；Unstaged / 未暂存；Staged / 已暂存 |
    | ③ | Commits / 提交 |

  - 增删统计："+N"用新增色，"−N"用删除色。
- 右侧：
  - "Show files / Hide files"：文件树开关，28px 全圆按钮；
  - 选项菜单，内容如下。

选项菜单：〔本机〕

- 视图：Word wrap；diff 视图 Split / Unified / Auto（Auto 在同时有增删时用分栏，窄面板或只有增或删时用统一视图）。
- 展开收起：Expand all / Collapse all。
- 内容：
  - Load full files（默认开）；
  - Rich preview；
  - Word diffs；
  - Hide white space；
  - Hide imports。
- 其他：Copy git apply command；Jump to file；Refresh。

**文件树**〔本机〕

- 行高 28px，文字 13px，目录吸顶，超长文件名在中间截断。
- Git 状态用 20px 的图标遮罩，不用字母；颜色取新增、删除、修改三色。
- 选中背景 primary-soft-active。
- 顶部过滤"Filter files… / 筛选文件…"，另有"Filter generated files"。
- 右键菜单：Copy path、Open in {app}、Add to chat。

**diff**〔本机〕〔公开〕

- 用的是 @pierre/diffs。
- 字体和配色：等宽，12px，行高 21.6px，行号列最小 4ch；新增和删除行的底色见第 3.2 节；行号列底色是对应色与背景混合（亮色 91% 背景，暗色 85%）。
- 文件头：吸顶，左右内边距 16px，路径从左侧截断，保留文件名；文件操作按钮悬停时才显示。
- 未改动的行折叠成一条分隔条（圆角 8px），带向上、向下展开的按钮。
- 文件菜单：
  - Copy path、Open in editor；
  - Collapse / Expand file；
  - Mark as viewed / unviewed；
  - Open file in a tab。
- 特殊状态：
  - "Binary file not shown"
  - "Diff too large to display"
  - 大 diff 时"This diff is large, showing one file at a time"，配 Next file / Previous file
  - "File has merge conflicts"
- 加载中显示骨架条，每行 22px，带 3s 的闪光动画。
- 范围下拉旁有暂存数（公开截图里是"Unstaged 4 ▾"）。〔公开〕

**hunk 和文件操作**〔本机〕

- 都是只有图标的按钮，ghost 色，28px 全圆，tooltip 写操作名：
  - hunk：Stage / Unstage / Revert；
  - 文件：Stage file / Unstage file / Revert file。
- 整组操作是一条浮在面板底部居中、距底 20px 的药丸（全圆角，elevated 底色，`shadow-lg`，0.5px 描边环），里面是"Revert all / 还原全部"和"Stage all / 暂存全部"（或"Unstage all"）。Staged 视图里没有 Revert all。
- 还原确认框：
  - 标题"Revert changes? / 是否还原更改？"
  - 正文"This action removes all of these changes."
  - 复选框"Don't ask again"
  - 按钮 Cancel；Confirm（危险色，默认获得焦点）
- 结果提示：
  - 暂存："Staged successfully"
  - 还原："Reverted {path}"；hunk 级是"Reverted hunk {n} in {path}"

**行内评论**〔本机〕〔公开〕

- 悬停行号出现"+"，点开评论框：
  - 占位"Add a comment… / 添加一条评论…"，按钮 Comment / Cancel；
  - 评论框最大宽 768px，圆角 `--radius-lg`，背景 surface-secondary 92%，获得焦点时边框变成 secondary 色。
- **没有单独的"发给 agent"按钮**：评论作为附件进入输入框，随下一条消息一起发出；附件标签"{annotations} · # selected items"，发出后在用户消息里显示"{n} comments"。
- agent 回复中的审查意见以"Codex"的身份显示在 diff 里，可以"Dismiss"（从 diff 里隐藏，聊天记录里仍保留）。

**提交、推送、PR**〔本机〕〔公开〕

- 入口是"Git actions / Git 操作"菜单：
  - Commit / Push / Create branch / Switch branch / Create pull request；
  - 流程进行中，菜单只显示当前阶段和 Cancel。
- 提交弹窗：
  - 宽 420px；
  - 标题"Commit or push / 提交或推送"；
  - "Commit to {branch}"，可以选"New branch"；
  - 提交信息框高 80px，占位"Commit message (leave blank to generate)…"；
  - 复选框"Include unstaged changes"；
  - 下一步三选一：Commit / Commit and push / Push。
- 创建 PR 弹窗：
  - 标题"Create PR"；
  - 字段 Title、Description（留空自动生成）；
  - 复选框"Commit and push local changes"；
  - 按钮 Create PR / Create draft PR。
  - 缺 `gh` 或没登录时给出对应提示，例如"Install GitHub CLI (gh) to create PRs"。

**空状态**〔本机〕

| 情况 | 文案 |
|---|---|
| 没有改动 | "No file changes yet / 尚无文件更改"，副标题"Changes in this project will appear here." |
| 没有未暂存的改动 | "No unstaged changes"，副标题"Code changes will appear here" |
| 没有已暂存的改动 | "No staged changes"，副标题"Accept edits to stage them" |
| 不是 Git 仓库 | "Create a Git repository"，副标题"Track, review, and undo changes in this project."，按钮"Create git repository" |
| 加载失败 | "Couldn't load changes"，按钮 Retry |

### 4.7 设置页

**外壳**〔本机〕

- 设置页占满主区。左侧导航沿用侧边栏样式：行高 30px，圆角 10px；顶部是"Back to app / 返回应用"。
- 内容区最大宽 768px，最小宽 min(100%, 320px)，水平居中；分节间距 40px。
- 设置行：左侧是标签（medium）加说明，右侧是控件。尺寸见第 3.4 节。
- 开关尺寸：32 × 19px，滑块 13px，开启时是强调色。〔本机〕

**导航**（Codex 相关部分，按显示顺序）〔本机〕〔公开〕

- 个人：General、Notifications、Appearance、Personalization、Keyboard shortcuts、Archived chats、Usage 等。
- 集成：Plugins、MCP servers、Skills、Browser、Computer use 等。
- 编码：Hooks、Connections、Git、Environments、Worktrees、Code review、Cloud 相关。
- 实际显示哪些分节，由账号和功能开关决定。

**各页的主要行**〔本机〕

| 分节 | 主要行（括号里是默认值） |
|---|---|
| General → Permissions | Default permissions；Show Full access in the composer（打开前有确认框） |
| General → General | Default file open destination；Integrated terminal shell；Language（Auto detect）；Confirm before closing a window；Bottom panel；Default terminal location（Bottom / Right）；Prevent sleep while running（关）；Automatically archive inactive chats；Launch at login；Show in menu bar |
| General → Composer | Show predictions（开）；Plain text composer（关）；Show context window usage（关）；Send shortcut（Enter / ⌘+Enter always / ⌘+Enter for multiline prompts）；Follow-up behavior（Queue / Steer 分段控件，默认 Steer）；Show educational tips |
| Appearance | Theme（Light / Dark / System，默认 System）；亮色和暗色各一块主题设置：主题下拉、Accent、Background、Foreground（色块加 hex）、Contrast 滑杆、UI font / Code font、Translucent sidebar；Import theme、Reset to default；UI font size（14px）；Code font size（12px）；Font smoothing；Use pointer cursors（默认关）；Reduce motion；Diff markers（Color / +/−） |
| Notifications | 系统通知关闭时显示横幅"Notifications are off"和按钮"Open System Settings"；Turn completion notifications（Never / Only when unfocused / Always，默认 Only when unfocused）；Enable permission notifications（开）；Enable question notifications（开）；Notification sound（Default / Classic / 系统声音 / None / 自定义） |
| Git | Branch prefix（`codex/`）；Always force push（关，用 `--force-with-lease`）；Create draft pull requests（开）；Pull request merge method；Commit instructions / Pull request instructions；Disable Git-based diffs；Review delivery（Inline / Detached） |
| Worktrees | Worktree root；Automatically delete old worktrees（开，上限 15）；worktree 列表每行可以新建会话或删除 |
| Archived chats | 列表、取消归档、Delete all |
| Keyboard shortcuts | 搜索框（"Search shortcuts"，可以按按键搜索）；按 App / Composer / Chat 分节；点组合键后直接按新键录入，冲突时提示"Already used by {shortcut}"；可以单项恢复，页底有"Reset all to defaults" |

### 4.8 通知

**系统通知**〔本机〕〔公开〕

| 种类 | 标题 | 正文 | 交互 | 什么时候不发 |
|---|---|---|---|---|
| 回合完成 | 会话标题，没有时用"Turn complete" | 最后一条 agent 消息，没有时用"ChatGPT finished a turn." | 可以在通知里直接回复（占位"Reply to ChatGPT"），点击跳到会话 | 设置为 Never；设置为 Only when unfocused 且窗口在前台；该会话正在显示且窗口有焦点 |
| 审批 | Command approval / File edit approval / Permission approval | 请求理由，没有时用"Approval required" | 按钮 Approve、Approve for session、Decline | 关掉 permission notifications |
| 提问 | 会话标题 | "Answer a question to proceed." | 点击跳到会话 | 关掉 question notifications |

- 没有单独的"出错"通知，失败按回合完成发出。同一条通知按 id 去重。〔本机〕
- 提示音：Default 和 Classic 是包里的两个 wav，其他选项用 macOS 系统声音，也可以导入自定义音频（MP3/WAV/FLAC/Ogg，不超过 30 秒、10MB）。〔本机〕

**应用内 toast**〔本机〕

- 顶部居中，最多同时 3 条，间距 8px，可以向右滑动关闭。
- 顶部留 40px，宽度不超过输入框旁的宽度（790px）。
- z-index 60。

**未读和角标**〔本机〕

- Dock 角标数 = 未读会话数 + 未读的自动化运行数。
- 未读可以手动设置（⌘⇧U），也可以一键清除（⇧Esc，有确认框）。

## 5. 快捷键表（macOS 默认）

来源：本机命令注册表〔本机〕，与官方快捷键表〔公开〕逐项对过，一致。只列和 Codex 视图有关的。

| 动作 | 英文 | 快捷键 |
|---|---|---|
| 新会话 | New chat | ⌘N，或 ⌘⇧O |
| 不属于项目的新会话 | New standalone chat | ⌘⌥O |
| 命令菜单 | Open command menu | ⌘K，或 ⌘⇧P |
| 搜索文件 | Search Files… | ⌘P |
| 设置 | Settings… | ⌘, |
| 快捷键一览 | Keyboard Shortcuts | ⌘/ |
| 切换左栏 | Toggle sidebar | ⌘B，或 ⌘⇧S（光标在输入框里时 ⌘B 是加粗，只有光标在开头才切换） |
| 切换侧边面板 | Toggle Changes panel | ⌘⌥B |
| 打开审查标签 | Open Changes tab | ⌃⇧G |
| 切换文件树 | Toggle navigation panel | ⌘⇧E |
| 切换底部面板 | Toggle bottom panel | ⌘J |
| 打开终端 | Open terminal | ⌃` |
| 浏览器标签 | Open browser tab | ⌘T |
| 关闭标签 / 关闭其他标签 / 重开 | Close tab / Close other tabs / Reopen closed tab | ⌘W / ⌘⌥W / ⌘⇧T |
| 全宽视图 / 循环切换布局 | Full view / Cycle workspace layout | ⌘⇧F / ⌘⇧B |
| 上一个 / 下一个标签 | Previous / Next tab | ⌘⇧[ / ⌘⇧] |
| 重命名会话 | Rename chat | ⌘⌥R |
| 归档会话 | Archive chat | ⌘⇧A |
| 置顶 / 取消置顶 | Toggle pin | ⌘⌥P |
| 标为未读 | Mark as unread | ⌘⇧U |
| 清除全部未读 | Clear all unreads | ⇧Esc |
| 下一个需要处理的会话 | Next chat needing attention | ⌘⌥A |
| Activity 视图 | Toggle activity view | ⌘⌥U |
| 侧边栏上一个 / 下一个 | Previous / Next sidebar item | ⌘⌥[ 或 ⌘⌥← / ⌘⌥] 或 ⌘⌥→ |
| 最近会话之间切换 | Previous / Next recent chat | ⌃⇧Tab / ⌃Tab |
| 最近会话 1–6 | Go to recent chat 1–6 | ⌘⌥1 … ⌘⌥6 |
| 数字直达 | — | 默认 ⌘1–9 切标签，⌃1–9 切侧边栏会话；设置里可以对调 |
| 后退 / 前进 | Back / Forward | ⌘[ / ⌘] |
| 会话内查找 | Find | ⌘F；下一个 ⌘G，上一个 ⌘⇧G |
| 复制工作目录 / 深链接 / 会话路径 | Copy … | ⌘⇧C / ⌘⌥L / ⌘⌥⇧C |
| 复制最后一个代码块 | Copy last code block | ⌘⇧; |
| 侧边会话 | Open side chat | ⌘⌥S |
| 模型和推理强度 | Select effort | ⌃⇧M |
| 最近用过的模型 | Open recent models | ⌃⌘M |
| 项目选择器 | Open project picker | ⌘⌥⇧O |
| 发送 | — | Enter（默认）；⇧Enter 或 ⌥Enter 换行 |
| 后台发送 / 运行中排队 | Send in background / Queue | ⌘Enter（Send shortcut 设为 ⌘Enter 时，排队变成 ⌘⇧Enter） |
| 停止回合 | — | 运行中按两次 Esc（第一次进入确认状态） |
| 批准 / 拒绝 | Approve / Decline request | Enter / Esc |
| 撤销 / 重做（App 操作） | Undo / Redo | ⌘Z / ⌘⇧Z |
| 环境动作 1 | Environment action 1 | ⌘⇧D |
| 缩放 | Zoom in / out / reset | ⌘+ / ⌘− / ⌘0 |
| 全屏 | Toggle full screen | ⌘⌃F |

没有默认快捷键、只在命令菜单里出现的命令：〔本机〕

- Search chats
- Steer、Queue
- Toggle plan mode
- Cycle speed、Cycle reasoning effort
- Toggle Local/Worktree
- Commit、Create PR
- Fork

另有两条：

- 官方表写着"输入框为空时按 ↑ 恢复上一条"，但本机包的输入框键表里没找到这段逻辑，需要实机确认。〔公开〕〔本机未核实〕
- ⌘. 在这个版本里是 ChatGPT 的"Open your dot"，不是中断。〔本机〕

## 6. 与 cupper 设计的差异

对照对象：`docs/design/07-desktop.md` 的 5.7.4、5.7.5、5.7.7，以及截图观察笔记。

### 6.1 需要更正的估计

| 项 | 现在的写法 | 一手资料 | 建议 |
|---|---|---|---|
| 侧边栏宽度 | 约 170pt（截图笔记） | 左栏默认 340px（含约 52px 导航栏），可拖动，范围 240–520；截图实测导航栏 52pt 加侧边栏 238pt | 改成"默认 340，可拖动，存储宽度，范围 240–520"。没有导航栏时，可以取 288 左右作为列表宽度【推测】 |
| 会话区最大宽度 | 约 520pt | 768px（内容 736px） | 改成 768 / 736 |
| 圆角 | 卡片、气泡、输入框都是约 12–16pt | 输入框和用户气泡 22px；卡片 20px；菜单和弹窗 15px；按钮和标签 12.5px；列表行 10px | 按第 3.5 节分档 |
| 右侧"浮起的圆角卡片" | 当成任务面板，P1 | 其实是两样东西：贴满高度的侧边面板（审查等标签，带 1px 边线），和 300px 的摘要卡片（输出内容、来源、计划）。截图里看到的是摘要卡片 | M1 的审查面板按"贴满高度的面板"做；摘要卡片单列为 P1 组件 |
| 权限按钮位置 | 输入框内部的底部一行（截图笔记，5.7.4 也这么写） | 一致。旧公开截图和文档写的"输入框下方"已经过时 | 维持 |
| Work in 和 Worktree 控件 | 新建会话页放在输入框下方 | 一致，放在 utility bar 里；已有会话也能按需显示 | 维持，注明已有会话也可以显示 |
| 用户气泡 | 浅灰气泡 | 前景 5% 的底色（亮色约 #F3F3F4），最大宽 70%，内边距 16 × 10，圆角 22 | 写进 token |
| 侧边栏底色 | #F5F5F5–#F0F0F0 | 默认是 macOS 半透明材质（`vibrancy: menu`）；关闭后亮色 #F6F6F6、暗色 #141414；截图里列表区 #F8F8F8 | 用 vibrancy，同时提供不透明的回退色 |
| 次要文字 | "中灰" | secondary = 前景 65%，tertiary = 前景约 50% | 写进 token |
| 会话状态色 | 运行蓝、需要处理琥珀、完成未读绿、出错红 | 运行中用灰色转圈；未读用蓝点；等待审批和需要输入用文字标签；出错有专门文案。绿、灰、红三个点在 Codex 里用于远程连接状态 | cupper 可以保留自己的语义，但运行中建议改成转圈，未读改成蓝点【推测】 |
| 审批卡片 | 消息流里的卡片 | 现行版本是替换输入框位置的请求面板：选项编号 1/2/3、推荐徽标、自由输入、Skip / Submit 和 Esc 关闭 | 5.7.4 的卡片描述需要改成"输入框位请求面板" |
| 行内评论"发给 agent" | 审查面板里有"发给 agent"按钮 | 没有单独按钮。评论作为附件进入输入框，随下一条消息发出 | 设计可以保留按钮，但交互上建议对齐成"附件进输入框" |
| 快捷键：切换审查面板 | ⌃⇧G | ⌃⇧G 是打开审查标签；切换整个侧边面板是 ⌘⌥B | 两个都加 |
| 快捷键：中断 | ⌘. | 运行中按两次 Esc；⌘. 被 ChatGPT 占用 | 改成 Esc 两次，⌘. 可以保留为 cupper 自己的别名 |
| 快捷键：上一个 / 下一个会话 | ⌃⇧Tab / ⌃Tab | 这一对是"最近会话"；侧边栏顺序上的上一个 / 下一个是 ⌘⌥[ / ⌘⌥] | 两组都加 |
| 快捷键：第 1–9 个会话 | ⌘1–⌘9 | 默认 ⌘1–9 切标签，⌃1–9 切会话，可以对调 | 按 Codex 默认值 |
| 快捷键：模型选择器、归档、重命名、下一个需要处理的会话、终端 | ⌃⇧M、⌘⇧A、⌘⌥R、⌘⌥A；终端 ⌘⌥T | 前四个一致；终端是 ⌃`，⌘⌥T 没有 | 终端改成 ⌃`，⌘⌥T 可以保留为 cupper 的"在外部终端打开" |
| 回合标题 | "用时 …" | "Worked for {time}"，运行中"Working for {time}"；13–14px secondary 色，下面一条 1px 边线 | 维持，补上样式 |
| 字体 | SF Pro、苹方、SF Mono | 一致；界面字重 430，界面字号 14，代码字号 12，聊天行高 1.625 | 补上字重、字号、行高 |

### 6.2 设计里漏掉的元素

| 元素 | 说明 | 建议优先级 |
|---|---|---|
| 左侧导航栏（rail） | 约 52px，ChatGPT 全局导航 | cupper 不需要 ChatGPT 的导航项，可以不做；如果要视觉接近，可以把"设置""更新提示"放进窄栏【推测】 |
| 侧边栏头部的铃铛和搜索 | Activity 视图入口和搜索 | 搜索列入 P1；Activity 对应"下一个需要处理的会话"，可以放在这里 |
| Pinned 分组、Chats（不属于项目）分组 | 截图和本机包都有 | 补进组件树 |
| 侧边栏的组织和排序菜单 | 按项目 / 合并 / 按连接；按创建、更新时间或手动排序 | P1 |
| 输入框的上下文用量环 | 底栏右侧，悬停看详情 | 可以对应 cupper 的上下文用量（如果 agent 能提供） |
| 发送按钮的三态 | 禁用 50%、可发送、运行中变停止 | 补进输入框规格 |
| 排队消息的预览、编辑、Steer、暂停后继续 | 输入框上方 | 设计里有 QueueTray，补上这些交互 |
| 计划药丸"Step n / m" | 浮在输入框上方 | MS-4 补上 |
| 摘要卡片 | 300px，三种显示方式 | P1 |
| 底部面板（终端抽屉） | 默认 280px，可拖动 | 设计里只有"在外部终端打开"；内置终端是否要做，需要 PRD 决定 |
| 标签条 | 右侧面板是多标签容器，标签可以拖到左右、底部或新窗口 | M1 只有审查一个标签，标签条外观先按第 2.4 节做 |
| 改动卡片 | 回合末尾的"{n} files changed +a −b"，带 Undo | 对应 cupper 的 diff 汇总，建议补上 |
| 文件树 | 审查面板里可以开关，28px 行 | 补进 FileList 规格 |
| 整组操作的底部药丸 | Revert all / Stage all 浮在面板底部 | 补进 StageBar 规格 |
| 窄窗口行为 | 768 以下左栏变浮层，右面板自动收起 | 补进 5.7.4 |
| 半透明侧边栏 | `vibrancy: menu`，可以关 | 补进主题设置 |
| toast 位置 | 顶部居中，最多 3 条 | 5.7.5 的 Toaster 补上位置 |
| 远程连接状态点 | 绿、灰、红加转圈 | 可以直接用于宿主状态 |

## 7. 核实情况

### 7.1 各部分的来源

| 部分 | 主要来源 | 可信度 |
|---|---|---|
| 窗口、标题栏、交通灯 | 〔本机〕主进程常量，〔截图〕实测 | 高 |
| 左栏宽度和行规格 | 〔本机〕，〔截图〕实测 | 高。"340 包含导航栏"是〔推测〕 |
| 会话列宽度、条目间距 | 〔本机〕，〔截图〕实测 736.5pt | 高 |
| 右侧面板、摘要卡片、底部面板 | 〔本机〕，〔截图〕实测摘要卡片 301pt、左移 154pt | 高 |
| 颜色 | 〔本机〕的种子值和推导规则，由本任务自行计算；亮色的正文、气泡、禁用按钮、选中行用〔截图〕核对 | 亮色高；暗色只有计算值，没有截图核对 |
| 字号、行高、圆角、阴影、动画 | 〔本机〕 | 高 |
| 组件结构和文案 | 〔本机〕的类名和本地化文案，部分补充〔公开〕 | 结构中等（只读到组件级，没逐行确认渲染顺序）；文案高 |
| 快捷键 | 〔本机〕命令注册表，与〔公开〕官方表一致 | 高 |
| 设置和通知 | 〔本机〕设置 schema 和通知代码，〔公开〕文档 | 高 |

### 7.2 公开物料和本机包冲突、以本机为准的点

- 公开截图都在 7 月合并之前：没有左侧导航栏，侧边栏顶部是 New chat / Search / Plugins / Automations，而不是"Codex ⌄"。
- 权限按钮：旧文档写在输入框下方，本机和作者截图都在输入框内部的底部一行。
- 亮色默认主题：旧截图是 Accent #0285FF、Foreground #0D0D0D；本机是 #339CFF、#1A1C1F，截图核对也是 #1A1C1F。
- 代码字号：旧截图是 14px，本机默认 12px。
- 审批：旧截图是消息流里的卡片，本机是替换输入框位置的请求面板。
- 回合标题：旧的 Windows 截图是居中带横线，本机和作者截图都是左对齐加下方分隔线。

### 7.3 没能确认的点

- 暗色主题没有截图。所有暗色值都是按规则计算的，T106 前最好补一张暗色截图核对。
- 暗色下输入框底色：#2D2D2D 还是 #363636，取决于两条 CSS 规则的生效条件，没有完全核清。
- 会话行的相对时间和 diff 统计，只在旧公开截图里见过，本机包里没有核实。
- "输入框为空时按 ↑ 恢复上一条"：官方表有，本机包里没找到对应逻辑。
- 回到底部按钮只有截图实测（约 28pt），没在代码里找到尺寸。
- 斜杠命令的完整列表来自官方文档；本机包里命令是动态下发的，没有逐项核对。
- 自动在分栏和统一 diff 之间切换的具体像素阈值没有找到。
