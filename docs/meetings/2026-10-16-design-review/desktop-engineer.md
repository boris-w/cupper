# 设计评审：桌面客户端工程师

## 总体结论

**修改后可以进入 gate/design。** 没有阻断项，不需要推翻任何 ADR。Q1 的连接模型（control 连接加上每个窗口一条数据连接）是对 ADR-03 的细化，方向对，安全意图也保住了。

但有 15 条"重要"，要在阶段 ③ 拆任务前改进设计，集中在三处：

1. **协议给界面的语义还缺几块**：快照里没有时间、回合折叠缺"最终回答"、工具摘要字段不全、命令输出的流式语义没定、历史分页的单位不对。5.12 规定"M1b 只加方法和事件，不改已有契约"，所以这些必须在 M1a 冻结 v1 契约之前补上。
2. **桌面端的几份合同写得不够细**：HostPort 生命周期、ShellAPI、流控、真实输入规则、CSP 与 Trusted Types 策略。`ui-core` 和 `apps/desktop` 会由不同任务实现，按现状拆分两边都会返工。
3. **升级与验证计划要补**：更新流程中"宿主已退出、App 还没退出"的空窗；M1a 最简界面的范围；SP-6 的计划。

## 意见

| # | 级别 | 位置（文件与小节） | 问题 | 依据 | 建议 |
|---|---|---|---|---|---|
| 1 | 重要 | `docs/design/07-desktop.md` 5.7.1、5.7.2；`docs/design/09-testing.md` 5.9.4 | **HostPort 的生命周期没有定义。**<br>① 渲染进程重载、崩溃（`render-process-gone`）或导航之后，旧的数据连接怎么处理没写：宿主上的旧订阅还会继续推数据，主进程会一直缓存。<br>② 关闭最后一个窗口、再从 Dock 重新打开时怎么处理没写（macOS 的惯例是关窗不退出 App）。<br>③ 宿主重启后的重连顺序没写。<br>④ 窗口连接的握手由主进程完成，渲染进程拿不到 `initialize` 的结果（`limits`、`features`、`weak_auth`、宿主版本）。<br>⑤ "每次真实输入只能用一次"与 5.7.6 的"重连后用同一个 `command_id` 重发"冲突：重发的 `interaction/respond` 没有新的输入，会被主进程以 `user_activation_required` 拒绝。 | NFR-1 验收里有"重载渲染进程"用例（5.9.4）；5.7.6 的命令重试；5.7.8 的真实输入规则 | 数据连接绑定到渲染进程的**文档实例**：在 `did-start-navigation`、`render-process-gone`、`closed` 时关闭连接，新文档调用 `open()` 时再新建。`open()` 改为异步，返回握手结果；宿主重连后经 `onConnectionState` 推送新的握手结果。重连时先连 control，再连 window。主进程按 `command_id` 记住已经放行的输入，重发同一个 `command_id` 不再要求新的输入。另外写清四种情况下的行为：关闭最后一个窗口、Dock 重开、注销或关机（`powerMonitor` 的 `shutdown`）时 NFR-8 确认框怎么处理。 |
| 2 | 重要 | `07-desktop.md` 5.7.1"流控"、5.7.6"流状态" | **确认依赖渲染进程的动画帧，窗口被遮挡后会频繁重置流。**<br>渲染进程"处理完一批才确认"，而事件"按动画帧批量应用"。在 macOS 上，窗口被其他窗口完全遮挡时页面就变为 hidden，rAF 停止。于是：渲染进程不再确认 → 主进程积压到 8 MB 后停止读取 → 宿主的发送队列超出 4 MiB 预算，重置这条流 → 用户切回窗口时整条流重新同步，长会话的滚动位置和展开状态都会丢。<br>"编辑器盖住 Cupper、agent 在后台跑"正是主要使用场景，重置会成为常态。 | Electron 44 `browser-window.md` 的 Page visibility 一节（macOS 会跟踪遮挡状态）；5.3.9 的 `max_queue_bytes` | 消息一到就应用到 store（hidden 状态下 MessagePort 的消息照常投递），应用后立即确认；只把"通知组件重新渲染"合并到 rAF。`backgroundThrottling` 保持默认值，并在文档里写明。SP-6 增加一个场景：窗口被遮挡、持续流式输出 10 分钟，切回后不应出现 `stream/reset`。 |
| 3 | 重要 | `07-desktop.md` 5.7.6；`09-testing.md` 5.9.6 | **没有写界面订阅哪些流。**<br>技术选型讨论会第二轮，桌面工程师的结论是"只有可见的会话订阅全部事件"，5.7 没有继承这一条。几个问题都没有答案：切换会话时是否退订旧会话？要不要保留最近几个会话的订阅，以便秒切？有待处理审批的会话要不要订阅？<br>这些决定了渲染进程的内存、流控预算、"6 个会话同时流式输出"这条预算的实际含义，以及切换会话的延迟。 | `docs/meetings/2026-10-16-tech-selection/desktop-engineer-round2.md` 第 24 行 | 在 5.7.6 写明订阅策略：索引流常驻；当前会话加最近 K 个（例如 3 个）保持订阅；其余会话退订，并丢弃它们的视图。在 5.9.6 和 SP-6 加"切换会话到首屏可见"的预算，例如 p95 ≤ 150 ms，命中缓存时 ≤ 1 帧。 |
| 4 | 重要 | `07-desktop.md` 5.7.3 ShellAPI | **ShellAPI 这份合同不完整。**<br>① `HostPort.send(json: string)` 只能发 JSON。但 `blob/put` 要求先发 `0x02` 二进制分片，M1a 就需要它（超过帧预算的长文本要改用 `text_blob`），M1b 粘贴图片也要。<br>② 没有入方向的命令通道：5.7.2 说"加速键在菜单里注册，转成渲染进程的命令"，深链接也要导航，但 ShellAPI 里没有 `onCommand` 或 `onNavigate`。<br>③ `requestPrivileged` 只返回 confirmed、cancelled、failed 三种状态。以完全访问新建会话时，用户确认后渲染进程拿不到 `session_id`，没法跳转到新会话。<br>④ `prefs.get`、`system.locale()`、`system.theme()`、`updates.state()`、`platform` 都是同步 getter。在沙箱 preload 里，同步取值只能用会阻塞渲染进程的 `ipcRenderer.sendSync`。<br>⑤ 权限处理器"一律拒绝"会让 `navigator.clipboard.writeText` 失效：streamdown 代码块的复制按钮、复制消息都依赖它（未实测）。<br>⑥ 拖入文件时取路径要用 `webUtils.getPathForFile`，沙箱下没有 `File.path`。 | 5.3.4 的 `blob/put`；Electron 44 `tutorial/sandbox.md`（沙箱 preload 可用的模块）；`session.md` 的权限类型 `clipboard-sanitized-write`；streamdown 2.7.0 源码里调用了 `navigator.clipboard.writeText` | ① 增加 `sendBlob(hash, offset, last, bytes)`，主进程出方向放行并限额。② 增加 `onCommand(cb)` 和 `onNavigate(cb)`。③ `requestPrivileged` 返回 `{status, result?}`。④ 静态信息在启动时一次性注入（`additionalArguments`，或第一次 invoke），变化的值走订阅。⑤ 放行 `clipboard-sanitized-write`，继续拒绝 `clipboard-read`。⑥ 如果要支持拖入文件生成 `@` 引用，增加 `pathForFile`。 |
| 5 | 重要 | `07-desktop.md` 5.7.8"真实输入"、5.7.7 | **按 Esc 拒绝会被主进程拦下。**<br>真实输入只认 Enter、Space 和点击，而 AP-1 要求 Esc 拒绝。这样用户按 Esc 拒绝时，主进程会返回 `user_activation_required`。<br>另外，中文输入法用 Enter 上屏候选词时，这一下按键不应算作真实输入。 | PRD AP-1；ADR-03 的原文是"真实的用户输入"，没有限定按键；Electron 44 的 `before-input-event` 带 `isComposing` 字段 | 拒绝类选项和 `dismissed` 不要求真实输入（拒绝不会扩大权限），或者把 Esc 加进白名单。`isComposing` 为真的按键不计入。写清提问表单用 ⌘Enter 提交时算不算真实输入。 |
| 6 | 重要 | `07-desktop.md` 5.7.8 CSP；ADR-03 的组件选型 | **发布用的 CSP 与选定的组件不兼容（已核对源码）。**<br>① @pierre/diffs 1.5.2 在渲染路径上直接给 `innerHTML` 赋 HTML 字符串（`components/FileDiff.js`、`File.js`），在 `require-trusted-types-for 'script'` 下会抛 TypeError。它还用 `createElement("style")` 注入主题样式，会被 `style-src 'self'` 拦下。<br>② Radix 的弹层组件经 react-remove-scroll → react-style-singleton 动态插入 `<style>`，同样会被拦（这个库支持 nonce）。<br>③ 开启 Trusted Types 后，`new Worker(url)` 也需要 TrustedScriptURL。设计把 diff 高亮放在 Web Worker 里，pierre 自己也会 `new Worker`。<br>④ streamdown 2.7.0 默认启用 rehype-raw，harden 的默认值放行全部链接前缀、协议和图片前缀，还允许 data 图片，与设计"禁用原始 HTML、不加载远程图片"的意图正好相反。 | 已核对的 npm 源码：@pierre/diffs 1.5.2、react-style-singleton 2.2.3、streamdown 2.7.0。另外 @streamdown/code 2.0.0 用的是 Shiki 的 JS 正则引擎，返回 token，不需要 wasm，也不写 `innerHTML`，这部分没有问题 | 在 5.7.8 写清 Trusted Types 策略：一个 `cupper` 命名策略，加一个受限的 `default` 策略。`default` 只放行 diff 组件产出的 HTML，并经 DOMPurify 输出 TrustedHTML；`createScriptURL` 只放行同源的 worker。样式改为每次加载生成 nonce：由 `protocol.handle` 下发 CSP 头时生成，再交给 get-nonce 和 pierre 使用。也可以把审查面板放进一个独立文档，单独配 CSP。streamdown 的插件配置写成明确要求，并纳入金样例测试。M1a 最简界面任务加一条验收：在发布 CSP 下渲染全部组件，`securitypolicyviolation` 事件数为 0。 |
| 7 | 重要 | `03-protocol-v1.md` 5.3.3、5.3.4（`stream/snapshot`、`history/page`） | **快照和历史里的事件没有时间。**<br>SP-4a 的 `EventBody` 只有 `{type, data}`，`ts` 在信封上；而快照和历史下发的都是 `ops: [EventBody]`。冷启动打开会话走的是快照，于是三样东西没有数据来源：进行中回合的"用时"计时、思考时长、消息时间。`turn.started` 也没有 `started_at`。 | `spikes/SP-4a-host-skeleton/crates/cupper-protocol/src/events.rs` | `turn.started` 和 `item.started` 的 data 带 `started_at`，`item.completed` 带 `ended_at`；或者让快照里的 op 保留 `ts`。 |
| 8 | 重要 | `03-protocol-v1.md` 5.3.5；`07-desktop.md` 5.7.4 | **Codex 的回合折叠缺少语义。**<br>回合结束后，"用时 … ⌄"把工作过程收起，最终回答留在外面。Codex 的 app-server 用 `agentMessage.phase` 区分过程说明和最终回答；cupper 的 `agent_message` 没有这个角色，界面只能自己猜"最后一段文字是回答"。这违反瘦客户端原则，而且 Claude 和 Codex 的猜法还不一样。<br>另外，Codex 在回合结束时有一个"改动摘要（文件数，增删行数）"，点进去就到审查面板。现在的 `snapshot.recorded` 不带统计，界面要给每个历史回合单独调一次 `git/diff`。 | 截图观察笔记的"会话区"；R01 第三节（`phase`、C5 文件修改项） | `turn.completed` 带 `final_item_ids`，或者给 `agent_message` 加 `role: commentary\|final`，由宿主在回合结束时判定。`snapshot.recorded{phase: end}` 带 `stat {files, added, removed}`。5.7.4 写清折叠规则：运行中展开；结束后默认收起过程；失败或取消时不收起。 |
| 9 | 重要 | `03-protocol-v1.md` 5.3.5 的 `ToolCall` | **单行摘要需要的字段不全。**<br>"已在 . 中搜索 <关键词>""已列出 <目录> 中的文件"需要搜索词和目录，fetch 需要 URL，这些字段都没有。界面只能退回 agent 给的英文 `title`，失去本地化。<br>`group_id` 的规则只说"两段 agent 文字之间"，没说夹在中间的 thought、notice、交互请求算不算打断分组。<br>Claude 的子代理产生的工具调用没有 `parent_item_id`，会平铺进主消息流，而且 M1 的金样例会把这种平铺结果固化下来。 | 截图观察笔记的"活动日志"；PRD MS-3、MS-6 | 增加 `query?`、`url?`（或者一个通用的 `subject`），并按 `tool_kind` 列一张表，写明提取层必须给出哪些字段（提取层和金样例都以它为准）。写明分组规则。预留 `parent_item_id?`。 |
| 10 | 重要 | `03-protocol-v1.md` 5.3.5；`06-agent-driver.md` 5.6.2、5.6.3 | **命令输出的流式语义没有定。**<br>驱动层有 `ToolUpdate.output: Append\|Replace`，协议里只有 `item.updated {…要更新的字段}`；`item.delta` 只有 `{item_id, text}`。Codex 的终端输出是逐行增量。如果每次 `item.updated` 都发完整输出，流量是 O(n²)，超过 16,000 字符后还会反复生成 blob。 | 5.6.3"命令输出"一行 | `item.delta` 增加 `field: text\|output`，并且可以合并。定义输出超出内联上限后的切换方式：改为 blob 加尾部预览，写清何时切换。界面按 `field` 追加内容。 |
| 11 | 重要 | `03-protocol-v1.md` 5.3.4、5.3.9；`04-data-model.md` 的 `turn_snapshots` | **大回合可能取不回来。**<br>`history/page` 单页不超过 48,000 字节，JSON 消息又不能跨帧；而回合快照是"一个回合一行 ops"。一个回合里只要有几段 16,000 字符的内联输出就超过 48 KB，按现在的写法这样的回合取不回来，而 PS-8（重启后看历史）是 P0。<br>另外，会话快照规定"进行中的回合全部包含"，也没有上限。 | 5.3.1"帧"；5.3.9 的限额 | `history/page` 的游标细到回合内的位置，例如 `next: {turn_no, op_index}`，允许一页只含半个回合。界面对"回合还没加载完"显示占位。进行中的回合在快照里同样分页，并规定单个回合的条目超过上限后怎么处理。 |
| 12 | 重要 | `07-desktop.md` 5.7.5、5.7.6 | **"按回合虚拟化"不够用。**<br>Codex 式的长回合有几百到几千次工具调用，一个回合展开就是几千行，整回合挂载会突破 5.9.6 的滚动预算。<br>另外两件事也没写：向上加载历史时怎样锚定滚动位置；流式输出时怎样"贴底"。 | NFR-6；5.9.6 | 改为**按行**虚拟化：把回合标题、活动组摘要、消息、卡片拍平成一个行列表，活动组折叠时只占一行。写明两条规则：向前插入历史后视口不跳动；流式增长时自动跟随，用户向上滚动后停止跟随并显示 ↓ 按钮。M1a 的消息流就按这个结构写。 |
| 13 | 重要 | `12-milestones.md` 5.12.2 | **M1a 最简界面的范围要补两条规则。**<br>(a) **架构按最终形态写，视觉从简**：store、订阅、行虚拟化、streamdown 配置、HostPort 都按终态实现。否则 SP-6（M1b 开头）测的是将被重写的代码，M1b 的"界面打磨"会变成重写。<br>(b) 5.9.4 的 NFR-1 用例要在界面上断言这些变化：排队条；回合结束为"已取消"或"已中断"；卡片显示"已失效"；"已在 cupper-cli 处理"；重载渲染进程后恢复。但 M1a 的清单里没有回合状态的显示（回合标题被放在了 M1b）、没有 Toaster、也没有提权横幅。M1a 的 cupper-cli 带 `elevate`，作者自用时必须有横幅和原生确认框。 | 5.9.4；5.12.3 消息流一行（"按回合折叠与耗时"在 M1b） | 把上面这些界面元素列进 M1a 的桌面端清单。M1a 只做一种语言，但从第一天起所有文案都走 i18n 的键。 |
| 14 | 重要 | `09-testing.md` 5.9.7 SP-6 | **SP-6 能回答它要回答的问题，但有缺口：**<br>① 主进程事件循环延迟决定要不要换连接方案，却安排在 M1b 开头，那时 M1a 的主进程代码已经写完。而后备方案"utilityProcess 宿主桥 + 委托票据"要修改 5.8.1 的签名要求表（现在写的是"helper 一律不满足"），代价不小。<br>② "1,000 和 5,000 条消息"没有定义构成：回合数、条目类型的比例都没写，也没有"单个回合 2,000 个条目"的极端用例。<br>③ 5.9.6 有"agent 输出到屏幕的延迟"这条预算，但 SP-6 没有对应的测法。<br>④ 缺四个场景：会话切换；窗口被遮挡时持续流式输出；向上翻页时的滚动稳定；审查面板打开 5,000 行 diff（应在发布 CSP 下测，见第 6 条）。<br>⑤ CI runner 是虚拟机，帧时间只能看趋势，这一点没写明。 | ADR-03 的改变条件；技术选型讨论会 C5 | ① 把场景 3 中主进程转发的部分前移到 M1a，作为 windowBridge 任务的验收，用合成负载测，不需要界面。② 用 SP-1 录制的分布生成夹具。③ 假 agent 在 delta 里带时间戳，渲染进程在提交后的那一帧记录时间。④ 补齐上述四个场景。⑤ 写明判定以作者的机器为准，CI 只看趋势。 |
| 15 | 重要 | `10-release.md` 5.10.5；`05-flows.md` 5.5.7 | **"宿主退出"到"App 退出"之间有一段空窗，设计里没写。**<br>`autoInstallOnAppQuit=false` 时，`quitAndInstall()` 这一刻才让 Squirrel 经本地代理取 zip、解包、校验签名，全部完成后 App 才退出（已核对源码）。这段时间宿主已经退出，界面还开着；几百 MB 的包要数秒以上（时长未核实）。 | electron-updater 6.8.10 `out/MacUpdater.js` 中的 `quitAndInstall`、`handleUpdateDownloaded` | 写清四点：(a) 空窗期间 `hostLauncher` 不得自动重新拉起宿主，界面显示"正在安装更新"。(b) Squirrel 报错时（签名不符、磁盘满），重新拉起宿主、恢复界面并提示。(c) 走 `before-quit-for-update` 时，跳过 `lifecycle` 的 NFR-8 检查，因为 control 连接已经断开。(d) M2 的 LaunchAgent 如果配置了 KeepAlive，要先停掉，否则 launchd 会把宿主拉回来。另外，首次启动时用 `app.isInApplicationsFolder()` 检查安装位置，不在"应用程序"里就提示调用 `moveToApplicationsFolder()`：从"下载"文件夹或 App Translocation 路径运行时，Squirrel 无法替换包。 |
| 16 | 建议 | `10-release.md` 5.10.1 | CLI 放在 `Contents/Resources/bin/cupper`。Apple 的规则是：helper tool 放在 `Contents/MacOS/` 或 `Contents/Helpers/`；Resources 只放非代码；也不要把代码放进更深的子目录。放在 Resources 的 Mach-O 会被当作资源封存，严格校验时可能出问题（未实测）。 | Apple "Placing Content in a Bundle" | 改为 `Contents/Helpers/cupper`。并写明用户怎样把它加进 PATH，例如设置页提供"安装命令行工具"，创建符号链接。 |
| 17 | 建议 | `07-desktop.md` 5.7.9；`09-testing.md` 5.9.4 | e2e 构建写的是"打包方式和 fuses 与发布构建相同、ad-hoc 签名"。app-builder-lib 的源码明确警告：ad-hoc 签名同时开启 hardened runtime，需要 `disable-library-validation`，否则可能因库校验而启动失败。所以 e2e 构建的签名和 entitlements 必然与发布构建不同；依赖 hardened runtime 的安全检查只能在发布流水线上验证。 | app-builder-lib 26.15.3 `out/mac/MacTargetHelper.js` | 5.7.9 写明 e2e 构建采用哪一种做法（关闭 hardened runtime，或加上这项 entitlement），5.9.4 标出哪些安全检查只在发布构建上跑。`enableCookieEncryption` 在 ad-hoc 构建上可能引发钥匙串提示（未核实），e2e 构建可以关掉它。 |
| 18 | 建议 | `07-desktop.md` 5.7.2 `hostLauncher`；`05-flows.md` 5.5.1 | 拉起方式前后不一致：5.7.2 写 `child_process.spawn` 加 `detached: true`，5.5.1 和 ADR-01 写 `posix_spawn` 加 `POSIX_SPAWN_SETSID`。用 Node 拉起时，有几件事不由我们控制：底层是否用 posix_spawn、是否用 `CLOEXEC_DEFAULT`，以及宿主会继承 Electron 主进程的全部环境变量。 | ADR-01；5.8.5 进程卫生 | 由 `cupper_native.node` 调用 `cupper-sys::spawn`，与拉起适配器用同一套参数：SETSID、`CLOEXEC_DEFAULT`、环境变量白名单、经 stdin 传密钥。这样行为可测，以后 Q21 要调整拉起方式时也有一个统一的入口。 |
| 19 | 建议 | `01-repo-build-ci.md` 5.1.2、5.1.5 | (a) 沙箱 preload 不支持 ESM，Vite 要把 preload 打成单个 CJS 文件。(b) 在 pnpm workspace 下，electron-builder 收集 `node_modules` 容易出问题（26.x 的具体行为未核实）。 | Electron 44 `tutorial/esm.md` | 主进程、preload、渲染进程的运行时依赖（包括 electron-updater）全部由 Vite 打进 bundle；`apps/desktop` 不声明 `dependencies`，asar 里只有构建产物，原生扩展放在 asar 外。发布 CI 增加 asar 文件清单的断言。 |
| 20 | 建议 | `05-flows.md` 5.5.1 冷启动 | 启动路径还有几处可以并行或提速。 | NFR-6；Electron 44 `protocol.md`、`structures/custom-scheme.md` | (a) 窗口的数据连接与创建 BrowserWindow 并行建立，主进程先缓存入方向的帧，port 到位后再交付。(b) 用 `show: false` 加 `ready-to-show` 避免白屏。(c) 注册 `app://` 时加上 `codeCache: true`（要求同时设 `standard`）。(d) 可以评估 Electron 44 新增的实验性 API `protocol.registerSource`：静态文件不经主进程的 JS 线程，能降低启动期主进程的负载；但它的响应头是静态的，与按次生成 nonce 的 CSP 冲突，要和第 6 条一起权衡。 |
| 21 | 建议 | `07-desktop.md` 5.7.4 | 视觉常量与截图笔记不一致：设计写侧边栏约 260、内容最大宽度约 760，截图笔记估计分别约 170pt、约 520pt。这两个尺寸直接影响"像不像 Codex"。 | 截图观察笔记；PRD UI-6 | 把这些常量做成 token，注明来源是截图笔记，偏离时写出理由。UI-6 的人工评审用同尺寸窗口与截图并排比对。 |
| 22 | 建议 | `07-desktop.md` 5.7.4、5.7.5 | 几处布局细节，关系到复刻度和以后的改动成本：<br>① 右侧工作区在 Codex 里是标签容器，M1 也应按标签容器做，即使只有"审查"一个标签。<br>② 侧边栏顶部的标题行（Codex 放产品切换、Activity、搜索）没有保留，P1 加入这些入口时侧边栏要重排。<br>③ 新建会话页没有写布局，Codex 把工作区和分支选择放在输入框下方。<br>④ UI-0 只有快捷键，没有界面入口，Codex 标题栏有"打开"按钮。<br>⑤ 弱认证标记的位置不一致：ADR-05 和 5.8.1 写在标题栏，5.7.4 放在侧边栏底部。 | R01 第二节；截图观察笔记 | 右侧做成标签容器；侧边栏保留标题行，先放产品名和宿主状态；新建会话页按 Codex 布局；标题栏加"打开"按钮（终端、编辑器、Finder）；弱认证标记的位置统一写清。 |
| 23 | 建议 | `07-desktop.md` 5.7.9 | 三种构建都注册 `cupper://`。同时装了发布版和开发版时，LaunchServices 把链接交给哪一个是不确定的，点击宿主发出的通知可能打开错误的 App。 | 5.2.9 通知点击走 `cupper://session/<id>` | 开发构建用 `cupper-dev://`，e2e 构建用 `cupper-e2e://`；宿主按构建种类使用对应的 scheme。 |
| 24 | 建议 | `03-protocol-v1.md` 5.3.3、5.3.6 | 交互卡片在消息流中的位置可能丢失：快照由视图生成，如果视图把条目和交互请求分开存放，快照重放之后，卡片相对于工具行的位置就没了。 | 5.3.3"快照的内容" | 写明快照 ops 保持原始的相对顺序，或者让交互请求带一个锚点，例如 `after_item_id`。 |
| 25 | 建议 | `07-desktop.md` 5.7.1 | 主进程的入方向处理还可以更简单：直接按字节块转发、按字节确认，切帧交给渲染进程的 host-client（它本来就要处理 `0x02` 分片）。另外两点要注意：MessagePortMain 的 transfer 只能转移 MessagePortMain，ArrayBuffer 会被复制一次，经 contextBridge 再复制一次；吞吐量够用，但不要把 Node 的 Buffer 连同它的内存池整块克隆过去。 | Electron 44 `message-port-main.md`、`context-bridge.md` | 在"主进程切帧"和"渲染进程切帧"之间选一种写清楚；转发时使用独立的 ArrayBuffer。 |

