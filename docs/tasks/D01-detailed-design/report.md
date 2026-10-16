# D01 汇报

- 执行者：子 agent（架构师）
- 分支：`docs/D01-detailed-design`
- 完成日期：2026-10-16

## 做了什么

我起草了 `docs/02-design.md` 第 5 节"详细设计"，按 brief 的 13 项拆成 `docs/design/` 下的 13 个文件，由第 5 节统一索引。同时更新了第 1 节，补上最终组件图（进程与部署视图、宿主模块视图）和一张全局约束表，文件头版本改为 v0.6。另外在任务目录提交了工作笔记 `docs/tasks/D01-detailed-design/notes.md`，记录外部事实的核对来源和主要取舍。全程只写文档，没有写产品代码，没有改 ADR、PRD、tech-stack 或 main。

**文档结构和各部分要点**

| 小节 | 文件 | 要点 |
|---|---|---|
| 5.1 仓库与工程 | `docs/design/01-repo-build-ci.md` | Rust 和 pnpm 两套工作区放在同一个仓库，两边只通过 `protocol/schema` 和 `runtime/manifest.json` 耦合。新增 `cupper-sys` crate，集中全部 `unsafe`；新增 `cupper-desktop-native`（napi-rs）和 `xtask`。列了构建命令，以及 CI 的各个任务：schema 和生成代码一致性、方向敏感的兼容比对、金样例、许可证门禁、iOS 编译检查、每晚在 macOS 14 上跑校验测试 |
| 5.2 宿主与 cupper-cli | `docs/design/02-host.md` | 宿主 15 个模块的职责、接口、依赖和数据结构。并发模型写清了线程、actor、channel、锁和规则：每条流只有一个写者；追加分三步；会话 actor 不等待耗时操作。还有回合状态机、排队、Steer、10 s 中断兜底、崩溃恢复；命令"受理"语义，结果和事件在同一事务里落盘；Git、文件、运行时安装器、进程监护；CLI 命令表；SP-4a 代码逐个文件的去留表 |
| 5.3 协议 v1 | `docs/design/03-protocol-v1.md` | `host/hello` 加 nonce 的握手；control/window 两种角色；约 55 个方法，逐个写出参数、结果、错误、是否幂等、scope、里程碑；索引流和会话流的事件目录，以及条目和 `ToolCall` 的字段；交互请求模型；**SP-1 的选项 ID → 生效范围对照表**；会话能力描述；错误码的 reason 清单；限额 |
| 5.4 数据模型 | `docs/design/04-data-model.md` | `host.db`、`raw.db` 的建表语句和索引；各类数据存放在哪里；ID 规则（principal 用稳定 ID）；每张表的保留和回收；先备份再迁移，加事件升级器 |
| 5.5 关键流程 | `docs/design/05-flows.md` | 10 张时序图：启动（带时间预算）、新建会话（含 worktree）、发送/排队/插话、审批先到先得、中断、宿主崩溃、适配器崩溃、升级、审查到提交、归档清理 |
| 5.6 驱动层 | `docs/design/06-agent-driver.md` | 每种 agent 一个 `AgentRuntime`；驱动层的命令和事件接口；Claude 和 Codex 提取层对照（退出码、diff、运行状态、错误、计划等）；配置项映射，带风险等级；CLI 的发现和版本检查；能力缓存和探测；资源回收；**26 条已知问题规避清单**；在 stdio 和 SDK 之间加一层线路分接，负责记录原始帧、给封闭枚举做容错、处理扩展消息 |
| 5.7 桌面端 | `docs/design/07-desktop.md` | 连接改为一条 control 连接加每个窗口一条数据连接；主进程模块；ShellAPI 接口；参照截图笔记的布局、组件树、状态管理、快捷键、i18n 和主题；**安全加固落点表**（fuses、CSP、白名单、原生确认、真实输入、导航与权限）；三种构建 |
| 5.8 安全 | `docs/design/08-security.md` | 固定令牌握手和签名要求的写法；退化模式及其局限；principal、角色和 scope；**命令 × principal 权限矩阵**；CLI 提权流程（血缘检查、限频）；进程卫生；审计日志；安全边界说明 |
| 5.9 测试 | `docs/design/09-testing.md` | 测试分层；假 agent 的脚本模式（新增 12 条命令）和录制回放模式；录制与金样例流水线；NFR-1 每条验收项对应的端到端用例；兼容矩阵（新增"同一适配器多会话并发"一项）；性能预算；**SP-6 测试计划** |
| 5.10 构建发布 | `docs/design/10-release.md` | 包结构；每个 Mach-O 的签名身份和 entitlements；清单在编译期内嵌进宿主；**electron-builder 的决定**；更新流程；Electron 版本；发布流水线 |
| 5.11 诊断 | `docs/design/11-diagnostics.md` | 日志与轮转；适配器 stderr 改用管道收集；脱敏规则；诊断包（P1）；崩溃信息 |
| 5.12 里程碑 | `docs/design/12-milestones.md` | M1a/M1b 的范围和退出标准；**覆盖 M1 全部 P0 的追溯矩阵** |
| 5.13 待定问题 | `docs/design/13-open-issues.md` | 25 个待定问题、对 ADR-01 至 ADR-11 的修订建议、对 PRD 验收口径的影响 |

