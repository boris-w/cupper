# T151 汇报

- 执行者：子 agent（全栈：宿主索引流与 ui-core 设置页）
- 分支：`feat/T151-settings-agents`
- 完成日期：2026-11-10
- PR：https://github.com/boris-w/cupper/pull/92
- CI：https://github.com/boris-w/cupper/actions/runs/42752460784 。`b6489b7b` 上 8 个检查全部通过。
- 基线：ui-baseline run 42747348250，核对后单独提交为 `b5371d21`。

## 做了什么

**宿主和协议**
- **`settings.updated` 事件**：
  - 索引流新增 `settings.updated {settings}`，带全部设置，内容与 `settings/get` 相同。
  - `IndexView.settings` 是可选字段，带 `skip_serializing_if`，后写的覆盖先写的，规则与 `host.updated` 相同。
  - TS 的索引视图、两处夹具、`pnpm gen` 的生成物一并更新。
  - 宿主里 `StreamView::Index` 改成装箱，因为视图变大以后 clippy 报 `large_enum_variant`。
- **设置改由 watch 通道持有**：`SettingsHandle` 用 `watch::Sender<Settings>`。索引启动时，视图不一致就写一条 `settings.updated`；每次更新生效后，索引再写一条。用 watch，多个更新的先后不会乱。
- **git 版本**：`AgentsUpdated.git` 接上 T094 的 git、git-lfs 版本检测，启动时在单独的任务里探测。
  - 新增 `GitTool::versions` 和 `GitReport::tools`。
  - 结果在第一条 agent 状态到来时一起写入，原因见"merge 之后的修正"第 2 条。

**界面**
- **设置页框架**：
  - 占满窗口，导航放在原来侧边栏的位置，顶部是"返回应用"。内容区最宽 768，分节间距 40。设置行左边是标签加说明，右边是控件。
  - 路由新增 `settings` 页，记住进来之前的那一页，"返回应用"回到那里。
  - 分页是一张表，后续页各加一行即可。T248 已经这样加了两页。
  - 三个入口：侧边栏底部的"设置"、菜单的 ⌘,、`navigate({kind: "settings"})`。
- **agent 状态**：
  - 每个 agent 一张卡：状态和不可用原因、CLI 路径、版本、已验证与否、安装方式、适配器和运行时、登录、配置目录、内存占用、重启按钮（调 `agents/restart`，进行中不可点，失败弹 toast）。
  - 另有一张卡列 git 和 git-lfs 版本。
  - 没装 Codex CLI 时显示"不可用：没有找到 CLI"。
  - 协议里开放枚举的值，这个版本不认识的显示"未知"。
  - 宿主给的路径、版本、套餐名都只作为文本显示。
- **错误条目的跳转**：`upgrade_cli`、`install_cli`、`restart_agent` 三种恢复指引旁边多了"查看 agent 状态"按钮，跳到设置页里该会话的 agent 那张卡，并把焦点放上去。为此改了 T143 的错误条目，原有用例已补上。

**merge**
- `4dc25326` merge 了 origin/main 9bce031e，main 带来了 T131、T238、T245、T246、T247、T249、T145。
- 冲突共 44 个，全是共享的基线 PNG，取 main 的版本，再由 ui-baseline 重新生成。
- 双方都改过的源码只有 `rows/plan-error.test.tsx`，自动合并，没有冲突。

**merge 之后的修正**（PR #92 上一轮检查暴露的、T151 自己的问题）
1. **`1a978b41`，ts 作业失败**：`host-client/src/views/index-view.ts` 不符合 prettier 格式。T151 改这个文件后，我没有对 host-client 跑 prettier。已按 prettier 格式化，并重排了文件头的注释。
2. **`1e108a16`，rust 作业里 `it_host_tests_no_real_cli_discovery_sees_only_the_test` 失败**：
   - 原因：启动时 git 版本的探测可能比 agent 的第一条状态先完成，于是宿主写出一条 `agents` 为空列表的 `agents.updated`。
   - 空列表的意思是"没找到 agent"，语义不对；用例读到了这一条。
   - 修正：索引先记住 git 版本，等第一条 agent 状态到了再一起写；已经有 agent 列表时，直接更新列表里的版本。单元用例补了断言：agent 状态到来之前不写这条事件。
