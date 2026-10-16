# R03 来源：Happy 源码深读笔记

- 访问日期：2026-10-16。源码通过 GitHub codeload 下载 tarball，解压到 worktree 之外的临时目录，只读，不执行。
- 标注：**核** = 读源码或官方文档核实；**推** = 根据源码推断，没有实际运行。
- 报告正文用 `[H编号]` 引用本文件的条目。

## H0 仓库与版本

| 仓库 | 提交 | 说明 | 许可证 |
|---|---|---|---|
| github.com/slopus/happy | `a301a0dc4`（2026-10-16） | monorepo，包含：移动和网页端 `packages/happy-app`（Expo）；原版 CLI `packages/happy-cli`（npm 包名 `happy`，v1.2.5）；中继服务器 `packages/happy-server`；自托管版 `packages/happy-server-self-host`；共享协议 `packages/happy-wire`；尚未发布的原生端 `packages/happy-ios`（Swift）和 `packages/happy-android` | MIT（根目录和各包都是）**核** |
| github.com/slopus/happy-desktop | `5bce846a1` | Electron 桌面端 | MIT **核** |
| github.com/slopus/happy-agent | `84a028bcb` | Happy Agent，桌面端使用的自研无头 agent 运行时 | MIT **核** |
| github.com/slopus/slopus.github.io | `5ace459bc` | happy.engineering 官网和文档的源码；线上 https://happy.engineering/docs/security/ 与源码一致 | — |
| slopus/happy-server、slopus/happy-cli | 已归档 | 旧的独立仓库，已并入 monorepo | 根目录没有 LICENSE |

下文路径中，`H:` 表示 `https://github.com/slopus/happy/blob/a301a0dc4/`，`HD:` 表示 happy-desktop，`HA:` 表示 happy-agent。

产品现状：Happy 现在有两条产品线，一是原版 CLI（`happy claude`、`happy codex`，包装原生 CLI），二是"桌面端 + Happy Agent"（自研 harness，支持多种模型和多人协作）。两条线共用同一个移动 App 和同一个中继服务器 **核**（H:README.md、HD:README.md）。

## H1 架构：手机或网页、中继服务器、CLI 或 daemon 三方

- 服务器技术栈：Fastify 提供 HTTP，Socket.IO 提供实时通道（只有一个端点 `/v1/updates`）；数据存在 Postgres（Prisma），多副本时用 Redis 做 Socket.IO 的 streams adapter；附件放在 S3 兼容存储，没配就退回本地 `./data/files` **核**（H:docs/deployment.md、H:docs/backend-architecture.md、H:docs/protocol.md）。
- **服务器的角色是"中继 + 持久化存储"**：会话、消息、机器状态、KV、artifact 都以密文形式落库；手机离线后重连，靠 `seq` 增量补拉 **核**。
- Socket 有三种 scope：`user-scoped`（App 或网页）、`session-scoped`（每个会话进程一条）、`machine-scoped`（每个 daemon 一条）；握手时带 `auth.token` 和 `clientType` **核**（H:packages/happy-server/sources/app/api/socket.ts:57-60）。
- RPC 机制：被调用方先 `rpc-register` 加入房间 `rpc:<userId>:<method>`，调用方发 `rpc-call`，服务器转成 `rpc-request` 送达。**方法名是明文**，参数和返回值用会话或机器的密钥加密 **核**（H:packages/happy-cli/src/api/rpc/RpcHandlerManager.ts:46-94）。
- daemon 常驻后台，向服务器注册机器，并在 127.0.0.1 上开一个控制用的 HTTP 端口；手机通过机器 RPC `spawn-happy-session` 远程新建会话 **核**。

## H2 原版 CLI 怎么包装各家 agent

- **Claude Code** 分本地和远程两种模式 **核**（H:docs/session-protocol-claude.md；H:packages/happy-cli/src/claude/claudeLocal.ts:312-320、claudeLocalLauncher.ts:84-94）。
  - 本地模式：用 cross-spawn 启动真正的 `claude` 交互进程，`stdio: ['inherit','inherit','inherit','pipe']`。这不是 PTY，而是直接继承当前终端；fd3 用来回传"思考中"状态。对话记录通过 SessionStart hook 和扫描 JSONL 会话文件获取。
  - 远程模式：手机发来消息时，先通过 `switch` 或 `abort` RPC 让本地进程退出，再改用 Claude Agent SDK 的 `query()` 流式驱动。