## 对待定问题的意见

| 问题 | 意见 |
|---|---|
| Q1 桌面端连接模型 | **同意。这是对 ADR-03 的细化，不是推翻。** 主进程不解析入方向，就无法把事件分给多个窗口；每个窗口一条连接，订阅、真实输入检查和流控都天然按窗口隔离。M1 只有一个窗口，实际就是两条连接。按 `SecCode.h` 的说明，签名校验只核对"确立身份所需的封存部分"，不会逐个哈希大包里的资源，连接开销小。安全意图保住了：套接字、fd 和令牌都留在主进程，渲染进程只拿到 window 角色，没有 `privileged`。需要补的是第 1、2、4、5、25 条。后备方案"utilityProcess + 委托票据"一旦触发，就要修改 5.8.1 的签名要求表，所以主进程转发的测量要提前到 M1a（第 14 条）。ADR-03 中"唯一连接，所有窗口共用"一句按 5.13.2 改写。 |
| Q4 codex-acp 随包 | **同意随包。** 它是单个 JS 文件（Apache-2.0），体积小，Codex 不需要联网安装，也少了一类失败。它只能随 App 发版升级，这正符合 ADR-08 的版本锁定。补一点：THIRD_PARTY_NOTICES 要列出它内联的依赖。它作为 JS 资源放在 `CupperHost.app/Contents/Resources/runtime/` 下，不影响签名布局。 |
| Q11 Codex `threadStatus` 与回合边界 | **同意。** 补充界面侧的一点：界面只看宿主给的 `SessionSummary.status` 和回合事件，不感知 `threadStatus`；"需要处理"只由 cupper 自己的待处理交互请求决定，不由 `waitingOnApproval` 决定，避免两个来源互相矛盾。 |
| Q14 晚到的工具输出 | **部分同意。** "不进入新回合"是对的，但完全丢弃输出不妥：用户需要知道命令在取消后还在运行、最后的退出码是多少，因为它可能有副作用。建议保留退出码和截断后的尾部输出，原条目标 `late_output`；同时在消息流的当前位置写一条 `notice`，因为原回合可能已经折叠或滚出视口。注意，这会在回合结束、`turn_snapshots` 写入之后修改条目，设计要二选一：要么重写该回合的快照，要么把晚到的信息只记在新的 notice 里。 |
| Q2（相关）通知身份 | 后备方案"由 UI 主进程弹通知"在桌面端还没有落点：主进程现在不订阅任何流。如果启用，需要在 control 连接上订阅索引流（量小，主进程可以解析），取出 `notification.intent`，通知被点击后走深链接导航。建议在 5.7.2 预留一个 `notifications` 模块（只在后备时启用），并在 M1b 开工前完成 SP-10 的通知部分。 |
| Q6 只出 arm64 | **同意。** 以后加 x64 只是多一个构建目标（Node、原生扩展、宿主各多一份），`latest-mac.yml` 本来就支持多架构，不涉及兼容代码。 |
| Q20 更新时的临时端口、ShipIt | **风险比预期小**（见核实记录）。代理只监听 127.0.0.1，带 64 字节随机生成的 Basic 口令和随机路径，只提供已经下载好的 zip。Squirrel.Mac 上游用 `rename()` 替换整个包（或 `Contents` 目录），不会原地覆盖正在运行的可执行文件。仍按计划做升级演练，并把第 15 条的空窗处理纳入演练。 |
| Q22 退化认证的局限 | **同意接受。** 补一点：开发时改了主进程代码会重启 Electron，新实例不知道旧的密钥。`pnpm dev` 脚本应在重启 Electron 时一并重启宿主，否则每次都会停在"需要重启宿主"。 |

