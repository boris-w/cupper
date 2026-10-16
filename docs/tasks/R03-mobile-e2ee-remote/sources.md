# R03 来源清单与摘录

- 访问日期：除特别注明外，均为 2026-10-16。
- 标注：**核** = 已核实（读过源码或官方文档原文）；**推** = 推测，或者只有第三方文章、营销文案作依据。
- 源码只读，没有执行。克隆在 worktree 之外的临时目录里，没有提交进本仓库。
- 报告正文用 `[编号]` 引用本文件的条目。

## A. Claude Code 官方

| 编号 | 来源 | 类型 | 要点（摘录或转述） |
|---|---|---|---|
| CC1 | https://code.claude.com/docs/en/remote-control | 官方文档 | 见下方摘录 |
| CC2 | https://code.claude.com/docs/en/mobile | 官方文档 | Claude App（iOS/Android）的 Code 标签同时承载云端会话、Remote Control 和 Dispatch；从 App 不能选 Bypass permissions；`/plugin`、`/resume` 等只能在本地终端用 |
| CC3 | https://code.claude.com/docs/en/desktop （Sessions from Dispatch 一节） | 官方文档 | Dispatch 是 Cowork 里的持续对话，可以派生 Code 会话；任务完成或需要批准时手机收到推送；只对 Pro/Max 开放 |
| CC4 | https://support.claude.com/en/articles/13947068 | 官方帮助中心 | Dispatch 要求桌面 App 保持打开、电脑保持唤醒；有限 beta；文中没有任何加密说明 |
| CC6 | github.com/anthropics/claude-code @ `03b5892` `LICENSE.md` | 仓库 | 原文："© Anthropic PBC. All rights reserved. Use is subject to Anthropic's Commercial Terms of Service"，即闭源的专有许可 |
| CC5 | https://code.claude.com/docs/en/channels | 官方文档 | Channels 是本地 MCP 插件，把 Telegram、Discord、iMessage 桥接进正在运行的会话；配对码加发送者白名单；可声明"permission relay"，从聊天里批准工具调用 |

### CC1 摘录（Remote Control，"Connection and security"等节）

> Your local Claude Code session makes outbound HTTPS requests only and never opens inbound ports on your machine. When you start Remote Control, it registers with the Anthropic API and polls for work. When you connect from another device, the server routes messages between the web or mobile client and your local session over a streaming connection.

> All traffic travels through the Anthropic API over TLS, the same transport security as any Claude Code session. The connection uses multiple short-lived credentials, each scoped to a single purpose and expiring independently.

> While Remote Control is connected, the session transcript, including your messages, Claude's responses, and tool activity, is stored on Anthropic servers.

> Organizations with Zero Data Retention enabled, or with the HIPAA configuration applied, can't enable Remote Control.

Trusted Devices（beta）：每台浏览器、手机或桌面 App 各自注册一个凭据，Anthropic 只保存设备公钥和基础元数据；登录超过 18 小时要用 Face ID、Touch ID、Windows Hello 或 passkey 重新确认；在 claude.ai/settings/account 删除设备后，凭据立即吊销。

其他要点：
- 三种启动方式：`claude remote-control`（server 模式，可以 `--spawn same-dir|worktree|session`，`--capacity` 默认 32）、`claude --remote-control`、在会话里输入 `/remote-control`。终端显示会话 URL，按空格显示二维码。
- 连接的设备能看到：实时对话、压缩和 `/clear` 的过程、git diff 面板，可以切换模型和 effort，可以上传图片和文件，跨会话消息也会同步过来。
- 推送：`/config` 里有两个开关，"Push when Claude decides"和"Push when actions required"。终端处于焦点时不推送；也可以用 `CLAUDE_CLIENT_PRESENCE_FILE` 判断用户是否在电脑前。
- 限制：交互进程一次只承载一个远程会话；本地进程必须一直运行；权限请求和 AskUserQuestion 一直等到有人回答，其他转发的对话框默认 5 分钟后过期。
- 可用范围：Pro、Max、Team、Enterprise 都能用，Team 和 Enterprise 要管理员先打开；不支持 API key、Bedrock、Vertex 或自定义 `ANTHROPIC_BASE_URL`。