- **Codex**：自己写了一个 `codex app-server` 客户端，在 stdio 上跑 JSON-RPC 2.0。源码注释写明 `@openai/codex-sdk` 只包了 `codex exec`，没有审批回调，满足不了"在手机上审批"的需求 **核**（H:packages/happy-cli/src/codex/codexAppServerClient.ts:1-14）。
- **Gemini 及其他 agent 走 ACP** **核**（H:packages/happy-cli/src/agent/acp/acpAgentConfig.ts:7、index.ts:361、721-743，依赖 `@agentclientprotocol/sdk ^0.14.1`）。
  - Gemini 通过 `gemini --experimental-acp` 接入。
  - `happy acp opencode` 是通用的 ACP 入口。
  - 另外还接了 OpenClaw，以及 agy（每轮运行一次 `agy --print`）。

## H3 Happy Agent 和桌面端这条线

- Happy Agent 原生实现 Happy 协议，不包装其他 CLI。它把自己注册成一台机器，把每个主会话投影到 Happy 服务器上。启动时从 `~/.happy/access.key` 导入 CLI 的凭据，所以已经配对过的机器不需要再扫码 **核**（HA:docs/happy.md、HA:INTEGRATIONS.md）。
- 没有本地和远程两种模式之分：手机发来的消息进入同一个会话、同一个队列，受同一个权限边界约束 **核**。
- **能力声明**：agent 把自己的能力写进加密的会话 metadata，手机据此决定显示哪些功能。能力包括 abort、附件、文件浏览、读、搜、写，模型和推理档位选择，权限模式选择，shell，steering（运行中插话），以及 `rpcMethods` 列表。metadata 里还带完整的模型目录和权限模式列表，每种模式标注语义类别（default、read-only、safe-yolo、yolo），手机只负责渲染，不需要理解具体语义 **核**（HA:docs/happy.md）。
- 消息同步以加密的 v3 HTTP 消息为准，socket 只负责通知"有新内容"；消息 ID 固定，重试是幂等的 **核**。
- 桌面端和团队服务器之间走 Tailcat。文档的描述是"WireGuard without a Tailscale account"，支持 NAT 穿透，地址里内嵌预共享密钥。手机不走 Tailcat，仍然走 Happy 中继 **核**（文档层面；Tailcat 的上游实现没有核实）（HA:docs/tailcat.md、官网 content/desktop/multiplayer.mdx:14-15）。
- 支持的 agent：原版 CLI 支持 Claude Code、Codex、Gemini、通用 ACP、OpenClaw、agy；桌面端和 Happy Agent 支持 Claude、Codex、Grok 等 **核**。

## H4 账号模型：没有用户名和密码

- 账号就是手机生成的 32 字节随机 master secret **核**（H:packages/happy-app/sources/app/(app)/index.tsx:55-60）。
- 登录流程：由 seed 派生 Ed25519 签名密钥对，对随机 challenge 签名后 `POST /v1/auth`；服务器用 tweetnacl 验签，以公钥（hex）为键 upsert Account，然后签发 token **核**（auth/authChallenge.ts；happy-server/.../authRoutes.ts:9-38）。
- token 由 privacy-kit 的 `createPersistentTokenGenerator` 生成，以服务器的 `HANDY_MASTER_SECRET` 为种子。**token 没有过期时间，服务器端也没有吊销表** **核**（happy-server/sources/app/auth/auth.ts）。
- 备份：master secret 以 base32 显示，格式为 `XXXXX-XXXXX-…` **核**（auth/secretKeyBackup.ts）。
- 本地存储：iOS 和 Android 用 expo-secure-store；**网页端存在 localStorage 里** **核**（auth/tokenStorage.ts）。

## H5 终端（CLI 或桌面）扫码配对

1. CLI 生成一次性的 X25519 box 密钥对，然后 `POST /v1/auth/request {publicKey, supportsV2:true}`。
2. 二维码内容是 `happy://terminal?<base64url(CLI 临时公钥)>`，**只有公钥**，没有服务器地址，也没有挑战值；网页端则是 `${webappUrl}/terminal/connect#key=<pubkey>` **核**（happy-cli/src/ui/auth.ts:92-101、api/webAuth.ts）。
3. 手机扫码后，用 `crypto_box_easy` 加上一把临时发送方密钥（效果相当于匿名的 sealed box），把应答加密给 CLI 的公钥，格式是 `ephPub(32)|nonce(24)|ct`。应答有两个版本 **核**（happy-app/sources/hooks/useConnectTerminal.ts:31-39、encryption/libsodium.ts）：
   - V1：直接把**整个 master secret** 发过去。
   - V2：`0x00 || contentPublicKey(32)`。
