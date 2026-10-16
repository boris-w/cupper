# SP-2 本机 CLI 与适配器的版本耦合、分发

- 派发日期：2026-10-16
- 执行者：子 agent（构建与发布工程师）
- 分支：`spike/SP-2-adapter-distribution`
- 依赖：无

## 背景

PRD 的 AG-1 要求 agent 程序使用用户本机已登录的官方 CLI，适配器由 cupper 锁定版本，但不随 App 再分发受 Anthropic 商业条款约束的组件。NFR-5 要求版本可控。

对齐会提出的问题（`architect.md` 3.1、3.2，以及 5.2 中的 SP-2）：
- 适配器默认用自带的 agent 程序，和"使用本机 CLI"冲突。
- claude-agent-acp 需要 Node 22 以上，Node 从哪里来还没定。

技术选型讨论会又补充了几点：
- ADR-01 规定 Node 和适配器按锁定的版本装在包外的 runtime 目录；
- 适配器最好打包成单文件，启动时就载入全部代码，这样升级时不怕原来的文件被替换（`desktop-engineer-round2.md` C6）；
- 不能用 Electron 自带的 Node 来跑适配器。

本机现状：Node v24.13.0，Claude Code 2.1.296，codex-cli 0.159.2。这个 codex 比 codex-acp 锁定的版本（R02 记录的是 0.160.1）还旧，正好可以用来测版本不一致的情况。没有安装 bun。

本任务是 ADR-08（agent 程序来源、适配器分发与版本锁定）和 ADR-11（开源许可证与第三方依赖的再分发）的依据。用户已许可使用本机已登录的 CLI 运行少量真实会话。

## 范围

- 做：
  1. **claude-agent-acp 配合本机 claude**：用 `npm install --omit=optional` 之类的方式，不安装 SDK 自带的原生 Claude Code 程序，再用 `CLAUDE_CODE_EXECUTABLE` 指向本机的 `claude`，看能不能正常工作。顺带列出 SDK 对 CLI 版本有没有要求。
  2. **codex-acp 配合本机 codex**：用 `CODEX_PATH` 分别指向本机的 0.159.2（比锁定版本旧）和一个比锁定版本新的 codex。新版本可以按官方渠道安装到临时目录，不要全局安装。观察会不会出现实验 API 不匹配，以及出错时是什么样子。
  3. **Node 运行时从哪里来**，比较下面几个方案：
     - 随 App 打包官方 Node，再用我们自己的身份重新签名，去掉 `get-task-allow` 这类 entitlement；
     - Node 单文件可执行（SEA）；
     - 用 `bun build --compile` 把适配器编成单个可执行文件。bun 可以从官方 GitHub release 下载到临时目录，核对校验和后使用，不要全局安装；
     - 使用系统已装的 Node。

     每个方案都要比较：能不能用、体积、启动耗时、能否签名和公证、hardened runtime 需要哪些 entitlement、能不能和 Electron 的 Node 完全分开。
  4. **适配器能否打包成单文件**：用 esbuild 或 bun 打包，启动时一次载入全部代码，并验证打包后的适配器功能正常。
  5. **首次运行时安装**：设计并试跑"按锁定版本安装、做完整性校验"的流程，例如带 lockfile 的 `npm ci` 加 integrity 校验，或者下载预先打好的单文件再核对哈希。目标目录用临时路径，模拟 `runtime/<名称>/<版本>/`。
  6. **许可证清点**：列出两个适配器及其完整依赖树的许可证，标出哪些不能随 App 再分发。重点看 `@anthropic-ai/claude-agent-sdk` 和它带的原生程序。codex-acp 在 GitHub 上显示的许可证是 "Other"，要核对 LICENSE 原文。另外说明 `bun --compile` 或 SEA 把依赖打进一个二进制，算不算再分发。
- 不做：不写产品代码，不做最终决定。

## 安全约束（必须遵守）

- 所有安装都放在 worktree 之外的临时目录里。不要用 `npm -g` 或 `brew` 做全局安装，不要修改全局的 npm 配置、`~/.claude` 或 `~/.codex`。
- 真实会话只用来证明"能用"，每种配置一两个简短无害的回合就够了。工作目录放在临时测试仓库里；只用默认的、或者更严格的权限模式；不要选择"总是允许"；能选模型时选最便宜的。
- 不要登出用户的账号；不要删除用户的任何数据；在报告里列出这次实测在本机留下的会话记录和临时目录。
- 遵守 `CLAUDE.md` 子 agent 守则第 7 条：外发请求里不能出现任何个人信息。

## 物料

- 脚本放在 `spikes/SP-2-adapter-distribution/`，带 README。
- 原始数据（体积、耗时、许可证清单）放在本任务目录下。
- 提交的内容必须去掉个人信息：本机路径一律写成 `$HOME`、`<tmp>`；不写用户名、签名身份的名称和 Team ID。

## 验收标准

- [ ] 第 1、2 项的实测结论，包括版本不一致时出错的形态。
- [ ] Node 来源对比表，加上推荐方案。
- [ ] 适配器单文件打包的结论。
- [ ] 首次运行安装流程的草案，包括校验方式和失败时怎么处理。
- [ ] 许可证清单，加上对 ADR-11 的建议。
- [ ] 对 ADR-08 的建议。

## 汇报要求

按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。其他物料提交在任务分支上。