## 认可的部分

- 套接字、令牌和签名校验全部留在主进程。原生扩展返回 fd，再用 `net.Socket({fd})` 包装，只用公开 API。
- 选择 electron-builder 加 electron-updater 的理由成立，决定性的那条行为有源码依据（我复核了 6.8.10）。
- 5.7.8 给每个加固项都写了落点和验证方式，CI 会断言 fuses 和 entitlements 白名单。
- ShellAPI 加上浏览器 mock 和 TS 写的 mock 宿主，界面截图测试不依赖 Electron。
- 快照就是事件序列，增量和快照用同一个 `apply`；TS 视图和 Rust 视图用同一批金样例交叉验证。
- 三种构建各有自己的 bundle id 和数据目录；显式设置 userData，避开了大小写不敏感的卷上 "Cupper" 与 "cupper" 落到同一个目录的问题。
- 升级演练被列为 M1b 发布任务的验收项。

## 核实记录

所有外部请求都只带 User-Agent `cupper-research`，没有任何个人信息。下载的 npm 包只在临时目录里解压阅读，没有执行，结束后已删除。

| 来源 | 结论 |
|---|---|
| `registry.npmjs.org` 的 dist-tags | electron 的 latest 是 44.8.0，beta 是 45.0.0-beta.5；electron-builder 26.15.3（v26 分支 26.17.0）；electron-updater 6.8.9（v26 分支 6.8.10）；streamdown 2.7.0；@pierre/diffs 1.5.2；@streamdown/code 2.0.0；shiki 4.5.0 |
| electron-updater 6.8.10 `out/MacUpdater.js` | `autoInstallOnAppQuit=false` 时，`quitAndInstall()` 才调用 `nativeUpdater.checkForUpdates()`，等到 `update-downloaded` 再调用 `nativeUpdater.quitAndInstall()`。代理监听 `127.0.0.1:0`，带 64 字节随机生成的 Basic 口令和 128 位十六进制的随机文件路径 |
| Squirrel.Mac 上游 master `Squirrel/SQRLInstaller.m` | 用 `rename()` 换掉整个包；设置了 `SquirrelMacEnableDirectContentsWrite` 时换掉 `Contents` 目录；跨卷（EXDEV）时先删除再移动。不会原地覆盖可执行文件。**未核对 Electron 44 实际钉住的是哪个提交** |
| Electron v44.8.0 `docs/api` 与 `docs/tutorial` | `MessagePortMain.postMessage` 的 transfer 只接受 `MessagePortMain[]`；有 `input-event`、`before-input-event`（带 `isComposing`）、`before-mouse-event` 三个事件；macOS 上窗口被遮挡时页面即为 hidden，关闭 `backgroundThrottling` 才会一直保持 visible；沙箱 preload 只能使用 `contextBridge`、`ipcRenderer`、`webUtils` 等少数模块，不支持 ESM；contextBridge 传递的值都会被复制；权限类型中有 `clipboard-sanitized-write`；`protocol.registerSource` 是实验性 API；自定义协议有 `codeCache` 特权；有 `app.isInApplicationsFolder()` 和 `moveToApplicationsFolder()` |
| streamdown 2.7.0 的 dist | 默认 rehype 插件是 raw、sanitize、harden；harden 默认放行全部链接前缀、协议和图片前缀，并允许 data 图片；Mermaid 用 `dangerouslySetInnerHTML` 输出 SVG；代码高亮由插件返回 token 渲染 |
| @streamdown/code 2.0.0 | 使用 `createJavaScriptRegexEngine`，不需要 wasm |
| @pierre/diffs 1.5.2 的 dist | 渲染路径里有 `components/FileDiff.js` 第 1518–1579 行、`components/File.js` 第 819–822 行对 `innerHTML` 赋值；`utils/hostTheme.js` 调用 `createElement("style")`；包里同时带 `createOnigurumaEngine` 和 `new Worker` |
| react-style-singleton 2.2.3 | 动态创建 `<style>`，支持 nonce |
| app-builder-lib 26.15.3 | `signIgnore` 是一组正则，在传给 osx-sign 的 `ignore(file)` 里按文件路径匹配；源码中有"ad-hoc 签名加 hardened runtime 需要 `disable-library-validation`"的警告；schema 里有顶层的 `electronFuses` |
| Apple "Placing Content in a Bundle" | helper tool 放在 `Contents/MacOS/` 或 `Contents/Helpers/`；"Resources are everything that's not code" |
| macOS SDK `Security.framework/Headers/SecCode.h` | `SecCodeCheckValidity` 只校验"确立身份所需的封存部分"，推断大型 Electron 包的校验开销不大（仍建议按 SP-4b 遗留项实测一次） |
| 仓库内 SP-4a `cupper-protocol/src/events.rs` | `EventBody` 只有 `{type, data}`，`ts` 只在 `Event` 信封上 |

**未核实**：

- 被遮挡的窗口在 Electron 44 中是否完全停止 rAF。这是 Chromium 对 hidden 页面的标准行为，但没有实测。
- 权限处理器一律拒绝时，`navigator.clipboard.writeText` 是否确实失败。
- `enableCookieEncryption` 在不使用 cookie 的情况下是否会访问钥匙串。
- Squirrel 接手、解包、校验一个几百 MB 的包实际需要多长时间。
- electron-builder 26 在 pnpm workspace 下收集依赖的具体行为。
- Electron 44 内置的 libuv 在 macOS 上拉起子进程时，是否用 posix_spawn 加 `CLOEXEC_DEFAULT`。
- CLI 放在 `Contents/Resources` 时，codesign 严格校验和公证的实际表现。