4. 手机带上自己的 Bearer token 发 `POST /v1/auth/response`，服务器把应答存进 `TerminalAuthRequest.response`。
5. CLI 每秒轮询 `/v1/auth/request`，拿到 `{token, response}` 后用自己的私钥解开 **核**（ui/auth.ts:150-210、persistence.ts:263-280）：
   - 解出 32 字节，就按 legacy 模式处理。
   - 解出 33 字节且首字节为 0，就按 dataKey 模式处理：保存 content 公钥，并在本地随机生成一把 machineKey。
   - 凭据写入 `~/.happy/access.key`，写文件时没有显式指定权限，由 umask 决定。
6. Happy Agent 用同一套协议，客户端侧有 120 秒过期，**服务器侧仍然没有过期** **核**（HA:packages/happy-agent-modules/sources/happy/HappyPairing.ts:10、96）。

## H6 加手机或浏览器，以及多设备与撤销

- 新设备生成一把临时密钥对，显示二维码 `happy:///account?<pubkey>`；已登录的设备扫码后，把**完整的 master secret** 用 box 加密发给新设备。也可以手动输入备份的 secret key **核**（hooks/useConnectAccount.ts:24-33、auth/authQRStart.ts、authQRWait.ts、HD:docs/mobile-access.md）。
- 所以所有手机和浏览器**共享同一个 master secret**，没有每台设备各自的密钥 **核**。
- 官方文档原文："Desktop cannot list or selectively revoke phones. Logging out on a phone removes that phone's local login; it is not a server-side per-device revocation guarantee." **核**（HD:docs/mobile-access.md）
- `DELETE /v1/machines/:id` 只删除 Machine 记录和 AccessKey，**不会让这台机器 CLI 的 token 失效** **核**（happy-server/.../machinesRoutes.ts:198-245）。在 server、cli 和 app 的同步与认证代码里搜不到 revoke 或 rotate，即没有密钥轮换和撤销机制 **核**。
- 官网安全页对"CLI 丢失"的说法是 "Remove from account. The machine keeps its machine key and the last 14 days' session DEKs" **核**。

## H7 加密库与密文格式

- 用到的库 **核**：
  - CLI：`tweetnacl`，加上 Node 自带 crypto 的 AES-256-GCM。
  - App 原生端：`@more-tech/react-native-libsodium`，AES-GCM 用 `rn-encryption`（issue #138 报告过它的已知问题）。
  - App 网页端：`libsodium-wrappers`。
  - Happy Agent：noble 系列的 NaCl 实现。
- 两种格式 **核**（happy-cli/src/api/encryption.ts、H:docs/encryption.md）：
  - **legacy**：`secretbox`（XSalsa20-Poly1305），直接用 master secret 做密钥，格式 `nonce(24)|ct+tag`。
  - **dataKey**：AES-256-GCM，格式 `ver(1)=0|nonce(12)|ct|tag(16)`。
- DEK 的包装方式：`tweetnacl.box` 加一把临时密钥对，得到 `ephPub(32)|nonce(24)|ct`，前面再加版本字节 `0x00`，最后 base64 编码放进 `dataEncryptionKey` 字段 **核**。

```ts
encryptWithDataKey(data: any, dataKey: Uint8Array): Uint8Array      // AES-256-GCM
libsodiumEncryptForPublicKey(data, recipientPublicKey): Uint8Array  // 临时密钥 + box
authChallenge(secret) -> tweetnacl.sign.keyPair.fromSeed(secret)    // Ed25519 登录
```

## H8 密钥层级

**核**（happy-app/sources/encryption/deriveKey.ts、sync/encryption/encryption.ts:13-30、官网 docs/security.mdx）

- KDF 是自定义的 HMAC-SHA512 密钥树，类似 BIP32，而不是 HKDF：
  - 根节点：`I = HMAC-SHA512(usage + " Master Seed", seed)`
  - 子节点：`HMAC-SHA512(chainCode, 0x00 || index)`
- content 密钥对：`contentDataKey = deriveKey(master, 'Happy EnCoder', ['content'])`，再用 `crypto_box_seed_keypair` 生成。私钥只留在手机和浏览器上，公钥交给 CLI。
- 每个会话的 DEK：由 CLI 随机生成 32 字节，用 content 公钥包装后上传（api.ts:37-66）。
- 每台机器的 DEK：就是 CLI 的 machineKey，同样包装后上传（api.ts:151-161）。
- 附件：密钥是 `deriveKey(sessionDEK, 'Happy Blobs', ['session'])`，附件用 secretbox 加密后上传到 S3 预签名 URL。
- 账户设置：用 master secret 经 secretbox 加密。
- 最小权限：V2 模式下 CLI 只拿到 content 公钥，因此只能解密自己创建的会话；手机恢复某个会话时，通过机器 RPC 把该会话的 DEK 交给 daemon。legacy V1 模式下 CLI 持有完整的 master secret。

