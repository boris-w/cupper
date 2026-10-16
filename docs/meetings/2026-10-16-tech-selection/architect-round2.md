# 技术选型讨论会第二轮：架构师回应

- 日期：2026-10-16
- 角色：架构师（子 agent）
- 本轮只读，没有修改任何文件。
- 本轮新核实了三项事实（2026-10-16），列在文末。

## C1 seq 语义

**最终立场：日志里的 seq 保持连续；下发时允许合并，合并后的事件用 `from` 字段声明它覆盖的 seq 区间。** 客户端检查的不变式是：本事件的 `from` 等于上一个事件的 `seq` 加 1。没有 `from` 字段时，视为 `from = seq`。收到快照后，从它的 `at_seq` 重新开始计算。这就是编排者提出的折中。

**接受的部分**：合并 delta 是带宽和电量的第一要务。移动端估算，合并后字节数降到约 1/6，帧数降到 1/8；桌面也要求按 30–50ms 合并。合并分两级：

1. **写入时合并**：宿主按 30–50ms 的时间窗，把同一条目的 delta 合成一条日志事件再落盘。窗口大小由 ADR-06 定。这一级桌面和手机都受益，也减轻了落盘压力。
2. **发送时合并**：每个连接的发送队列在积压时、或按客户端类型（手机）再合并一次，合并结果带 `from`。宿主为某个客户端过滤掉的事件（例如手机无权查看的内容）也用 `from` 覆盖。

**坚持的部分**：连续性必须能在客户端校验。原因如下：

- 游标续传只在重新连接时起作用。同一条连接里，如果宿主的发送队列有 bug（例如慢客户端被重置时和新事件发生竞争），客户端会一直显示被截断的文本，自己却发现不了。
- 远程链路上，Noise 的计数器 nonce 能发现中继丢帧或乱序；本地 UDS 没有这一层，应用层的覆盖校验是连接内唯一的检查。
- NFR-1 的"不丢也不重复"测试可以直接断言这条不变式。
- 代价只是事件上多一个可选字段。

**回答移动端"只靠游标续传会不会漏掉某些场景"**：会漏掉上面说的"连接内丢失"。另外，移动端担心"M1 的客户端假设 seq 连续，M3 就要返工"，现在合并从 M1 起就写进协议，桌面在 M1 就会收到带 `from` 的事件，这个担心就不存在了。

## C2 schema 来源

**最终立场：仍以 Rust 类型为源，以生成的 JSON Schema 作为对外契约；移动端的三个条件全部接受；撤回"iOS 通过 UniFFI 共用 cupper-client"。** 如果 SP-4 中的 Swift 生成链达不到下面的验收标准，就改用 Protobuf。

**撤回 UniFFI 传协议类型**：同意移动端的理由，即每个事件都要穿过一次 FFI，而且 UniFFI 仍是 0.x。改为：

- iOS 使用由 schema 生成的原生 Swift 类型，客户端同步逻辑自己实现；
- UniFFI 只用于 E2EE 核心，是否采用待 SP-8 验证；
- 桌面和 iOS 两份同步实现的一致性，靠共享的金样例，加上一组"同步一致性"测试脚本来保证，覆盖断线、重同步、`from` 覆盖、冲突。

**接受三个条件**：

- **(a) 兜底代码由生成器产出。**
  - 生成器：仓库内自写的 Swift 生成器（作为 xtask）读取 JSON Schema。它为每个联合类型生成 `case unknown(type: String, raw: JSONValue)`，为每个开放枚举生成 `case unknown(String)`。
  - schema 规则：所有联合类型都用 `type` 字段作内部标签；所有枚举和联合类型都是开放的；事件信封带 `fallback{kind, summary}`，用于给不认识的条目显示占位。这一点采纳了移动端"结构化兜底"的设计。
- **(b) M1 的 CI 里就有 SwiftPM 包**，包含生成的类型和双向的金样例解码测试：新代码解旧样例；上一个发布版本的生成类型解新样例。
- **(c) 二进制内容走单独的帧类型**，JSON 里不出现 base64（见 C3）。

**不改选 Protobuf 的理由**：

