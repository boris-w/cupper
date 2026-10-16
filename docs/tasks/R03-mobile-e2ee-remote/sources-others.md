# R03 来源：Omnara 与其他第三方工具笔记

- 访问日期：2026-10-16。源码浅克隆到 worktree 之外的临时目录，只读、未执行。
- 标注：**核** = 已核实（读过源码，或者看过官方文档、README）；**推** = 推测，或者依据只是第三方文章、营销文案、经工具摘要的帖子。
- 报告正文用 `[O编号]` 引用本文件的条目。

## O1–O3 Omnara：三代产品，已退出"手机遥控本地 agent"赛道

### O1 第一代（2025-07 到 2025-11），Python 写的 CLI wrapper

依据：github.com/omnara-ai/omnara 的历史提交 `d3634b0c7e`（2025-10-10）。

- **架构** **核**
  - 后端是中心化云服务：FastAPI、PostgreSQL、Supabase 认证。
  - `omnara` 命令用 `pty` 启动 claude CLI（`src/integrations/cli_wrappers/claude_code/claude_wrapper_v3.py`）。
  - 结构化消息靠 tail `~/.claude/projects/*.jsonl` 拿到。
  - 权限提示靠解析终端 buffer 识别（`_extract_permission_prompt`）。
  - `src/relay_server/` 是一个终端中继，用 WebSocket 传原始 PTY 帧，帧类型有 OUTPUT、INPUT、RESIZE、METADATA。
- **登录**：Supabase 账号，加上 API Key（RS256 JWT），没有扫码配对 **核**。
- **加密**：只有 TLS，服务器能读到全部内容 **核**。
- **推送**：走 Expo Push。标题是 "{Agent} needs your input"，正文是问题原文的前 100 个字符，以明文经过 Omnara 服务器和 Expo/APNs **核**（`src/servers/shared/notifications.py`）。
- **移动端**：Expo RN 应用，有实例列表、消息流、回答问题、通知设置、API Keys、订阅几个页面 **核**。
- **许可证**：Apache-2.0，可以自托管（docker-compose）**核**。
- **弃用**：2025-11-09 的提交 `70b298d913` 加了弃用说明，原文："wrapper around the Claude Code CLI, which became unfeasible to maintain with Claude Code's constant updates" **核**。

### O2 第二代（2025-11 到 2026 年中），闭源

- 改用 Claude Agent SDK 和 Codex app-server 实现，新的 CLI 和后端都不公开 **核**（依据：旧 README 中的通知、pypi.org/project/omnara）。
- 以下来自 Launch HN 帖子（news.ycombinator.com/item?id=46991591，约 2026-02），帖子内容经 WebFetch 摘要获得，所以标 **推**：
  - 本地有一个 headless daemon，通过出站 WebSocket 连接 Omnara 服务器；服务器负责中继，也保存会话。
  - 本机离线时可以转到云沙箱继续运行。
  - 创始人原话："We don't have true E2EE yet because our service needs access to message content"。
  - 评论区把 Happy、hapi 当作替代选择，并认为"服务器能读消息"是不可接受的硬伤。
- App Store 页面（id6748426727）在 2026-10-16 返回 404，无法确认是否已经下架 **推**。

### O3 第三代（2026-09-02 重新发布），已转型

依据：main 分支提交 `9a9e735dd7`（2026-10-15）**核**。

- 新定位是 "open-source alternative to Claude Managed Agents"（`docs/changelog.mdx`）。
- 技术栈：Go，事件日志存在 Postgres，worker 无状态；每台机器跑一个 `omnarad`，主动向外连接控制面。
- 只提供 Web dashboard，没有原生移动端，也没有提到 E2EE。
- 许可证 Apache-2.0，可以用 docker compose 自托管。

## O4 Remodex

依据：github.com/Emanuele-web04/remodex，提交 `f67b1499db`，Apache-2.0，约 3.3k stars，最后推送于 2026-10-01。作者自己说项目还 "very early"。

- **架构** **核**
  - 链路：iOS（SwiftUI）↔ WebSocket relay ↔ Mac bridge（Node，npm 包名 `remodex`）↔ `codex app-server`（stdio 上的 JSON-RPC）。也支持 OpenCode。
  - bridge 会接入 Codex Desktop 的本地 IPC 总线，与桌面端同步对话状态。
- **relay** **核**（`relay/README.md`、`relay.js`）
  - 连接地址是 `/relay/{sessionId}`，一台 Mac 配一台手机。
  - 状态只在内存里，不落盘，不保存历史。