## H9 哪些加密、哪些明文

**核**（happy-server/prisma/schema.prisma、H:docs/encryption.md、H:docs/protocol.md、storage/types.ts）

**加密的内容**
- 会话的 metadata（路径、主机名、摘要等）和 agentState（权限请求涉及的工具和参数）。
- 全部消息。
- 机器的 metadata 和 daemonState。
- artifact、KV 的值、账户设置、附件。
- RPC 的参数和返回值。

**服务器可见的明文**
- 账户：公钥（hex）、姓名、用户名、头像、GitHub 资料；好友关系和动态。
- 会话：id、tag（随机 UUID）、`seq`（据此可以推出消息条数）、active、lastActiveAt、各类时间戳、projectId。
- 消息：长度和时间戳。
- 机器：id 和在线时间。
- `Project.externalId`；KV 的键（源码注释写的是 "Unencrypted for indexing"）。
- **UsageReport，即每个会话的 token 数和费用，是明文**（apiSession.ts:900-925）。
- 在线和"思考中"这类临时事件。
- 推送 token、IP，以及 `X-Happy-Client` 版本头。
- RPC 方法名，比如能看出调用了 `bash` 或 `readFile`。

**第三方凭据不在 E2EE 范围内**：GitHub OAuth、用户登记的 OpenAI、Anthropic、Gemini 密钥，都是用服务器自己的 `HANDY_MASTER_SECRET` 加密，服务器自己能解开。

## H10 E2EE 之外的数据流出

- 语音：使用 ElevenLabs 语音时（经 LiveKit WebRTC），会把会话目录和当前会话的完整上下文发给 ElevenLabs **核**（H:docs/voice-architecture.md）。
- 分析：接了 PostHog，文档称不包含内容，可以关闭 **核**。

## H11 推送

- 通道：Expo Push（`https://exp.host/--/api/v2/push/send`），由 Expo 转发给 APNs 和 FCM；推送 token 通过 `POST /v1/push-tokens` 存到服务器 **核**（happy-server/sources/app/push/pushSend.ts）。
- **推送内容是明文** **核**（happy-cli/src/api/pushNotifications.ts:16-75、250-300，已复核）。
  - 标题是固定文案："It's ready!"、"Permission request"、"Clarification needed"。
  - **正文是会话标题**（`getSessionNotificationBody` 返回 `getSessionTitle(metadata)`，即摘要或工作目录名）。
  - data 字段里有 sessionId 和 sessionTitle。
  - 推送不包含工具名和命令参数，但会话标题和目录名会以明文经过 Happy 服务器、Expo 和 APNs/FCM。
- 在场抑制：只要有 UI 客户端处于活跃状态，服务器就不发推送 **核**（pushRoutes.ts:89-149、pushDispatch.ts）。`happy notify` 和缺少 sessionId 的兜底路径会由 CLI 直接调用 Expo **核**。
- **没有用 iOS Notification Service Extension 解密** **核**：只搜到一条提到 NSE 的 TODO，内容是关于富媒体图片的。
- 官网说的"titles never included"指的是 PostHog 分析，不包括推送。

## H12 移动端功能

**核**（happy-app 的路由、sync/ops.ts、CLI 注册的 RPC、官网 desktop/mobile.mdx 和 mobile-app.mdx）

**查看**
- 会话列表：按机器和项目分组，显示在线和"思考中"状态。
- 实时对话记录。

**交互与控制**
- 发消息，可以附加加密的图片；Happy Agent 支持在运行中插话。
- **审批权限请求**：RPC `permission`，可以选择本会话内一直允许、设置 allowedTools、修改工具入参。
- 回答 agent 的提问。
- 中止（`abort`）、结束会话（`killSession`）、切换本地和远程模式（`switch`）。
- 切换模型、推理档位和权限模式。

**代码与文件**
- **查看 diff**：基于 git status 和 difftastic，路由为 `session/[id]/changes`。
- 浏览和查看文件、ripgrep 搜索、写文件。
- **远程 shell**：RPC `bash`，会话级和机器级都有。