**electron-builder 还是 Forge：选 electron-builder（26.x）+ electron-updater（6.8.x）**

- 决定性的理由是"安装时机由我们控制"（ADR-01）。我核对了 electron-updater 6.8.10 的 `MacUpdater` 源码：`autoInstallOnAppQuit = false` 时，下载好的更新一直不会交给 Squirrel.Mac，直到调用 `quitAndInstall()`。这正好实现"宿主先退出、再安装"。7.x 把这个开关改名为 `autoInstallEvent: "manual"`，语义不变。
- Forge 默认用 Electron 内置的 autoUpdater，文档写明"下载好的更新总会在下次启动时应用"，无法暂缓。要控制时机只能三选一：等宿主退出后才下载；自己伪造 electron-updater 的元数据，属于补丁代码；像 Codex 那样接 Sparkle，要写原生扩展。Codex 用了 Forge 却配 Sparkle，恰好说明这一点。
- 其他方面两者相当：
  - fuses：electron-builder 26.x 的 schema 里有 `electronFuses`，已核对。
  - 签名与公证：都支持。
  - 嵌套 helper：electron-builder 用 `extraFiles` 加 `signIgnore` 处理；Forge 的 `optionsForFile` 更顺手。
  - Vite：Forge 有官方插件，但这只是一次性的小成本。
  - 成熟度：Forge 8 在 9 月 29 日才正式发布。
- 产品先例两边都有大产品（R04、R05）。选 electron-builder 也和 ADR-01、ADR-03 现有的写法一致，不需要改 ADR。
- 首次正式发布前要做一次升级演练，确认 ShipIt 是"移入新包、移走旧包"，不会原地覆盖（SP-4b 的 S4）。
- Electron 起始版本定为 44.8.0；macOS 26 的卡顿问题已在 2025-09 修复，42 及以后的版本都已包含。

**M1a / M1b 的划分**

- **M1a 宿主骨架**：完整的协议 v1 类型；宿主的全部核心模块，但驱动层只接 Claude；完整的 cupper-cli；假 agent 的两种模式和金样例；最简桌面端，包括主进程安全链路、侧边栏、消息流、审批和提问卡片、发送/排队/中断。退出标准是 NFR-1 的验收项全部自动化通过。
- **M1b 桌面 MVP**：开头先做 SP-6；然后是 Codex、Git 与 worktree、审查面板、`@` 引用和图片、斜杠命令、模型/模式选择、Steer、通知、agent 状态页、界面打磨、UI-0、签名公证和更新、许可证门禁，防休眠顺带做。退出标准是作者用两种 agent 走完 S1–S4，并且 NFR-2、NFR-9 通过。

**SP-4a 代码怎么用**：协议 crate 基本沿用；连接、发送队列、命令账本、崩溃恢复、系统调用沿用；存储、会话编排、驱动层需要重构。逐个文件的去留和要改的地方见 5.2.15。

## 关键取舍

1. **桌面连接模型**：ADR-03 写的是"主进程一条连接，所有窗口共用"。但主进程不解析入方向的 JSON，就既分发不了多窗口的事件，也认不出自己特权请求的响应。所以改为主进程持有全部连接：一条 control 连接加每个窗口一条数据连接。安全意图不变（Q1）。
2. **命令幂等**：SP-4a 是命令执行完再单独存结果，宿主在两者之间崩溃会导致重复执行。改为结果和事件同一事务落盘；改动用户仓库的 git 命令先写一条 `pending_external`；principal 改用稳定 ID，这样 UI 重连后重试仍能命中。
3. **命令顺序**：同一连接发往同一会话的命令，由读循环按顺序直接投进会话 actor 的邮箱，不再每个请求起一个 task（SP-4a 下 send 和 interrupt 可能乱序）。
4. **历史**：每个回合结束时写回合快照，会话视图只保留最近 20 个回合，`history/page` 从回合快照读取。这样"日志有保留窗口"和"历史以宿主记录为准"两条 ADR-06 的要求能同时成立。另外新增 `turn_no` 作为排序键。
5. **Codex threadStatus**：SP-1 s05 里，计划确认请求发出时线程状态已经是 idle。所以有 prompt 在途时以 prompt 响应为准，threadStatus 只用于显示状态和界定自发回合（Q11）。
6. **选项映射**：从录制里抽出全部 optionId 后发现，Claude 的 `allow-with-updates` 在 Write 上只对本会话有效，在 Bash 上会写入持久规则，两者 ID 相同。只能按工具区分，判断不了就当作持久处理（Q7）。
7. **线路分接层**：原始帧记录、SDK 封闭枚举的容错、扩展消息处理，都放在这一层里做，不去分叉 SDK。

