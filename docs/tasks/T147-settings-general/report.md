# T147 汇报

- 执行者：子 agent（前端与桌面主进程工程师）
- 分支：`feat/T147-settings-general`
- 完成日期：2026-11-10

T147 完成：通用、通知、存储三页和宿主的防休眠开关都做了。PR #101（https://github.com/boris-w/cupper/pull/101）在 `646f69d0` 上 8 个检查全部通过（CI 运行 https://github.com/boris-w/cupper/actions/runs/42767676533）。规模对 main 是 +627，在裁定的停止线 700 以内。

## 做了什么

**范围调整（都按编排者的裁定）**
- 外观页开工前移出，由 T152、T154、T252 各自加行（计划 v1.3.92）。
- "默认权限"移出，成了待定问题 Q44。原因是宿主没有新会话默认权限这个设置，各 agent 的模式清单也只在会话建好后才有，要先设计。
- 特权操作 `settings` 移出。剩下的各行都是 `write` 级设置或界面偏好，直接 `settings/update` 或写 prefs，这个操作在本任务里没有使用方。我提的 `policy/describe` 样例写法不一致的问题，已记进交接事项。
- 安装命令行工具拆成 T253（已合并，PR #102）。

**宿主的防休眠**
- 新设置键 `host.prevent_idle_sleep`，`write` 级，默认开。
  - 生成的 schema 里这个键是可选的，zod 带 `.default(true)`。
  - T151 之后存下的 `settings.updated` 事件和检查点都没有这个键，读的时候按开处理。
- `PowerCore` 加输入 `Enabled(bool)`，持有条件改成"计数 > 0、已启用、未退出"。
- `Power::enabled` 在两处调用：启动时读初值；`SettingsPort` 在每次更新成功后把当前值交给它，核心收到与现状相同的值时什么都不做。
- 设计 5.2.11 的表里补了这一行，是编排者改的（main `96c0b044`）。

**界面**
- 通用：
  - 发送快捷键：界面偏好，两项菜单。
  - 跟进消息默认排队：界面偏好 `composer.followUpQueue`。说明里写了 Claude 插话的副作用。
  - agent 工作时防止休眠：宿主设置。
- 通知：
  - 三个开关。
  - T250 合并后加了授权被拒的提示：`denied` 时指向"系统设置"的"通知"；`not_determined`、`authorized` 和没有这个字段时都不显示。
- 存储：
  - `host/storage` 的五项大小，每次打开页面重新统计一次，`partial` 时注明"未统计完"。
  - "清除"走新的特权操作 `purge_snapshots`（ShellAPI 只做附加）。主进程的原生确认框显示 `policy/describe` 给的大小，默认按钮是取消。
  - 宿主回 `conflict/turn_running` 时提示"先停下回合再清除"；清除成功后重新统计。
- 几页共用的部件：
  - `useSettings`：读索引流里的设置，写 `settings/update`，失败时弹 toast。
  - `SwitchRow`。
  - `formatBytes`：放在 `@cupper/i18n`，主进程的确认框和页面都用它。
- 页面顺序按设计 5.7.4：通用、通知、已归档会话、agent 状态、连接与设备、存储、命令行工具。

**开关的修复**
- 暗色主题里关着的开关看不到滑块：滑块 #2d2d2d，轨道约 #2f2f2f。
- 按裁定改成两种主题都用白色滑块。Tailwind 的默认调色板没有引入，`bg-white` 不会生成任何样式（第一次改时本机截图里滑块直接没了），所以加了令牌 `--color-switch-thumb`。
- 编排者说合并时会把这个令牌补进设计 5.7.10。

## 关键取舍

- **新键在旧数据里按开处理，不用 upcaster 升版本。** 升版本要连带提高 `VIEW_VERSION`，所有检查点会作废一次。编排者认可。
- **`SettingsPort` 每次更新都把值交给电源核心，不在端口里判断变没变。** 核心本来就忽略重复的值，代码更少。
- **防休眠的集成测试保留了扩展的一段**，约 40 行：关掉后重启，回合不持有；运行中打开立即持有，关掉立即释放。我验证过：去掉启动时读初值那一行，这个用例会失败。编排者裁定保留。
- **`turn_running` 的提示用同一个 toast id 替换 shell 的通用提示**，没有改 `usePrivileged`。
- **快捷键写成文字**（"Command + Enter"）：截图用的中文字体子集里没有 ⌘、⇧。授权提示里的路径也因此写成「系统设置」的「通知」，没有用 "›"。
- **授权被拒的提示等 T250 合并后才做。** 在那之前生成的 zod 会剥掉不认识的字段，界面永远读不到这个值，写了也没法测。

## 验证方式

**PR #101，`646f69d0`**：8 个检查都通过：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。

**本机，单次运行：**
- Rust：
  - `cargo fmt --check` 和 clippy（`-D warnings`）通过。
  - 宿主 lib 测试 512 个通过。
  - IT-power-assertion 通过，并做了上面说的去掉那一行的验证。
  - 协议的 Rust 测试和夹具测试通过。
- 协议：
  - `pnpm gen` 在合并后没有差异。
  - schema-compat：0 breaking，`Settings`、`SettingsPatch` 的新字段都判为 additive。
  - shell-api 的 tsc 构建（frozen 检查）通过。
