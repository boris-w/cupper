# SP-4b macOS 本地安全实测

- 派发日期：2026-10-16
- 执行者：子 agent（安全工程师）
- 分支：`spike/SP-4b-local-security`
- 依赖：无（与 SP-1、SP-2、SP-4a 并行）

## 背景

ADR-05 提议用 Unix 域套接字连接宿主，并通过 `LOCAL_PEERTOKEN` 取对端的审计令牌，校验对端的代码签名。ADR-01 提议把宿主做成 app-like 的 helper bundle，升级时由我们控制安装时机，拉起运行时之前先比对 cdhash。

这些设计里有几个假设还没有实测，讨论会标为"推"：
- 审计令牌转换成 SecCode 是否可靠，论坛上有 EPERM 的报告；
- 磁盘上的代码被替换之后，签名校验的表现（`errSecCSStaticCodeChanged`）；
- agent 自带的沙箱能不能挡住连接套接字；
- 血缘检查能挡住哪些逃逸方式。

开始前必读：
- `docs/decisions/ADR-05-local-transport-auth.md`
- `docs/decisions/ADR-01-host-process.md`
- `docs/meetings/2026-10-16-tech-selection/mobile-security-engineer.md` 第 4 节
- 同目录下的 `architect.md` ADR-05 一节，以及三份第二轮回应

## 范围

1. **对端代码签名校验**：
   - 写一个最小的 UDS 服务端（Rust，可以用 `security-framework` 或 `objc2-security`）。拿到连接后：用 `getsockopt(LOCAL_PEERTOKEN)` 取审计令牌 → `SecCodeCopyGuestWithAttributes(kSecGuestAttributeAudit)` → 用 `SecCodeCheckValidity` 按要求校验。
   - 再写几个测试客户端：
     - 签了"目标 identifier"的客户端；
     - 签了别的 identifier 的客户端；
     - 未签名的客户端；
     - 用 node 写的客户端；
     - ad-hoc 签名的客户端。
   - 验证服务端能认出第一个，拒绝其余的，并检查 hardened runtime 和 `get-task-allow` 这两个标志。
   - 反方向也要测：客户端校验服务端，也就是"UI 校验宿主"。
2. **签名方式**：本机已有可用的代码签名身份。可以用它们给测试二进制签名，但**不要创建、导入或删除证书，也不要改动钥匙串**。签名要求要分别测两种：
   - 带 Team ID 的要求，用于发布构建；
   - 退化模式（开发构建，ad-hoc 签名）下能做到什么程度。

   提交的内容和报告里都不能写签名身份的名称和 Team ID，用占位符代替。
3. **代码被替换后的表现**（ADR-01 升级演练的缩小版）：
   - 在临时目录里做一个测试用的 `.app`，里面带一个 helper bundle；
   - 让 helper 运行起来，然后模仿 Sparkle 的做法替换整个 `.app`（先写到新位置再改名，或者用 `ditto` 覆盖）；
   - 验证：旧进程能否继续运行；新客户端按审计令牌校验旧进程的结果，会不会出现 `errSecCSStaticCodeChanged`；旧进程拉起子进程之前比对 cdhash 能否发现版本变了。
4. **agent 沙箱能不能挡住连接套接字**：在临时测试仓库里，分别用以下方式让 agent 执行一条连接测试套接字的命令（例如用 `nc -U`，或者一行 python），记录连接是否成功：
   - Claude Code 开启沙箱、使用默认权限；
   - Codex 的只读、工作区、Agent 模式。

   用户已许可使用本机已登录的 CLI 运行真实会话。约束和 SP-1 相同：只在临时目录里跑；不用"完全访问"或 bypass 模式；不选"总是允许"；不修改 `~/.claude` 和 `~/.codex`；回合尽量少。
5. **血缘检查**：
   - 宿主怎样判断"申请方是不是某个适配器进程的后代"，例如用 `proc_pidinfo` 加父进程链，或者进程组、会话 ID；
   - 测试以下逃逸方式能否绕过：`nohup`、双重 fork、`setsid`、`disown`。`launchctl submit` 会在系统里注册任务，**这一项不做**，只在报告里分析。
6. **其他注入面**（只读分析，或者在临时目录里做最小验证）：同一用户的进程能否看到别的进程的 argv 和环境变量；`DYLD_*` 环境变量和 `task_for_pid` 对开了 hardened runtime 的二进制是否无效。

- 不做：
  - 注册 LaunchAgent 或 SMAppService；
  - 任何会触发 TCC 弹窗的操作；
  - 测试通知身份。

  这些会改动系统设置或打扰用户，留给 SP-10。Electron 相关的检查（fuses、helper 的 identifier）要等 ADR-03 定下来再做。

## 物料

- 代码放在 `spikes/SP-4b-local-security/`，带 README，可以重复运行。
- 提交的内容里不能有个人信息：本机路径、用户名、签名身份名称、Team ID 一律用占位符。

## 验收标准

- [ ] 第 1 至 6 项各自的实测结论，附运行命令和输出摘要。
- [ ] 列出审计令牌校验的边界情况：会不会出现 EPERM、什么条件下出现、怎么处理。
- [ ] 对 ADR-05 的结论：方案 1（Unix 域套接字加对端签名校验）能否成立；如果不能，切换到 XPC 的理由是否充分。
- [ ] 对 ADR-01 升级流程的结论和修改建议。
- [ ] 一份要写进文档的"安全边界说明"草稿：SEC-8 能做到什么、做不到什么。

## 汇报要求

按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。代码和其他物料提交在任务分支上。