1. **移动端看重的 `UNRECOGNIZED` 和 `unknownFields` 只在二进制格式下成立。** swift-protobuf 的 JSON 解码默认遇到未知字段就报错；打开 `ignoreUnknownFields` 后，未知的枚举值（尤其是字符串形式的）会被静默丢弃，也不会保留 unknownFields（见源码 `JSONDecodingOptions.swift` 的注释）。M1 到 M3 都走 JSON，所以这套兜底规则不管选哪条链都要自己定。
2. **信封要另写。** JSON-RPC 的 `params` 随 `method` 变化，proto 的 oneof 表达不了，信封只能在 proto 之外手写，或者改成 proto 风格的包装形状。另外，int64 在 JSON 里会变成字符串，oneof 会多包一层，TS 侧不如 `type` 标签的判别联合自然。
3. **先例与映射成本。** ACP 和 Codex app-server 都以 Rust 类型为源；宿主里的 Rust 类型直接就是契约，省掉了"prost 生成类型和内部类型互相映射"这一层。

**承认 Protobuf 的一个真实优势**：`buf breaking` 有成熟的破坏性变更检查。我们的 schema 只用四种结构：对象、带 `type` 标签的联合、开放枚举、可选字段。在这个受限范围内，用 xtask 自己写兼容性检查，规则不多。

**切换条件（SP-4 验收）**：Swift 生成器在 2 人日内做到以下三点，做不到就改用 Protobuf（以 `.proto` 为源，M1 仍用 JSON 映射，信封也随之改成 proto 定义）：

- 全部消息类型都能生成可编译的代码；
- 金样例的双向解码全部通过；
- 人为新增一个事件类型和一个枚举值后，旧的生成类型能把它们解成 `unknown`，而不是解码失败。

## C3 帧大小上限

**最终立场：接受本地和远程统一用 64KB 上限，同一个分帧层到处通用。** 如果本地放宽到 1 MiB，开发期就会有代码依赖大消息，问题要到 M3 才暴露。

**帧格式**（修正第一轮的设计，增加类型字节）：

- 结构：`u32 长度 | u8 帧类型 | 负载`。
- 大小：整帧不超过 65,519 字节，即 Noise 单条消息的上限 65,535 字节减去 16 字节的 tag。实现上取 64,000 字节，留出余量。
- 帧类型：
  - `0x01`：JSON 消息；
  - `0x02`：blob 分片。负载是一个小头部（blob hash、偏移、是否最后一片），后面跟原始字节；
  - `0x03`：预留给二进制消息编码。
- 批量不需要单独的帧类型：`stream/events` 本身就是数组，宿主按上限切分即可。

**超限内容的处理**：

- JSON 消息不跨帧分片，超过上限的内容一律改成 blob 引用：快照分页；长文本条目在快照里只给预览加 blob 引用；工具输出、整文件 diff、图片都走 blob。
- 第一轮漏掉了客户端发给宿主这个方向，这里补上：粘贴的长 prompt、图片等也受同一上限约束。协议需要新增 `blob/put`：客户端用 `0x02` 帧上传，宿主返回 hash，`session/send` 只引用这个 hash。

## C4 CLI 认证

**最终立场：**

- CLI 默认只读，不保存任何凭据；写操作经用户在场确认后，授权只在这条连接上有效。
- "可信 CLI 凭据"作为可选项保留，默认关闭，采用移动端的挑战-应答设计。
- 测试用的 principal 采用移动端的写法。

**分歧的核心**：文件型钥匙串的 ACL 信任的是 cupper-cli 这个程序。agent 执行 cupper-cli 时，钥匙串不会弹窗，挑战-应答也会正常完成。挑战-应答防得住重放和假宿主套取秘密，防不住 agent 直接调用正版 CLI。数据保护钥匙串加"每次都要用户在场"能挡住，但前提是 CLI 也做成带描述文件的 app-like bundle；而且每次都要在场确认，实际上就和我提的"按连接确认"等价了。

**危险的不只是 `approver` 范围**：向另一个处于完全访问模式的会话 `session/send`，等于借那个会话的权限执行命令。例如，受限会话 A 里的 agent 通过 CLI 给会话 B 发指令。所以我坚持：**任何写操作**默认都要用户在场确认，不只是审批类操作。

**接受移动端的以下几点**：

- 只要存在凭据，就必须走挑战-应答，秘密不离开钥匙串；
- `approver` 范围（审批、回答提问、切到完全访问、`allow_always`）每次都要用户在场，有效期很短。可信 CLI 凭据永远不包含这个范围；
- 可信 CLI 凭据用于脚本自动化，默认关闭。打开时，UI 要明确提示"本机的 agent 也能使用它，包括向其他会话发消息"；
- 开发构建和自行构建（没有 Team ID）时，使用退化的认证模式，并在 UI 上提示"本地鉴权较弱"。

