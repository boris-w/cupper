# 技术选型讨论会第二轮：桌面客户端工程师回应

- 日期：2026-10-16。本轮只读，没有修改仓库里的任何文件。
- 外部事实都在当天查证，编号 [Rx] 对应文末来源；标"判断"的地方是我的工程经验，没有外部数据支撑。

## C5 Electron 里由谁持有到宿主的连接

**最终立场：接受架构师的方案。** M1 只由主进程持有一条到宿主的连接，所有窗口都在这条连接上复用订阅，不用 utilityProcess。

**核实结果：**
- **utilityProcess 用的 helper。** 它默认用基础的 `<App> Helper.app` 启动，即 `CHILD_NORMAL`；只有设置 `allowLoadingUnsignedLibraries` 时才改用 Plugin helper [R1]。Chromium 的其他 utility 进程（网络、存储、数据解码等）也用这个基础 helper（判断）。所以放行它，就会把这些进程一起放进来。
- **helper 的签名 identifier 取决于打包工具。**
  - @electron/packager（Electron Forge 用的就是它）给 Renderer、GPU、Plugin 三个 helper 设的 bundle ID 和基础 helper **完全相同**（源码里的 `identifierIgnoresSuffix`）[R2]。放行 helper 就等于放行渲染进程和 GPU 进程，架构师的担心成立。
  - electron-builder 会给它们加后缀加以区分 [R3]，但上一条的问题依然存在。
- **"主进程认证后把 fd 交给 utilityProcess"做不到。** utilityProcess 的 stdio 只能配置 stdout 和 stderr，消息里只能传 MessagePortMain [R1]，要传 fd 就得写原生插件。我否决这个折中。
- **可行的折中是委托票据。** 主进程在已认证的连接上向宿主申请一张一次性票据，经 MessagePort 交给 utilityProcess；宿主只接受"helper identifier 加有效票据"的连接。这能行，但会让 ADR-05 变复杂，只作为后备。

**主进程转发的卡顿风险：低**，前提是守住以下四条：
1. 主进程只按长度前缀切帧，不做 JSON 解析和校验，校验放在渲染进程里。
2. 大约每 16 ms 批量转发一次。
3. 单帧不超过 64KB，与移动端的建议一致；大 blob 分块，按需拉取。
4. 渲染进程跟不上时，主进程停止读取，交给宿主的每客户端发送队列去断开并重同步（这是架构师已有的设计）。

只有可见的会话订阅全部事件。即使 6 个会话同时流式输出，数据量也在每秒几百 KB 以内，每帧的转发开销远低于 1 ms（判断）。SP-6 增加一个指标：在"6 个会话流式输出 + 拉取 5 MB blob + 5,000 条重同步"同时发生时，主进程事件循环延迟的 p99 小于 5 ms。达不到再启用票据方案。

**坚持的点：** 凭据和套接字不进渲染进程，渲染层只能通过很窄的 API 发命令。

**新增一条（回应安全）：** 对批准、回答提问、切换权限模式这类命令，主进程只在通过 `before-input-event` / `input-event` [R10] 观察到一次真实的 Enter 或点击之后 1 秒内放行。这样即使渲染进程被注入脚本，也不能悄悄地自我批准。

## C6 宿主放在哪里、怎么升级

**最终立场：**
- 接受把宿主放在包内。形式采用移动端提议的 app-like helper：`Contents/Helpers/cupper-host.app`，设为 LSUIElement，带描述文件。
- 升级沿用架构师的"标记待升级、空闲时自行退出"。
- 我撤回"包外的版本化 runtime 目录"作为默认方案，改作后备。

**架构师的方案已经解决的：** 宿主二进制本身被替换的问题。Rust 单文件在启动时就全部载入，替换磁盘上的文件不影响正在运行的进程。

**还没解决的三点，以及补法：**

1. **旧宿主在替换后拉起 Node 和适配器。**
   - 问题：替换之后，旧宿主拉起的是新包里的版本，违反 NFR-5 的版本锁定；替换过程中，路径还可能短暂不存在。
   - 补法：宿主启动时记录 Node 和适配器的 cdhash，每次拉起之前比对一次。不一致就进入"待升级"，新建会话和适配器重启都等新宿主来做。
2. **正在运行的适配器在旧文件被删后崩溃。**
   - 问题：适配器在旧包的文件被删除后，如果还要懒加载模块，就会崩溃。
   - 补法（转给 ADR-08）：适配器打包成单文件（bundle 或 SEA），启动时就载入全部代码。
