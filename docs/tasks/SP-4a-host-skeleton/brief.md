# SP-4a 宿主骨架走通（Rust）

- 派发日期：2026-10-16
- 执行者：子 agent（后端工程师，Rust）
- 分支：`spike/SP-4a-host-skeleton`
- 依赖：无（与 SP-1、SP-2、SP-4b 并行）

## 背景

PRD 的 NFR-1 把"会话宿主与客户端之间的契约"列为 M1 必须一次做对的不可逆决定。技术选型讨论会提议用 Rust 写宿主（ADR-02），并给出了以下几项的设计：

- 进程形态（ADR-01）
- 协议，包括分帧、seq 与合并、命令幂等、宽容读取（ADR-04）
- 本地传输（ADR-05）

本任务用一个能跑的最小骨架验证这些设计。它也是 ADR-02 的"改变条件"的依据：如果 Rust 骨架明显超出估算，而且原因主要在语言本身，就要重新考虑语言选择。这个骨架的代码可能成为 M1 第一批任务的起点。

开始前必读：
- `docs/decisions/ADR-01-host-process.md`、`ADR-02-host-language.md`、`ADR-04-host-protocol.md`、`ADR-05-local-transport-auth.md`
- `docs/meetings/2026-10-16-tech-selection/minutes.md`
- `docs/meetings/2026-10-16-prd-arch-alignment/architect.md` 议题 1.3 和 2.3
- `architect.md` 与 `architect-round2.md` 中关于协议的部分

## 范围

在 `spikes/SP-4a-host-skeleton/` 下建一个 Rust workspace：

- **`cupper-protocol`**：
  - 不做 I/O；
  - 帧格式是 `u32 长度 | u8 帧类型 | 负载`，单帧不超过 64,000 字节；
  - 带 JSON-RPC 信封，以及 `initialize`、`stream/subscribe`、`stream/events`、`stream/reset`、`session/create`、`session/send`、`session/interrupt`、`interaction/respond`、`blob/get` 这些消息类型的一个子集；
  - 枚举和联合类型默认开放，并带 `Unknown` 兜底；
  - 用 schemars 导出 JSON Schema。
- **`cupper-host`**：
  - Unix 域套接字，放在临时数据目录里，目录权限 0700；
  - 用 `flock` 保证单实例；
  - 用 SQLite 存事件日志，同一条流的 seq 连续；
  - 订阅时按游标补发，游标太旧就先给快照；
  - 写入时合并文本片段；对慢连接做发送时合并，产生带 `from` 的区间事件；
  - 每个连接有自己的发送队列，超出上限就发 `stream/reset`，读取 ACP 的那条路径永远不能被阻塞；
  - 命令按 `command_id` 幂等；
  - 交互请求先到先得，后到的返回 `conflict`；
  - ACP 驱动用官方 Rust SDK `agent-client-protocol`；
  - 拉起适配器时：每个适配器一个进程组，环境变量走白名单，用 `POSIX_SPAWN_CLOEXEC_DEFAULT`；
  - 崩溃恢复：清理残留进程，把运行中的回合标记为"已中断"，把待处理的请求标记为"已失效"，然后用 `session/resume` 继续。
- **`cupper-cli`**：能订阅、发送、中断、批准和回答提问，供测试用。
- **`cupper-fake-agent`**：一个按剧本输出 ACP 帧的假 agent。如果 SP-1 的录制文件已经提交到 main，可以拿来用；没有的话，先自己写几个简单的剧本。
- **认证**：只实现 `test_harness` principal 和 CLI 只读的 principal。`test_harness` 只在 `test-harness` 编译特性下存在，密钥通过继承的 fd 传入。代码签名校验不在本任务的范围内，属于 SP-4b。

**自动化测试**（使用 `test-harness` 构建和假 agent）：

1. 两个客户端同时订阅，看到的内容一致。
2. 审批先到先得，后到的一方收到 `conflict`。
3. 客户端断开期间产生的事件，重连后补齐，不丢也不重复。客户端检查"`from` 等于上一个 seq 加 1"这个不变式。
4. 一个慢客户端不会拖住 agent：用假 agent 高速输出，同时让一个客户端不读。
5. 对宿主执行 `kill -9`，重启后会话可以继续，没有残留的适配器进程。
6. 命令重试（同一个 `command_id` 发两次）不会重复执行。
7. 所有帧都不超过上限（性质测试）。

**真实适配器冒烟测试**：用 claude-agent-acp 配合本机的 `claude`，在临时测试仓库里跑通"新建会话 → 一个回合 → 一次审批 → 杀掉宿主 → 重启后 resume"。codex-acp 有余力再做。

**测量**：
- 冷启动：从拉起宿主到客户端收到 index 快照的耗时（目标 p95 不超过 400 ms）；
- 宿主的常驻内存：空闲时，以及挂 6 个假会话时。

**代码生成验证**：
- 用 @hey-api/openapi-ts 从导出的 JSON Schema 生成 TS 类型和 zod 校验，确认可用，特别是带 `type` 标签的联合类型和 `Unknown` 兜底；
- 确认 `cupper-protocol` 能编译到 `aarch64-apple-ios-sim`。可以执行 `rustup target add aarch64-apple-ios-sim`，在报告里注明。

**返工记录**：在 `notes.md` 里记录开发过程中的编译修复轮数和卡点，作为 ADR-02 判断语言是否合适的依据。

**不做**：桌面 UI、代码签名校验、LaunchAgent、通知、Git 服务、加密。

## 安全约束（必须遵守）

- 宿主只用临时数据目录和临时套接字，不碰任何生产路径，也不碰用户的其他目录。
- 真实适配器冒烟测试的约束和 SP-1 相同：只在临时测试仓库里跑；只用默认的、或者更严格的权限模式；不要选择"总是允许"；不修改 `~/.claude` 和 `~/.codex`；回合尽量少，能选模型时选最便宜的。
- 依赖只用 crates.io 和 npm 上的正式发布版本，不要全局安装任何东西。
- 遵守 `CLAUDE.md` 子 agent 守则第 7 条：外发请求里不能出现任何个人信息。
- 提交的内容里不能有个人信息（本机路径、用户名等）。

## 验收标准

- [ ] workspace 能构建，自动化测试 1–7 全部通过，并在报告中给出运行命令和输出摘要。
- [ ] 真实适配器冒烟测试的结论，包括 resume 是否成功。
- [ ] 测量数据：冷启动、内存。
- [ ] TS 生成链和 iOS 编译检查的结论。
- [ ] 返工记录，以及对 ADR-02"改变条件"的判断。
- [ ] 对 ADR-01、04、05、06 的修改建议，例如发现设计上行不通、或者可以简化的地方。

## 汇报要求

按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。代码、`notes.md` 和其他物料提交在任务分支上。
