# T081 汇报（ui-core III：侧边栏、提权横幅、重连状态与 toast）

- 执行者：子 agent（前端工程师）
- 分支：`feat/T081-ui-sidebar`
- 完成日期：2026-10-20

T081 已完成：侧边栏、提权横幅、添加项目、重连状态、Toaster 都接到了索引流上。UI-sidebar、UI-banners、UNIT-ui-sidebar 全部通过，UI-csp-zero 仍为 0。在干净的 worktree 里 `cargo xtask ci pr --record` 通过，摘要已提交。改动共 874 行，在约 900 的上限以内。

## 做了什么

**Toaster**（`feedback/toaster.tsx`）
- `ToasterProvider` 包在 `HostProvider` 外层（你合并时保留了这个位置）。
- 顶部居中，距顶用 `--layout-toast-top`，最宽用 `--layout-composer-adjacent-max-w`。
- 最多 3 条，新的挤掉最旧的；同 id 的 toast 替换旧的。
- 普通 toast 4 秒后消失；sticky 的一直留着，直到关闭或被替换。每条都有关闭按钮。

**`usePrivileged`**（`feedback/privileged.ts`）

| 返回值 | 反馈 |
|---|---|
| `confirmed` | 不弹 toast，结果由流显示 |
| `cancelled` | 不弹 toast，是用户自己取消的 |
| `busy` | toast：已有确认框打开 |
| `cooldown` | toast：还要等几秒（向上取整） |
| `user_activation_required` | toast：需要用户直接点击 |
| `failed` | toast，带主进程给的语义码 |

`requestPrivileged` 调用本身出错时按 `failed`（`unexpected`）处理。

**重连状态**：`ConnectionToast` 在连接处于 `reconnecting` 或 `failed` 时挂一条 sticky toast，回到 `ready` 后收起。

**侧边栏**（`sidebar/`）
- 外框三个插槽的默认值，从 `useStreams().index` 读数据，用 `useEntry` 订阅。
- 头部：产品名加宿主状态点。绿、灰、红三种颜色，连接中是蓝色转圈；tooltip 里写状态、宿主版本和待安装的升级。替换了 T084 的默认头部。
- "新会话"一行；"项目"小标题旁是"添加项目"，走 `add_project`。
- 项目分组：
  - 按 `position` 排序；
  - 信任状态不是 `trusted`（`stale` 也算）就标"未信任"；
  - 悬停或获得焦点时出现"+"（在该项目里新建会话）和一个空菜单，菜单项归 T153；
  - 先显示 5 条会话，其余用"展开显示"和"收起"切换。
- 会话行：
  - 30 高，标题和项目名对齐；
  - 行首显示运行中转圈、出错图标或未读蓝点；
  - 出错图标的原因按 `error_code` 映射成文案，不认识的码显示"会话出错"；
  - 行尾按 `pending_kind` 写"等待批准"或"需要用户输入"，不认识的 kind 按"需要用户输入"；
  - 悬停 tooltip 写 agent 名；当前会话带 `aria-current`。
- 提权横幅在底部卡片里：显示最早的一条申请，多条时加"另有 n 条申请"；"查看"走 `elevation`。

**截图设施**
- 截图页的夹具加了 `index`：mock 宿主对每个订阅都回 snapshot 模式，再发一页 `stream/snapshot`。索引流的 op 来自夹具，其他流为空。
- 新增 UI-sidebar（亮色、暗色、窄窗口浮层）和 UI-banners（提权横幅、两条反馈 toast、重连 toast）。

**其他**
- 新 token `--animate-spin`（1.25 s 一圈），开启"减少动态效果"时停下。
- 自绘图标：文件夹、新会话、加号、出错、关闭、转圈。
- 新增 `sidebar` 文案命名空间，`shell` 命名空间加了反馈文案。所有文案都走 i18n。

## 关键取舍