3. **签名校验在替换后失败。**
   - 问题：Apple 对 `errSecCSStaticCodeChanged` 的定义是"进程开始运行之后，磁盘上的代码被修改了" [R4]。新 UI 按审计令牌校验旧宿主时可能因此失败，于是"新 UI 连接旧宿主"这一步走不通，需要 SP-4 实测。
   - 补法：控制安装时机，不让替换发生在宿主运行期间。Electron 内置的 autoUpdater 会在"下次启动时"自动应用已经下载的更新 [R5]，挡不住这种情况。所以改用 electron-updater，关掉 `autoInstallOnAppQuit` [R6]，手动控制安装，顺序如下：
     1. UI 通知宿主"待升级"；
     2. 宿主空闲后自行退出；
     3. UI 执行 quitAndInstall；
     4. 新 UI 拉起新宿主。

     M2 的后台模式下，关闭 UI 时不安装更新。
   - 边缘情况：用户手动拖入新版本覆盖安装。旧宿主会自己发现，等空闲后退出；新 UI 显示"等待旧版本宿主结束"，期间不信任旧宿主，也不向它发送命令。

**验证与后备：** SP-4 增加一项升级演练：在宿主和适配器都在运行时替换整个 .app，逐项检查上面三点。如果演练失败，就启用后备方案：
- 用 `ditto` 把签名完整的 `cupper-host.app`（含 embedded.provisionprofile）复制到 `~/Library/Application Support/cupper/runtime/<cdhash>/`。
- 签名、entitlement 和描述文件都跟着 bundle 走，与所在路径无关；我们自己写入的文件不带隔离属性。
- SMAppService 要求 BundleProgram 在包内，所以包内放一个很小的启动器，由它 exec 到选定的版本，PID 不变。

以上几点都是判断，要随 SP-4 一起验证。

## ADR-04：TS 类型的生成

**最终立场：接受架构师的方案。** 路径是：Rust 类型 → schemars → JSON Schema（提交到仓库）→ 用 @hey-api/openapi-ts 生成 TS 类型和 zod 校验。

**不用 ts-rs 作主路径。** 一份 JSON Schema 能同时服务 TS、Swift 和代码审查，UI 还需要 zod 做运行时校验，这两样 ts-rs 都提供不了。

**接受的条件：**
1. **事件信封保持开放。** `type` 是普通字符串，每种事件的 `data` 单独定义 schema，不把所有事件做成一个封闭的 `oneOf`。ACP 的生成脚本里写明，hey-api 会丢掉"可扩展联合"的语义，他们为此写了一整套自定义解析器和类型守卫 [R7]。我们用开放的信封绕开这个问题；遇到未知类型，UI 显示占位。
2. **整数精度。** ID 一律用字符串；seq 约定小于 2^53，否则也改用字符串。
3. **zod 不用 strict 模式**，未知字段放行。开发和测试构建校验每一条消息；生产构建只校验非 delta 的事件，delta 只做轻量检查。
4. **CI。** 用 `generate --check` 检查生成结果是否过期；生成出来的 TS 也提交到仓库，方便 agent 阅读；同一批金样例在 TS 端用 zod 校验，在 Swift 端做解码测试。
5. **ts-rs 只作后备。** 只有在 hey-api 对 serde 枚举的输出无法使用时，才改用 ts-rs 生成类型。不同时维护两套生成工具。

## 回答架构师

1. **文本片段的合并时间窗。**
   - 按订阅者分别设置：本地桌面每个条目 33–50 ms 合并一次，单次合并不超过 16KB。
   - 条目的第一个片段立即发出，以降低首字延迟。
   - 条目完成时，或者要发出其他事件之前，先把缓存的片段刷出去，保证因果顺序。UI 另外再按帧合并。
   - 手机用 150–250 ms，或者按积压程度自适应，由移动端决定。
   - 落盘时要不要合并，交给 ADR-06。
2. **后台模式下的系统通知：由宿主 helper 弹出全部系统通知，UI 自己不弹。**
   - 好处：只有一个通知身份，只需要一次授权，UI 退出后照样能用。
   - UI 把"窗口是否在前台"报告给宿主，同时满足 AP-3 和 MB-12。
   - 用户点击通知后，helper 打开 `cupper://session/<id>`，拉起 UI 或把它带到前台。
   - 不选"静默拉起 UI"，因为启动成本高、Dock 图标会闪一下、还要占用内存。
   - 需要实测 helper 在系统通知设置里显示的名称和图标是不是 cupper。如果不行，就退回"UI 在时由 UI 弹，不在时由 helper 弹"。