## B. Codex 官方

| 编号 | 来源 | 类型 | 要点 |
|---|---|---|---|
| CX1 | https://learn.chatgpt.com/docs/remote-connections （由 developers.openai.com/codex/app/mobile 308 跳转而来） | 官方文档 | 见下方摘录 |
| CX2 | github.com/openai/codex @ `e790ce2`（2026-10-16）`codex-rs/app-server-transport/src/transport/remote_control/protocol.rs` | 源码，Apache-2.0 | 见下方摘录 |
| CX3 | 同一仓库 `codex-rs/app-server/README.md`（userVerification 一节） | 源码文档 | 本地 UI 可以生成一把设备密钥（ECDSA P-256，用于签名，macOS 上实现），`userVerification/verify` 对 challenge 签名，用来证明"这次批准确实是本人在这台设备上做的"；README 原文："WebSocket and remote-control peers must use their own device authenticator" |
| CX4 | 同一仓库 `codex-rs/app-server-daemon/README.md` | 源码文档 | `codex app-server daemon enable-remote-control`、`bootstrap --remote-control`：通过 SSH 启动的无头 Codex 也能开启 remote control，"used by remote clients such as the desktop and mobile apps" |
| CX5 | 同一仓库 `codex-rs/exec-server/src/noise_channel.rs`、`exec-server/README.md` | 源码 | 另一条链路：harness 与远程 exec-server 之间的中继使用 `Noise_hybridIK_X25519+MLKEM768_AESGCM_SHA256`（clatter 库），中继帧是 protobuf，里面是密文；harness 的公钥由 registry 授权 |
| CX6 | https://codex.danielvaughan.com/2026/05/15/codex-mobile-chatgpt-app-relay-architecture-remote-agent-control/ 及同站 2026-06/07 的几篇 | 第三方博客 | 称 Codex 移动端用 X25519、Ed25519 和 AES-256-GCM 做端到端加密，但文中承认这些细节"attributes to a community analysis of Remodex"，并非来自 OpenAI。**推**，本报告不采信 |

### CX1 摘录

- 主机端操作路径：Settings > Connections > Control this Mac or PC，然后扫码，"pairs that phone with that host"。两端必须是同一个 ChatGPT 账号和 workspace，并按要求完成 MFA、SSO 或 passkey 验证。
- 原文："A secure relay layer keeps trusted machines reachable … without exposing them directly to the public internet." 页面没有讲加密方式，也没说 OpenAI 服务器能看到什么。
- 手机端能做的：在主机的项目里新建对话或继续已有对话、发送追问、回答问题、引导正在进行的工作、批准命令、查看输出、diff、测试结果、终端输出和截图、接收"完成"和"需要关注"的通知、在不同主机和对话之间切换。
- Handoff：把一个对话连同它的 Git 状态从一台主机移交到另一台。
- SSH 主机：读取 `~/.ssh/config`，通过 SSH 启动远程 app-server。原文："Do not expose app-server transports on shared or public networks. Use a VPN or mesh networking tool"。
- 主机休眠、断网或关闭 App 时，远程访问随之中断；有些任务可以退回云端容器继续。

### CX2 摘录（remote control 传输层）

```rust
// protocol.rs —— 端点
websocket_url: "wss://chatgpt.com/backend-api/wham/remote/control/server"
enroll_url / refresh_url / pair_url / pair_status_url: ".../wham/remote/control/server/{enroll,refresh,pair,pair/status}"

pub struct StartRemoteControlPairingResponse {
    pub(super) pairing_code: String,
    pub(super) manual_pairing_code: Option<String>,   // 无摄像头时手动输入的配对码
    pub(super) server_id: String,
    pub(super) environment_id: String,
    pub(super) expires_at: String,
}

pub enum ClientEvent {
    ClientMessage { message: JSONRPCMessage },        // 应用层 JSON-RPC 明文（在 TLS 内）
    ClientMessageChunk { segment_id, segment_count, message_size_bytes, message_chunk_base64 },
    Ack { segment_id: Option<usize> },
    Ping,
    ClientClosed,
}
pub(crate) struct ClientEnvelope { event, client_id, stream_id, seq_id, cursor }
```