**测试 principal 的最终写法**：

- `test_harness` 只存在于启用了 `test-harness` 编译特性的 debug 或测试构建里。release 二进制里根本没有这段代码，不是靠运行时开关关闭。
- 密钥由测试工具通过继承的 fd 传给宿主。
- 只能配合临时数据目录和临时套接字使用。
- NFR-1 的验收用这种构建加假 agent 来跑。

## C5 Electron 里由谁持有到宿主的连接

**最终立场：接受由 utilityProcess 作为数据通路上的"宿主桥"。** 条件是用"主进程认证 + 委托票据 + 父进程绑定"来确认它的身份，而不是在签名要求里直接放行 helper。

**已核实的事实**：

- utilityProcess 默认由普通的 Helper 拉起。只有设置了 `allowLoadingUnsignedLibraries`，才会改用 `Helper (Plugin)`。
- electron-builder 默认的 identifier：普通 Helper 是 `<appId>.helper`；渲染、GPU、插件 helper 分别是 `.helper.Renderer`、`.helper.GPU`、`.helper.Plugin`。
- 无法把已认证的 fd 交给宿主桥：utilityProcess 的 `stdio` 只能把 stdout/stderr 设为 pipe、inherit 或 ignore；`postMessage` 只能转移 MessagePortMain。

**推测（SP-4 核实）**：按 Chromium 的 helper 划分，普通 Helper 同时承载网络服务等 Chromium 自己的 utility 进程。所以"放行 `<appId>.helper`"这条签名要求无法精确到宿主桥。

**方案**：

1. **控制连接。** 主进程保留一条低流量的连接，按 `<appId>` 的签名要求认证，principal 是 `desktop_ui`。它负责窗口生命周期、通知意图、在场信号，以及签发委托。
2. **委托票据。** 主进程通过控制连接向宿主申请一张一次性票据，几秒内有效，再用 `postMessage` 交给宿主桥。票据只在内存里传递，不经过渲染进程、环境变量或文件。
3. **宿主桥接入。** 宿主桥连接宿主时出示票据，宿主同时检查三点：
   - 对端签名满足 `<appId>.helper` 加 Team ID；
   - 票据有效，而且没有用过；
   - 对端的父进程 PID 等于控制连接对端的 PID。主进程一直活着，这个 PID 不会被复用。

   三点都满足，这条连接就继承 `desktop_ui` 的权限。
4. **永远不接受的连接。** `.helper.Renderer`、`.helper.GPU`、`.helper.Plugin` 发起的连接一律拒绝；宿主桥也不能设置 `allowLoadingUnsignedLibraries`。
5. **SP-4 验证。** 在签名构建里核实宿主桥的实际 identifier；确认渲染进程拿不到票据；确认直接执行 Helper 二进制无法冒充宿主桥。

## C6 宿主二进制的位置与升级

**最终立场：**

- 宿主做成 app-like 的 helper bundle，放在 .app 里面运行，接受移动端的要求；
- Node 和适配器放在包外按版本分目录，接受桌面工程师的方向，这本来也是 ADR-08 的结果；
- 宿主本身不拷贝到包外。

**布局**：

- 宿主：`Cupper.app/Contents/Helpers/CupperHost.app`（设为 LSUIElement，有自己的 identifier）。到 M3 需要使用数据保护钥匙串时，在这里嵌入描述文件。这也让宿主有了正常的 TCC 身份和通知身份，正好回答了我第一轮留下的问题"后台模式下由谁弹通知"。
- Node 和适配器：按 AG-1 和 NFR-5，它们本来就不随 App 再分发，而是首次运行时按锁定版本安装，所以天然就在包外：`~/Library/Application Support/<App>/runtime/node/<ver>/` 和 `runtime/adapters/<name>/<ver>/`。
  - 宿主启动时，把自己锁定的版本解析成绝对路径，之后只用这些路径。
  - 新版本并排安装，旧宿主继续用旧版本；没有任何宿主在引用的版本才清理。

**宿主本身留在包内的理由**：