## 验证方式

- mermaid 语法：在会话临时目录（不在仓库里）装了 mermaid 11.4.1 和 jsdom，解析 `docs/02-design.md` 和 `docs/design/*.md` 里的全部 20 张图，全部通过；用一张故意写错的图做反向验证，能报错。
- 链接：自写脚本检查 98 个跨文档链接和 GitHub 风格的锚点，0 个失效。
- 个人信息：对新文档、笔记和提交信息扫描用户名、邮箱、`/Users/`、临时目录、claude.ai 链接，没有命中。外发请求只有 npm registry、GitHub raw 文件、GitHub 搜索 API、Electron releases JSON，User-Agent 都是 `cupper-research`，不带任何认证。
- 提交：15 个提交末尾都有 `Refs: D01` 和 `Co-Authored-By`。

对照验收标准：

- [x] 第 1 节有最终组件图（两张图加约束表）。
- [x] 第 5 节覆盖 brief 的 1 到 13 项，每项一个文件。
- [x] M1 的全部 P0 需求都在追溯矩阵里找得到对应组件，覆盖 AG、PS、CP、MS、AP、GT、UI、SEC-6/8/10、NFR-1/2/4–12 和第 8 节合规。
- [x] 协议 v1 草案覆盖 M1 用到的全部方法和事件，逐个标了里程碑。
- [x] 有 SQLite 表结构草案。
- [x] 每个关键流程都有时序图，brief 要求的 8 个之外，崩溃恢复拆成两张，另加归档清理和 CLI 提权。
- [x] 有待定问题清单（25 项）。

## 提交列表

```
d88f03e docs(design): fix consistency issues found on review
9b2420a docs(design): add final component diagrams and index section 5
8ffed82 docs(design): add open questions and ADR revision proposals
4db0c20 docs(design): add M1a/M1b scope split and P0 traceability matrix
14b5e84 docs(task): add D01 working notes with source checks and trade-offs
4000534 docs(design): choose electron-builder and add release and diagnostics design
71e70f9 docs(design): add test strategy and SP-6 test plan
dad89bd docs(design): add security implementation and permission matrix
cbcbc7a docs(design): add desktop process, layout and hardening design
0dc5942 docs(design): add key flow sequence diagrams
c03836e docs(design): add agent driver layer design
09d5046 docs(design): add SQLite data model, ID rules, retention and migrations
6845e7f docs(design): add protocol v1 contract draft
6d596e7 docs(design): add host component design and SP-4a reuse plan
01b1f0a docs(design): add repository layout, build commands and CI pipeline
```

## 遗留问题 / 范围外发现

**待定问题清单**（详见 5.13.1，每项都附了建议和定夺时机）

| # | 问题 | 建议 |
|---|---|---|
| Q1 | ADR-03 的连接模型做不到 | 改为 control 连接加每窗口一条数据连接 |
| Q2 | 宿主 helper 弹通知要等 SP-10（M2 前）验证，但 AP-3 是 M1b 的 P0 | 把 SP-10 的通知部分提前；不行就让 UI 弹 |
| Q3 | 只带 allow-jit 的重签 Node 能否公证、首次执行多慢 | 等有 Developer ID 后实测；后备是改放进 `Helpers/` |
| Q4 | codex-acp 随包还是下载，ADR-08 没写明 | 随包 |
| Q5 | bundle id 前缀 | **gate/design 时由用户确定** |
| Q6 | 只出 arm64 | **用户确认** |
| Q7 | Claude `allow-with-updates` 同一个 ID 两种语义 | 保守映射，并向上游反馈 |
| Q8 | 同一适配器多会话并发 prompt 没有实测（ADR-09 的前提） | 列入兼容剧本，备好后备方案 |
| Q9 | SDK 3.3 是否支持扩展方法 | M1a 先做技术验证，不支持就由分接层处理 |
| Q10 | Claude 自发回合何时结束 | 用启发式判定，靠录制验证 |
| Q11 | threadStatus 和回合边界 | 见关键取舍第 5 条 |
| Q12 | Claude 在 SDK 会话里拿不到 TodoWrite 的计划，haiku 上也没有思考流 | 补测；不行就调整 MS-4 的验收口径，**需要用户确认** |
| Q13 | 取消暂存需要 staged 范围，但它在 PRD 里是 P1 | M1 提供最小的 staged 范围，**需要用户确认** |
| Q14 | 晚到的输出 | 丢弃并在原条目上标记 |
| Q15 | SQLite 用 NORMAL 还是 FULL | 做断电测试再定 |
| Q16 | 保留参数 | 先按草案实现，再按实际数据调整 |
| Q17 | 快照对象回收 | M2 |
| Q18 | 从登录 shell 取环境可能慢、有副作用 | 缓存结果，设超时 |
| Q19 | 各种安装方式下 CLI 的目录布局 | 实现时逐个核实 |
| Q20 | electron-updater 7 迁移；ShipIt 的替换方式；安装时 UI 进程临时监听 127.0.0.1 | 升级演练，写进安全说明 |
| Q21 | 宿主由 UI 拉起时 TCC 归属可能算在 UI 上 | SP-10 |
| Q22 | 退化认证模式下 UI 重启连不上旧宿主，开发构建和自行构建里 NFR-8 不成立 | 接受，**需要用户确认** |
| Q23 | 宿主环境下 git 的 SSH、GPG 凭据 | 实现时逐项验证 |
| Q24 | macOS 14、15 上的对端校验语义没测过 | CI 补上 |
| Q25 | Codex 的 revise_plan 和权限模式取值没实测 | 补测；在此之前未知值一律按完全访问处理 |