3. **`b6489b7b`，rust 作业的 `test-host-shuttle` 以 SIGBUS 失败**：
   - 失败的是 PROP-stream-seq，即 `store/streams/interleavings.rs`。本机用同一条命令单份复现了，同样是 signal 10，没有任何输出。
   - **证据**：在 lldb 下，停在 `EXC_BAD_ACCESS (code=2, address=0x1060cbe58)`，sp 是 0x1060cbe30。两者都落在 `[0x1060c8000-0x1060cc000) ---` 这页没有任何权限的保护页里；紧挨着它上面是 `[0x1060cc000-0x1060dc000) rw-`，大小 64 KiB，正是 shuttle 默认的 0xf000（60 KiB）按页取整后的协程栈。崩溃时的调用栈是 `IndexView::snapshot_ops` 的 `Chain<Flatten<array::IntoIter<[Option<EventBody>; N]>>>` 迭代器链。
   - **原因**：`EventBody` 本身是 680 字节，没变。T151 让索引视图的单项事件从 3 个变成 4 个（多了 `settings`），快照里的数组多了 680 字节；debug 构建里这份迭代器状态在每一层迭代器帧里各放一份，越过了 60 KiB。macOS 上踩到保护页报的是 SIGBUS，所以没有 Rust 的溢出提示。
   - **修法**：在用例里显式给栈。用 `Runner::new(DfsScheduler::new(None, false), config)`，`config.stack_size = 256 KiB`，并写了注释说明原因。实测 96 KiB 和 128 KiB 都能通过，取 256 KiB 留余量。没有缩小视图。

**基线核对**（`b5371d21`）
- 68 个文件逐字节比对：24 个（含 `meta.json`）完全相同，44 个有变化，恰好就是 main 自 T151 上次基线以来改过的那些。
- 逐像素比对后，新图相对 main 的版本只有三类差别，其他像素完全相同：
  - 侧边栏底部多了"设置"一行，44 张都有；
  - 提权横幅随之上移，10 张；
  - `stream-error` 亮暗两张多了"查看 agent 状态"按钮。
- 与别的任务重叠的图，main 的变化都还在，肉眼看过：
  - T238：审查面板和提交对话框；
  - T245：还原弹框；
  - T145：输入框的"+"。

## 关键取舍

- **设置的推送用 watch 通道**：不在 `SettingsPort` 里事后通知，是为了让多个更新写进流里的先后与生效的先后一致，后写的覆盖先写的。
- **git 版本等到第一条 agent 状态再写**：空的 `agents` 列表表示"没找到 agent"，不能为了提前显示 git 版本而发出这样一条事件。
- **"返回应用"由路由记住来处**；错误条目的跳转直接定位到对应 agent 的卡片。这两处都是开工前与编排者定的。
- **shuttle 用例显式给栈**，而不是缩小视图。视图的形状由协议决定，测试环境的栈应该跟着需要走。
- **抽出共用的 `CommandButton`**：这一步在 T248 里做，重启和恢复共用；T248 里两份几乎一样的代码不该留着。

## 验证方式

- **PR #92 上 8 个检查全部通过**（run 42752460784，`b6489b7b`）：rust、ts、ui、protocol、e2e、license、testdata-scan、compat-framework。
- **本机**（都是单份）：
  - cargo fmt 通过，clippy（--all-features）干净。
  - 宿主 lib 用例 528 条通过；lifecycle 集成测试 20 条通过；shuttle 配置下 `prop_stream_seq` 通过，clippy 干净。
  - tsc、eslint、`prettier --check .` 通过。
  - vitest：ui-core 131、i18n 10、host-client 67、protocol 171 条，全部通过。
  - `cargo xtask ci protocol` 通过，`schema-compat` 0 breaking。
