# ADR-02 会话宿主的实现语言

- 状态：提议
- 日期：2026-10-16
- 来源：`docs/meetings/2026-10-16-tech-selection/`（`minutes.md`；`architect.md` ADR-02；`mobile-security-engineer.md` 第 2 节；`desktop-engineer.md` 第 8 节）

## 背景

宿主是 cupper 里权限最高的进程：它持有审批权，能执行 git，能拉起 agent。它也是本地鉴权的信任锚（ADR-05），M3 起还要保存设备身份密钥。

不管宿主用什么语言，Node ≥ 22 都必须有，因为 claude-agent-acp 依赖它。所以 Node 从哪里来是 ADR-08 要解决的问题，不能作为宿主必须用 Node 的理由。

M3 需要和 iOS 共享协议，以及端到端加密相关的代码。

## 可选方案

TypeScript/Node、Rust、Swift，另以 Go 作为对照。

## 决策

**宿主用 Rust。** 主要理由：

1. **在 ACP 里，我们是客户端。** 官方的 Rust SDK（`agent-client-protocol` 3.3.0）就是 Zed 作为 ACP 客户端在生产中使用的那一个。
2. **签名属性干净。** 宿主是单个签名的 Mach-O，开启 hardened runtime 也不需要任何放宽的 entitlement。官方 Node 二进制带有 `get-task-allow`、`disable-library-validation` 等 entitlement，同一用户下的其他进程可以向它注入代码，不适合用来跑持有审批权的宿主。
3. **系统接口可以直接调用。** `LOCAL_PEERTOKEN`、Security 框架、IOKit 都能直接用，不需要原生插件。
4. **M3 可以共享代码。** 加密核心可以通过 UniFFI 交给 iOS，也就是"一份实现，两端使用"。
5. **对 AI 写的代码更有保障。** 宿主是长期运行、并发密集的守护进程，编译器能替 AI 写的代码兜住一部分错误。界面放在 AI 最熟悉的 React/TS 上。这种分工和 Codex 一致：Rust 的 app-server，加上 Electron 外壳。

**crate 划分（初版）**

| crate | 内容 |
|---|---|
| `cupper-protocol` | 协议类型、帧、版本常量；不做 I/O；不依赖宿主内部；能编译到 iOS |
| `cupper-host` | 网关、策略、编排、交互代理、存储、驱动、Git |
| `cupper-cli` | 第二客户端，用于 NFR-1 验收 |
| `cupper-fake-agent` | 按剧本输出 ACP 帧，供测试使用 |
| `cupper-client`（视 ADR-15 而定） | 不做 I/O 的客户端同步逻辑，由 CLI 使用，也可能由 iOS 使用 |
| `cupper-crypto`（M3） | 配对、Noise、推送加解密 |

**给 AI 编码 agent 的约束**（gate/design 通过后写进 `CLAUDE.md`）：
- 运行时用 tokio，模块之间通过 channel 通信；
- `unsafe` 只能出现在 `sys` 模块里；
- 错误类型统一用 thiserror；
- 共享的 crate 不做 I/O。

## 后果

- **好处**：宿主作为信任锚，签名属性干净；M3 可以共享代码；ACP 客户端用的是官方 SDK。
- **代价**：两种语言（Rust 加 TS），两套工具链；贡献者的门槛略高；协议类型需要代码生成，见 ADR-04。
- **改变条件**：
  - SP-4 的 Rust 宿主骨架超出估算的 50% 以上，而且原因主要在语言本身。此时改为 TS 宿主，加密部分仍用 Rust crate，由宿主通过 napi-rs 调用，并自行签名一份收紧过 entitlement 的 Node 运行时。
  - ACP Rust SDK 在我们需要的功能上，连续一个以上的发版周期落后于 TS SDK。

## SP-4a 实测结论（2026-10-16）

**维持用 Rust。**

- AI agent 写了约 7,000 行 Rust（含测试），编译修复 3 轮，共 4 个错误，每个都是一两行就修好；没有出现和借用检查器反复拉扯的情况。不满足"改变条件"。
- **额外的成本**主要在三处，都不大：
  - 约 150 行 `unsafe` 系统调用；
  - cargo 合并依赖的特性时，会带来隐性的行为变化：实测中 serde_json 的特性被合并，导致导出 schema 的键顺序变了；
  - ACP SDK 3.x 的新 API 需要读源码文档。
- **注意**：ACP 的 Rust SDK 里有几个枚举是封闭的（`StopReason`、`PermissionOptionKind`、`ToolCallStatus`）。适配器发来未知的值时，整条消息会解析失败。M1 要在驱动层加一层容错，并向上游反馈。
- **gate/design 通过后写进 CLAUDE.md 的约束**：
  - ACP 回调里只做 channel 发送，耗时的任务另开 task；
  - SDK 锁定精确版本；
  - 导出给其他语言的产物，不能依赖 serde_json 的 map 键顺序；
  - `unsafe` 只能出现在 `sys` 模块。

## 设计评审修订（2026-10-16）

来源：设计评审会（`docs/meetings/2026-10-16-design-review/minutes.md`）、用户决定（`docs/meetings/2026-10-16-gate-design-decisions/minutes.md`）、详细设计 v0.7 的 5.13.2。本节是细化，不推翻上文；上文中被替换的写法已就地改正并注明。

- **crate 表补充**：`cupper-sys`（集中全部 `unsafe`，是"`unsafe` 只在 sys 模块"在多 crate 下的落法）、`cupper-desktop-native`（napi-rs，桌面主进程用来拉起和校验宿主）、`xtask`。
- 协议的 `Method` trait 声明 `SCOPE` 和 `IDEMPOTENCY`，由表驱动测试逐格检查权限矩阵。
- **写进 `CLAUDE.md` 的约束新增**：
  - 时间只经 `Clock` 获取、ID 只经 `IdGen` 生成，clippy `disallowed-methods` 禁止直接调用 `Instant::now`、`SystemTime::now`、`tokio::time::*`、`std::thread::sleep` 等；
  - 有状态的决策（回合状态机、队列、交互代理、自发回合判定、合并器）写成不做 I/O 的纯核心 `step(input, now) -> Vec<Action>`；
  - 宿主的所有子进程一律经 `cupper-sys::spawn` 拉起。
  详见详细设计 5.1.7、5.2.16。