- **二维码内容** **核**：`{v:2, relay, sessionId, macDeviceId, macIdentityPublicKey, expiresAt, displayName}`，5 分钟后过期。见 `phodex-bridge/src/secure-transport.js` 中的 `MAX_PAIRING_AGE_MS = 5*60*1000`，已复核。
- **握手** **核**（secure-transport.js 第 330–360 行、第 786–917 行，已复核 HKDF 和 AES-256-GCM 部分）
  - 双方各有一把长期的 Ed25519 身份密钥；每个会话另生成 X25519 临时密钥，再加上双方各自的 nonce。
  - transcript 以 `remodex-e2ee-v1` 开头，包含 sessionId、双方设备 ID、双方身份公钥、双方临时公钥、nonce、expiresAt。
  - Mac 用 Ed25519 签 transcript，手机用二维码里的公钥（或之前保存的信任记录）验证；手机也签名，交给 Mac 验证。
  - 会话密钥由 HKDF-SHA256 派生：salt 是 SHA256(transcript)，info 里带上方向，得到两个方向各一把 AES-256-GCM 密钥。
  - nonce 由"方向 + 单调递增的计数器"组成，用来防重放。每个会话都有前向保密。
- **中继能看到什么** **核**（README 的 "Security and Privacy" 一节）：连接元数据、sessionId、设备 ID、公钥、nonce、握手结果；看不到应用层载荷。
- **配对弱点** **推**（根据源码推断）：二维码里没有一次性秘密，配对只靠 sessionId 和 5 分钟有效期，而 relay 本身就知道 sessionId。恶意 relay 理论上可以抢先用伪造的手机身份完成配对，Mac 侧没有 SAS 或用户确认。另外，重新扫码会自动替换掉原来信任的手机。
- **推送** **核**（`relay/push-service.js`）
  - 可选功能，默认关闭；开启后经 APNs 发送。
  - 标题是线程标题（明文），正文是 "Response ready" 或失败信息。
  - 没有使用 NSE，所以推送不是端到端加密的。
- **移动端功能** **核**（README）：实时流、审批、计划模式、子 agent、steer、排队、照片附件、git 操作（commit、push、branch 等）、回滚补丁、语音、手机直连的 SSH 终端、小组件。
- **自托管** **核**：`./run-local-remodex.sh`，或设置 `REMODEX_RELAY`。推荐用 Tailscale 访问自建 relay；README 提到在 iOS 上通过局域网用 `ws://` 不稳定。

## O5 Happier（happier.dev）

依据：github.com/happier-dev/happier，提交 `aa5969bc94`，MIT，约 2k stars。它是 Happy 的分支，作者原本是 Happy 的贡献者（README 中的 "Why Happier"）**核**。

- **架构** **核**
  - 和 Happy 一样：中心 relay 存密文（Postgres、WebSocket，消息带单调 `seq`），每台机器上的 daemon 负责启动 agent。
  - 客户端：Expo 写的 iOS、Android、Web 应用，以及 Tauri 桌面端。
  - 支持多种 agent，README 原话："Any agent that speaks ACP can be added"。
- **加密** **核**（`docs/encryption.md`）
  - legacy 模式用 secretbox。
  - dataKey 模式下每个会话一把 DEK，内容用 AES-256-GCM 加密，DEK 用 box 封装后存储。
  - 时间戳、ID、版本号是明文。
- **存储策略** **核**：服务器声明三种策略之一：`required_e2ee`、`optional`、`plaintext_only`。客户端可以在本地固定要求 `require_e2ee`，防止服务器把加密降级。
- **配对修复** **核**（`docs/encryption.md` 第 607–613 行，已复核）
  - 原文："Terminal pairing v3 adds a 32-byte secret to the QR/deep link and authenticates the sealed content-key response with HMAC-SHA-256"。
  - 文档也承认，在强制使用 v3 之前，"a malicious relay can still downgrade the exchange to a forged legacy response"。
  - 文档还承认，Web 端配对防不住恶意的自托管 relay，因为网页的 JS 本身就由 relay 下发。
- **推送** **核**
  - 走 Expo，由 CLI 直接调用 Expo API，不经过 Happier relay。
  - 通知带 Allow/Deny 和回答按钮。
  - "Request previews" 默认开启，会带上命令、文件路径和问题文本，因此内容以明文经过 Expo 和 APNs/FCM；这个预览可以关闭。
  - 没有使用 NSE。