- **对照验收标准**：
  - UI-settings-agents（vitest 3 条和截图亮暗两张）通过；宿主部分有单元用例：索引写 `settings.updated` 和 git 版本、设置的 watch、git 版本的格式化、视图属性用例。✓
  - `schema-compat` 0 breaking；`settings.updated` 和 `IndexView.settings` 按附加式改动通过。✓
  - 渲染进程的安全约束：文案都走 `@cupper/i18n`；宿主和 agent 给出的字符串只作为文本节点显示，没有用 `innerHTML` 一类写法，也没有进入样式。✓
  - PR 上 8 个检查全部通过，链接见上。✓

## 规模

- **最终 +932 −111**（`cargo xtask size`）。停止线 480，放宽后的硬停止线 640，超出部分已获编排者接受。构成：
  - 宿主和协议首轮 +228（其中用例约 108；协议要求每种事件都有夹具样例和往返样例）；
  - 界面 +624，其中产品代码 354、文案 53、用例 217；
  - merge 后的修正：prettier 和 git 版本时序约 +23；shuttle 修正 +57 −44，其中约 41 行是 rustfmt 的重排（调用少了第二个参数，闭包体整体少缩进一级），真正的改动约 16 行。
- **为什么没有逐个模块统计**：守则第 8 条要求每完成一个模块统计一次行数。我在宿主部分做完后统计过（+228），并按推算报了超限；界面部分却一口气写完才统计，结果是 +624，远超推算的 390，超限没能尽早发现。这是我的失误。之后的 T248 已改为每个模块跑一次 `cargo xtask size`。

## 设计偏差 / 补充

- **T248 改了 T151 的行为**：T248 按设计 5.7.4 的顺序把已归档会话排在 agent 状态前面，所以不带分页打开设置时（侧边栏的"设置"、⌘,），默认显示已归档会话，不再是 agent 状态。T151 的用例在 T248 分支上跟着调整了。
- **"已验证范围"只显示状态**：协议里只有 `compat`（已验证、更新未测、更旧未测、不支持），没有范围的具体数值，所以显示的是状态文字。

## 提交列表

```
b6489b7b fix(host): give PROP-stream-seq's shuttle threads a larger stack
b5371d21 test(ui): baselines for T151 on the merged main
1e108a16 fix(host): no agents.updated without agents for the git versions
1a978b41 style(host-client): format the index view as prettier does
4dc25326 Merge remote-tracking branch 'origin/main' into feat/T151-settings-agents
ad16aac5 test(ui): baselines for the settings page and the sidebar's settings row
ce32e33d docs(T151): size after the UI part
4a6d80ff feat(ui): settings page with the agent status
c0dd7001 docs(T151): size after the host part
77e40eb1 Merge branch 'main' into feat/T151-settings-agents
ce552e88 feat(host): write settings.updated and the git versions
714110b8 feat(protocol): settings.updated and IndexView.settings
1e5f9f4f docs(T151): estimate before starting
```

## 遗留问题 / 范围外发现

- **交接事项（shuttle 栈）**：哪个 shuttle 用例的视图或状态变大了，都可能撞上 shuttle 默认 60 KiB 的线程栈。在 macOS 上表现为没有任何输出的 SIGBUS。排查时用 lldb 看 sp 是否落在协程栈下方的保护页里；修法是用 `Runner` 加 `Config::stack_size` 显式给栈，并注明原因。PROP-stream-seq 现在是 256 KiB，实测 96 KiB 就够。
- **本机构建的提醒**：`cargo clean` 之后，宿主的部分 lib 用例和 lifecycle 集成测试要先构建假 agent（`cargo build -p cupper-fake-agent`）和 CLI 的 e2e 构建（`cargo build -p cupper-cli --features test-harness`）；CI 的 rust 作业会自己构建这两样。
- **不在本任务范围、没有做**：`stream` 命名空间的 `error_project_untrusted` 文案，见交接事项 T111 那一行。
- **T248**：分支 `feat/T248-settings-archived` 停在 `4dc9a535`，等 T151 合并后 merge main。
- worktree 保留着。