**对 ADR 的修订建议**（5.13.2；全部是细化，没有推翻任何一项）

- **ADR-01**：
  - 运行时载荷放进 `CupperHost.app/Contents/Resources/runtime/`。
  - "运行中不读包内资源"只有一个例外：自检通过后，按内嵌清单逐文件核对哈希地复制。
  - Electron 的 userData 设到 `cupper/ui/`：产品名 Cupper 和数据目录 cupper 在大小写不敏感的卷上是同一个目录。
  - 补退出码约定，以及 Q2、Q21。
- **ADR-02**：crate 表补上 `cupper-sys`、`cupper-desktop-native`、`xtask`；`Method` trait 声明 `SCOPE` 和 `IDEMPOTENCY`。
- **ADR-03**：连接模型（Q1）；打包工具定为 electron-builder；Electron 44.8.0 及跟进策略；宿主侧加 400 ms 最短回答间隔，作为渲染进程 500 ms 输入保护的后盾；e2e 构建的宿主同时开 `dev-auth` 和 `test-harness`。
- **ADR-04**：
  - 新增 `host/hello`，`initialize` 加 nonce 和 role。
  - 命令是"受理"语义，结果和事件同事务落盘；账本按稳定 ID 记。
  - `session/create` 必须带第一条消息；`history/page` 从回合快照读取；`blob/put` 先发分片再发请求。
  - hunk 的前置条件具体化为 `diff_token`；新增 `git_error` 错误码和 reason 清单。
- **ADR-05**：principal 加角色，新增 `privileged`、`ui_local` 两个 scope，给出权限矩阵；血缘检查的信号具体化；UI 之后每次读取都比对宿主令牌；写明退化模式的局限。
- **ADR-06**：
  - 新增回合快照，会话视图只保留 20 个回合的窗口。
  - blob 改为文件；原始帧放进单独的 `raw.db`。
  - toolCallId 只作 upsert 键；按 Q11 限定 threadStatus 的使用范围。
  - 索引流不再包含已归档会话；交互请求记录所属的适配器实例。
- **ADR-07**：晚到的输出"丢弃并标记"；加入线路分接层；上一个 prompt 不返回时，先 close 再 resume 才发新的。
- **ADR-08**：codex-acp 随包；清单内嵌进宿主二进制；已验证的 CLI 范围写进清单；Node 公证的后备方案。
- **ADR-09**：并发 prompt 的实测和后备方案；回收阈值（2 GiB 或空闲 30 分钟）；崩溃后的重启退避。
- **ADR-10**：`diff_token`；最小 staged 范围；仓库锁的范围；git 的环境变量约定。
- **ADR-11**：门禁工具具体化；CI 计算 claude 打包哈希时用隔离目录；对最终产物再查一遍。

**范围外发现**（没有改动，留给编排者处理）

1. `docs/tech-stack.md` 有两处过时：
   - "打包与更新"一行还写着"electron-builder 还是 Forge 在详细设计时定"；
   - "agent 接入"一行还写着"分发方式待 SP-2"和"ADR-08（待定）"，但 ADR-08 已经提议。
2. `docs/02-design.md` 第 3 节 SP-6 那一行可以补上"测试计划见 5.9.7"。我没有改第 3 节，因为它不在 brief 范围内。
3. 5.1.7 列出了 gate/design 之后要写进 `CLAUDE.md` 的代码约束，例如每个方法必须声明 scope、单写者、ui-core 不准 import electron，供编排者届时合并。
4. 交 gate/design 时需要用户拍板的有：Q5 bundle id 前缀、Q6 只出 arm64、Q12 和 Q13 的验收口径、Q22 退化模式的局限，以及 5.13.2 中的全部 ADR 修订。