- 在 `remote_control/` 目录里搜 `encrypt|cipher|noise|x25519|ed25519` 没有命中（测试文件除外），只有 rustls 提供的 TLS。由此得出：开源的 app-server 把 JSON-RPC 消息放在 TLS 里交给 chatgpt.com 中继，**应用层没有端到端加密**，中继能读到全部内容（**核**，前提是 ChatGPT 桌面 App 与 CLI 共用这一套 app-server 传输层；闭源的桌面 App 是否另外加了一层，**推**为没有，但无法核实）。
- `clients.rs` 里有 `revoke_remote_control_client`：吊销是调用服务端 API 做访问控制，不涉及密钥轮换。

## C. 加密协议与原语

| 编号 | 来源 | 要点 |
|---|---|---|
| K1 | https://noiseprotocol.org/noise.html （Revision 34，2018-07-11） | 握手模式的命名规则：首字母 N 表示没有静态密钥，K 表示对方事先已知，X 表示握手中传输，I 表示立即发送；以 K 结尾的模式（NK、IK、KK）支持 0-RTT；以 K 或 I 开头的模式，响应方在收到发起方的传输消息之前只有"弱"前向保密；`psk` 修饰符把 32 字节预共享密钥混入握手 |
| K2 | https://doc.libsodium.org/key_exchange | `crypto_kx`：X25519 加 BLAKE2b-512，输出两把方向不同的会话密钥（rx 和 tx），"Separate keys per direction let counters serve as nonces" |
| K3 | https://doc.libsodium.org/secret-key_cryptography/secretstream | `crypto_secretstream_xchacha20poly1305`：消息被截断、删除、重排、重复或篡改都能检测出来；`TAG_REKEY` 丢弃旧密钥并派生新密钥；`TAG_FINAL` 标记流结束 |
| K4 | https://signal.org/docs/specifications/sesame/ （Revision 2，2017-04-14） | 多设备：发送方和对方的每台设备、自己的其他设备各建一个会话，"encrypts the plaintext using that active session"，逐台设备分别加密；设备被删除后标记为 stale |
| K5 | https://www.rfc-editor.org/rfc/rfc9420.html （MLS，2023-07） | 异步群组密钥协商，提供前向保密（FS）和泄露后安全（PCS）；通过 Add、Remove 加 Commit 增删成员；开销按群组规模的对数增长 |
| K6 | https://www.rfc-editor.org/rfc/rfc8827.html （WebRTC 安全架构，2021-01） | DTLS-SRTP 的密钥第三方无从得知；但"the signaling server can potentially mount a man-in-the-middle attack"，除非用户能独立核对指纹；IdP 机制"has not been widely adopted" |
| K7 | https://tailscale.com/blog/how-tailscale-works （2020-03-20） | 节点之间端到端加密（WireGuard）；协调服务器只交换公钥和策略；"there is never a way for a DERP server to decrypt your traffic"；"the private key never, ever leaves its node" |
| K8 | https://docs.iroh.computer/concepts/relays | iroh 的 relay 在无法直连时转发密文，"Relay servers do not have access to the data being transmitted"；"roughly 9 out of 10 networking conditions allow a direct connection"；relay 可以自托管 |
| K9 | https://www.rfc-editor.org/rfc/rfc8291.html | Web Push 消息加密（ECDH P-256，`aes128gcm`），"secured against inspection, modification, and forgery by a push service"；但推送服务仍然知道是哪个应用服务器在给哪个用户代理发消息 |

## D. 推送与移动平台