3. **深链接只做导航：不影响我对 PS-7 的设想。** 多窗口是 UI 内部的行为；"在新窗口中打开会话"本身也只是导航。M1 不做通过深链接预填草稿。
4. **最低支持的 macOS 版本。**
   - Electron 从 38 起要求 macOS 12 [R8]，UI 本身没有高于 13 的要求。
   - 我建议定为 14：测试矩阵更小；13 很可能已经不再有安全更新（判断）；CryptoKit 的 HPKE 也是从 14 开始才有的。

## 回答移动与安全工程师

1. **能否按清单设置 fuses：可以承诺。**
   - 打包时只对发布构建烧录，CI 用 `@electron/fuses read` 校验产物。
   - 启动参数里出现 `--remote-debugging-port` 时，主进程直接退出，因为这一项没有对应的 fuse。
   - 对开发的影响：日常用 `electron .` 加 Vite 热重载，跑的是没有设置 fuses 的开发二进制，不受影响。
   - 但 Playwright 驱动 Electron 时固定会加 `--inspect=0 --remote-debugging-port=0` [R9]，发布构建因此跑不了端到端测试。解决办法是另出一个 e2e 构建：打包方式和 fuses 与发布构建相同，只放开调试，并使用不同的 bundle identifier，发布版的宿主不接受它。
   - 开发构建同样使用独立的 identifier 和名称（cupper Dev）。
2. **深链接和自动化入口：可以承诺，不会触发审批或提权。**
   - 深链接按白名单路由，只做导航，参数只接受 ID。
   - 不提供 AppleScript 字典，不注册 Services 菜单。
   - 拖放只往输入框里插入内容，不会发送。
   - `second-instance` 和 `open-file` 也只做导航。
   - C5 里"必须观察到真实输入才放行"那一条，作为第二道防线。
3. **两种状态都能处理。**
   - 本端不可操作的审批：卡片显示为只读，给出原因，并标出哪些客户端可以处理；这张卡片上 Enter 和 Esc 不起作用。
   - 已连接的客户端列表：设置里新增"连接与设备"页，列出每个客户端的类型、名称、签名状态、连接时间和最近活动；M3 再加上撤销。
   - 这需要宿主提供两样东西：交互请求上的 `actionable_by` 和原因码，以及索引流里的 `clients`。
4. **开发构建或自行构建时显示"本地鉴权较弱"：接受。**
   - 宿主下发语义码，由 UI 做本地化。
   - 首次启动时弹一次说明；之后在标题栏放一个小标记，在设置页常驻显示。
   - 不在每次启动时弹出模态框。

## 来源

查证日期均为 2026-10-16。

- R1 Electron v44.8.0：`docs/api/utility-process.md`；`shell/browser/api/electron_api_utility_process.cc`（`CHILD_NORMAL` / `kChildProcessHelperPlugin`）。https://github.com/electron/electron
- R2 @electron/packager `src/mac.ts`（`updateHelperPlist`、`identifierIgnoresSuffix`）。https://github.com/electron/packager
- R3 electron-builder `packages/app-builder-lib/src/electron/mac/electronMac.ts`。https://github.com/electron-userland/electron-builder
- R4 Apple 文档：errSecCSStaticCodeChanged。https://developer.apple.com/documentation/security/errseccsstaticcodechanged
- R5 Electron autoUpdater 文档（"will always be applied the next time the application starts"）。https://www.electronjs.org/docs/latest/api/auto-updater
- R6 electron-updater AppUpdater 的 `autoInstallOnAppQuit`。https://www.electron.build/electron-updater.class.appupdater
- R7 ACP TS SDK 的 `scripts/generate.js`（关于可扩展联合的注释）。https://github.com/agentclientprotocol/typescript-sdk
- R8 Electron 38.0.0 发布说明（移除对 macOS 11 的支持）。https://www.electronjs.org/blog/electron-38-0
- R9 Playwright 源码 `packages/playwright-core/src/server/electron/electron.ts`。https://github.com/microsoft/playwright
- R10 Electron v44.8.0 `docs/api/web-contents.md` 中的 `input-event` 和 `before-input-event` 事件。