- TypeScript：
  - ui-core 149 个测试、桌面主进程 108 个测试通过。
  - shell-api、i18n、字体字符检查通过。
  - tsc、prettier、eslint 通过。

**对照验收标准：**
- **UI-settings-general**：通用、通知、存储三页，两种主题。基线来自 ui-baseline 42766695967，单独提交在 `646f69d0`。
  - 三页新图。
  - 已归档、agent 状态、连接、命令行工具这几页的旧图只在左侧导航一栏里变了（多了新页面的行）。
  - 其他基线和 meta.json 与 main 逐字节相同。
  - 开关修复用前后两次 ui-baseline 核对过（42761076628 对 42764424484）：只有暗色的通用、通知两张变了，亮色六张逐字节相同。
- **组件测试**：
  - 开关读写和失败时的 toast。
  - 发送快捷键菜单、跟进排队、防休眠。
  - 授权提示：`denied` 显示，`not_determined`、`authorized`、没有字段时都不显示。
  - 存储：大小只作文本显示、"未统计完"、清除走 shell、`turn_running` 提示、清除后重新统计。
  - "安全设置走特权路径"这一条随特权操作 `settings` 一起移出。
- **主进程单元测试**：`purge_snapshots` 的四种情况：
  - 确认框显示宿主给的大小；
  - 默认取消；
  - 有回合时宿主拒绝；
  - 确认后发出的命令与描述时是同一个 `command_id`。
  - `install_cli` 的测试在 T253。
- **安全约束**：
  - 特权操作只在主进程执行。
  - 文案都走 i18n。
  - 宿主给的大小只作为文本显示。
- **宿主约束**：电源核心仍是纯核心，没有 I/O；设置的写者没变。

## 规模

最终对 main +627（含测试，不含生成物和基线）。经过：
1. 开工前估约 665。
2. 移出默认权限和特权 `settings` 后约 575，停止线 600、硬线 680。
3. 宿主模块实际 +146（估 75），停止线升到 660、硬线 720。
4. 通知模块之后预计约 683，停止线升到 700。
5. 存储模块 +194（估 120）之后，加上安装命令行工具会超过 720，于是它拆成 T253。

超出的部分几乎都在测试和共用部件上（集成测试那一段、`useSettings`/`SwitchRow`、`formatBytes`、各模块的测试），功能范围没有扩大。main 上的任务书仍写着 600/680，所以 `cargo xtask size` 会报超出停止线 27；按编排者的裁定是 700/720。

## 提交列表

```
646f69d0 test(ui): UI-settings baselines with the general, notifications and storage pages
82f6c819 Merge branch 'main' into feat/T147-settings-general
750bc593 docs(task): T147 notes on the switch's thumb
4e608841 fix(ui): the switch's thumb is white in both themes
262ca54b feat(ui): the notifications page says where to allow them once macOS denied them
0dee9309 Merge branch 'main' into feat/T147-settings-general
61107f46 docs(task): T147 notes on the split and the merge of main
5a86d261 feat(ui): the follow-up queue setting on the general page
a9cfb011 test(ui): the settings tests wait for the host's answers, not a timer
bb727fbc Merge branch 'main' into feat/T147-settings-general
97ed2bf3 docs(task): T147 notes on the storage and general pages
979c3986 test(ui): UI-settings-general screenshots of the general, notifications and storage pages
14548b3f feat(ui): the general settings page
d79f2d59 feat(ui): the storage settings page and purge_snapshots
e4abe93e feat(ui): the notifications settings page
5b70a010 feat(host): host.prevent_idle_sleep switches the power assertion
```

**三次 merge main：**
- `bb727fbc`：带进 T248、T242，无冲突。
- `0dee9309`：带进 T250，无冲突。合并提交上跑过 fmt、clippy、宿主 lib 测试和协议测试。
- `82f6c819`：带进 T253，7 个文件有冲突，都保留两边：
  - 页面表：我的三页按设计 5.7.4 的顺序放，命令行工具放最后；
  - `PrivilegedAction`、`run` 的分支、`parseAction` 的清单；
  - privileged.json 和 settings.json 的末尾；
  - 测试文件里的两组 describe。

## 遗留问题 / 范围外发现

- **偶发失败的测试（a9cfb011）**：第一次 merge main 后，T248 的已归档会话用例和我的存储用例大约 5 次失败 2 次。原因是宿主的回答经 mock 的消息端口回来，等一个定时器周期有时不够；改成等元素出现后连跑 8 次都通过。编排者在 main 上另跑了 test-repeat 42761117861 留档（见 main 的 `8bf09500`）。
- **`policy/describe` 的协议样例**：`admin.rs` 里写的 `details` 是 `{key, value}`，宿主实际给的是 `{settings: {...}}`。已记进交接事项，留给第一个做安全设置行的任务。
- **"默认权限"**：待定问题 Q44，要先设计。
- **特权操作 `settings`**：ShellAPI 里仍保留，主进程照旧回 `unsupported`，随第一个安全设置行再做。
- **设计 5.7.10**：令牌表要补 `--color-switch-thumb`，编排者说合并时补。
- 物料都在 `docs/tasks/T147-settings-general/notes.md`（已提交）：估算与规模记录、各项取舍、开关修复的核对过程。