| 编号 | 来源 | 要点 |
|---|---|---|
| P1 | https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications （通过 Apple 文档 JSON 接口读取原文） | Notification Service Extension 的用途之一是"Decrypt data sent in an encrypted format"；"only about 30 seconds"；超时或没有调用 completion handler 时显示原始内容；payload 必须带 `mutable-content: 1` 和 `alert`（title、subtitle 或 body）；关闭提醒或只有声音、角标的推送不会走扩展 |
| P2 | https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.usernotifications.filtering | 需要向 Apple 申请的 entitlement，允许 NSE 收到推送后不展示（传空内容即可静默），要求 `apns-push-type: alert` |
| P3 | https://developer.apple.com/documentation/usernotifications/generating-a-remote-notification | APNs payload 上限 4 KB（4096 字节），VoIP 推送为 5 KB |
| P4 | github.com/signalapp/Signal-Server @ `ccc22f9`（main，2026-10-16）`service/.../push/APNSender.java` | Signal 的 APNs payload 不含任何消息内容：`setMutableContent(true).setLocalizedAlertMessage("APN_Message")`，或者只有 `content-available`；客户端被唤醒后自己去拉取密文并解密 |
| P5 | https://spec.matrix.org/latest/push-gateway-api/ | `event_id_only` 格式："only the event_id, room_id, counts, and devices are required to be populated"，推送网关看不到消息内容 |
| P6 | https://firebase.google.com/docs/cloud-messaging/customize-messages/set-message-type （据搜索摘要） | FCM 的 notification 和 data 消息上限都是 4096 字节；data 消息只装自定义键值对，由 App 自行处理 |
| P7 | https://developer.apple.com/library/archive/technotes/tn2277/_index.html | App 被挂起后不执行任何代码，系统"may choose to reclaim resources out from underneath a network socket"；只有 VoIP App 能保活 socket（已归档文档，但结论仍然适用，**推**） |
| P8 | https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy | 本地网络隐私：iOS 14 起、macOS 15 起，第一次访问局域网会弹窗请求授权，需要在 Info.plist 写 `NSLocalNetworkUsageDescription` |
| P9 | https://docs.expo.dev/push-notifications/sending-notifications/ | Expo 推送服务地址是 `exp.host/--/api/v2/push/send`，默认"does not require any authentication"，可以开启 access token 加强安全；由 Expo 把推送转给 FCM 和 APNs，所以 Expo 能看到 payload（文档没有直说，**推**）；payload 上限约 4 KiB |
| N1 | https://tailscale.com/docs/reference/faq/other-vpns （据搜索摘要） | "iOS and Android enforce a limit of running only one VPN at a time"：手机装了 Tailscale，就不能同时使用其他 VPN |

## E. 协议背景

| 编号 | 来源 | 要点 |
|---|---|---|
| ACP1 | https://agentclientprotocol.com/protocol/overview | ACP 基于 JSON-RPC 2.0，agent "typically run as subprocesses of the Client"。Client 调 Agent 的方法：`initialize`、`session/new`、`session/prompt`、`session/load`、`session/cancel` 等。Agent 调 Client 的方法：`session/request_permission`（基线），`fs/read_text_file`、`fs/write_text_file`、`terminal/*`、`elicitation/create`（可选），以及 `session/update` 通知。页面没有提到远程或多客户端场景 |

## F. Happy、Omnara 和其他第三方工具

- Happy：见 `sources-happy.md`（编号 H0–H15）。
- Omnara、Remodex、Happier、HAPI、VibeTunnel、CloudCLI、Vibe Kanban、Moshi、Viberra 等：见 `sources-others.md`（编号 O1–O12）。

这两份笔记由本任务派出的两个并行 fork 读源码、查官方文档整理而成。关键结论我回到源码复核过，包括：Happy 配对应答没有认证，推送正文是会话标题；HAPI 的推送 payload 格式；Remodex 的 HKDF 和 AES-GCM 实现；Happier 配对 v3 的 HMAC；Vibe Kanban 的 SPAKE2 依赖。