**会话管理**
- **新建会话**：选择机器、目录、agent、模型、权限模式，然后调用 `spawn-happy-session`。
- 恢复、fork、复制会话、回退点、归档和删除。
- 停止 daemon、删除机器。

**其他**
- 语音（付费）、artifact、好友和收件箱、用量页、自定义服务器地址。

## H13 自托管

- `happy server`：内嵌 PGlite 和本地文件存储，不需要 Postgres、Redis 或 S3；会写入 `settings.serverUrl`，设置后不再回退到公共服务器 **核**（happy-server-self-host/README.md）。
- Docker：根目录有 Dockerfile，必需的环境变量只有 `HANDY_MASTER_SECRET`；生产部署用 Postgres、Redis、S3，另有 Kubernetes 清单 **核**（官网 content/docs/guides/self-hosting.mdx、H:docs/deployment.md）。
- 客户端指向自托管服务器的方式：App 首屏的齿轮里填 Server URL；CLI 用 `HAPPY_SERVER_URL`；Happy Agent 用 `HAPPY_AGENT_HAPPY_SERVER_URL`。桌面端暂时没有这个设置。默认的公共服务器是 `https://api.cluster-fluster.com` **核**。
- **自托管后仍然依赖的外部服务**：推送走 Expo（地址在代码里写死为 exp.host）；语音默认走官方服务 **核**。
- 服务器体量：官方称约 12,000 行 TypeScript。

## H14 已知安全问题

- GitHub security-advisories 返回空；没有找到第三方安全审计；issue #329 "Security Contact?"（2026-01）仍未关闭 **核**。
- 下表为 2026-10-16 查看的状态。

| issue | 内容 | 状态 | 读源码后的判断 |
|---|---|---|---|
| #1503（2026-07-10） | **配对时交付 content key 没有认证**：恶意中继可以塞入自己的公钥，从而拿到所有会话的 DEK，读取全部内容，还能伪造消息让 agent 执行 bash，等于在用户电脑上远程执行代码 | 未关闭，无回复 | **核**：CLI 的 auth.ts:191-196 和 Happy Agent 的 HappyPairing.ts:267-288 都不加验证地接受应答（auth.ts 已复核） |
| #674 | 配对请求在服务器上没有过期时间，二维码被拍到后可以无限期重放 | 未关闭 | **核**：数据库 schema 里没有 expiresAt |
| #416 | 文档曾称"只存公钥哈希"，实际存的是 hex 格式的公钥 | 未关闭 | **核** |
| #138 | rn-encryption 依赖有已知问题 | 未关闭 | 依赖仍在使用 |
| #1279、#599/#1089、#1086 | 日志泄露环境变量、缺少限流、Mermaid 渲染的 XSS | 未关闭 | 没有核实 |
| #1262 | 请求加 PIN 或生物识别的应用锁 | 未关闭 | 由此推断目前没有应用锁（**推**） |

补充分析（**推**，根据源码推断，没有实测）：
- `POST /v1/auth/request` 只要提交公钥就会返回 `{token, response}`，不要求证明持有对应的私钥。而二维码里放的正是这个公钥，所以任何看到过二维码的人，都能在手机批准之后拿到这个账户的 token。他解不开 response，但可以读取全部密文和元数据、删除数据、发送推送。再加上 #674 配对请求永不过期，风险会被放大。
- token 永不过期，服务器上也没有吊销表，所以设备丢失后无法在服务端作废它的 token。

## H15 对 cupper 的借鉴（来自本笔记的归纳）

1. 中继兼做密文存储，配合 `seq` 增量同步，手机离线后可以补拉；RPC 和同步共用一条 Socket.IO 连接。Happy Agent 是后来才接入的，说明这种结构支持后续扩展。
2. "每个会话一把随机 DEK，再用账户的 content 公钥包装"这种密钥层级，让桌面端只需持有公钥，满足最小权限；以后要分享会话，也只需把 32 字节的 DEK 重新包装一次。
3. 要避开的坑：配对应答必须经过认证；配对请求要在服务器上设置过期时间；领取 token 时要证明持有对应私钥；token 必须可以吊销；每台设备要有自己的密钥；推送正文不能带明文；用量、在线状态、RPC 方法名这类元数据要在设计阶段逐一列出来。
4. 能力声明（capabilities、模型目录、带语义类别的权限模式）把手机端和具体 agent 解耦，正适合"ACP + 多 agent + 手机统一渲染"的思路。
5. 手机端的远程 `bash`、写文件、新建会话，相当于把电脑上的代码执行权交给了手机。这类操作应该受 agent 自身的沙箱和权限边界约束，Happy Agent 就是这么做的。