- **移动端功能**（README，**推**，属营销描述，没有逐项核实）：新建会话、worktree、审批收件箱、逐行 diff 评论、git 和 PR、终端、语音、分享会话、中途切换 agent、迁移会话到其他机器。
- **自托管** **核**：`happier relay host install`，或用 Docker。

## O6 HAPI（tiann/hapi）

依据：提交 `317e39a0bb`，AGPL-3.0，约 5.3k stars。定位是 local-first 的 Happy 替代品 **核**。

- **架构** **核**（`docs/guide/how-it-works.md`）
  - 用户自建 hub：一个 Bun 单二进制，用 SQLite 存明文数据。
  - CLI 包装器和 hub 之间用 Socket.IO，客户端和 hub 之间用 REST 加 SSE。
  - 客户端有 Web/PWA、Telegram Mini App、原生 iOS 和原生 Android。
- **连接** **核**
  - `hapi hub --relay` 走 tunwg（WireGuard 加 TLS），默认的 relay 是 relay.hapi.run。TLS 在 hub 端终止，证书由 hub 自己向 Let's Encrypt 申请，relay 只按 SNI 转发。
  - 也可以用 Cloudflare Tunnel、Tailscale 或 ngrok。
  - **推**：relay 的运营方控制 DNS，理论上可以为同一个域名另签证书来做中间人，但这样会在证书透明度日志里留下痕迹。
- **认证** **核**：只有一个 256 位的共享 token `CLI_API_TOKEN`，用二维码或手动输入完成配对。应用层没有 E2EE，hub 能看到明文，但 hub 是用户自己的。
- **推送：唯一核实做了端到端加密推送的产品** **核**（`relay/README.md` 第 21–40 行、第 72 行，已复核）
  - 官方的 hapi-push-relay 持有 App 的 APNs 和 FCM 凭据。
  - hub 用每台设备各自的 AES-256-GCM 密钥加密通知内容。
  - iOS 的 payload 形如 `{"aps":{"mutable-content":1,"alert":{"title":"HAPI","body":"New activity"}},"hapi":{"v":1,"e":"<envelope>"}}`，由 NSE 解密后改写通知。
  - Android 发 data-only 的 FCM 消息，App 解密后再弹本地通知。
  - relay 只能看到平台、设备 token、IP、时间和大小。
  - relay 不认证客户端，靠限流防滥用：每个 token 每分钟 30 次，每个 IP 每分钟 300 次，信封不超过 3200 字节。
  - README 原文的说法是：拿到 token 的人只能"trigger iOS generic alerts, but cannot encrypt valid content without the AES key"。
- **移动端功能** **核**
  - 原生 App：审批、回答问题、附件、排队和 steer、文件浏览、git diff、听写。
  - Web 端另有远程终端和语音。
- **支持的 agent** **核**：Claude Code、Codex、Cursor、OpenCode 等约 10 种。

## O7 VibeTunnel（amantus-ai/vibetunnel）

依据：提交 `f78324f5a6`，MIT，约 4.7k stars **核**。

- **架构**：macOS 菜单栏 App 加本地 Node 服务，用 `vt <cmd>` 把任意终端转成浏览器里的终端。传的是 PTY 字节流，不是结构化消息。
- **连接**：局域网、Tailscale（推荐，用 Serve 或 Funnel）、ngrok、Cloudflare Quick Tunnel 等。
- **认证**：系统密码、SSH key，加 JWT；没有应用层 E2EE。
- **推送**：Web Push（VAPID），以及 macOS 原生通知。
- **移动端**：PWA；原生 iOS 版标注为 "work in progress"。

## O8 CloudCLI / Claude Code UI（siteboon/claudecodeui）

依据：提交 `92ccebc660`，AGPL-3.0-or-later，约 14.1k stars **核**。

- **架构**：本地 Node 服务直接读写 `~/.claude` 等原生配置，界面是 Web/PWA，支持 Claude Code、Cursor CLI、Codex、OpenCode。另有商业托管版。
- **连接**：README 的说法是 "open [yourip]:port in any browser on your network"，要从外网访问得自己配隧道。
- **认证**：本地账号（bcrypt 加 JWT），没有 E2EE。
- **推送**：Web Push，标题是会话名，正文带 provider 和消息或工具名（`notification-orchestrator.service.js`）。

## O9 Vibe Kanban（BloopAI/vibe-kanban）

依据：提交 `d5cbb5380f`，Apache-2.0，约 28k stars。2026-04-10 公司宣布关闭，项目转由社区维护 **核**（vibekanban.com/blog/shutdown）。

