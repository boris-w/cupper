# 技术选型讨论会：移动与安全工程师发言

- 日期：2026-10-16
- 角色：移动与安全工程师（子 agent），独立发言
- 范围：从 M3（iOS + 中继 + E2EE）和本地安全的角度审视 ADR-01 至 ADR-05，重点是 ADR-02、ADR-04、ADR-05；同时说明 ADR-12、ADR-15 对现在这些选择的约束
- 依据：议程；PRD v1.0（第 6.2–6.4 节、NFR-1、NFR-2、NFR-10、SEC-8）；R03 报告；`architect.md` 1.3、1.4、2.3、2.6、3.5、5.1
- 标注：**核** 指读过一手来源（本机 Xcode SDK 26.5 头文件、官方文档、仓库源码或注册表）；**推** 指推断；**实测** 指在本机（macOS 26.4）跑过。来源编号 [S#] 见文末，访问日期都是 2026-10-16。

## 核心结论

1. **ADR-02：从 M3 和安全的角度，我推荐用 Rust 写宿主。** 如果大家按 M1 成本选了 TS，我也能接受，但有一个硬条件：配对、E2EE、推送加密、钥匙串访问、对端签名校验，全部放进同一个 Rust crate。桌面通过 napi-rs 调用它，iOS 将来通过 UniFFI 调用它。TS 里不手写任何协议级的密码学。我不推荐 Swift 宿主，因为 ACP 没有官方 Swift SDK，而 ACP 每 1–2 周发一版。
   跨端要共享的是 E2EE 核心（大约 1–3 千行），不是协议类型，也不是渲染。协议类型应该由 schema 生成各端的原生类型。
2. **ADR-04 里真正不可逆的是 schema 语言和演进规则，编码可以晚定。**
   - schema 源必须能生成 Swift 类型，并且能容忍未知的枚举值和未知字段。按这个条件，我首选用 Protobuf 当 schema，M1 线上仍然走它的 JSON 映射。
   - 下面几条现在就要定：帧大小上限（≤ 64KB，和 Noise 对齐）、二进制帧、批量帧、"delta 可以合并，所以单个客户端看到的 seq 可以不连续"。
   - 我做了一个粗略的模拟：对流式输出来说，合并 delta 能把字节数降到约 1/6，换成二进制编码只能再省约 40%。所以电量和流量主要看合并和订阅粒度，编码排在后面。
3. **iOS 由用户自己构建，版本必然错开。协议要做到以下几点：**
   - 握手时协商版本区间和能力位；
   - 审批和提问采用"必须理解"语义：手机不认识的审批类型，只显示、不能批准，并且由宿主强制执行；
   - 审批摘要由宿主计算，手机原样回显，不做跨语言的规范化；
   - 金样例双向兼容测试从 M1 起就进 CI，包括一个 Swift 解码测试；
   - 密码套件不在线协商，随大版本固定，避免降级。
4. **ADR-05：推荐 Unix 域套接字，配合以下几层防护：**
   - 双方都用 `LOCAL_PEERTOKEN` 取对端的审计令牌，校验代码签名；
   - CLI 的凭据要经用户在场确认后才发放，认证时走挑战-应答；
   - 按 principal 和权限范围分级授权：审批和提权只开放给桌面 UI、已配对手机，以及经过用户在场确认的 CLI；
   - 启动 agent 时做进程卫生（清理环境变量、关闭继承的 fd）。
   XPC 要等宿主改成 LaunchAgent、并且有 Swift 代码时才明显更好，我把它留作备选。继承的 socketpair 只能当启动阶段的辅助手段。
5. **SEC-8 的真实边界要说实话。** 没有哪种本地传输挡得住下面两条路：一是"直接运行合法的 cupper-cli"；二是"绕开 cupper，直接改 agent 自己的权限配置"。
   - 真正的墙是 agent 自己的沙箱：Claude Code 和 Codex 在 macOS 上默认都禁止连接 Unix 套接字 [S20][S21]。
   - cupper 能做的，是关掉"一行命令就能替自己批准"的捷径，让攻击变成"注入一个已签名的进程"或者"骗过用户在场确认"，并且留下痕迹。
   - 这一点要写进文档，不能宣传成"隔离"。
6. **这些地方现在选错，M3 就要返工：**
   - 宿主没打包成 app-like bundle。macOS 的数据保护钥匙串和安全隔区都需要描述文件，而描述文件只能放在 app-like bundle 里 [S10]。
   - 用了 Electron 却没烧 fuses；用 Node SEA 打包却没关掉 NODE_OPTIONS 扩展。这两种情况下，对端代码签名校验形同虚设 [S16][S17]。
   - principal 类型没有预留"远程设备"。
   - 命令信封没有预留签名字段。
   - 用了 CryptoKit 没有的原语，比如 BLAKE2、XChaCha、crypto_box [S5]。
   - 推送的"通知意图"schema 没有考虑 4KB 上限。
7. **代码主要由 AI 编码 agent 来写，所以密码学必须遵守以下规则：**
   - 只调用库，不手写原语；
   - 只保留一份实现，用外部向量校验（Noise 的 cacophony、RFC 9180、Wycheproof），再用一份独立的实现做交叉验证；
   - 负面测试必须覆盖"恶意中继替换配对应答"，以及篡改、重放、降级；
   - 密码学目录的改动必须经过人工审查。
   共享同一份代码时，两端互通测试会天然通过，证明不了正确性，所以必须有外部向量。

---

## 1. 判断前提：M3 的几个事实

- **iOS 由用户自己构建**（U7），使用自己的 APNs 凭据，中继也是自己部署（RL-1、RL-3）。所以 iOS 的每一个构建依赖，都会变成用户的门槛；桌面 App 自动更新而手机不会，两边的版本必然错开（NFR-10）。
- **M3 的中继无状态**（U1）。推送采用 N-2，也就是桌面按每台设备分别加密、iOS 的 NSE 解密（架构师 3.5）。断线后靠游标从桌面补齐。
- **iOS 只在前台保持连接**，后台靠推送唤醒（R03 §3）。这意味着电量和流量主要花在前台的流式输出和重连上。
- **NFR-1 的瘦客户端原则**：分组、状态、耗时都由宿主算好，手机不需要会话逻辑。

所以"跨端共享"真正需要覆盖的只有这几项：

| 内容 | 两端都需要？ | 共享方式 |
|---|---|---|
| 协议类型（事件、命令、能力） | 是，量最大 | 由 schema 生成各语言的原生类型，不要求同一种语言 |
| E2EE 通道（Noise 握手、传输、重连） | 是，风险最高 | 共享一份实现，或者两份实现加外部向量 |
| 配对（二维码解析、psk、密钥确认） | 是 | 同上 |
| 推送加密 | 桌面加密，NSE 解密 | 同上，也可以两端都用 HPKE 这种标准构造 |
| 审批摘要、版本与能力逻辑 | 是，但逻辑很薄 | schema 加金样例 |
| 会话逻辑、渲染 | 否（瘦客户端） | 不共享 |

---

## 2. ADR-02 宿主语言

### 2.1 核实过的事实

| 事项 | 状态 | 来源 |
|---|---|---|
| UniFFI | 0.32.2（crates.io，2026-09-23），仍是 0.x，几乎每个小版本都有破坏性变更（0.32.0，2026-06-30）。Swift 绑定被官方定位为 production-quality，但近期仍在修 FFI 边界的问题，例如 0.31.2 修了 "`FfiConverterString` 会悄悄去掉开头的 BOM"，0.31.1 修了 "async 代码内存泄漏"。生产用户有 matrix-rust-sdk（Element X iOS 用它）和 Bitwarden 的 sdk-internal（锁定 `uniffi = "=0.32.0"`）【核】 | [S1][S2][S3] |
| snow（Rust 的 Noise 实现） | 0.10.0（2025-07-19），仓库 2026 年仍有提交。README 原文："This library has not received any formal audit"。跟踪 Noise rev 34，除 fallback 修饰符外的特性都已实现，支持 psk 修饰符；仓库带 cacophony 测试向量【核】 | [S4] |
| clatter（Rust，带后量子扩展的 Noise） | 2.3.0（2026-08-30），同样没有正式审计。README 建议"不需要 PQ 时优先用 snow"。Codex 的 exec-server 中继就用它（R03 CX5）【核】 | [S6] |
| noise-c | 没有发布过版本，最后一次提交是 2023-12-06，基本不再维护。**不推荐**【核】 | [S7] |
| JS 的 Noise 库 | 没有成熟的通用库。`noise-handshake`（holepunch）只支持 psk0 位置，依赖 sodium-universal；libp2p-noise 绑定在 libp2p 上【核】 | [S8] |
| noble（JS 的曲线、加密、哈希） | 已审计，最近一次是 2026-08 Trail of Bits 的审计。但 README 自己说明：JIT 和 GC 使 JS 很难做到常量时间【核】 | [S9] |
| swift-sodium | 2026 年恢复维护（0.10.0 于 2026-01-06，0.11.0 于 2026-05-04），仓库里自带预编译的 `Clibsodium.xcframework`【核】 | [S11] |
| CryptoKit | Curve25519 和 ChaChaPoly 从 iOS 13 起，HKDF 从 iOS 14，HPKE 从 iOS 17 / macOS 14，ML-KEM768/1024 和 X-Wing 从 iOS 26 起。**没有** XChaCha20、BLAKE2、Argon2。安全隔区只支持 P-256（iOS 26 起增加 ML-KEM），**不支持 X25519**【核，SDK swiftinterface】 | [S5] |
| Node 访问钥匙串 | `node-keytar` 已于 2022-12 归档，需要自己找原生插件替代【核】 | [S12] |
| ACP 官方 SDK | TS v1.8.0、Rust 3.3.0（都在 2026-10-09 发布）；没有官方 Swift SDK（R02 §2）【核】 | R02 |

### 2.2 两条共享路径的比较

| | **A. 共享 Rust 核心（UniFFI）** | **B. 两份实现 + 测试向量** |
|---|---|---|
| 正确性 | 只有一份实现，安全评审的范围减半（M3 退出标准里有安全设计评审） | 两份都要审；Swift 端没有成熟的 Noise 库，要基于 CryptoKit 自己实现 Noise（推测 600–800 行），由 AI 来写风险高 |
| 互通 | 两端代码相同，互通天然成立。**但这证明不了正确性**，仍然需要 cacophony 向量和一份独立的实现做交叉验证 | 互通测试本身就有意义；向量是硬性要求 |
| 用户自建 iOS 的门槛（U7） | 要装 Rust 工具链和 iOS target，或者使用预编译的 xcframework。预编译 xcframework 等于让用户信任一个密码学二进制，和"自建以获得信任"的初衷有冲突，需要可复现构建和校验和。swift-sodium 也是这么处理的 [S11] | 只用 SwiftPM，Xcode 里一键构建 |
| NSE | NSE 也要链接 Rust 库。体积不是问题；内存上限要实测（推） | 推送如果用 HPKE，iOS 直接用 CryptoKit，NSE 很轻 |
| 版本风险 | UniFFI 是 0.x，大约每季度有破坏性变更 [S1] | 没有 FFI 风险 |
| 后量子 | 需要时 clatter 已经可用 | CryptoKit 的 X-Wing 要 iOS 26 起；Noise 的 PQ 部分要自己实现 |

**我的倾向**：E2EE 握手和传输用 A（snow）。推送解密可以用 B：两端都按 RFC 9180 HPKE 实现，桌面用 rust-hpke（0.14.1，用 RFC 向量测试 [S13]），iOS 用 CryptoKit 的 HPKE [S5]，这样 NSE 不必链接 Rust。最终由 ADR-15 和 SP-8 决定。**宿主选 Rust（或"TS + Rust 安全 crate"），这两个选项都保留；纯 TS 宿主就只剩 B。**

### 2.3 按 8 条标准打分（1–5，越高越好，只代表我负责的范围）

| # | 标准 | TS/Node | Rust | Swift | TS + Rust 安全 crate |
|---|---|---|---|---|---|
| 1 | 复刻保真度 | 中性（宿主不画界面） | 中性 | 中性 | 中性 |
| 2 | M3 的 iOS 共享 | 2：只能共享 schema 生成的类型，加密要在 Swift 再写一份 | 4：E2EE 核心可以经 UniFFI 共享 | 5：类型、加密、钥匙串代码直接共享 | 4：同 Rust 列 |
| 3 | 生态成熟度 | ACP 5；安全相关 2–3（Noise 库薄弱，keytar 已归档，对端签名校验要写原生插件） | ACP 4；安全相关 5（snow、RustCrypto、hpke、security-framework 3.7.0） | ACP 2；安全相关 4（CryptoKit 很强，但没有 Noise 库） | ACP 5；安全相关 5 |
| 4 | AI 编写的效率和出错率 | 效率 5；安全代码容易出错（Buffer/Uint8Array 混用、编码、时序）：3 | 效率 3；类型状态机能把"握手 → 传输"这类不变量编进类型：4 | 效率 3；Swift 6 并发和 Codable 联合类型容易出错：3 | 4；但跨 FFI 边界多出一类错误 |
| 5 | 性能与资源 | 3 | 5 | 5 | 3 |
| 6 | 签名、公证、分发 | 3：hardened runtime 下需要 JIT 相关权限；Node SEA 默认接受 `NODE_OPTIONS` [S17]；每个原生插件都要单独签名 | 5：普通的 Mach-O | 5 | 3 |
| 7 | 贡献者上手 | 5 | 3 | 3 | 4（大多数人不会碰这个 crate） |
| 8 | 长期维护 | 3：宿主持有审批权和 M3 的身份密钥，npm 的传递依赖树大，2025-09 有 chalk/debug 投毒和 Shai-Hulud 蠕虫 [S22] | 4 | 2：ACP 要自己维护 | 3：两套工具链 |

### 2.4 推荐和改变条件

**推荐 Rust 宿主。** 理由按分量排序：
- 安全关键的进程（审批权、M3 的身份密钥）是普通的原生二进制，可以开 hardened runtime 而不需要 JIT 例外；
- 密码学、钥匙串、代码签名校验都有成熟的库；
- ADR-15 时两条共享路径都还能选。

**可以接受 TS 宿主，但有三个前提**：
1. 存在一个 `cupper-secure` Rust crate，负责配对、Noise、推送加密、钥匙串访问、对端签名校验，通过 napi-rs 接入（napi 3.14.2，2026-10-06）。
2. 宿主如果用 Node SEA 打包，设置 `execArgvExtension: "none"` [S17]，并评估 hardened runtime 例外权限的影响。
3. 宿主打包成 app-like bundle，见第 6 节。

**在以下情况我会改变推荐**：
- SP-4 显示 Rust 宿主骨架的工期明显超过 TS（比如超过 1.5 倍），或者 ACP Rust SDK 在 SP-1 要用到的能力上落后于 TS SDK：改为推荐"TS + Rust 安全 crate"。
- ADR-03 选了 SwiftUI 桌面，团队愿意自己维护 ACP 的 Swift 绑定：Swift 宿主的 M3 收益最大，我会支持，前提是 ACP 层的维护由架构师兜底。
- 如果 ADR-02 在 TS 和 Rust 之间僵持，建议花半天做 UniFFI 冒烟：把 snow 的握手编进 iOS 模拟器和 NSE target，量出构建步骤和包体，再做决定。

---

## 3. ADR-04 协议

### 3.1 schema 怎样同时生成 Swift 类型

对 M3 来说，schema 源要满足四个条件：
1. 有成熟的 Swift 生成器；
2. 未知的枚举值和联合变体能落到兜底分支，而不是解码失败。Swift 的 `Codable` 枚举遇到未知值默认会抛错；
3. 能原生携带二进制（bytes）；
4. 能同时生成 TS（以及 Rust）。

| 候选 | Swift 生成 | 未知值处理 | 评价 |
|---|---|---|---|
| **Protobuf（proto3，使用 `optional`）** | swift-protobuf 由 Apple 维护（1.39.0，2026-10-12）；TS 用 protobuf-es，Rust 用 prost 0.14.4 | proto3 枚举生成 `UNRECOGNIZED(Int)`，消息保留 `unknownFields` [S14] | 四条全满足。JSON 映射是标准的，M1 可以走 JSON。要注意几个坑：int64 在 JSON 里是字符串、默认值会被省略、oneof 的表示方式 |
| JSON Schema 或 TypeSpec → quicktype | 能生成 Swift `Codable`，但 `oneOf` 和带判别字段的联合类型支持弱，最后多半要手写（推） | 要自己约定 | 可以用，但必须在 M1 就用 Swift 解码测试证明生成链能跑通 |
| 以 TS 类型为源（zod 等） | 链路最长 | 弱 | Swift 会永远是二等公民 |
| 以 Rust 类型为源 + typeshare | typeshare 支持 Swift（typeshare-core 1.13.4） | 未知变体的处理要核实（推） | 只在宿主是 Rust 时考虑 |
| CBOR/MessagePack + CDDL | Swift 生成器很少 | 弱 | 不推荐 |

**推荐**：Protobuf 作为 schema 源。M1 线上用 proto3 的 JSON 映射，便于调试、写测试夹具，AI 也好读。M3 的远程通道在握手时可以协商切到二进制 protobuf，用的是同一套生成代码。
**可以接受的替代**：架构师选择 JSON Schema 或 TypeSpec 也可以，条件是：M1 的 CI 里有 Swift 解码测试；未知变体的兜底由生成器产出，不靠手写；二进制内容走单独的帧类型，不放进 JSON 里做 base64。
**UniFFI 不用来传协议类型**：UniFFI 的 record 是 FFI 类型，不是线上格式。让每个事件都穿过一次 FFI 既浪费又多一层风险。

### 3.2 编码对移动端带宽和电量的影响

我用一个粗略模型估算一轮长回合：3000 个 token 级的 delta，正文共 37KB。假设每帧的中继路由信封是 64 字节，Noise 每条消息的开销是 18 字节（16 字节 tag 加 2 字节长度），JSON 里的 ID 是 ULID 字符串。结果【实测，模型估算】：

| 方案 | 总字节 | 相对正文 | 帧数 |
|---|---|---|---|
| JSON，每个事件一帧 | 910KB | 24.4 倍 | 3000 |
| 二进制（估算），每个事件一帧 | 448KB | 12.0 倍 | 3000 |
| JSON，8 个事件批成一帧 | 699KB | 18.7 倍 | 375 |
| **JSON，把连续 8 个 delta 合并成一个事件** | **147KB** | **3.9 倍** | 375 |
| 同上，加每帧 deflate | 116KB | 3.1 倍 | 375 |
| 二进制，合并 delta | 89KB | 2.4 倍 | 375 |

结论：
- **合并 delta 是第一位的**：字节数降到约 1/6，帧数降到 1/8。帧数少意味着 AEAD 运算少、无线电唤醒少、主线程解码少。合并之后，二进制编码只能再省约 40%。电量主要花在屏幕和无线电上，解码 JSON 的 CPU 开销是次要的（推）。
- **这需要协议现在就定的语义**：
  - `item.delta` 只追加、可以合并，宿主的每客户端发送队列可以按客户端类型（手机）或积压情况合并；
  - 所以单个客户端收到的 **seq 单调递增，但可以不连续**。客户端不能用"seq 不连续"来判断丢消息，丢失检测靠游标续传（NFR-2）。如果 M1 的客户端假设 seq 连续，M3 就要返工。
- **订阅粒度**：手机默认只订阅索引流和当前打开的会话；历史向前分页；大内容（整个文件的 diff、图片）按引用拉取。快照只给最近 N 个条目的投影，不要一次给出整段会话，否则长会话重同步时手机会一次拉几 MB（这一点影响 ADR-06）。
- **不要在加密之前压缩**：中继能看到密文长度，而内容里既有秘密（文件里的 key），也有攻击者能影响的内容（agent 读取的网页），压缩后存在 CRIME 类的长度旁路（推，风险低，但没必要冒）。M3 先不压缩，合并 delta 已经拿到大部分收益。E2EE 之后中继的 WebSocket 压缩也就失效了。
- **编码真正敏感的地方是推送**：APNs payload 上限 4KB（R03 P3），扣掉 `aps` 字段、base64 膨胀和 HPKE 封装（32 字节 enc 加 16 字节 tag），可用的明文大约 2.9KB（推，估算）。"通知意图"的 schema 要紧凑，命令预览要截断，详情等打开 App 后再拉取。

### 3.3 iOS 自建、版本必然错开时的版本协商

常见的错开方向是桌面较新（Sparkle 自动更新），iOS 停在用户某次构建时的提交。也可能反过来：用户从 main 分支构建了 iOS。

1. **握手**：`hello{proto:{min,max}, client:{kind, version, commit}, features[]}` → `welcome{proto_selected, host_features[], host_version, min_supported, upgrade_hint}`。同一个大版本内只做加法；破坏性变更就升大版本。宿主承诺至少支持前 N 个大版本，或者一个固定的时间窗口（建议不少于 6 个月，交架构师定）。版本过旧时，手机显示"请重新构建 iOS App（最低提交 X）"，而不是直接报错退出。
2. **未知内容**：未知字段忽略；未知的枚举值落到 `unknown`；未知的事件类型显示为"不支持的条目"占位。占位用结构化的兜底字段（例如 `fallback_kind` 加一个短文本），不能直接丢弃，否则用户会误以为没事发生。
3. **审批和提问的"必须理解"语义（安全相关）**：手机在 `features` 里声明自己支持哪些交互请求类型和版本。对手机不支持的类型，宿主下发时标记 `actionable=false`，手机只显示"请在电脑上处理，或升级 App"。**宿主在处理 resolve 时也要按 principal 的能力再检查一遍**，不能相信客户端的判断。否则一个旧 App 可能在没显示出某个关键新字段的情况下就批准了。
4. **摘要绑定（SEC-10）**：`content_digest` 由宿主对自己存储的规范表示计算，随请求一起下发；手机 resolve 时原样回显，宿主比较。手机不重新计算，所以不需要跨语言的 JSON 规范化（Swift 的 `JSONEncoder` 和 JS 的 `JSON.stringify` 在字段顺序、数字格式上本来就不一致）。也就不存在"切到二进制编码后算不出同一个摘要"的问题。
5. **命令信封现在就预留 `attestation` 字段（可选）**，M3 和 M4 可以放手机的签名。比如锁屏上直接批准，或者 M4 的存储转发，签名对象是 `(request_id, digest, option, command_id, ts)`，可以用安全隔区里的 P-256 密钥加 Face ID 签（CryptoKit 安全隔区只支持 P-256 [S5]）。
6. **密码套件不在线协商**：一个大版本固定一个套件，比如 `Noise_*_25519_ChaChaPoly_SHA256`，协议名和版本写进 Noise 的 prologue（会被握手哈希认证）。换套件就升大版本，WireGuard 也是这么做的。这样中继没有机会做降级。
7. **中继信封单独版本化**：中继只解析路由信封，不碰应用协议的版本。中继也由用户自己部署，它的升级节奏和 App 无关。
8. **金样例**：每个协议版本的样例都留在仓库里。CI 里做两个方向的测试：新代码解码旧样例；用上一个已发布版本生成的 Swift 类型解码新样例（向前兼容）。**M1 就建一个只含生成类型和解码测试的 SwiftPM 包**（成本推测约 1 人日），不要等到 M3 才第一次用 Swift 解码这套协议。

### 3.4 Noise 对帧格式的要求

- 一条 Noise 消息最多 65535 字节（R03 K1），所以宿主协议的帧上限定为 64KB，**本地传输也用同一个上限**，同一个分帧层到处都能用。超过上限的内容走 blob 引用或分片。
- 帧类型字段要能区分：JSON 消息、二进制消息、批量（一帧装多个事件）、blob 分片。
- 帧和传输解耦：本地是 UDS 上的长度前缀帧；远程是中继 WebSocket 上的 Noise 消息，一帧对一条消息。

### 3.5 打分和推荐

| # | 标准 | JSON + TS 类型为源 | JSON + JSON Schema/TypeSpec | **Protobuf 为 schema（M1 用 JSON 映射）** | CBOR + CDDL | Rust 类型 + typeshare |
|---|---|---|---|---|---|---|
| 1 | 复刻保真度 | 中性 | 中性 | 中性 | 中性 | 中性 |
| 2 | iOS 共享 | 2 | 3 | 5 | 2 | 3 |
| 3 | 生态成熟度 | 4 | 3 | 5 | 2 | 3 |
| 4 | AI 效率和出错率 | 5 | 3 | 4 | 2 | 4 |
| 5 | 移动端带宽 | 3 | 3 | 5（可以切二进制） | 4 | 3 |
| 6 | 签名、分发 | 中性 | 中性 | 中性 | 中性 | 中性 |
| 7 | 贡献者上手 | 5 | 3 | 4 | 2 | 3 |
| 8 | 长期维护 | 3 | 3 | 4 | 2 | 3 |

**推荐**：Protobuf 作为 schema，M1 用 JSON 映射，再加上 3.2–3.4 节的语义规则。
**改变条件**：如果架构师给出一条 JSON Schema 或 TypeSpec → Swift 的生成链，并且在 SP-4 里证明它能生成带未知值兜底的联合类型，我就不坚持用 Protobuf。真正不能让步的是第 3.3、3.4 节的规则，以及 M1 就有 Swift 解码测试。

---

## 4. ADR-05 本地传输与认证（SEC-8）

### 4.1 威胁模型

攻击者是 agent，或者 agent 启动的子进程（包括 MCP server 和 hooks，它们都在 Claude Code 的沙箱之外，以用户的全部权限运行 [S20]）。它和宿主是同一个 uid，可能被提示注入驱动。目标是替自己批准、切换到完全访问，或者选择"总是允许"。

| 编号 | 攻击路径 |
|---|---|
| P1 | 直接连上宿主的套接字，发命令 |
| P2 | 偷凭据：文件、环境变量、argv、钥匙串 |
| P3 | 运行合法客户端，比如 `cupper-cli approve ...` |
| P4 | 向合法进程注入代码：`ELECTRON_RUN_AS_NODE`、`NODE_OPTIONS`、`--inspect`、`DYLD_*`、调试器附加、改 asar 或 JS 资源 |
| P5 | 绕开 cupper：改 `~/.claude/settings.json` 或 `~/.codex/config.toml` 的权限规则、改 cupper 的数据库或配置、用 GUI 脚本点"批准" |
| P6 | 冒充宿主：杀掉宿主，在同一个路径上 bind 一个假宿主，骗取 UI 的输入或 CLI 的凭据 |
| P7 | 其他入口：深链接（PS-7）、AppleScript、Services 菜单 |

### 4.2 macOS 的能力和限制（核实）

| 事项 | 结论 | 来源 |
|---|---|---|
| Unix 域套接字的对端身份 | `sys/un.h` 里有 `LOCAL_PEERCRED`、`LOCAL_PEERPID`、`LOCAL_PEERTOKEN`（"retrieve peer audit token"）。审计令牌 → `SecCodeCopyGuestWithAttributes(kSecGuestAttributeAudit)` → 用代码签名要求做校验，Apple 自己的 authd 就是这么用的【核：头文件和 authd 源码】。Apple 的公开文档没有专门描述这条路径；论坛上有人报告某些进程会返回 EPERM（100001）【核：论坛】，**需要 SP-4 实测** | [S15][S18] |
| XPC 的对端签名要求 | `xpc_connection_set_peer_code_signing_requirement` 从 macOS 12；`NSXPCConnection.setCodeSigningRequirement` 和 `NSXPCListener.setConnectionCodeSigningRequirement` 从 macOS 13；`xpc_listener_create` 从 macOS 14，它的 `set_peer_code_signing_requirement` 从 14.4；新的 `xpc_peer_requirement`（team identity、entitlement、lwcr）从 macOS 26。按消息校验，基于审计令牌，没有 pid 复用的竞态【核】 | [S15] |
| 嵌在 App 里的 XPC 服务 | 原文："an XPC service is private, and is available only to the main application that contains it"；launchd 按需启动，空闲时 SIGKILL。**不适合做宿主**：CLI 连不上，而且有状态的服务会被杀掉【核】 | [S19] |
| 具名的 Mach 服务 | 需要 LaunchAgent。macOS 13 起用 `SMAppService.agent(plistName:)`，plist 放在 App 的 `Contents/Library/LaunchAgents`【核】。所以 XPC 实际上等于选了 ADR-01 的方案 c | [S23] |
| 钥匙串 | 两种实现：文件型钥匙串用 ACL，列出受信任的 App，其他程序访问时弹窗；数据保护钥匙串用访问组。原文："These entitlements must be authorized by a provisioning profile. Your program needs an app-like bundle structure in which to embed that profile"。命令行工具默认没有这种结构。生物识别保护和安全隔区密钥都只能用数据保护钥匙串【核】 | [S10] |
| Hardened Runtime | 防止代码注入、DLL 劫持和篡改进程内存；JIT 等能力需要显式的例外权限；公证必须开启它【核】 | [S24] |
| Electron fuses | `runAsNode`、`nodeOptions`、`nodeCliInspect` 默认**开启**；`embeddedAsarIntegrityValidation`、`onlyLoadAppFromAsar` 默认**关闭**【核】 | [S16] |
| Node SEA | `execArgvExtension` 默认是 `"env"`，也就是接受 `NODE_OPTIONS`；只有设为 `"none"` 才忽略【核】 | [S17] |
| agent 自身的沙箱 | Claude Code 在 macOS 上用 Seatbelt，默认只允许连向沙箱代理，Unix 套接字要用 `allowUnixSockets` 显式放行；但 MCP server、hooks 和"不带沙箱重试"的命令都在沙箱外运行【核】。Codex 的 Seatbelt 基础策略是 `(deny default)`，Unix 套接字按白名单放行（`seatbelt.rs`，提交 37eaae6，2026-10-07）【核】 | [S20][S21] |
| 同用户进程能看到什么 | 本机 macOS 26.4 上，同用户进程的 argv 可以用 `ps` 看到；这次测试里看不到环境变量（可能受测试环境影响，**不要依赖**）【实测】 | — |
| 嵌套沙箱 | 给 agent 外面再套一层 cupper 自己的 Seatbelt，可能让 agent 自己的沙箱失效（已经在沙箱里的进程，再 `sandbox_apply` 可能被拒）【推，需要实测】。所以不把它当默认手段 | — |

### 4.3 各方案的真实防护效果

| 攻击路径 | A. UDS + 文件令牌（0600） | B. UDS + 钥匙串令牌 | **C. UDS + 对端代码签名 + CLI 用户在场凭据 + 授权分级（推荐）** | D. XPC（LaunchAgent）+ 签名要求 | E. 继承的 socketpair |
|---|---|---|---|---|---|
| P1 直连 | 挡住 | 挡住 | 挡住 | 挡住 | 挡住（没有可以连的汇合点） |
| P2 偷凭据 | **挡不住**（同用户能读文件） | 文件型钥匙串：会弹窗，是绊线不是墙；数据保护钥匙串：挡住，但需要描述文件 | UI 没有凭据可偷；CLI 的凭据在钥匙串里，并且走挑战-应答 | 没有凭据 | 没有凭据（前提是 fd 不泄漏给子进程） |
| P3 运行合法客户端 | 挡不住 | 挡不住 | **靠授权分级挡住**：审批和提权需要用户在场 | 挡不住（还要靠授权分级） | 不适用（CLI 拿不到这个通道） |
| P4 注入 | — | 取决于加固 | 取决于加固：hardened runtime、fuses、SEA 设 none | 同 C | 同 C |
| P5 绕开 cupper | 挡不住 | 挡不住 | 挡不住（要靠 agent 的沙箱） | 挡不住 | 挡不住 |
| P6 冒充宿主 | 挡不住（客户端把令牌明文发给了假宿主） | 挡不住 | **挡住**：客户端也校验宿主的签名，CLI 用挑战-应答 | 挡住（客户端设置服务端签名要求） | 挡住（父子关系天然双向认证） |
| P7 其他入口 | 与传输无关，要单独规定 | 同左 | 同左 | 同左 | 同左 |
| 能否同时服务 UI、CLI 和 LaunchAgent 形态 | 能 | 能 | 能 | 只适用 LaunchAgent；Node/Rust 客户端需要绑定 | 只服务首次拉起宿主的那个 UI；UI 崩溃重启后（宿主还在，NFR-8）用不了 |

### 4.4 推荐方案（C）的具体做法

1. **传输**：Unix 域套接字（SOCK_STREAM），长度前缀分帧，单帧上限 64KB，和 ADR-04 一致。套接字放在宿主自己创建的 0700 目录里，启动时校验目录的属主和权限。注意 macOS 的 `sun_path` 只有 104 字节，建议放在 `$TMPDIR` 下的子目录，或者用较短的路径。
2. **双向认证**：
   - 发布构建（Developer ID 签名）：双方都用 `LOCAL_PEERTOKEN` 取对端的审计令牌，按代码签名要求校验，要求包括 `anchor apple generic`、Team ID 和组件标识符。UI 校验宿主，宿主校验 UI。
   - 每条连接只在开始时校验一次，所以要禁止通过 SCM_RIGHTS 传递 fd，连接也不允许中途换 principal（推，SP-4 要覆盖）。
   - 开发构建和自行构建（没有 Team ID）：退化为"用户在场确认后发放的凭据 + 挑战-应答"，UI 上明确提示"开发构建，本地鉴权较弱"。
3. **CLI**：第一次运行 `cupper-cli auth` 时，桌面 UI 弹窗确认（或者用 Touch ID），然后发放一份有范围、有期限的凭据，存进钥匙串。认证时用 HMAC 做挑战-应答，并绑定到这条连接，不把秘密发给对面。
4. **授权分级**（接在架构师的 `policy(principal, command)` 上）：
   - principal 类型：`desktop_ui`、`cli{scopes}`、`remote_device{device_id}`（M3）、`test_harness`（只在 debug 构建里编译进去）。
   - **审批、回答提问（计划确认可能顺带切换权限模式）、切到完全访问、`allow_always`**，只开放给 `desktop_ui`、`remote_device`（受 SEC-9 约束）和带 `approver` 范围的 CLI。`approver` 范围每次都要用户在场，有效期很短。
   - NFR-1 验收里的"CLI 处理审批"，用 debug 构建里的 `test_harness` principal 配合假 agent 来跑。release 构建忽略这个开关，否则攻击者可以杀掉宿主、带测试开关重启。
5. **启动 agent 时的进程卫生**：清掉 cupper 相关的环境变量和套接字路径；用 `POSIX_SPAWN_CLOEXEC_DEFAULT` 关闭所有继承的 fd；argv 里不放秘密。宿主自己开 hardened runtime；Electron 烧 fuses；Node SEA 设 `execArgvExtension: "none"`。
6. **其他入口**：深链接、AppleScript 字典、Services 菜单、拖放，都不能触发审批或提权。宿主和 App 都不申请辅助功能、自动化、屏幕录制这类 TCC 权限，因为 agent 是宿主的子进程，可能被系统算作同一个责任进程（推，需要核实）。
7. **可观测**：每次连接都记审计日志（对端路径、签名状态、principal）；桌面 UI 提供"已连接的客户端"列表。M3 的设备管理界面可以直接复用它。
8. **继承的 socketpair**：在 ADR-01 方案 b 下，可以用它在启动阶段把宿主的身份传给 UI，省去第一次握手。但正常运行时不能依赖它，否则 UI 一重启、或者宿主改成 LaunchAgent，这条路就断了。
9. **不采用**：文件里的 bearer 令牌、环境变量传凭据、localhost 端口（PRD 本来就禁止）。

### 4.5 打分

| # | 标准 | A | B | **C** | D | E |
|---|---|---|---|---|---|---|
| — | **SEC-8 的实际防护** | 1 | 2 | **4** | 4 | 3（只覆盖 UI） |
| 1 | 复刻保真度 | 中性 | 中性 | 中性 | 中性 | 中性 |
| 2 | iOS 共享 | 中性：远程走另一条路；但 C 的 principal 模型 M3 可以直接用 | 中性 | 中性 | 中性 | 中性 |
| 3 | 生态 | 5 | 4 | 4（审计令牌和 SecCode 要几十行原生代码：Rust 用 security-framework 或 objc2-security，Node 要写插件） | 2（实际上只有 Swift/ObjC 能用） | 4 |
| 4 | AI 编写的效率和出错率 | 5 | 4 | 3 | 3 | 4 |
| 5 | 性能 | 5 | 5 | 5 | 5 | 5 |
| 6 | 签名和分发 | 5 | 3（ad-hoc 签名的构建每次重新构建都会弹窗） | 3（完整强度需要 Developer ID） | 2（需要签名，还要注册 LaunchAgent） | 5 |
| 7 | 贡献者上手 | 5 | 4 | 4（有退化模式） | 2 | 4 |
| 8 | 长期维护 | 5 | 4 | 4 | 3 | 3（另一条路径还是要做） |

**推荐 C。** 在以下情况我会改变推荐：
- SP-4 发现"审计令牌 → SecCode"在 UDS 上不可靠（EPERM 之类）：改用 D。这需要 ADR-01 从 M1 起就把宿主做成 LaunchAgent，并加一个 Swift 写的薄 XPC 前端。
- ADR-02 选了 Swift：直接用 D。
- 宿主从 M1 起就是 LaunchAgent：C 和 D 差不多，按语言选。

### 4.6 要写进文档的边界

SEC-8 能做到三件事：
- agent 的沙箱开着时，操作系统层面就挡住它连宿主；
- agent 不带沙箱执行命令时，"一行命令替自己批准"这条捷径被关掉；
- 冒充宿主、偷凭据会留下痕迹。

SEC-8 做不到的是：挡住一个以同用户身份、不带沙箱执行任意代码、并且有明确恶意的进程。它可以直接改 agent 自己的权限配置（P5）。

建议文档里推荐用户开启 agent 自带的沙箱，并说明"不带沙箱重试"和 MCP server 都在边界之外 [S20]。这和架构师在对齐会上说的"纵深防御，不是绝对隔离"一致。

---

## 5. ADR-01 和 ADR-03 的 M3 附注

- **ADR-01**：
  - M1 用 App 拉起的子进程，我同意。
  - 但宿主的二进制从第一天起就应该能作为 LaunchAgent 运行（M3 之前要实现后台常驻，NFR-8）。
  - **宿主打包成 app-like 的 helper bundle**，这样才能放描述文件，使用数据保护钥匙串和安全隔区 [S10]。如果是裸的 Mach-O 或 SEA，就只能用文件型钥匙串，M3 存身份密钥时要返工。
  - 防休眠（NFR-13）需要 IOPMAssertion。Node 宿主可以退一步用 `caffeinate -i -w <pid>` 子进程（推）。
- **ADR-03**：
  - 用 Electron 的话，fuses 是硬性要求（RunAsNode、NodeOptions、NodeCliInspect 关；ASAR 完整性校验和 OnlyLoadAppFromAsar 开）[S16]，否则对 UI 做签名校验没有意义。
  - 不论用哪种 UI 技术，UI 进程都不持有任何长期秘密。
  - 渲染代码是否和 iOS 共享，不影响我的判断：iOS 应该用 SwiftUI 原生实现（NSE 和钥匙串本来就必须原生），Markdown 和 diff 另外实现。宿主输出语义化事件已经让 iOS 成本大幅下降。

---

## 6. ADR-12、ADR-15 对现在这些选择的约束（现在选错会让 M3 返工的地方）

| # | 现在的选择 | 选错的表现 | M3 的后果 | 现在的做法和成本 |
|---|---|---|---|---|
| 1 | 原语集合（ADR-02、05 以及任何 M1 里的加密） | 用了 libsodium 特有的构造：`crypto_box`/XSalsa20、secretstream/XChaCha、`crypto_kx`/BLAKE2b；或者选了 JS 的 noise-handshake（BLAKE2b，只支持 psk0） | iOS 只能引入 libsodium，CryptoKit 用不上；或者 Noise 的模式选择被一个 JS 库卡住 | 定一份**原语白名单**：X25519、Ed25519、ChaCha20-Poly1305（IETF 版，96 位 nonce）、AES-256-GCM、SHA-256/512、HMAC、HKDF、HPKE（RFC 9180），以及只用于签名的 P-256。全部在 CryptoKit、RustCrypto、noble 三者的交集里 [S5]。几乎零成本 |
| 2 | 宿主打包（ADR-01） | 裸二进制或 SEA | 用不了数据保护钥匙串和安全隔区；NSE 共享密钥的方案也要跟着改 | 做成 app-like 的 helper bundle，成本低 |
| 3 | principal 模型（ADR-05、04） | principal 等于"本地对端的代码身份" | 远程桥要么嵌进宿主，要么改认证模型 | principal 定义为枚举，预留 `remote_device{device_id}`；远程桥作为宿主进程内的模块，由它确认设备的 Noise 静态公钥后建立 principal。成本低 |
| 4 | 帧格式（ADR-04） | 没有上限的消息，或者依赖字节流语义 | 套 Noise 时要重做分帧 | 64KB 上限、帧类型、批量帧、blob 分片。成本低 |
| 5 | seq 语义（ADR-04、06） | 客户端假设 seq 连续 | 宿主没法为手机合并 delta，流量和帧数多出 6 到 8 倍 | 写成"单调但可以不连续；丢失检测靠游标" |
| 6 | 审批摘要（ADR-04） | 客户端重新计算 JSON 规范化摘要 | 跨语言对不上，或者绑定的不是用户看到的内容 | 宿主计算，客户端回显；"必须理解"由宿主强制执行 |
| 7 | 命令信封（ADR-04） | 没有签名或证明字段 | 锁屏审批、M4 的存储转发要改信封 | 预留可选的 `attestation` |
| 8 | 密码套件（ADR-12） | 握手里在线协商套件 | 恶意中继可以降级 | 套件随大版本固定，写进 prologue |
| 9 | 配对认证（ADR-12） | 只靠二维码认证"桌面 → 手机"一个方向 | 重现 Happy #1503 | 二维码里带一次性 psk。**宿主把手机公钥写进设备表之前，必须看到经 psk 认证、并完成密钥确认的手机消息**。具体用 IKpsk1、IKpsk2 还是 XXpsk3，由 SP-8 定。注意：IKpsk2 的 psk 在第二条消息里，宿主要等手机发来第一条传输消息才算确认，这正是 AI 容易漏掉的地方 |
| 10 | 设备密钥（ADR-12、15） | 想把 Noise 静态密钥放进安全隔区 | 安全隔区不支持 X25519 [S5] | X25519 静态密钥放在钥匙串里（`AfterFirstUnlockThisDeviceOnly`，通过访问组和 NSE 共享）。安全隔区的 P-256 只用于可选的"本人批准"签名 |
| 11 | 通知意图的 schema（ADR-04 → ADR-14） | 结构随意、体积大 | 超出 4KB；NSE 解析失败 | 单独定一个紧凑、单独版本化的推送 schema；NSE 解不开时显示通用文案（MB-6） |
| 12 | host_id 和 device_id | 由主机名或硬件序列号派生 | 中继能关联到具体机器，和 SEC-1 冲突 | 随机生成。成本为零 |
| 13 | 共享方式（ADR-15） | 宿主选纯 TS，或者 TS 里手写了 Noise | M3 只能"两份实现 + 向量"，安全评审要审两份 | 见第 2 节：宿主用 Rust，或者 TS 加 Rust 安全 crate |
| 14 | iOS 最低版本（ADR-15） | — | — | 用户自己构建、自己使用，可以要求 iOS 17 以上（为了 CryptoKit 的 HPKE）。需要 X-Wing 或 ML-KEM 时要 iOS 26 |

不需要现在做的事：选定 Noise 模式、实现配对、生成设备密钥、做中继。这和架构师 1.4 节的意见一致。

---

## 7. AI 编码 agent 写密码学：风险与兜底

**常见错误**（多数在 R03 调研的产品里出现过，或者是这类代码的典型问题）：
- 重连后计数器归零，但继续用同一把密钥，导致 nonce 重用；长期密钥配随机生成的 96 位 nonce；
- Noise 的 nonce 字节序：ChaChaPoly 是小端，AESGCM 是大端；
- 漏掉 psk 的有效性规则，或者漏掉密钥确认；没有把握手哈希作为通道绑定使用；
- 对配对应答照单全收（Happy #1503）；
- 用 `==` 比较 MAC；用 `Math.random`；把秘密写进日志；
- base64 和 base64url 混用；对重新序列化后的 JSON 做签名；
- HKDF 的 `info` 没有做域分离；同一把 X25519 密钥同时用于 Noise 和推送，造成跨协议复用；
- 接受服务端声明的明文模式（违反 SEC-5）；
- 用设备时钟判断过期，没考虑时钟偏差；
- 没有认证流的结尾，导致截断攻击。

**兜底措施**：
1. **规则**：在 `CLAUDE.md` 的子 agent 守则里加一条："不实现任何密码学原语或协议状态机，只调用白名单里的库；`crypto/` 目录的改动必须由人工审查，并逐条对照安全检查清单"。
2. **API 防误用**：用类型状态表达"握手 → 传输"（snow 本身就是这样设计的）；密钥类型不实现序列化和 `Debug`；不对外暴露 nonce 参数。
3. **外部向量**：Noise 用 cacophony 向量（snow 的仓库里自带 [S4]）；HPKE 用 RFC 9180 的向量（rust-hpke 自带 [S13]，CryptoKit 端也要跑）；原语层用 Wycheproof（由库负责）。我们自己再维护一份 `cupper-vectors`：二维码格式、配对 transcript、推送 payload、审批摘要，由实现 A 生成、实现 B 验证，人工审查后提交。
4. **独立实现交叉验证**：即使两端共享 Rust 核心，CI 里也要拿一份独立的实现来对跑。可以是一个基于 CryptoKit 的最小 Swift Noise，也可以是 Python 的 `noiseprotocol` 脚本。目的是防止"两端一起错"。
5. **负面测试**：逐字节篡改、重放、乱序、截断、降级尝试、过期的二维码，以及**恶意中继替换配对应答必须失败**（M3 退出标准）。
6. **防止"对着测试写代码"**：向量文件只能由人更新；加性质测试和 fuzz（Rust 端用 cargo-fuzz 测解码器和握手输入）。
7. **互通矩阵**：CI 里把当前桌面和前若干个 iOS 协议包版本、录制的 transcript 两两组合对跑（同时服务 NFR-10）。
8. **本地鉴权的测试**：SP-4 里加几条用例：沙箱内和沙箱外的 agent 子进程分别尝试连接宿主；假宿主抢占路径；`ELECTRON_RUN_AS_NODE` 和 `NODE_OPTIONS` 注入；不经用户在场的 CLI 审批。

---

## 8. 推荐组合

| ADR | 推荐 |
|---|---|
| ADR-01 | M1 由 App 拉起的独立子进程；二进制从一开始就能作为 LaunchAgent 运行；打包成 app-like 的 helper bundle；宿主和 App 不申请辅助功能、自动化类的 TCC 权限 |
| ADR-02 | **Rust**（首选）；可以接受"TS + Rust 安全 crate（napi-rs）"；不推荐 Swift |
| ADR-03 | 不在我的范围。安全上的要求：用 Electron 就必须烧 fuses；UI 不持有长期秘密 |
| ADR-04 | Protobuf 作 schema（proto3 + `optional`），M1 线上用 JSON 映射；长度前缀帧，单帧 ≤ 64KB，有帧类型和批量帧；delta 可合并，seq 可以不连续；版本区间 + 能力位 + 审批"必须理解"（宿主强制执行）+ 摘要回显；预留 `attestation`；金样例和 Swift 解码测试从 M1 起进 CI；M3 远程可以协商二进制；加密之前不压缩 |
| ADR-05 | UDS + 审计令牌双向代码签名校验 + CLI 用户在场凭据（挑战-应答，存钥匙串）+ principal/scope 分级 + 进程卫生 + 其他入口不能提权；开发构建有退化模式并明确提示；XPC 是备选 |
| ADR-12 预约束 | 原语白名单；套件随大版本固定；psk 认证加密钥确认；X25519 放钥匙串，P-256 安全隔区密钥只用来签名；推送倾向用 HPKE |
| ADR-15 预约束 | SwiftUI 原生；类型由 schema 生成；E2EE 核心倾向用 UniFFI 共享（宿主是 Rust 时），由 SP-8 和"用户自建门槛"实测决定；最低 iOS 17 |

---

## 9. 希望其他参会者回应的问题

**请架构师回应**
1. 你接受"seq 单调但对单个客户端可以不连续（为合并 delta）"吗？丢失检测完全交给游标续传，是否有遗漏的场景？
2. 远程桥放在宿主进程内（我的倾向）还是独立进程？如果是独立进程，principal 委托怎么设计？
3. schema 源选 Protobuf 加 JSON 映射，你能接受吗？如果选 JSON Schema 或 TypeSpec，Swift 的生成链是什么？你是否同意 M1 的 CI 里就加 Swift 解码测试？
4. 宿主是否同意打包成 app-like 的 helper bundle？LaunchAgent 化放在 M2 还是 M3 前？
5. NFR-1 验收里 CLI 处理审批：用"只在 debug 构建里存在的 `test_harness` principal"实现，可以吗？release 里 CLI 的审批必须用户在场，你有没有异议？
6. 宿主协议对旧客户端的支持窗口定多长？（我建议至少 6 个月或 N 个大版本。）

**请桌面客户端工程师回应**
1. 如果选 Electron，能否承诺按第 5 节烧 fuses？对开发流程（热重载、调试）有什么影响？
2. 深链接（PS-7）和任何自动化入口，能否承诺不触发审批和提权？
3. UI 能否处理"这条审批在本端不可操作"和"已连接客户端列表"这两种状态？后者会在 M3 复用为设备管理。
4. 开发构建或自行构建（没有 Team ID）时，显示"本地鉴权较弱"的提示，你能接受吗？

**请两位共同回应**
1. 密码学目录由谁负责人工审查？是否同意把"不手写原语"写进 `CLAUDE.md` 的子 agent 守则？
2. ADR-02 如果在 TS 和 Rust 之间僵持，是否同意先做半天的 UniFFI 冒烟（snow 编进 iOS 模拟器和 NSE target），再做决定？

---

## 来源（访问日期都是 2026-10-16）

| 编号 | 来源 | 要点 |
|---|---|---|
| S1 | mozilla/uniffi-rs `CHANGELOG.md`（main）；crates.io `uniffi`（0.32.2，更新于 2026-09-23） | 0.x；0.32.0（2026-06-30）有破坏性变更；Swift 的 BOM、async 泄漏修复 |
| S2 | matrix-org/matrix-rust-sdk `bindings/matrix-sdk-ffi/Cargo.toml`；matrix-rust-components-swift（推送于 2026-10-02） | Element X iOS 的 UniFFI 依赖 |
| S3 | bitwarden/sdk-internal `Cargo.toml` | `uniffi = "=0.32.0"` |
| S4 | mcginty/snow README 和 `tests/vectors/cacophony.txt`；crates.io `snow` 0.10.0（2025-07-19） | 没有正式审计；Noise rev 34；带 cacophony 向量 |
| S5 | Xcode 26.5 SDK 中 CryptoKit 的 `arm64e-apple-ios-macabi.swiftinterface` | HPKE iOS 17；HKDF iOS 14；ChaChaPoly iOS 13；ML-KEM、X-Wing iOS 26；安全隔区只有 P-256 和 ML-KEM；没有 XChaCha、BLAKE2 |
| S6 | jmlepisto/clatter README；crates.io 2.3.0（2026-08-30） | PQ 混合 Noise；没有审计 |
| S7 | rweather/noise-c（GitHub API） | 没有发布版本；最后提交 2023-12-06 |
| S8 | holepunchto/noise-handshake README 和 `noise.js`；ChainSafe/js-libp2p-noise | 只支持 psk0；依赖 sodium-universal |
| S9 | paulmillr/noble-curves README 的 Security 一节 | 2026-08 Trail of Bits 审计；JS 无法保证常量时间 |
| S10 | Apple TN3137 "On Mac keychain APIs and implementations" | 访问组的权限需要描述文件，要有 app-like bundle；生物识别和安全隔区都需要数据保护钥匙串 |
| S11 | jedisct1/swift-sodium（发布记录和 README） | 0.10.0（2026-01-06）、0.11.0（2026-05-04）；自带预编译的 xcframework |
| S12 | atom/node-keytar（GitHub API） | 已归档（2022-12） |
| S13 | rozbb/rust-hpke README；crates.io `hpke` 0.14.1（2026-09-06） | RFC 9180，带向量；没有付费审计 |
| S14 | apple/swift-protobuf `Documentation/API.md`；最新发布 1.39.0（2026-10-12） | `UNRECOGNIZED(Int)`、`unknownFields` |
| S15 | Xcode 26.5 MacOSX SDK：`sys/un.h`、`xpc/connection.h`、`xpc/listener.h`、`xpc/peer_requirement.h`、`Foundation/NSXPCConnection.h`、`Security/SecCode.h` | 见 4.2 节列出的可用版本 |
| S16 | electron/electron `docs/tutorial/fuses.md`（main） | 各 fuse 的默认值 |
| S17 | nodejs/node `doc/api/single-executable-applications.md`（main） | `execArgvExtension` 默认是 `"env"` |
| S18 | Apple 开源的 Security `authd/process.c`；Apple Developer Forums thread 744791 | 审计令牌 → SecCode 的用法；EPERM 案例 |
| S19 | Apple "Creating XPC Services"（Daemons and Services Programming Guide，archive） | 服务是 App 私有的；空闲时被 SIGKILL |
| S20 | Claude Code 文档 "Configure the sandboxed Bash tool"（code.claude.com/docs/en/sandboxing） | macOS 用 Seatbelt；`allowUnixSockets`；哪些东西在沙箱外 |
| S21 | openai/codex `codex-rs/sandboxing/src/seatbelt.rs`、`seatbelt_base_policy.sbpl`（提交 37eaae6，2026-10-07） | `(deny default)`；Unix 套接字白名单 |
| S22 | BleepingComputer "Self-propagating supply chain attack hits 187 npm packages" 及同期报道 | 2025-09 chalk/debug 投毒、Shai-Hulud |
| S23 | Apple `SMAppService`、`agent(plistName:)` 文档 | macOS 13+；plist 放在 `Contents/Library/LaunchAgents` |
| S24 | Apple "Hardened Runtime" 文档 | 防注入；JIT 需要例外权限；公证必须开启 |
| — | crates.io（2026-10-16 查询） | napi 3.14.2、security-framework 3.7.0、objc2-security 0.3.2、typeshare-core 1.13.4、prost 0.14.4、agent-client-protocol 3.3.0 |
| — | 本机实测（macOS 26.4） | 同用户进程的 argv 可见；第 3.2 节的编码与合并估算脚本（假设条件写在该节） |

---

*给编排者的备注（不属于发言正文，存档时可以删掉）：第一次查询 crates.io API 时，我在 User-Agent 里误带了用户的邮箱（crates.io 要求 UA 带联系方式）。之后的请求都改用了不含个人信息的 UA，没有其他外发。这次讨论没有修改、创建或提交仓库里的任何文件；临时脚本和抓取的资料只放在 scratchpad 目录。*

> 编排者存档说明：上面这条备注按"所有工作留痕"的原则保留，不删除。备注中没有出现邮箱本身。处理情况见会议记录。