- **拆分**：按裁定，会话页的两个警告框拆给了 T137。
- **组件分两层**：`sidebar/parts.tsx` 只按 props 画，交互用例直接测它；`sidebar/index.tsx` 只负责从 store 取数据。T080 合并前我先写了第一层。
- **侧边栏列表订阅整个索引视图**：索引流的事件不多，T080 的 store 每个动画帧最多刷新一次，没有拆成按行订阅。
- **反馈只给用户看不到结果的返回值**：`confirmed` 和 `cancelled` 不弹 toast，界面上已经能看到结果，或者是用户自己取消的。
- **toast 的"向右滑动关闭"没做**：只给了关闭按钮。

## 验证方式

- **UNIT-ui-sidebar**（7 条，jsdom）覆盖：
  - 六种返回值各自对应的反馈；
  - cooldown 时的 toast 文案；
  - toast 3 条上限和按 id 替换；
  - 重连 toast 的出现与收起；
  - 会话行的各种状态、两种标签和"未信任"；
  - 展开与收起；点会话导航，点"+"进入该项目的新建会话页；
  - "添加项目"和"查看"会调用对应的 `requestPrivileged`，"查看"取最早的一条申请。
- **截图**：UI-sidebar 3 张、UI-banners 1 张。UI-shell 的四张基线有变化，差异只在侧边栏：多了"新会话"一行和"项目"小标题，因为那几个用例的夹具没有索引数据。基线用 `pnpm ui:baseline` 重新生成，逐张看过；`meta.json` 没有变。
- **UI-csp-zero** 仍为 0。
- **审阅截图**在 `docs/tasks/T081-ui-sidebar/screenshots/`：截图页加载同样的夹具，把字体换回系统字体（SF Pro、苹方）后截取，1280 × 820，共四张：侧边栏亮色、暗色、窄窗口浮层、横幅与 toast。
- **F2**：没有查看本机的 Codex App 包。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T081-ui-sidebar/ci/`，全部通过，摘要 `pr-beea4e31966a.json` 已提交：

  | 作业 | 结果 |
  |---|---|
  | rust | 1,821 通过 |
  | protocol | 185 通过 |
  | ts | 410 通过 |
  | license | 通过 |
  | testdata-scan | 通过 |
  | ui | 11 通过 |

  e2e 作业还没实现。
- 用 `git merge-tree` 试合了最新的 main，没有冲突。
- **规模**：874 行（实现约 606，测试约 268；不含 notes、lock、截图基线），在约 900 的上限以内。逐模块的估计与实际对照在 notes 里。

## 提交列表

```
89d35a8 docs(T081): record the ci pr summary
beea4e3 docs(T081): record the sidebar, the feedback and the review screenshots
cfe2926 feat(ui-core): add the sidebar on the index stream
f7d906b Merge branch 'main' into feat/T081-ui-sidebar
d21178e feat(ui-core): add the Toaster and the feedback of privileged actions
eeba5af docs(T081): record the checks, the size estimate and a split proposal
```

f7d906b 是你合进来的 main。

## 遗留问题 / 范围外发现

- **截图比对阈值偏宽**：Playwright 默认的逐像素阈值是 0.2，悬停底色（正文色 5.3%）这类细微差异不会让比对失败。这次我是在截图前把指针移开、再按无悬停状态重新生成基线来避开的。阈值要不要调严，请交给 T118 的后续或 T152 决定。
- **没做的部分**：
  - 项目菜单的菜单项归 T153；
  - 侧边栏的设置入口等设置页（T036）；
  - 会话行悬停时行尾的菜单按钮和右键菜单（T153）；
  - toast 的滑动关闭。
- **给 T137 的交接**：
  - `usePrivileged` 已有 `trust_project` 和 `confirm_config` 的反馈，直接调用即可；
  - 警告框用 T128 的 `Alert`；
  - "配置待确认"的数据在会话流的 `configured.confirmation_required` 里，从 `useStreams().session(id)` 读。