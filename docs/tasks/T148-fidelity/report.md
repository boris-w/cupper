# T148 汇报

- 执行者：子 agent（前端，ui-core）
- 分支：`feat/T148-fidelity`（PR #134，头 67796e93）
- 完成日期：2026-11-23

T148 交付完毕：外框（A）和侧边栏（B）已改成 Codex 的样子。PR #134 在 67796e93 上 8 个检查全部通过，分支上跑的 nightly 也全过，规模 +169 −126（停止线 260）。

## 做了什么

**第一步：偏差清单**
- 写在 `notes.md`，共 30 条，归入 A、B、C、D、I、J 六组，另有 L（要作者看一眼）和 M（不做）。
- 依据：
  - 参考截图，只写文件名；
  - ChatGPT.app 的 App 包，只读，解到临时目录、用完已删，没有拷进仓库；
  - T106 的记录；
  - 交接事项里的 13 行。

**第二步：按你定的范围改 A 和 B**
- **A1 外框**（先做原型；两张截图是 fixture 数据，作为物料放在任务目录）：
  - 顶栏横贯整窗。左段与侧边栏同宽，放交通灯留位和侧边栏开关，右端一条 16 高的竖线。
  - 侧边栏和主区放到顶栏下方：侧边栏左侧两角、主区右侧两角是 12 的圆角，对应 Codex 的 `radius-xl-base`。
  - 侧边栏底色是 `color-mix(in oklab, var(--color-surface) 65%, transparent)`，与 Codex 的 `.sidebar-navigation` 一样。
  - 窗口底色：半透明时用系统材质；不半透明或开了"降低透明度"时，用新增的 `--color-surface-chrome`（亮 #F2F2F2、暗 #2E2E2E）。这个值取自 T106 实测的标题栏色，去掉了暖色调。原来的 `--color-surface-sidebar` 已删掉。
  - 窄窗口的浮动侧边栏放在顶栏下方：surface 底，右侧圆角，卡片阴影。
  - 设置页也用同一套外框。
- **A3**：侧边面板开关挪到最右，"⋯"在它左边。
- **A4**：会话标题前加文件夹图标。
- **B1**：会话行的转圈、出错、未读挪到行尾，与"等待批准"一类标签共用一格，悬停时让给菜单按钮。单元测试核对状态在标题之后。
- **B2–B4**：
  - 侧边栏头部改 18 semibold，高 40；
  - 分组标题改 14；
  - 项目下有会话时用打开的文件夹（图标自绘）。
- **设计**：5.7.4 的外框、顶栏、窗口背景、窄窗口、侧边栏状态表、行规格、头部，以及 5.7.10 的颜色表、半透明说明、圆角表，都已改在分支上。

## 关键取舍

- **左边缘的圆角**：cupper 没有导航栏，侧边栏左上角的圆角落在窗口左边缘，会露出一小块顶栏的颜色。我看过原型的亮、暗两张截图，效果与 Codex 在导航栏边上的那个角相近，所以照做。
- **chrome 色怎么来的**：叠上 65% 的 surface 之后，侧边栏约 #FBFBFB 和 #202020，与 Codex 实测的列表区 #FAFAFA、#212121 相近。
- **侧边面板也在顶栏下方**：与 Codex 一致。因此只截审查面板的那些基线矮了 44。
- **行数**：外框容器换了，侧边面板和设置页的块都要挪进去，带来一部分重新缩进的行数。

## 验证方式

- **本机**：`pnpm lint` 通过，ui-core 201 项通过。另外用 ui-screenshots 单份截了几张图看效果（不生成基线）：原型、窄窗口、设置页、侧边栏。
- **验收标准**：
  - [x] 第一步：清单覆盖了要求的各个区域和 T148 的交接事项，每项都写了依据和估计行数。
  - [x] 第二步：按定下的范围改完，单元测试跟上。
    - 基线由 ui-baseline 生成（all=true，run 45146189956），83 张全换，逐类对照过原因，写在提交信息里。
    - 第一次跑 ui-baseline（45145266825）失败了：UI-theme 断言 body 是 surface，而现在 body 画的是外框。已改成同时检查 session 区和外框两处。
  - [x] PR #134 的 8 个检查全部通过：https://github.com/boris-w/cupper/pull/134 （运行 https://github.com/boris-w/cupper/actions/runs/45152918863 ）。
  - 分支 nightly（45145278239）全过，包括 focus 用例，外框改动没有影响焦点顺序。

## 提交列表

```
9fdf1bbd test(ui): baselines for Codex's frame and sidebar
8b069f17 test(ui): UI-theme reads the session area and the frame
823710c2 docs(task): T148 notes on step two
0fc2cfd1 feat(ui-core): the sidebar as Codex's: states at the row's end
e31f240e feat(ui-core): Codex's frame, the title bar across the window
36bda283 docs(task): T148 step one, the list of deviations from Codex
```

另有你合进 main 的 67796e93。

## 遗留问题 / 范围外发现

1. **T272 的基线**：T148 先合并后，T272 的基线需要在合入 main 后重新生成（你已经安排了）。
2. **侧边栏开关的按下底色**：工具栏按钮按下时有灰底，所以侧边栏开着时，开关下面有一块灰底；Codex 的侧边栏开关没有这块底色。这次没有改。如果要跟 Codex 一致，可以只对侧边栏开关去掉按下底色，约 2 行，可以归入 T272 或以后再做。
3. **真机上的观感**：半透明模式下顶栏、外框和侧边栏三层的实际效果，只能在真机上看，已列在 L 组。