1. M2 的 LaunchAgent 要求 plist 放在 `Contents/Library/LaunchAgents`，`BundleProgram` 是相对于 bundle 的路径。宿主放到包外，就得再加一个启动器。
2. 包外是用户可写的位置，agent 能替换宿主二进制。UI 的双向签名校验会拒绝冒牌宿主，但结果是拒绝服务。放在 /Applications 的包内更干净。
3. Rust 宿主是单个静态链接的二进制，启动时就加载了全部代码，运行中不会从包里懒加载任何东西。Sparkle 或 Squirrel 替换整个 .app 之后，旧进程会继续用原来的 inode 运行（推测，SP-4 实测）。

**升级规则**：

- 宿主运行中不读取包内的任何资源，只用启动时解析好的绝对路径，也不重新执行自己的路径。
- 新 UI 发现宿主版本较旧，就把它标记为"待升级"；宿主空闲时退出，由 UI 从新包里重新拉起。

**回答"包外版本化目录与 helper bundle 的签名、描述文件能否兼容"**：

- 把带签名和描述文件的 helper bundle 原样拷到包外，签名仍然有效，因为签名和位置无关。
- 但这样会失去上面理由 1 和 2 的好处，所以不建议。
- 描述文件在包外能否被 AMFI 正常接受，我没有核实。

**SP-4 新增一项**：宿主运行期间，按 Sparkle 的方式替换整个 .app，验证三件事：旧宿主继续运行；新会话能正常拉起适配器；宿主空闲后能平滑切换到新版本。

## 回答桌面工程师的问题

1. **多窗口**：整个 App 共用宿主桥的一条连接。
   - 每个窗口从宿主桥拿一个自己的 MessagePort。
   - 同一条流被多个窗口订阅时，宿主桥只向宿主订阅一次，再分发给各个窗口。
   - 新开的窗口从宿主桥内存里的投影拿快照。这只是内存里的投影，不算 NFR-1 禁止的"缓存会话数据"。
   - 在场信号按整个 App 计算。
2. **会话内搜索放在宿主，条目带摘要信息**：两点都同意。
   - 搜索由宿主的事件存储提供，例如 SQLite FTS5，具体由 ADR-06 定。返回条目 ID、字段名和匹配位置；位置用 UTF-16 码元偏移，JS 和 Swift 用起来都方便。
   - 条目摘要包括行数、字节数、退出码、耗时、diff 统计和预览。它们都是语义字段，不带本地化文案。
3. **hunk 的稳定 ID 和内容摘要**：可以提供。
   - hunk ID 按内容寻址：对文件路径、改动前后的 blob 摘要、hunk 内容（变更行加上下文，不含 `@@` 行号）一起做哈希。所以同一文件里别的 hunk 造成的行号偏移，不会改变它的 ID。
   - 命令带上 `hunk_id` 和 diff 的基线（index 和工作区的摘要）作为前置条件。宿主重新计算当前的 diff，找不到同一个 hunk 就返回 `conflict`，原因是 `state_changed`。
   - 具体做法以 SP-5 的结论为准。
4. **2 秒冷启动的预算**：宿主侧 p95 不超过 400ms：

   | 环节 | 预算 |
   |---|---|
   | 拉起宿主并完成初始化 | 约 150ms |
   | 握手和签名校验 | 约 100ms |
   | 首屏快照：索引，加最近一个会话的最后一页（约 50 个条目，不加载整段会话） | 约 150ms |

   剩下约 1.6 秒留给 Electron 外壳启动和首帧绘制。宿主已经在运行时（UI 重启，或 M2 的后台常驻），宿主侧只需约 100ms。数据迁移不计入这个预算，要单独显示进度。以上都以 SP-4 实测为准。

## 本轮新核实的事实（2026-10-16）

| 事实 | 来源 |
|---|---|
| swift-protobuf 的 JSON 解码默认遇到未知字段就报错；打开 `ignoreUnknownFields` 后，未知的枚举值（尤其是字符串形式）会被静默忽略 | apple/swift-protobuf 主分支 `Sources/SwiftProtobuf/JSONDecodingOptions.swift` |
| utilityProcess 的 `stdio` 只能把 stdout/stderr 设为 pipe、inherit 或 ignore；`postMessage` 只能转移 MessagePortMain；设置 `allowLoadingUnsignedLibraries` 时改由 `Helper (Plugin).app` 拉起 | electronjs.org/docs/latest/api/utility-process |
| electron-builder 默认的 helper identifier：`${appBundleIdentifier}.helper`、`.helper.Renderer`、`.helper.GPU`、`.helper.Plugin` | electron-builder `packages/app-builder-lib/scheme.json` |