- **配对** **核**（crates `trusted-key-auth`：`spake2.rs`、`key_confirmation.rs`、`trusted_keys.rs`；已复核依赖 spake2）
  - 主机显示一个 6 位一次性码，客户端输入后用 **SPAKE2** 跑 PAKE，并做密钥确认。
  - 配对成功后，把客户端的 Ed25519 公钥登记为受信任的密钥。
- **请求签名** **核**：每个请求带 Ed25519 签名和时间戳，时间偏差容忍 ±30 秒（`request_signature.rs`）。
- **传输方式一：relay tunnel** **核**
  - 主机主动发起 WSS 连接，用 yamux 多路复用，relay 边缘需要配通配证书。
  - **推**：relay 能看到隧道里的 HTTP 明文；签名只解决认证，不提供机密性。
- **传输方式二：WebRTC** **核**：用 DataChannel 代理 HTTP 和 WebSocket，信令走 relay；只配了 Google STUN，没有 TURN（`relay-webrtc/src/client.rs`）。
- **移动端**：只有响应式 Web，没有推送。

## O10 Moshi（getmoshi.app）：SSH/Mosh 加组网的代表

**核**（getmoshi.app/docs）

- 闭源的 iOS 和 Android 终端 App，通过 SSH 或 Mosh 直连用户自己的机器（无公网 IP 时配合 Tailscale），自动重新挂接 tmux。
- 终端内容不经过 Moshi 的服务器（Chat View 文档原话："without … sending its transcript through Moshi's servers"）。
- 推送：主机上的 `moshi-hook` 把事件（title、message、projectName 等字段）发到 `api.getmoshi.app`，再经 Expo 或 APNs 推给手机。文档没有声明推送内容加密，因此视为明文 **推**。
- 支持 Live Activity 和 Apple Watch 审批；SSH 私钥存在 Secure Enclave 或钥匙串里。
- 同类产品：Blink、Termius、Secure ShellFish **推**。

## O11 Viberra：WebRTC P2P 的代表（早期项目）

依据：提交 `3996d594a4`，Apache-2.0，10 stars **核**。

- **架构**：Control API（FastAPI）负责身份和信令；主机端 agent 用 node-pty 加 WebRTC；浏览器端用 xterm.js；自带 coturn 做 TURN 回退。
- **安全**：用 libsodium 的 Ed25519 设备密钥签名请求，再用 PSK 加 agent 公钥校验 agent hello。传输靠 DTLS 加密。
- **推送**：没有。

## O12 其他（只做了简要了解）

- **CUI**（wbopan/cui）：基于 Claude Code SDK 的 Web UI，带推送。2026-03-20 归档，README 给出的理由是官方的 Remote Control、Claude Code Web 和 Dispatch 已经覆盖了它的用途 **核**。
- **AgentAPI**（coder/agentapi）：把 CLI agent 包装成 HTTP API 的组件，MIT，已废弃并归档 **核**。
- **Catnip**（wandb/catnip）：GitHub 仓库返回 404，无法核实 **推**。
- **Grass**（codeongrass.com）：托管 VM 加手机 App，本地模式据称走局域网或 Tailscale **推**（营销页面）。
- 闭源、没有深入看的：Tactic Remote、Remote Coding、AgentsRoom、ServerCC 等。

## 横向记录

- **连接方式**
  - WebSocket 中继：Remodex、Happier、Omnara 第一代和第二代、Vibe Kanban（tunnel 模式）。
  - 隧道型中继：HAPI（tunwg）。
  - P2P WebRTC：Vibe Kanban、Viberra。
  - Tailscale：VibeTunnel、Remodex、HAPI、Moshi、Grass。
  - Cloudflare Tunnel：HAPI、Remodex、VibeTunnel、Vibe Kanban。
  - 局域网：CloudCLI、VibeTunnel。
  - SSH/Mosh：Moshi。
- **推送**
  - 端到端加密的：只有 HAPI。
  - 明文经过第三方的：Omnara、Happier、Remodex、Moshi。
  - Web Push（VAPID）：VibeTunnel、CloudCLI。
- **集成层的趋势**：靠 PTY 和屏幕抓取的包装器已经被放弃。Omnara 第一代因维护不动而弃用，AgentAPI 也已归档。仍然活跃的产品都改用结构化协议：Codex app-server、Claude Agent SDK、ACP。
- **市场信号**：简单包装器正在被官方功能挤出市场。CUI 已归档，Omnara 已转型，Vibe Kanban 的公司已关闭，AgentAPI 已废弃。
