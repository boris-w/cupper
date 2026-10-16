# cupper 开发任务拆分（M1a、M1b）

- 版本：v0.1
- 日期：2026-10-16
- 状态：草案，待计划评审（通过后打 tag `gate/plan`）
- 依据：技术设计 v1.1（[`02-design.md`](02-design.md) 与 [`design/`](design/12-milestones.md)，v1.0 打 tag `gate/design`；v1.1 推迟 Developer ID 发布、新增 Q35）；PRD v1.2（[`01-prd.md`](01-prd.md)）；ADR-01 至 ADR-11；设计评审纪要 [`meetings/2026-10-16-design-review/minutes.md`](meetings/2026-10-16-design-review/minutes.md)；用户决定 [`meetings/2026-10-16-gate-design-decisions/minutes.md`](meetings/2026-10-16-gate-design-decisions/minutes.md)（含"补充：Apple Developer 账号"）
- 用途：编排者照着本文件逐个写 brief、派发任务（[`00-process.md`](00-process.md) 的"任务生命周期"）；任务完成后在第 9 节打勾

## 0. 结论

- **共 128 个任务**：M1a 74 个（含 2 个条件任务），M1b 49 个（含 5 个条件任务），"发布（推迟）"5 个。规模：M1a 非条件任务 S 5、M 43、L 24；M1b 非条件任务 S 8、M 25、L 11；发布组 S 1、M 1、L 3。L 的都在表后注明了为什么不再拆。
- **M1a 关键路径**（按规模加权，S=1、M=2、L=3，共 27）：T001 → T010 → T011 → T014 → T026 → T033 → T043 → T044 → T064 → T066 → T099。另有两条几乎等长的链：桌面链 T015 → T071 → T075 → T076 → T077 → T090 → T091，网关链 T030 → T033 → T034 → T038 → T046 → T090。任务表里加粗的编号都在零松弛的链上。
- **M1b 关键路径**（从 T099 起，共 15）：T099 → T120 → T121 → T130 → T131 → T171 → T173 → T179。审查面板一线（git 写操作 → 审查面板 → 审查文档 → S3/S4 端到端）最长；SP-6 → 三栏布局 → 审查面板是并列的一条。
- **v1.1 的变化已经落实**：Developer ID 签名、公证、签名后冒烟、RC、electron-updater 与升级演练、发布流水线、许可证门禁第二层、第三方声明都放进"发布（推迟）"组（T180–T184），前提是用户开通账号，不计入 M1b 的退出标准，也不在 M1b 关键路径上；M1b 新增"自用构建"（T160、T161、T006 继续每个 PR 运行）和 Q35 的技术验证（T105，通过则条件任务 T162）。
- **检查点**：技术验证 T003（Q8、Q9）→ 驱动层；测量闸门 T078 → 契约冻结 T016；M1a 退出 T099；SP-10（T101）、Q8 Codex（T102）、Q29（T104）、Q35（T105）可以在 M1a 期间提前做；SP-6（T103）在 M1b 开头。每个检查点不通过时启用哪个条件任务、影响哪些任务，见 4.4 和 7.3。
- **现在就需要用户定的事**（第 6 节第 1 行）：GitHub 仓库怎样建（U-P1，CI 和 M1a 退出标准都依赖它）；契约冻结的时机（P-1）；真实 Codex 运行改写 `~/.codex/config.toml` 时怎样处理（P-14）。
- **设计里发现的遗漏和矛盾**共 15 条，见第 8 节"待处理"，其中 P-1、P-5、P-6 会影响任务的写法，需要在相关任务派发前定下来。

## 1. 拆分原则

### 1.1 粒度与规模

- 一个任务交给一个子 agent 一次完成，在独立 worktree 的独立分支上开发、测试、提交（[`CLAUDE.md`](../CLAUDE.md) 的子 agent 守则）。
- 规模按改动行数估计（含测试，不含生成物、录制、金样例和截图基线）：

  | 规模 | 行数 | 说明 |
  |---|---|---|
  | S | ≤ 400 | 半天到一天 |
  | M | 400–900 | 一到两天 |
  | L | 900–1,500 | 两到三天；表后的注写明为什么不能再拆 |

- 分支名按 [`CLAUDE.md`](../CLAUDE.md) 的约定：`<类型>/<编号>-<英文短名>`，例如 `feat/T043-session-actor`；技术验证类任务用 `spike/<编号>-<短名>`，代码放 `spikes/<编号>-<短名>/`，不进产品代码。
- 上限约 1,500 行。子 agent 开工后发现会明显超出，先停下来汇报，由编排者按注里的拆分点拆成两个任务，不硬塞进一个分支。

### 1.2 每次合并后 main 都可用

- 每个任务合并后，`pnpm lint`、`pnpm test`（以及已存在的 `test:golden`、`test:ui`、`test:e2e` 冒烟子集）全绿。
- **未实现的方法统一返回 `unsupported`**：宿主按方法注册执行者，没有执行者的方法返回 `unsupported`。M2、M3 的方法（`session/queue_edit`、`diag/export`、`stream/ack`）在 M1 结束时仍是这样，所以这是正式行为，不是桩。按依赖顺序合并时不需要任何临时代码。
- **接口由使用方先定义，实现方后接上**：例如 T040 定义 `driver::api`，T043 用进程内替身测试，T055 实现；T043 的快照端口在 T061 合并前为空，回合快照记为 `skipped`（与非 git 项目相同）。替身只出现在测试代码里。
- 不留技术债（[`CLAUDE.md`](../CLAUDE.md) 的工程原则）：条件任务只在触发时才派发，触发前不写后备代码（见 P-7）。

### 1.3 契约先行

- 协议的全部类型（含 M1b 才用的方法和事件）在 T010–T015 完成，宿主、CLI、host-client、界面都在它之后；`driver::api` 在 T040；ShellAPI 在 T070；帧级 TS mock 宿主在 T071。
- 冻结（T016）之前修改契约：单独提交，在同一任务里重新生成 `packages/protocol`，在汇报里列出改了什么；冻结之后只能附加（[5.12.5](design/12-milestones.md#5125-契约冻结清单)），CI 的 `protocol` 作业把关。冻结的时机见 P-1。

### 1.4 测试基础设施先行

| 基础设施 | 任务 | 之后的使用者 |
|---|---|---|
| 用例编号追踪与 `pending-ids.toml` | T002 | 每个任务在实现用例的同一提交里删除对应的登记 |
| 时钟、纯核心框架、故障点、`test_config` | T022、T023 | 全部宿主任务 |
| 假 agent、录制格式与回放 | T051、T052 | 驱动、集成测试、金样例、测量闸门 |
| test-harness 测试接口（`clock_advance`、`seed`、录制导出） | T047 | 集成测试、闸门、性能、兼容套件 |
| 帧级 TS mock 宿主、浏览器版 ShellAPI | T071、T070 | windowBridge 单测、截图回归 |
| 截图回归基础、`ui` 作业 | T080 | 全部界面任务 |
| e2e 夹具、`e2e` 作业 | T090 | 全部端到端任务 |
| 兼容套件框架 | T064 | 真实录制（T066、T113） |

每个功能任务自带它的单元测试和用例；"集成测试""e2e"任务只承担跨模块、跨进程的用例。

### 1.5 并行与区域

区域借用 CODEOWNERS 的思路，判断哪些任务可以同时进行而不冲突：

| 区域 | 代号 | 路径 |
|---|---|---|
| 工程与 CI | C | 根目录配置、`.github/`、`tools/trace-ids`、`tools/license-gate`、`tools/codex-drift`、`tests/` |
| 协议 | P | `crates/cupper-protocol`、`protocol/`、`tools/gen-protocol-ts`、`tools/schema-compat`、`packages/protocol` |
| Rust 宿主 | H.core、H.store、H.gw、H.idx、H.orch、H.drv、H.git、H.rt、H.diag | `crates/cupper-host/src/` 下对应模块（core 含 clock、supervisor、test 接口；idx 含 attention、trust、settings） |
| 系统接口 | H.sys | `crates/cupper-sys` |
| CLI | H.cli | `crates/cupper-cli` |
| 假 agent 与测试数据 | H.fake | `crates/cupper-fake-agent`、`testdata/`、`tools/sanitize`、`tools/rec-shape-diff` |
| xtask 与运行时 | H.xt | `crates/xtask`、`runtime/` |
| TS 库 | T.lib | `packages/shell-api`、`packages/i18n`、`packages/host-client` |
| 界面 | T.ui | `packages/ui-core` |
| 原生扩展 | D.nat | `crates/cupper-desktop-native` |
| 桌面主进程 | D.main | `apps/desktop` |
| 文档、原型实测 | Doc、SP | `docs/user/`；`spikes/` |

规则：

- 不同区域的任务可以并行；同一区域的任务默认串行，除非 brief 写明各自只碰哪些文件（例如 T012 与 T013 按文件分开）。
- 共享的热点和避免冲突的约定：
  - 工作区成员用通配（`crates/*`、`packages/*`、`apps/*`、`tools/*`），新 crate 和 package 不改根清单；`[workspace.dependencies]` 的冲突由编排者合并时处理。
  - `crates/cupper-host/src/lib.rs` 只放 `mod` 声明，各模块的接线放在模块内部。
  - 一个 CI 作业一个 workflow 文件。
  - i18n 文案按功能分文件（`locales/<语言>/<功能>.json`）。
  - `tests/pending-ids.toml` 一行一个编号、按编号排序。
  - 表结构在首个自用构建交付之前只改 `0001`（P-4）。
- 实际并行度：作者机器上同时运行的子 agent 建议不超过 3–4 个（Rust 编译占 CPU）；CI 上标准 macOS runner 最多同时 5 个作业，每个 PR 用到 3 个（[5.1.6](design/01-repo-build-ci.md#516-ci-流水线)），所以同一时间在 CI 上验证的分支不宜超过两个。

### 1.6 用例编号

- 验收标准引用 [5.9.10](design/09-testing.md#5910-测试编号与人工验证清单) 的编号；本计划新增的编号写"新增"，命名同样是 `<层>-<范围>-<名称>`。截图回归用 `UI-` 前缀（5.9.10 的层里没有它，见 P-3）。
- 每个编号要么有同名的测试或清单项，要么登记在 `tests/pending-ids.toml` 里并写明里程碑：M1a、M1b、发布（推迟）、M2。里程碑退出时它名下的登记必须清空（T099、T179 核对）。

### 1.7 条件任务与可以提前的任务

- **条件任务**（名称前标"（条件）"）只在检查点不通过时派发：T057、T079、T119、T139、T158、T159、T162（T162 是 Q35 *通过*时派发）。依赖图里用虚线表示。
- **可以提前的任务**（前置一栏标"可提前"）：T101、T102、T104、T105、T110、T114。依赖满足后可以在 M1a 期间派发，但评审资源优先给 M1a 的关键链。其余 M1b 任务在 T099 之后派发。

### 1.8 合并与 CI

- 子 agent 不 push（[`CLAUDE.md`](../CLAUDE.md)）。编排者把任务分支推到 GitHub 触发 CI，全绿后本地 `git merge --no-ff`，再推 main。
- 仓库在 GitHub 上建好之前（U-P1），任务的 CI 验收用本机的等价命令代替，输出存进任务目录；`ui` 的截图基线和 M1a 的"20 次全绿"只能在 CI 上做（P-2）。

### 1.9 "用户参与"一栏

- **是**：需要用户在场或操作（系统授权框、实体键盘、证书、亲自走流程），或会消耗用户的额度。
- **知情**：在作者机器上运行、会弹窗口或长时间占用机器，派发前告知时段。
- **可选**：用户愿意时可以手动补一项验证。

## 2. 任务清单

前置一栏只列直接依赖，可由其他依赖推出的间接依赖省略。加粗的编号在关键链上（松弛为零）。

### 2.1 M1a：宿主骨架

| 编号 | 名称 | 类型 | 设计依据 | 前置 | 做什么 / 不做什么 | 验收（用例编号） | 规模 | 用户参与 | 区域 |
|---|---|---|---|---|---|---|---|---|---|
| **T001** | 仓库与工程骨架 | build | 5.1.1–5.1.6 | — | 做：Rust 与 pnpm 两套工作区（成员用通配）、工具链锁定、clippy 的 `disallowed-methods` 与 `await_holding_lock`、`deny.toml`、lint 与格式配置、CI 通用加固和 `rust`、`ts`、`license`（暂只有 `cargo deny`）三个作业；迁移 SP-4a 的 `frame.rs` 建立 `cupper-protocol`。不做：其他 crate 和 TS 包 | 三个作业通过；`disallowed-methods` 的负面样例让 clippy 失败；新增 UNIT-proto-frame | M | 是：U-P1（GitHub 仓库） | C、P |
| T002 | 用例编号追踪 trace-ids | build | 5.9.10、5.1.6 | T001 | 做：`tools/trace-ids`（抽取设计文档里的编号，与 Rust 测试名、Playwright 标题、人工清单比对）、`tests/pending-ids.toml` 按里程碑登记全部编号、`testdata-scan` 作业首版。不做：PII 扫描、兼容报告校验 | `testdata-scan` 通过；新增 UNIT-trace-ids | S | 否 | C |
| T003 | 技术验证：Q8（Claude）、Q9、SP-4b #7 与版本复核 | spike | 5.12.2（技术验证）、5.13 Q8 与 Q9、5.9.5、5.8.4 | — | 做：用 SP-1 的 TS 客户端跑 Claude 并发剧本；用 SDK `=3.3.0` 写探针确认扩展消息的收发；观察真实适配器拉起命令时的进程树；复核锁定版本和测量闸门的负载数字。不做：产品代码 | 报告：Q8 逐轮数据（按 5.9.5 的通过条件）、Q9 逐项结论、SP-4b #7 结论、版本建议；配置文件前后哈希 | M | 是：真实 Claude CLI，消耗少量额度（U-R5 已许可） | SP |
| T004 | xtask runtime-pack 与运行时清单 | build | 5.1.5、5.10.3、5.6.6 | T001 | 做：`xtask` crate；`runtime-pack`（下载并校验 Node、codex-acp、esbuild-wasm，本机重签 Node，隔离目录里算 claude-agent-acp 打包产物哈希，提取并归类适配器读取的环境变量）与 `--verify`；`runtime/manifest.json`、`runtime/locks/`、打包脚本、许可证文本。不做：宿主侧安装器 | `runtime-pack --verify` 通过；新增 UNIT-runtime-pack-envscan（未归类的变量名让命令失败） | M | 否 | H.xt |
| T005 | xtask host-bundle | build | 5.10.1、5.10.2、5.7.9 | T004、T023、T045 | 做：`cargo xtask host-bundle` 按构建种类组装 `CupperHost.app`（Info.plist、`Helpers/node`、`Resources/runtime/`）并 ad-hoc 签名，单独签 CLI。不做：Developer ID 签名（T180） | 新增 IT-host-bundle-layout（布局、identifier、Info.plist 不含用途说明） | S | 否 | H.xt |
| T006 | 许可证门禁第一层 | ci | 5.1.6（license）、ADR-11 | T004、T074 | 做：`tools/license-gate` 第一层（按锁文件和 metafile 求每个 JS 入口的生产依赖闭包，出现 Anthropic SDK 或 Claude Code 就失败；运行时组件许可证齐全）与负面样例，接入 `license` 作业。不做：最终产物扫描（T182） | `license` 作业通过；新增 UNIT-license-gate-negative | M | 否 | C |
| **T010** | 协议基础：信封、ID、错误码、规范 JSON 与注解 | feat | 5.3.1、5.3.8、5.3.9、5.4.4、5.2.15 | T001 | 做：迁移 SP-4a 的 rpc（`Method` 加 `SCOPE`、`IDEMPOTENCY`）、open_enum、text、coalesce、sync、transport；事件信封与流 ID、ID 前缀类型、错误码与 reason、限额、RFC 8785 与 `d_` 摘要、六种 schema 注解的机制、整数上界。不做：具体的事件和方法 | UNIT-digest-jcs；新增 UNIT-proto-base | M | 否 | P |
| **T011** | 协议事件目录与领域类型 | feat | 5.3.5、5.3.6、5.3.7、5.12.5 第 2 组 | T010 | 做：两条流的全部事件（含 M1b 才产生的）、条目类型、`ToolCall` 与按 `tool_kind` 的必填字段表、`late_tool_output`、交互请求与 `Option.effect`、能力描述、`PromptContent`、`SessionSummary`、`AgentStatus`；开放枚举、`Unknown` 兜底、严格模式开关。不做：方法、视图 | 新增 UNIT-proto-events（每种事件往返，含未知类型、未知枚举值、残缺数据） | L | 否 | P |
| **T012** | 协议方法 I：握手、流、项目、agent、会话与交互 | feat | 5.3.2、5.3.3、5.3.4 | T011 | 做：`host/hello`、`initialize`（各 `auth` 变体、`role`、`features`、`limits`、`host_proof`）及生命周期、流、项目、agent、会话、交互各方法的参数与结果，逐个声明 scope 与幂等。不做：其余方法（T013） | 新增 UNIT-proto-methods-core（往返；方法级 scope 与 5.8.3 一致） | M | 否 | P |
| **T013** | 协议方法 II：git、文件、设置、认证、诊断与测试 | feat | 5.3.4 | T011 | 做：git、worktree、文件、`workspace/open`、设置、`auth/*`、`policy/describe`、`diag/*`、`test/*` 各方法，以及按构建特性门控的方法在 schema 里的标注。不做：任何实现 | 新增 UNIT-proto-methods-ext | M | 否 | P |
| **T014** | 协议视图与同步规则 | feat | 5.3.3、5.2.8（视图窗口）、5.7.6 | T011 | 做：`IndexView`、`SessionView`（20 回合或 2,000 条目的窗口、进行中回合、待处理交互、配置、能力、队列）、`apply` 与按 `turn_no` 排序、`late_by_ref`、快照 ops 的生成与切分、连续性与 epoch 重置规则、视图的序列化格式。不做：存储侧检查点、TS reducer | 新增 PROP-view-coalesce（合并后应用等于逐条应用）、UNIT-view-window、UNIT-view-late-by-ref | M | 否 | P |
| **T015** | schema 导出与 TS 生成链 | build | 5.3.10、5.1.5、5.1.6（protocol） | T012、T013、T014 | 做：`export-schema` 完整输出、`protocol/schema/v1.json`、`tools/gen-protocol-ts`（hey-api 与三处后处理）、`packages/protocol` 生成物、`protocol/fixtures/`（每种消息和事件，含未知值）、`protocol` 作业。不做：兼容比对与冻结（T016） | `protocol` 作业通过（生成物无差异、整数检查、fixtures 在 Rust 和 TS 各往返一次）；新增 UNIT-proto-fixtures、UNIT-ts-fixtures | M | 否 | P、T.lib、C |
| T016 | 契约冻结 | chore | 5.12.5 | T072、T077、T078 | 做：`tools/schema-compat`（方向敏感）、`protocol/released/v1-m1a.json`、`packages/shell-api/api-m1a.d.ts` 与类型断言，接入 `protocol` 作业。不做：契约内容的修改 | 破坏性改动的样例（删字段、下发方向必填改可选、上行新增必填、ShellAPI 删成员）让 CI 失败，附加式改动通过；新增 UNIT-schema-compat | M | 否 | P、T.lib、C |
| T020 | cupper-sys I：进程与系统信息 | feat | 5.1.3、5.8.5、5.2.13 | T001 | 做：迁移 SP-4a 的 `sys.rs`：`spawn`（SETSID 可选、`CLOEXEC_DEFAULT`、显式 stdio、默认信号处置、环境变量白名单）、进程组清理、libproc（启动时间、父进程链、会话与进程组）、`kern.bootsessionuuid`、`DARWIN_USER_TEMP_DIR`。不做：IOPM、UserNotifications | 新增 UNIT-sys-spawn（只继承 fd 0–2；去掉 CLOEXEC 的变异检查应失败）、UNIT-sys-reap-group | M | 否 | H.sys |
| T021 | cupper-sys II：对端校验与代码签名 | feat | 5.8.1、5.1.6（security-macos14）、Q24 | T001 | 做：合并 SP-4b 的 Security FFI：`LOCAL_PEERTOKEN`、`LOCAL_PEERPID`、SecCode 校验与动态状态字、编译期签名要求、csops、四类错误；`security-macos14` 工作流。不做：网关里的使用 | 新增 IT-sys-peer（最后一次 I/O 进程语义、exec 与 fork 被拒、`SCM_RIGHTS` 被丢弃、csops、错误码）；`security-macos14` 首次通过 | M | 否 | H.sys、C |
| T022 | 宿主核心骨架 I：时钟与纯核心框架 | feat | 5.2.16、5.2.3 | T001 | 做：`cupper-host` crate（lib、bin，特性 `dev-auth`、`test-harness`）、`Clock`、`SystemClock`、`ManualClock`、`IdGen`、纯核心 `step(input, now)` 的框架与测试驱动器。不做：启动契约（T023） | 新增 UNIT-clock-manual、UNIT-idgen | M | 否 | H.core |
| T023 | 宿主核心骨架 II：启动契约、注入点与故障点 | feat | 5.2.13、5.8.1（启动载荷）、5.9.9 | T022 | 做：`--data-dir`、`--build-info`；stdin 启动载荷（2 s 超时、编译期门控、读完指向 `/dev/null`、退出码 1）；`test_config` 全部注入点的结构；故障点框架；无特性运行的用例。不做：注入点的使用方 | 新增 UNIT-startup-payload、UNIT-failpoint、UNIT-release-features（5.9.1 的无特性运行） | M | 否 | H.core |
| T024 | 存储 I：写线程、读池与持久性 | feat | 5.2.8、5.9.8（写线程、数据代次） | T010、T022 | 做：写线程（组提交、每操作 savepoint、只在 `SystemClock` 下等 2 ms、确认回执）、4 个只读连接、两档持久性配置、`data_epoch` 判定纯函数与注入输入。不做：表结构（T025） | 新增 UNIT-writer-savepoint、UNIT-epoch-decision（IT-NFR2-epoch 的决策表） | M | 否 | H.store |
| T025 | 存储 II：表结构、迁移与升级器 | feat | 5.4.2、5.4.6 | T023、T024 | 做：`0001` 迁移（5.4.2 的全部表与 raw.db）、迁移框架（编进二进制、同事务更新版本、`VACUUM INTO` 备份、退出码 4 和 6、两脚本之间崩溃不重跑）、事件升级器框架、检查点的 `view_v`。不做：业务读写 | IT-migrations | M | 否 | H.store |
| **T026** | 存储 III：流、检查点、回合快照与历史 | feat | 5.2.3、5.2.8、5.3.3、5.4.5 | T014、T025 | 做：迁移 SP-4a 的 hub 为 `store::streams`（追加三步、订阅在流锁内读取、懒加载、淘汰）、扇出接口、回合结束同事务写检查点和不可变的回合快照、`history/page` 读取、events 裁剪。不做：订阅协议（T033） | 新增 PROP-stream-seq（shuttle）、PROP-stream-replay（快照加历史、检查点加事件、裁剪后重建都等于重放） | L | 否 | H.store |
| T027 | 存储 IV：blob、原始帧与保留清理 | feat | 5.2.8、5.4.5 | T025 | 做：按内容寻址的 BlobStore、原始帧有界队列写 raw.db（满了丢弃并计数）、各类保留清理（经 Clock）。不做：blob 传输（T033） | 新增 UNIT-blobstore、UNIT-raw-queue、UNIT-retention | M | 否 | H.store |
| T028 | 日志与脱敏 | feat | 5.11.1、5.11.2 | T022 | 做：tracing JSON Lines 与按大小轮转、两档脱敏、`info` 不记内容、适配器 stderr 管道读取（每实例保留 64 KB）、panic hook。不做：诊断包（M2） | UNIT-redaction、UNIT-redact-auth-status | M | 否 | H.diag |
| **T030** | 网关 I：连接、握手与本地认证 | feat | 5.2.4、5.8.1、5.3.2 | T012、T021、T025 | 做：迁移 SP-4a 的 conn、outbox、wire；uid、签名结论与固定令牌（每次读取比对）、`host/hello`、只接受一次的 `initialize`、四种认证与绑定信道的双向证明、principal 与角色、套接字路径与权限、`host.lock`、写循环与慢客户端重置、`ping`、连接审计。不做：方法分发 | 新增 IT-handshake（nonce 不符、二次 initialize、中继时证明失败、无特性构建拒绝 harness）、IT-pinned-token、UNIT-proof-binding | L | 否 | H.gw |
| T031 | 策略 | feat | 5.2.5、5.8.2、5.8.3 | T012、T013、T022 | 做：方法级由 `SCOPE` 驱动；资源级规则（档位、privileged 选项、安全设置键、window 角色的输入证据与按键、400 ms、信任门控的钩子）；`policy/describe`。不做：账本 | UNIT-tier-rules；新增 UNIT-policy-matrix（按 5.8.3 逐格） | M | 否 | H.gw |
| T032 | 命令账本 | feat | 5.2.5（账本） | T025 | 做：principal 稳定 ID 加 `command_id` 为键、三种状态、去掉 `command_id`/`attestation`/`input` 后的 JCS 参数哈希、48 小时内存表、`idempotency_mismatch`、`outcome_unknown`、结果与首批事件同事务的接口。不做：路由 | 新增 UNIT-ledger | M | 否 | H.gw |
| **T033** | 网关 II：分发、订阅、历史与 blob 传输 | feat | 5.2.4、5.3.3、5.3.4（流与内容） | T026、T027、T030、T031、T032、T045 | 做：读循环 → 策略 → 账本 → 路由（会话命令按序投递会话邮箱接口，索引命令投递索引接口）、每连接 64 个在途、`stream/subscribe` 四种情况与快照分页、`history/page`、blob 上传下载、断开清理、订阅模式写日志（Q16）。不做：会话和索引的业务 | IT-NFR2-epoch；新增 IT-gw-sync（迁移 SP-4a 端到端测试 1–7）、IT-command-order | L | 否 | H.gw |
| **T034** | 索引 actor 与关注度 | feat | 5.2.9、5.3.5（索引流）、5.5.10 | T033、T035、T060 | 做：`IndexActor`（索引流与 `projects` 表的唯一写者）、`project/add`、`project/trust`、`project/trust_info`、状态计算与 100 ms 合并、`session/mark_read`、只采信 desktop_ui 的在场、`notification.intent`、`clients`/`agents`/`host`。不做：系统通知、`project/remove` | UNIT-index-status、IT-notification-intent；新增 IT-project-add-trust | L | 否 | H.idx |
| T035 | 项目信任扫描 | feat | 5.2.6（项目信任）、5.9.8 | T011、T022 | 做：`trust` 模块：工作区到 git 根目录每一级的项目配置、符号链接一律跟随、2,000 个文件或 16 MB 上限、摘要、findings、工作区外标记、与已信任摘要的差异、状态转换。不做：门控（T044） | 新增 UNIT-trust-scan（链接的各种情形、上级目录、上限、目标改写后转 stale） | M | 否 | H.idx |
| T036 | 设置 | feat | 5.2.11（设置） | T033 | 做：`settings` 表、`settings/get`、`settings/update`（安全相关键要 privileged）、默认值与读取接口。不做：界面偏好 | 新增 UNIT-settings | S | 否 | H.idx |
| T037 | 进程监护与宿主生命周期 | feat | 5.2.13、5.5.6 | T020、T026、T027、T030 | 做：启动 10 步串联、`Supervisor`（按 `adapters` 表清理残留、空闲退出、有序退出、`host/status`、`host/exit`、`host/shutdown`、SIGTERM）、自检与待升级、退出码、主线程 `CFRunLoop`。不做：防休眠（T156）、准备阶段的会话恢复（T043） | 新增 IT-self-check、IT-lifecycle（空闲退出、有序退出、启动时回合转 interrupted、交互转 expired） | L | 否 | H.core |
| **T038** | CLI 提权（宿主侧） | feat | 5.8.4、5.8.6 | T034、T055 | 做：`auth/elevate`（血缘检查、全局一个未决、被拒静默、2 分钟过期）、`auth/elevation_get`、`auth/elevation_respond`、授权绑定连接与固定令牌、索引流的提权事件、审计。不做：原生确认框（T077） | 新增 IT-elevate（适配器后代被拒、限频、过期、断开即失效） | M | 否 | H.gw |
| T040 | 会话纯核心 I：回合状态机与队列 | feat | 5.2.6、5.2.16、5.6.2 | T011、T022 | 做：`driver::api`（5.6.2 的命令、事件、`AgentError` 分类）；`TurnMachine`（各状态、3 s 准备超时、10 s 中断兜底、在途规则、先 close 再 resume）；`QueueCore`。不做：Steer（T140）、自发回合（T041） | PROP-queue-fifo、PROP-interrupt-10s、PROP-turn-single-prompt、PROP-prompt-inflight-no-end | L | 否 | H.orch |
| T041 | 会话纯核心 II：自发回合、合并器与最终回答 | feat | 5.2.6、5.3.5、5.2.15 | T040 | 做：`AutonomousJudge`（Q10）、`Coalescer`（迁移写入时合并）、`group_id` 与 `final_item_ids` 判定、晚到输出与回合快照不可变的模型检查。不做：actor 接线 | PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable、UNIT-final-items、UNIT-grouping；新增 PROP-coalescer | M | 否 | H.orch |
| T042 | 交互代理纯核心 | feat | 5.2.7、5.3.6 | T031 | 做：`InteractionBroker`：入账（摘要、`actionable_by`）、回答检查链、撤回与失效、Claude 多问题提问的拆分与合并、`unsupported_schema`。不做：选项对照表数据（T053） | 新增 PROP-interaction（先到先得、终态唯一）、UNIT-question-mapping | M | 否 | H.orch |
| **T043** | 会话 actor I：创建、发送、中断与持久化 | feat | 5.2.6、5.5.2（本地模式）、5.5.3、5.5.5 | T033、T041、T042 | 做：`SessionManager`、`SessionActor` 适配层、`session/create`（本地模式，第一条消息随受理落盘）、`send`（排队）、`interrupt`、`queue_resume`、`get`、`capabilities`、交互的接线、回合事件与检查点、快照端口、准备阶段的恢复；用进程内驱动替身测试。不做：档位、信任门控、崩溃（T044） | 新增 IT-orch-lifecycle（进程内：创建到回合结束、第一条消息不丢、命令与事件同事务） | L | 否 | H.orch |
| **T044** | 会话 actor II：档位、信任门控、崩溃与晚到输出 | feat | 5.2.6（配置项、项目信任、崩溃与恢复、晚到的工具输出） | T035、T043 | 做：档位核对与 `config_unconfirmed`、回合中越权先改回再中断、`session/confirm_config`；三个时机重扫信任、stale 暂停、"不加载项目配置继续"；适配器退出、K6、K7；`late_tool_output` 与 `may_still_run`。不做：进程级集成测试（T062、T063） | 新增 IT-orch-gating（进程内：越权、stale、适配器退出、晚到输出的回合结构） | L | 否 | H.orch |
| T045 | cupper-cli I：客户端库与 Mirror | feat | 5.2.14、5.3.3 | T012、T014、T020 | 做：迁移 SP-4a 的 client 与 mirror：握手（peer、harness）、请求关联、订阅与连续性、epoch 重置、套接字发现、发布构建的宿主签名校验与拒绝 `weak_auth`、`--build-info`。不做：子命令 | 新增 UNIT-mirror | M | 否 | H.cli |
| **T046** | cupper-cli II：命令行 | feat | 5.2.14 | T038 | 做：`status`、`sessions`、`watch`、`history`、`new`、`send`、`interrupt`、`pending`、`approve`/`reject`、`answer`、`elevate`、`repl`、`--json`、`--harness-key-fd`（仅测试构建）。不做：`cupper diff`（T120）、`--steer`（T140） | 新增 IT-cli-commands（真实宿主加假 agent，逐个命令断言 `--json` 输出） | M | 否 | H.cli |
| T047 | test-harness 测试接口与录制导出 | feat | 5.2.16、5.9.9、5.9.3 | T034、T043、T052 | 做：`test/clock_advance`（全宿主静止屏障）、`test/run_maintenance`、`test/seed`（含 SP-1 分布和单回合 2,000 条目）、`host/status.pid`、`diag/recording_export`、其余注入点接线。不做：通知的记录模式（T150） | 新增 IT-clock-advance、IT-seed、IT-recording-export | M | 否 | H.core |
| T050 | 驱动层 I：传输、线路分接与 ACP 连接 | feat | 5.6.1、5.6.10、5.2.16 | T003、T022 | 做：`AcpTransport`（stdio、内存双工）、线路分接（编号、结构化脱敏入原始帧、K19 改写、扩展消息路由）、ACP 连接任务（回调只发 channel、`fs`/`terminal` 不支持、`clientInfo.name` 固定）、按 Q9 结论实现扩展请求。不做：提取层、AgentRuntime | 新增 UNIT-tap、UNIT-transport-memory | M | 否 | H.drv |
| T051 | 假 agent：剧本模式与 persona | feat | 5.9.2 | T001 | 做：迁移 SP-4a 的假 agent；启动配置（persona、登录状态、故障注入、进程分段）、5.9.2 新增的全部剧本命令、原始行通道、全局不变式。不做：回放模式（T052） | 新增 UNIT-fake-scripts（每个剧本命令在两种 persona 下的帧） | L | 否 | H.fake |
| T052 | 录制格式、回放模式与 SP-1 录制转换 | feat | 5.9.2、5.9.3 | T002、T051 | 做：录制格式 v1 的读写库、假 agent 回放模式、SP-1 的 48 份录制一次性转换入 `testdata/recordings/`、`tools/sanitize` 迁移、`testdata/` 通用 PII 扫描接入 `testdata-scan`。不做：录制导出（T047） | 新增 UNIT-rec-format、UNIT-replay；PII 扫描通过 | M | 否 | H.fake |
| T053 | Claude 提取层、选项对照表与 A 层金样例 | feat | 5.6.3–5.6.5、5.3.6、5.9.3 | T040、T050、T052 | 做：Claude 提取层（逐帧纯函数）、选项对照表（按工具区分 `allow-with-updates`，Q7）、kind 兜底与未知 kind、配置项与档位、能力提取；A 层金样例运行器接入 `rust` 作业；"每份录制都有金样例"和"取值都在表里"两项检查。不做：Codex（T110） | GOLD-A-claude-s01、s02c、s03、s05、s06、s07、s07b、s09（及其余 Claude 录制）、GOLD-A-unknown-enums、UNIT-option-map、UNIT-capabilities、UNIT-capabilities-plan | L | 否 | H.drv、H.fake |
| T054 | CLI 发现、登录 shell 环境与可信 PATH | feat | 5.6.6、5.2.10（程序路径） | T020、T022 | 做：登录 shell 取环境、可信 PATH 规则、`claude`/`git`/`git-lfs` 的绝对路径、数据驱动的 CLI 解析器（Q19 Claude 部分）、版本分类、配置目录校验、适配器环境变量规则、git 版本检测。不做：Codex 的安装布局（T111） | UNIT-cli-resolver（Claude 样例）、UNIT-version-classify、UNIT-realpath；新增 UNIT-env-rules | M | 否 | H.drv |
| T055 | 驱动层 II：AgentRuntime 与 Claude 接入 | feat | 5.6.1、5.6.2、5.6.7、5.6.8、5.8.5 | T027、T028、T053、T054 | 做：`AgentRuntime`（拉起适配器并写 `adapters` 表、拉起前核对、路由、各驱动命令、退出广播、重启退避与 crashed、能力缓存与探测目录、`agents/capabilities`、`agents/refresh`、`AgentStatus`、资源采样接口、stderr 接日志、测试构建的 agent 注入）。不做：Codex、Steer | IT-process-hygiene、IT-K13-echo-cwd、IT-K21-reap-group、IT-K26-client-info；新增 IT-adapter-restart | L | 否 | H.drv |
| T056 | 运行时安装器 | feat | 5.2.12、5.10.3、ADR-08 | T004、T020、T025 | 做：清单编译期内嵌、组件状态机、自检后按清单复制包内文件、按锁文件下载 npm tarball（SRI、路径检查、不运行脚本、UA 不含用户信息）、本机打包 claude-agent-acp 并核对哈希、原子提交、旧版本回收、registry 注入。不做：codex-acp 的复制（T111） | 新增 IT-runtime-install（SP-2 的 5 种故障加"复制中包被替换"，本地夹具离线运行） | L | 否 | H.rt |
| T057 | （条件）Q8 后备：同一种 agent 的 prompt 全局串行 | feat | 5.13 Q8、ADR-09 | T055 | 触发：T003 判定 Claude 不通过。做：按 agent 配置的串行闸门，或只对该 agent 退回每会话一个适配器进程（按 T003 的报告二选一） | 新增 IT-q8-fallback | M | 否 | H.drv |
| T060 | Git 服务 I：GitCmd 三档与仓库档防护 | feat | 5.2.10（调用约定、仓库档、指纹） | T054 | 做：`GitCmd` 构造器（固定参数、绝对路径、可信 PATH、环境、工作目录、超时）、三档类型从第一次调用起分开、仓库档的读配置、拒绝、覆盖、不重试、指纹比对与 notice。不做：快照档（T061）、用户档（T123） | 新增 IT-git-repo-tier-malicious、UNIT-git-fingerprint | M | 否 | H.git |
| T061 | Git 服务 II：快照档与回合快照 | feat | 5.2.10（快照档）、5.2.6、5.9.8 | T043、T060 | 做：私有对象库、每工作区 GIT_DIR、白名单配置、复制 info 文件与 sharedindex、保留 mtime、自定义过滤器占位、LFS 标准取值、排除超大和嵌套仓库、`snapshot.recorded`，接入快照端口；Q32 快照档逐键确认。不做：diff（T120） | IT-git-snapshot-parity；新增 IT-git-racy、IT-git-filter-placeholder、IT-git-snapshot-malicious、PERF-snapshot | L | 否 | H.git |
| T062 | 宿主集成测试 I：回合、交互与历史 | test | 5.9.1、5.6.9、5.9.8 | T044、T047、T055 | 做：真实宿主进程加 claude 形态假 agent 加 CLI 库的用例，发现缺陷就地修。不做：codex 形态的用例（T112） | IT-K1-close-resume、IT-K5-withdraw、IT-K6-crash-resume、IT-K6-output-then-crash、IT-K9-autonomous、IT-K14-old-cli-auth、IT-auth-none、IT-K19-unknown-kind-privileged、IT-history-from-host、IT-thought-claude、IT-plan-entries、IT-form、IT-plan-confirm、IT-interaction-expire | L | 否 | H.orch、H.drv |
| T063 | 宿主集成测试 II：信任、档位、并发、崩溃与外发 | test | 5.9.1、5.9.8 | T044、T047、T055、T061 | 做：信任与档位的进程级用例、6 个会话共用假适配器、M1a 范围的故障点、高负载下反复 `kill -9`、宿主放进拒绝出站的 Seatbelt。不做：git 写操作和 worktree 的故障点（T121、T124） | IT-config-unconfirmed、IT-trust-stale、IT-trust-rescan-before-prompt、IT-trust-settingsources、IT-six-sessions-shared-adapter、IT-NFR9-failpoints（M1a 部分）、IT-NFR9-crash-under-load；新增 IT-host-egress-seatbelt | L | 否 | H.orch、H.store |
| **T064** | 兼容套件框架 | feat | 5.9.5、5.1.6（compat-framework、testdata-scan） | T044、T047、T055、T056 | 做：`cargo xtask compat`（5.9.5 的剧本全集，含 Q8、未信任项目、Q28、Q34、思考流）、只断言结构、真实配置的前后哈希对比、报告与 JSON schema、录制脱敏入库、`--fake`；`compat-framework` 作业；`testdata-scan` 加报告校验和已验证范围重算；`tools/rec-shape-diff`；`lsof` 断言。不做：真实 CLI 的运行 | `compat --fake` 在 CI 通过；新增 UNIT-compat-report | L | 否 | H.xt、H.fake |
| T065 | B 层金样例运行器 | test | 5.9.3（B 层、规范化） | T044、T047 | 做：进程内宿主核心加内存传输加按录制 `t` 推进的 ManualClock、按 `x-id`/`x-time`/`x-path` 的规范化、严格模式、`pnpm test:golden` 的 B 层；先用假 agent 经宿主产出的录制 | 新增 GOLD-B-fake | M | 否 | H.orch、H.fake |
| **T066** | 首批真实录制：Claude 兼容套件 | test | 5.9.3、5.9.5 | T064、T065 | 做：在作者机器上用真实 Claude 跑兼容套件；录制、A/B 层金样例、报告入库；"已验证 CLI 范围"由报告生成；记录 Q8 复核、Q10、Q28、Q34（Claude）的结论 | MAN-compat（Claude）、COMPAT-claude-thinking、COMPAT-untrusted-project；新增 GOLD-B-claude | M | 是：真实 Claude CLI，消耗额度（思考流剧本用 sonnet 或 opus） | H.fake |
| T067 | 宿主性能测量 | test | 5.9.6 | T037、T047 | 做：迁移 SP-4a 的 `measure.rs`（含签名校验的宿主冷启动、200 会话的索引快照、6 个活跃会话的内存）与每晚趋势作业 | 新增 PERF-host-cold-start、PERF-index-snapshot、PERF-host-memory | S | 否 | H.core |
| T070 | shell-api 与 i18n | feat | 5.7.3、5.7.7（本地化） | T001 | 做：`@cupper/shell-api`（5.7.3 的完整 M1a 接口与浏览器 mock）、`@cupper/i18n`（i18next、语义码为键、按功能分文件、一种语言、不可见字符转义工具）。不做：Electron 实现 | 新增 UNIT-shell-mock、UNIT-i18n-escape | M | 否 | T.lib |
| **T071** | host-client I：连接、切帧、请求与续传 | feat | 5.7.6、5.3.3 | T015、T070 | 做：连接状态机、切帧（含 `0x02`）、请求关联、断线后同一 `command_id` 重发、订阅与游标（含 `data_epoch`）、快照重置、连续性与三种计数；帧级 TS mock 宿主。不做：视图（T072） | 新增 UNIT-hc-conn、UNIT-hc-sync | L | 否 | T.lib |
| T072 | host-client II：视图 reducer 与交叉验证 | feat | 5.7.6、5.9.1（差分测试） | T071 | 做：TS 的两个视图 reducer（窗口、历史插入、`late_by_ref`）与按条目订阅的版本号；与 Rust 视图用 fixtures 交叉验证（接入 `protocol` 作业）；每晚差分测试 | 新增 UNIT-views-parity、UNIT-views-differential（每晚） | M | 否 | T.lib、P |
| T073 | cupper-desktop-native | feat | 5.1.3、5.7.2、5.8.1 | T020、T021、T023 | 做：napi-rs 扩展：经 `cupper-sys::spawn` 拉起宿主并写启动载荷、连接并完整校验宿主签名后返回 fd、每次读取的令牌比对、`dev_key` 证明与 `host_proof` 校验。不做：Electron 侧逻辑 | 新增 UNIT-native（对 ad-hoc 宿主跑通拉起、校验、证明） | M | 否 | D.nat |
| T074 | 桌面主进程 I：应用骨架、构建与加固基线 | feat | 5.7.2、5.7.8、5.7.9、5.10.6 | T070 | 做：`apps/desktop`、Vite 构建三个入口、`app://cupper` 与 CSP、路径规范化、webPreferences、导航与新窗口拦截、权限处理器、开关白名单、单实例锁、userData、构建种类常量、dev 与 e2e 的打包配置、prefs、main.log、菜单骨架。不做：宿主连接、发布配置（T160） | 新增 UNIT-main-hardening；`pnpm build:e2e` 产物可启动 | L | 否 | D.main |
| **T075** | 桌面主进程 II：拉起宿主、control 连接与退出确认 | feat | 5.7.1、5.7.2、5.5.1 | T005、T071、T073、T074 | 做：`hostLauncher`（载荷留内存、被杀后重传、500 ms、退出码 3–6）、`hostConnection`、`controlClient`、`lifecycle`（退出确认、关机跳过）、"无法验证宿主"、`CUPPER_E2E_DIR` | 新增 UNIT-launcher | M | 否 | D.main |
| **T076** | 桌面主进程 III：windowBridge 与 preload | feat | 5.7.1、5.7.3、5.7.8（命令白名单） | T075 | 做：每窗口 `windowBridge`（数据连接绑定文档实例、代发 initialize、入方向字节块批量、出方向解析与白名单与重新序列化、`sendBlob`、8 MB 流控、重连顺序、按窗口断连的钩子）、preload 的 ShellAPI 实现。不做：真实输入与特权（T077） | 新增 UNIT-bridge-whitelist、UNIT-bridge-flow、UNIT-bridge-lifecycle | L | 否 | D.main |
| **T077** | 桌面主进程 IV：真实输入、特权操作与 Q26 | feat | 5.7.8（真实输入、原生确认框）、5.7.2、Q26 | T076 | 做：`inputGuard`（9 条规则、可注入时钟）、证据写入与已放行 `command_id` 表、`privileged`（取内容、原生确认框、control 发送、一次一个、逐次加长的冷却）、`add_project` 流程；在 Electron 上复测 Q26 | 新增 UNIT-input-guard、UNIT-privileged；MAN-keyboard | L | 是：MAN-keyboard 需要作者用实体键盘确认 | D.main |
| T078 | 测量闸门：主进程转发 | test | 5.7.1（测量闸门）、5.9.6 | T047、T055、T076 | 做：按判定负载（索引流加 4 条会话流、5 MB blob、5,000 事件重同步）产生负载，真实渲染进程消费，测主进程事件循环延迟 p99 与流重置，可见和遮挡各一次，另出 6 条流的压力报告 | 判定报告（p99 < 5 ms 且无流重置）；新增 PERF-bridge-gate | M | 知情：在作者机器上弹窗口运行约 20 分钟 | D.main、H.core |
| T079 | （条件）utilityProcess 宿主桥加委托票据 | feat | Q33、5.7.1、5.8.1 | T078 | 触发：T078 不达标。做：协议加 `auth/delegate` 与 `auth: ticket`、网关的票据认证、主进程取票据交给 utilityProcess、由它持有数据连接；编排者先修订 5.7.1、5.8.1 与 ADR-03 | 重测 PERF-bridge-gate 达标；新增 IT-ticket-auth | L | 是：ADR-03 修订需要用户确认 | D.main、P、H.gw |
| T080 | ui-core I：骨架、状态层、安全规则与截图测试 | feat | 5.7.5、5.7.6、5.7.8、5.9.1（截图回归） | T072 | 做：`packages/ui-core`、Providers、流 store（到达即应用、确认、按条目订阅）、订阅策略（索引加 LRU 3）、zustand、TanStack Query、路由、`security/policies.ts`、ESLint 安全与文案规则、截图测试基础、`ui` 作业与基线工作流、发布 CSP 下的违规计数 | `ui` 作业通过；新增 UNIT-ui-store、UI-csp-zero | L | 否 | T.ui |
| T081 | ui-core II：侧边栏、新建会话与全局状态 | feat | 5.7.4、5.7.5、5.12.2（界面） | T080 | 做：侧边栏（项目分组、状态点、"未信任"、提权横幅）、新建会话页（Claude、本地模式）、添加项目、页头与"未信任""配置待确认"横幅、弱认证标记、重连状态、Toaster | 新增截图 UI-sidebar、UI-new-session、UI-banners | M | 否 | T.ui |
| T082 | ui-core III：消息流 | feat | 5.7.4（消息流）、5.7.6（长会话）、5.7.8（Markdown） | T080 | 做：按行虚拟化、加载更早的历史与视口锚点、贴底与 ↓；带状态的回合标题（不折叠）、用户与助手消息（streamdown 配置）、工具单行摘要、思考、提示、错误。不做：高亮、计划、恢复指引、折叠（T143） | 新增 UNIT-md-injection（Markdown 注入金样例）、UNIT-stream-anchor、截图 UI-stream | L | 否 | T.ui |
| T083 | ui-core IV：交互卡片、排队条与输入框 | feat | 5.7.4（卡片）、5.7.7、5.7.8 | T080 | 做：三类卡片与未知类型、各状态、不可见字符转义、"已截断"、"效果未知"、500 ms 与"可以操作"标记、"请再按一次"、privileged 选项经 `requestPrivileged`；排队条；输入框（发送、排队、中断、输入法）。不做：模式选择、Steer | UNIT-composer-ime；新增截图 UI-cards | L | 否 | T.ui |
| **T090** | e2e 基础设施 | test | 5.9.4（环境）、5.9.9 | T046、T077、T081 | 做：Playwright `_electron` 夹具（e2e 构建、每用例临时根目录、临时仓库、假 agent、CLI 以 `--harness-key-fd` 接入）、原生确认框替换并记录、`__cupperTest`、按窗口断连、事件屏障、按键辅助；`e2e` 作业 | E2E-single-instance、E2E-quit-confirm | M | 否 | D.main、C |
| **T091** | NFR-1 验收套件 | test | 5.9.4（NFR-1） | T037、T044、T082、T083、T090 | 做：严格按 5.9.4 写五个用例，含 CLI 提权走原生确认、"可以操作"后两端同时回答、三种断开、两次带屏障的 `kill -9` | E2E-NFR1-1、E2E-NFR1-2、E2E-NFR1-3、E2E-NFR1-4、E2E-NFR1-5 | M | 否 | D.main、H.cli |
| **T092** | 安全 e2e 与外发检查 | test | 5.9.4（安全）、Q27 | T083、T090 | 做：e2e 构建上的安全用例；验证 `nettop` 采样整个 App 进程树的可行性（Q27），不可行就换方法并上报 | E2E-SEC-csp、E2E-SEC-real-input、E2E-SEC-privileged、E2E-SEC-fake-host、E2E-SEC-egress | M | 否 | D.main |
| **T093** | 交互与项目 e2e | test | 5.9.4、5.12.4 | T083、T090 | 做：卡片的键盘操作、提问卡片、持久选项的原生确认、添加项目与信任、新建 Claude 会话 | E2E-approve-enter-esc、E2E-question-card、E2E-persistent-option-native、E2E-project-add-trust、E2E-new-session-agent（Claude） | M | 否 | D.main、T.ui |
| **T099** | M1a 退出验收 | chore | 5.12.2（退出标准） | T006、T016、T062、T063、T066、T067、T091、T092、T093 | 做：在退出候选的同一个提交上按平时并行度连续跑 20 次（失败一次就查根因并清零）；核对 `pending-ids` 的 M1a 条目已清空、契约已冻结 | 20 次运行记录存档；M1a 同步会纪要 | S | 是：M1a 同步会，用户确认 | C |

**注**

- **T001**：仓库还没有远端（P-2）。U-P1 落实之前，三个作业的验收用本机命令代替（`cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo deny check`、`pnpm lint`），输出存进任务目录，workflow 文件照写。工具链版本按 5.1.4"开工时按当天复核"。
- **T002**：编号的匹配规则见 P-3。初始的 `pending-ids.toml` 按第 5 节登记里程碑。
- **T003**：报告是以下任务的输入：T050（Q9 决定扩展请求走 SDK 还是分接层）、T055 和 T057（Q8）、T038（SP-4b #7 影响血缘信号）、T074（Electron 版本）、T078（负载数字）。代码放 `spikes/T003-driver-preflight/`，做法同 SP-1：运行前记下配置文件哈希，用最便宜的模型，用临时仓库。可以与 T001 同时开始。
- **T005**：5.9.4 要求 e2e 构建"打包方式与发布构建相同"，需要组装 `CupperHost.app`；5.12.2 没有列它（P-8）。
- **T011（L）**：事件目录是第 2 组冻结字段的主体，`ToolCall` 的必填字段表、交互模型、能力描述互相引用，拆开就不能一次评审完整的事件契约。代码以类型为主，测试是机械的往返。
- **T012、T013**：都只依赖 T011，彼此并行；按文件分开（例如 `methods/core.rs` 和 `methods/ext.rs`）。
- **T016**：排在测量闸门之后，理由见 P-1。若编排者决定按 5.12.5 的原文执行，T016 改为只依赖 T015、T070，Q33 的改动按附加式进入。
- **T023**：`test_config` 只经 stdin 的启动载荷传入，不设 argv 开关（P-9）。
- **T025**：表结构变更的规则见 P-4。
- **T026（L）**：流状态、检查点、回合快照共用"追加三步"和"回合快照写入后不可变"两条不变式，PROP-stream-replay 必须同时覆盖三者。
- **T030（L）**：握手、固定令牌和四种认证是一条不可分的安全路径，只合并一半会在 main 上留下可以绕过的连接入口。
- **T033（L）**：replay、snapshot 与断开清理共用连接和订阅状态；IT-gw-sync 迁移 SP-4a 的 7 个端到端测试，要求完整的分发路径。
- **T034（L）**：索引流的唯一写者同时承载项目、状态、在场和通知意图，状态计算依赖全部输入；`project/add` 用 T060 的仓库档取 git common dir。
- **T037（L）**：启动顺序、残留清理、空闲退出和有序退出共用子进程登记与退出路径，IT-lifecycle 需要它们同时存在。有序退出经 `AgentRuntime` 的关闭接口关适配器，T055 合并前这一步没有对象可关。
- **T040（L）**：TurnMachine 和 QueueCore 由同一组性质测试驱动（出队取决于回合结束），驱动接口类型是它们的输入。
- **T043（L）**：一个邮箱、一个动作执行器承担创建、发送、中断和回合结束，拆开后 main 上的会话走不完一个回合。驱动用实现 `driver::api` 的进程内替身测试，不等 T055；索引通知经 T033 定义的接口，T034 合并后直接接上。
- **T044（L）**：四类处理都是"驱动事件或外部变化 → 暂停、中断或写提示"，共用暂停与恢复逻辑和 IT-orch-gating 的夹具。
- **T047**：`clock_advance` 的静止屏障要覆盖全部 actor、写线程和扇出，实现方式由子 agent 设计、编排者审。`diag/recording_export` 放在这里，是为了让 T064 和 T065 可以并行。
- **T051（L）**：persona 决定每个剧本命令发出的帧形态，分开后剧本命令的测试无法断言真实形态。假 agent 不依赖宿主，第 1 波就能做。
- **T052**：SP-1 的录制只进 A 层（5.9.2）。
- **T053（L）**：提取层、对照表和 A 层金样例互为验收；金样例文件是生成物，不计行数。Q7 按"按工具区分，判断不出按持久"实现；是否向上游反馈由用户决定（第 6 节）。
- **T055（L）**：拉起、路由、退出广播与重启作用于同一个适配器实例的生命周期，K21 和进程卫生用例需要完整路径。用例直接驱动 `AgentRuntime`，不经会话 actor，所以不等 T043。
- **T056（L）**：下载、校验、打包、原子提交是一个整体，5 种故障用例要覆盖全过程。`cupper-host --self-test`（5.10.7 第 6 步）也在这里实现。外发请求只访问 `registry.npmjs.org`，UA 不含用户信息。
- **T061（L）**：各种 index 形态、过滤器和恶意配置的断言共用同一套临时仓库夹具和 GIT_DIR 生成逻辑。作者机器上没有 git-lfs（Q32），LFS 用例只在 CI 上运行，本机跳过并在汇报里注明。
- **T062、T063（L）**：每个约 15 个进程级用例，已按主题拆成两个任务，再拆只会重复搭夹具。T063 的 Seatbelt 只套宿主（纪要 D13）。
- **T064（L）**：剧本、报告 schema、录制入库与 `--fake` 是一个闭环，`compat-framework` 作业要整条链路才能运行。
- **T066**：按 U-R4 用真实配置加前后哈希对比；Q8 在这里由宿主再复核一次。
- **T071（L）**：连接状态机、切帧和续传共用一个连接对象，"断线后同一 command_id 重发"要三者一起测。
- **T074（L）**：加固基线必须与应用骨架同时出现，否则 main 上会有一个未加固的 Electron 应用。Electron 版本按 T003 的复核（5.10.6：45 正式发布满两周就从 45 开始）。
- **T076（L）**：生命周期、转发和流控作用于同一条连接，UNIT-bridge-flow 要三者一起。建议同时用 TS mock 宿主按判定负载做一次预测量（不作判定），尽早发现主进程转发的问题（R1）。
- **T077（L）**：真实输入规则和特权操作共用输入证据的记录与消耗，`requestPrivileged` 本身也要求真实输入。
- **T078**：判定以作者机器为准（5.9.6）。不达标立即触发 T079，T016 等 T079 合并后再做。
- **T080（L）**：状态层、订阅策略和安全规则是 T081–T083 的共同前提，截图测试至少要一个页面才能建立基线。ui-core 只依赖 ShellAPI 和 HostPort，不受测量闸门结论的影响，所以不必等 T078。
- **T082、T083（L）**：各自围绕一个组件（消息列表；卡片与输入框）和它的滚动或键盘规则。
- **T099**：20 次运行"按 CI 平时的并行度"，需要 GitHub 上的 macOS runner（P-2）。

### 2.2 M1b：桌面 MVP

| 编号 | 名称 | 类型 | 设计依据 | 前置 | 做什么 / 不做什么 | 验收（用例编号） | 规模 | 用户参与 | 区域 |
|---|---|---|---|---|---|---|---|---|---|
| T101 | SP-10 通知身份（Q2） | spike | 5.13 Q2、5.2.9 | —（可提前） | 做：在 `Contents/Helpers` 里的 helper bundle 申请通知授权、弹通知、点击回调并按 bundle id 激活主 App，确认系统设置里的名称和图标。不做：TCC 归属与 disclaim（M2） | MAN-notification（初次）；判定报告 | S | 是：会弹系统授权框，需要用户在场 | SP |
| T102 | 技术验证：Q8（Codex）、SP-4b #7（Codex）与 Q31 | spike | 5.13 Q8、Q31、5.9.5 | —（可提前） | 做：用 SP-1 的 TS 客户端跑 Codex 并发剧本；观察 codex-acp 拉起命令时的进程树；核对 `agentMessage.phase` 是否经 `_meta` 透出；初查 Codex 权限模式的取值（Q25） | 报告：Q8 逐轮数据、SP-4b #7、Q31 结论；配置文件前后哈希 | M | 是：真实 Codex CLI，消耗额度；可能改写 `~/.codex/config.toml` | SP |
| **T103** | SP-6 渲染压力测试 | spike | 5.9.7（场景 1–8） | T099 | 做：在作者机器上按 5.9.7 跑场景 1–8（含 8 小时浸泡和遮挡），出带数据和 trace 的报告，不达标项列成后续任务。不做：场景 9（T104） | SP-6 报告；MAN-ime | M | 是：占用作者机器（浸泡 8 小时）；场景 6 需要作者用拼音输入 | SP、T.ui |
| T104 | Q29 审查文档可行性（SP-6 场景 9） | spike | 5.7.5（审查文档）、5.9.7、Q29 | T074（可提前） | 做：在 `spikes/` 里用与 `apps/desktop` 相同的协议注册和 CSP 搭双源原型，验证 constructable stylesheet、跨源 iframe 里的 Worker 与模块脚本、桌面确认的 8 点、5,000 行 diff 的冷启动、内存、滚动和违规数 | Q29 判定报告 | M | 知情：在作者机器上测量 | SP |
| T105 | 技术验证：Q35 本机签名身份 | spike | 5.13 Q35、5.8.1、5.7.9 | T005、T073（可提前） | 做：分别用钥匙串自建的代码签名证书和免费 Apple ID 的 Apple Development 证书给本机构建签名，验证宿主按签名要求认出 UI、hardened runtime 与库校验下各组件能否运行、CLI 能否接入、UI 被杀后新 UI 能否接上旧宿主 | Q35 判定报告（两种来源分别给结论）；仓库里不出现证书摘要和 Team ID | M | 是：在钥匙串建证书或登录 Xcode，需要用户在场 | SP |
| T110 | Codex 提取层、选项对照表与 A 层金样例 | feat | 5.6.3–5.6.5、5.3.6、Q25、Q31 | T053、T102（可提前） | 做：Codex 提取层（`threadStatus`、`_meta.codex.error`、文本形式的错误、读取类命令、Q31）、选项对照表、配置项与档位（未知取值按 full_access） | GOLD-A-codex-s02、s04、s04b、s05、s06、s07、s10x（及其余 Codex 录制）；UNIT-option-map（Codex 部分） | L | 否 | H.drv |
| T111 | Codex 接入：发现、运行时与回收 | feat | 5.6.6–5.6.8、5.2.6、5.2.12 | T099、T110 | 做：codex-acp 从包内复制、`CODEX_PATH` 解析（Q19 Codex 布局、K15）、`CODEX_HOME` 校验、未信任项目返回 `project_untrusted`、`RunState` 接入自发回合、回收策略、`agents/restart` | UNIT-cli-resolver（Codex 样例）、IT-K20-reclaim；新增 IT-codex-untrusted | L | 否 | H.drv、H.rt |
| T112 | Codex 回合语义集成测试 | test | 5.6.9（K2–K4、K7、K12、K17、K18） | T111 | 做：codex 形态假 agent 的进程级用例，发现缺陷就地修 | IT-K2-reject-ends-turn、IT-K3-late-output、IT-K4-cancel-pending-approval、IT-K7-restartable、IT-K12-plan-confirm-idle、IT-K17-text-error、IT-K18-upstream-retry | M | 否 | H.orch、H.drv |
| T113 | Codex 兼容矩阵与 B 层补录 | test | 5.9.5、5.9.3 | T112 | 做：用真实 Codex 跑兼容套件（Q8 复核、Q25、Q34、未信任项目拒绝启动），用宿主重录取消场景补 K3 的 B 层金样例，已验证范围写进清单 | MAN-compat（Codex）、COMPAT-untrusted-project（Codex）；新增 GOLD-B-codex-s07 | M | 是：真实 Codex CLI，消耗额度 | H.fake |
| T114 | codex-drift 每日检查 | ci | 5.1.6（codex-drift）、ADR-08 | T004（可提前） | 做：由 SP-2 的脚本迁移 `tools/codex-drift`，每日作业在发现新 codex 版本时比对 app-server 方法，失败开 issue | 作业首次运行通过；新增 UNIT-codex-drift | S | 否 | C |
| T119 | （条件）Q8 后备（Codex） | feat | 5.13 Q8、ADR-09 | T111 | 触发：T102 判定 Codex 不通过。做：对 Codex 启用 T057 的串行闸门（T057 未做时在这里实现） | IT-q8-fallback（codex 形态） | M | 否 | H.drv |
| **T120** | Git 服务 III：diff、file_diff、status 与 branches | feat | 5.2.10（diff 与 hunk、git/status）、5.3.4 | T099 | 做：四种范围的 `git/diff`（`diff_token`、`caveats`、`index_unmerged`、`stat`、`filter_unsupported`）、`git/file_diff`（三种操作单位）、`DiffCache`、`git/status`、`git/branches`、`cupper diff`。不做：写操作 | IT-git-diff-scopes | L | 否 | H.git、H.cli |
| **T121** | Git 服务 IV：暂存、取消暂存与还原 | feat | 5.2.10（hunk 操作、并发）、5.2.5 | T120 | 做：仓库档 `apply --cached` 和 `-R`、`RepoLock`、`pending_external` 强制同步、`stale_diff`、暂存前 `check-attr`、写操作前后的故障点；Q32 仓库档写操作逐键确认；LFS 路径实测（CI 上） | 新增 IT-git-stage；IT-NFR9-failpoints 补 `pending_external` 两点 | M | 否 | H.git |
| T122 | IT-git-hunk：迁移 SP-5 的 82 个用例 | test | 5.9.8（git 服务） | T121 | 做：把 SP-5 的 82 个 hunk 用例和快照、worktree 的边界用例改写成 Rust 测试 | IT-git-hunk | M | 否 | H.git |
| **T123** | Git 服务 V：提交与推送（用户档）与 Q23 | feat | 5.2.10（用户档）、Q23 | T120 | 做：`commit -F -`、`push --set-upstream`、错误映射与脱敏的 stderr 尾部、指纹变化的确认参数；Q23（SSH agent、GPG、凭据助手）的验证与恢复指引 | IT-git-commit-push（本地裸仓库） | M | 可选：用作者的 SSH 和 GPG 配置手动验证 Q23 | H.git |
| T124 | worktree 创建（CP-5） | feat | 5.2.10（worktree）、5.5.2、5.2.13 | T099 | 做：`worktree add`（位置与权限、分支名校验与后缀、lock、1 秒后 refresh、进度、required 过滤器的提示）、`session/create` 的 worktree 分支、崩溃后复用已登记的 worktree、对应故障点 | IT-worktree-create；IT-NFR9-failpoints 补 worktree 一点 | L | 否 | H.git、H.orch |
| T125 | worktree 检查、归档、恢复与重命名 | feat | 5.2.10、5.2.6、5.5.9 | T120、T124 | 做：`worktree/check` 的 7 条规则、`session/archive`、`session/restore`、`session/rename`、`session/list_archived`、`project/remove` | IT-worktree-check-rules、IT-session-archive-restore | L | 否 | H.git、H.orch、H.idx |
| T126 | 文件搜索、提示内容映射与在外部应用打开 | feat | 5.2.11（文件服务）、5.3.4 | T099 | 做：`files/search`（快照档 `ls-files`、非 git 目录遍历、缓存、模糊匹配、UTF-16 区间）；`image`、`file_ref`、`review_comment`、`text_blob` 映射为 ACP 内容块；`workspace/open` | 新增 IT-files-search、UNIT-prompt-mapping、IT-workspace-open | M | 否 | H.git、H.orch |
| T127 | 存储用量与清除快照对象 | feat | 5.4.5、5.7.4（设置页"存储"） | T099 | 前提：编排者先补设计（P-6）。做：查询各类数据大小、清除快照对象的方法（附加式），清除后旧回合的 diff 返回 `snapshot_missing` | 新增 IT-snapshot-purge | S | 否 | H.git、H.store、P |
| **T130** | 审查面板：主文档部分 | feat | 5.7.4、5.7.5、5.5.8 | T121、T123、T152 | 做：右侧标签容器里的审查面板：范围切换、文件列表、暂存栏（还原要主文档确认控件和真实输入）、待发送评论与"发给 agent"、提交框（hooks 与指纹提示）、`caveats`；为 diff 视图留接口 | 新增 UNIT-review-actions、截图 UI-review | L | 否 | T.ui |
| **T131** | 审查文档（独立源 iframe） | feat | 5.7.5（审查文档）、5.7.8 | T104、T130 | 前提：T104 通过。做：第二个渲染入口与 `app://cupper-review`、独立 CSP、iframe 的 sandbox 与导航限制、@pierre/diffs（`workerFactory`、`shiki-js`）、版本化 MessageChannel 协议（两侧 zod）、无状态视图、`reviewDocumentUrl`；新入口接入许可证门禁第一层 | E2E-SEC-review-iso、E2E-review-comment；新增 UNIT-review-injection | L | 否 | D.main、T.ui |
| T139 | （条件）审查面板后备：DiffHunksRenderer | feat | 5.7.5（后备方案） | T104、T130 | 触发：T104 不通过。做：`DiffHunksRenderer` 生成 HAST，在主文档用 React 渲染，自写虚拟滚动和 hunk 交互 | E2E-review-comment；新增 UNIT-hast-render | L | 否 | T.ui |
| T140 | Steer：宿主、驱动、CLI 与界面 | feat | 5.2.6、5.6.2、5.5.3、NFR-7 | T111 | 做：纯核心的 Steer 判定、驱动 `Steer`（按 Q9 结论）、能力里的 `steer`、`cupper send --steer`、界面默认排队并可显式插话；能力降级的用例 | PROP-steer-only-prompting、IT-K10-steer-explicit-only、IT-K11-steer-idle-queued、E2E-composer-steer-default、IT-degrade-* | L | 否 | H.orch、H.drv、T.ui |
| T141 | 配置选择：set_config 与模式选择器 | feat | 5.2.6（配置项）、5.6.5、5.8.2 | T099 | 做：宿主 `session/set_config`（档位规则、`mode_set_by`）；输入框里的模型、推理强度、权限模式选择器；完全访问走 `requestPrivileged`；`elevated` 提示 | E2E-full-access-native；新增 IT-set-config-tier | L | 否 | H.orch、T.ui |
| T142 | 输入框增强 I：@ 文件引用与拖入文件 | feat | 5.7.4（输入框）、5.7.3 | T126 | 做：`@` 模糊搜索的弹出层（UTF-16 区间高亮）、拖入文件生成引用（ShellAPI 新增 `pathForFile`，主进程用 `webUtils.getPathForFile`） | E2E-file-ref | M | 否 | T.ui、D.main |
| T143 | 消息流增强 I：代码高亮、计划与恢复指引 | feat | 5.7.4（消息流）、MS-1、MS-4、MS-5 | T099 | 做：Shiki 懒加载与双主题、计划与待办条目（entries 带状态，markdown 计划）、错误条目的恢复指引（语义码到文案和操作） | E2E-error-guidance；新增截图 UI-plan、UI-error | M | 否 | T.ui |
| T144 | 新建会话页：两种 agent、工作区与分支 | feat | 5.7.4（新建会话页）、CP-5、AG-3、AG-4 | T111、T124、T141 | 做：agent 选择（带可用状态）、本地或新建 worktree、起始分支、分支名显示在页头和标题栏、第一条消息前显示缓存的能力 | E2E-new-session-agent（两种）、E2E-new-session-before-first-message | M | 否 | T.ui |
| T145 | 输入框增强 II：图片、斜杠命令、↑ 与发送键 | feat | 5.7.4（输入框）、CP-1、CP-2、CP-3 | T142 | 做：粘贴或拖入图片（`HostPort.sendBlob` 与 `blob/put`）、斜杠命令菜单（来自能力）、输入框为空时 ↑ 恢复、Enter 或 ⌘Enter 的设置（prefs） | E2E-image-paste、E2E-slash-commands、E2E-send-key-setting | M | 否 | T.ui |
| T146 | 消息流增强 II：回合折叠与晚到输出 | feat | 5.7.4（回合折叠、晚到的输出）、5.3.5 | T143 | 做：按回合折叠与耗时（`final_item_ids`、始终显示在折叠之外的条目）、`late_tool_output` 的显示与跳转、原条目旁的"最终退出码"、`may_still_run` 提示 | 新增截图 UI-stream-fold、UI-late-output | M | 否 | T.ui |
| T147 | 设置页 II：通用、通知、外观、存储与命令行工具 | feat | 5.7.4（设置页）、UI-2 | T127、T151 | 做：通用、通知开关、外观（主题覆盖、语言覆盖）、存储（大小、清除快照对象）、"安装命令行工具"（按 P-6 补充的设计） | 新增截图 UI-settings-general | M | 否 | T.ui、D.main |
| T150 | 系统通知与 ui/navigate | feat | 5.2.9、5.7.2、AP-3 | T099、T101 | 前提：T101 通过。做：cupper-sys 的 UserNotifications、宿主 `notifier`（主线程 run loop、内置中英文案、设置开关、`test_config.notifier: record`）、点击后经 control 连接发 `ui/navigate` 并按 bundle id 激活、`ShellAPI.onNavigate` 只导航不聚焦卡片 | MAN-notification（产品内复核）；新增 IT-notifier-record、E2E-notification-navigate | L | 是：MAN-notification 复核 | H.idx、H.sys、D.main、T.ui |
| T151 | 设置页 I：agent 状态、连接与已归档会话 | feat | 5.7.4（设置页）、AG-2、AG-5 | T111、T125 | 做：设置页框架；agent 状态（CLI 路径与版本、已验证范围、登录状态、配置目录校验、运行时、资源占用、重启）、连接与设备、已归档会话（恢复） | 新增截图 UI-settings-agents | M | 否 | T.ui |
| **T152** | 三栏布局与亮暗主题 | feat | 5.7.4、5.7.7（主题）、PS-1、UI-3、UI-6 | T103 | 做：Codex 式三栏（尺寸 token、右侧标签容器、侧边栏标题行、新建会话页布局）、亮暗两套 token（两个文档共用）、跟随系统与覆盖、Shiki 双主题的接入 | 截图 UI-layout（两种主题）；MAN-ui6 | L | 是：MAN-ui6 需要作者并排比对 | T.ui、D.main |
| T153 | 会话管理界面、快捷键与"打开"按钮 | feat | 5.7.7（快捷键）、5.5.9、PS-4、UI-0、UI-4 | T125、T126 | 做：归档时的 worktree 清理提示、重命名、5.7.7 的快捷键（菜单加速键经 `onCommand`，含 ⌘⌥A 跳到下一个需要处理的会话）、标题栏"打开" | E2E-archive-cleanup、E2E-next-needs-action、E2E-shortcuts、E2E-workspace-open | M | 否 | T.ui、D.main |
| T154 | 中英文与截图基线 | feat | 5.7.7（本地化）、UI-5 | T131、T140、T144、T145、T146、T147、T150、T153 | 做：补齐中英文文案（含主进程原生确认框）、跟随系统语言与覆盖、截图回归扩到两种主题乘两种语言 | 截图回归（两种语言） | M | 否 | T.ui、D.main |
| T155 | SP-6 场景改写为每晚性能回归 | test | 5.9.7（产出） | T103 | 做：把场景 2、3、4、7、8 改写成每晚的性能回归测试与趋势阈值 | 新增 PERF-ui-stream、PERF-ui-long-session、PERF-ui-switch、PERF-ui-occluded | M | 否 | T.ui、C |
| T156 | 防休眠（NFR-13） | feat | 5.2.13（防休眠） | T099 | 做：cupper-sys 的 IOPMAssertion、Supervisor 的引用计数、`host.updated.power_assertion` | 新增 IT-power-assertion | S | 否 | H.sys、H.core |
| T157 | 安全边界说明（用户文档） | docs | 5.8.7、Q21、Q28、Q34 | T113 | 做：按 5.8.7 写进用户文档，含"系统权限会被 agent 继承"、Q28 和 Q34 的结论、自用构建的认证方式 | MAN-security-doc | S | 否（建议用户审阅） | Doc |
| T158 | （条件）主进程通知后备 | feat | 5.7.2（notifications）、Q2 | T099、T101 | 触发：T101 不通过。做：主进程在 control 连接上订阅索引流，取 `notification.intent` 弹通知（不放批准按钮），点击只导航；替代 T150 的宿主通知部分 | E2E-notification-navigate；新增 UNIT-main-notifications | M | 否 | D.main |
| T159 | （条件）SP-6 不达标项的优化 | perf | 5.9.7（判定与后续） | T103 | 触发：T103 有不达标项。做：按报告逐项优化，必要时拆成多个任务 | 对应的 PERF 用例达标 | 待定 | 否 | T.ui |
| T160 | 自用构建：发布配置与本机安装 | build | 5.12.3（自用构建）、5.7.9、5.10.1 | T099 | 做：`electron-builder.release.yml`（extraFiles、fuses、asarUnpack、Info.plist、URL scheme）；自用构建脚本（Q35 定案前用开发构建的身份与退化认证，不含测试钩子，见 P-5）；本机安装与更新说明 | 新增 IT-selfuse-build（产物布局、无测试钩子、`--build-info`） | M | 否 | D.main、H.xt |
| T161 | release-config 每晚构建 | ci | 5.1.6（release-config）、5.10.7 | T160 | 做：不签名的发布配置构建：fuses 逐项、`ElectronAsarIntegrity`、asar 清单、未知开关即退出、测试钩子标记扫描、`--build-info`、`runtime-pack --verify` | E2E-SEC-switches；作业首次通过 | M | 否 | C、D.main |
| T162 | （条件）本机签名构建种类 | feat | 5.13 Q35、5.8.1、5.7.9、ADR-05 | T105、T160 | 触发：T105 通过。前提：编排者补设计和 ADR-05（用户确认）。做：本机构建配置注入签名要求（不进仓库），宿主、原生扩展、CLI 按它校验，自用构建走 `codesign` 认证 | 新增 MAN-local-signed（UI 被杀后新 UI 接上旧宿主、CLI 只读接入、`weak_auth` 为 false） | L | 是：需要作者本机的签名身份 | H.sys、D.nat、D.main、H.xt |
| T170 | M1b 端到端：S1 与 S2 | test | 5.9.4（M1b 的端到端） | T144、T150 | 做：用假 agent 跑 S1（claude 与 codex 两种 persona 各一个会话，各在一个 worktree，侧边栏状态）、S2（审批、提问、通知意图的记录） | E2E-S1、E2E-S2 | M | 否 | D.main、T.ui |
| **T171** | M1b 端到端：S3 与 S4 | test | 5.9.4（M1b 的端到端） | T131、T140 | 做：用假 agent 跑 S3（审查、行内评论、按 hunk 暂存、提交）、S4（排队、Steer、中断）；Q29 不通过时 S3 走 T139 的视图 | E2E-S3、E2E-S4 | M | 否 | D.main、T.ui |
| T172 | M1b 端到端：NFR-2、NFR-9 与容量 | test | 5.9.4（NFR-2、NFR-9）、NFR-11 | T111、T121、T124 | 做：断线后同一 `command_id` 重发不重复执行、适配器和 CLI 崩溃后恢复、6 个会话同时运行；复核故障点全集 | E2E-NFR2-retry、E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash、E2E-six-sessions、IT-NFR9-failpoints（全集） | M | 否 | D.main |
| **T173** | S1–S4 逐步检查表 | docs | 5.12.3（退出标准 ①） | T170、T171 | 做：写 MAN-S1 至 MAN-S4 的检查表：步骤、通过条件、CLI 与适配器版本、日期 | 检查表经编排者审阅 | S | 否 | Doc |
| T174 | Q16 保留参数复核 | chore | 5.13 Q16 | T160 | 做：按宿主日志里的订阅模式和游标落后量调整窗口和保留参数，补边界性质测试 | 新增 PROP-retention-boundary | S | 是：需要作者自用一段时间后的本机日志 | H.store |
| **T179** | M1b 退出验收 | chore | 5.12.3（退出标准） | T154、T157、T161、T172、T173、T174 | 做：作者用自用构建、两种 agent 按检查表走完 S1–S4 并存档；核对 NFR-2、NFR-9 的用例和 `pending-ids` 的 M1b 条目 | MAN-S1、MAN-S2、MAN-S3、MAN-S4；M1b 同步会纪要 | S | 是：作者亲自走 S1–S4（真实 CLI，消耗额度） | C |

**注**

- **可以提前的任务**（T101、T102、T104、T105、T110、T114）见 1.7。
- **T101**：只验证 helper 的通知身份；TCC 归属与 disclaim（MAN-tcc）留在 M2 的 SP-10。
- **T102**：作者机器上的 Codex 走自定义网关，常因过载重连（SP-1），回合偏慢，结果"不确定"时重跑。`~/.codex/config.toml` 的处理见 P-14。
- **T105**：两种签名身份都要试，结论分别写；证书摘要、Team ID、钥匙串里的证书名只出现在本机配置里，汇报只写脱敏后的描述。
- **T110（L）**：同 T053。
- **T111（L）**：发现、运行时复制和回收都作用于 Codex 的 AgentRuntime，IT-K20-reclaim 需要完整路径。
- **T120（L）**：四种范围共用 `diff_token` 与 `DiffCache`。
- **T121**：Q32 的仓库档写操作逐键确认、LFS 实测都在这里做。
- **T124（L）**：worktree 添加、`session/create` 的 worktree 分支和崩溃恢复是同一个准备流程。
- **T125（L）**：7 条规则的结论直接决定归档和恢复的行为。
- **T127、T147**：派发前编排者先补设计（P-6）。
- **T130（L）**：面板各部分共用同一个 `diff_token` 和选择状态；右侧标签容器来自 T152。
- **T131、T139（L）**：审查文档是一个完整的独立文档加消息协议，后备方案是整体替代，两者只做其一。
- **T140（L）**：宿主、驱动、CLI 和界面要同时出现，否则宿主侧就是没人用的代码。IT-K11 用 codex 形态，所以排在 T111 之后。
- **T141（L）**：`session/set_config` 的宿主侧与选择器同时出现才有使用方。
- **T142、T145**：同在输入框组件里，T145 排在 T142 之后；**T143、T146** 同在消息流里，T146 排在 T143 之后。
- **T150（L）**：通知身份、点击回调和激活是 SP-10 验证过的同一条路径，拆开无法端到端验证。SP-10 不通过时由 T158 替代宿主通知部分。
- **T152（L）**：布局和主题 token 一起改动全部外框组件，分开会让同一批组件改两遍。排在 SP-6 之后（[5.12.1](design/12-milestones.md#5121-划分原则)）。
- **T154**：放在最后，两种语言的截图基线要覆盖全部界面。
- **T157**：v1.1 仍写"发布前必须完成"；本计划排在 M1b 退出前，见 P-15。
- **T160**：自用构建的种类见 P-5；Q35 通过后由 T162 换成本机签名的身份。
- **T162（L）**：签名要求要同时改宿主、原生扩展、CLI 和构建脚本，缺任何一端都连不上。
- **T170、T171**：SP-10 不通过时 T170 改依赖 T158；Q29 不通过时 T171 改依赖 T139。
- **T179**：作者用 T160 的自用构建走 S1–S4；Q35 通过时建议先完成 T162。

### 2.3 发布（推迟）

**前提：用户开通 Apple Developer Program 账号**（2026-10-16 用户决定暂不开通，先自用，见 [用户决定的补充](meetings/2026-10-16-gate-design-decisions/minutes.md#补充apple-developer-账号2026-10-16)）。派发时间标"账号到位后"；不计入 M1b 的退出标准，不在 M1b 关键路径上。

| 编号 | 名称 | 类型 | 设计依据 | 前置 | 做什么 / 不做什么 | 验收（用例编号） | 规模 | 用户参与 | 区域 |
|---|---|---|---|---|---|---|---|---|---|
| T180 | Developer ID 签名与公证 | build | 5.10.2、U-R3 | T161；账号到位 | 前提：账号到位。做：宿主、CLI、Node、App 与 helper 的 Developer ID 签名、entitlements 白名单、公证与 staple（App、DMG）、Team ID 写进签名要求 | MAN-node-notarize；新增 SMOKE-SIGNED-codesign | L | 是：Developer ID 证书、App Store Connect API key | H.xt、D.main、C |
| T181 | 发布流水线、签名后冒烟与 RC | ci | 5.10.7 | T180 | 做：`release.yml`（受保护 tag 与 environment、版本一致、macOS 14 最近一次通过、逐个二进制构建、`--self-test`、产物检查、签名后冒烟、草稿上传）、RC 演练 | SMOKE-SIGNED-*（含 SMOKE-SIGNED-reattach） | L | 是：GitHub 受保护环境与密钥 | C |
| T182 | 许可证门禁第二层与第三方声明 | ci | 5.10.7（第二层）、ADR-11 | T161 | 做：最终产物的内容指纹扫描（锁文件按哈希放行）与负面样例、THIRD_PARTY_NOTICES 生成 | 新增 UNIT-license-gate-l2、UNIT-third-party-notices | M | 否 | C |
| T183 | 自动更新与升级流程 | feat | 5.10.5、5.5.7、Q20 | T181 | 做：electron-updater 与 `updater`、`host/prepare_upgrade`、安装空窗、安装位置检查、`before-quit-for-update`；升级演练 | MAN-upgrade；新增 IT-prepare-upgrade | L | 是：升级演练需要两个签名版本 | D.main、H.core |
| T184 | 首次正式发布 | chore | 5.10.7、ADR-11、PRD 第 8 节 | T157、T182、T183 | 做：确认 bundle id 前缀（Q5 的最后机会）、大范围公开前确认政策边界、发布草稿 | 发布记录 | S | 是：用户决定发布与公开范围 | C |

**注**

- **T180（L）**：签名和公证必须一次覆盖全部 Mach-O，否则公证不通过。
- **T181（L）**：流水线的每一步以前一步的产物为输入，签名后冒烟要完整的签名产物。
- **T182**：技术上不需要账号，按 v1.1 归入本组；如果想在作者日常使用前就核对第三方声明，可以提前（R11）。
- **T183（L）**：更新器、`host/prepare_upgrade` 与安装空窗是一条升级路径，MAN-upgrade 要整条演练。在此之前 `host/prepare_upgrade` 按 1.2 返回 `unsupported`。

## 3. 依赖图

实线是依赖，虚线连着条件任务；红色粗边框是关键链上的任务；灰色圆角框是其他里程碑里的前置任务。只画直接依赖。

### 3.1 M1a

```mermaid
flowchart LR
  subgraph G0["工程、工具与技术验证"]
    T001["T001<br/>工程骨架"]
    T002["T002<br/>trace-ids"]
    T003["T003<br/>技术验证 Q8/Q9"]
    T004["T004<br/>runtime-pack"]
    T005["T005<br/>host-bundle"]
    T006["T006<br/>许可证门禁 L1"]
  end
  subgraph G1["协议"]
    T010["T010<br/>协议基础"]
    T011["T011<br/>事件目录"]
    T012["T012<br/>方法 I"]
    T013["T013<br/>方法 II"]
    T014["T014<br/>视图与同步"]
    T015["T015<br/>schema 与 TS 生成"]
    T016["T016<br/>契约冻结"]
  end
  subgraph G2["系统接口、核心与存储"]
    T020["T020<br/>sys 进程"]
    T021["T021<br/>sys 对端校验"]
    T022["T022<br/>时钟与纯核心"]
    T023["T023<br/>启动契约与注入"]
    T024["T024<br/>写线程"]
    T025["T025<br/>表结构与迁移"]
    T026["T026<br/>流与回合快照"]
    T027["T027<br/>blob 与原始帧"]
    T028["T028<br/>日志与脱敏"]
  end
  subgraph G3["网关与宿主服务"]
    T030["T030<br/>网关 I 握手认证"]
    T031["T031<br/>策略"]
    T032["T032<br/>命令账本"]
    T033["T033<br/>网关 II 分发订阅"]
    T034["T034<br/>索引 actor"]
    T035["T035<br/>信任扫描"]
    T036["T036<br/>设置"]
    T037["T037<br/>进程监护"]
    T038["T038<br/>CLI 提权"]
  end
  subgraph G4["会话编排与 CLI"]
    T040["T040<br/>回合状态机与队列"]
    T041["T041<br/>自发回合与合并"]
    T042["T042<br/>交互代理"]
    T043["T043<br/>会话 actor I"]
    T044["T044<br/>会话 actor II"]
    T045["T045<br/>CLI 库"]
    T046["T046<br/>CLI 命令"]
    T047["T047<br/>测试接口与录制导出"]
  end
  subgraph G5["驱动与 agent"]
    T050["T050<br/>传输与分接"]
    T051["T051<br/>假 agent"]
    T052["T052<br/>录制与回放"]
    T053["T053<br/>Claude 提取层与 A 层"]
    T054["T054<br/>CLI 发现"]
    T055["T055<br/>AgentRuntime"]
    T056["T056<br/>运行时安装器"]
    T057["T057<br/>Q8 后备（条件）"]
  end
  subgraph G6["Git、集成测试与兼容"]
    T060["T060<br/>GitCmd 与仓库档"]
    T061["T061<br/>快照档"]
    T062["T062<br/>集成测试 I"]
    T063["T063<br/>集成测试 II"]
    T064["T064<br/>兼容套件框架"]
    T065["T065<br/>B 层运行器"]
    T066["T066<br/>首批真实录制"]
    T067["T067<br/>宿主性能"]
  end
  subgraph G7["桌面"]
    T070["T070<br/>shell-api 与 i18n"]
    T071["T071<br/>host-client I"]
    T072["T072<br/>host-client II 视图"]
    T073["T073<br/>原生扩展"]
    T074["T074<br/>主进程 I 骨架"]
    T075["T075<br/>主进程 II 拉起与连接"]
    T076["T076<br/>windowBridge"]
    T077["T077<br/>真实输入与特权"]
    T078["T078<br/>测量闸门"]
    T079["T079<br/>utilityProcess 后备（条件）"]
  end
  subgraph G8["界面"]
    T080["T080<br/>ui-core 骨架"]
    T081["T081<br/>侧边栏与新建会话"]
    T082["T082<br/>消息流"]
    T083["T083<br/>卡片与输入框"]
  end
  subgraph G9["e2e 与退出"]
    T090["T090<br/>e2e 基础设施"]
    T091["T091<br/>NFR-1 套件"]
    T092["T092<br/>安全 e2e"]
    T093["T093<br/>交互 e2e"]
    T099["T099<br/>M1a 退出"]
  end
  T001 --> T002
  T001 --> T004
  T004 --> T005
  T023 --> T005
  T045 --> T005
  T004 --> T006
  T074 --> T006
  T001 --> T010
  T010 --> T011
  T011 --> T012
  T011 --> T013
  T011 --> T014
  T012 --> T015
  T013 --> T015
  T014 --> T015
  T072 --> T016
  T077 --> T016
  T078 --> T016
  T001 --> T020
  T001 --> T021
  T001 --> T022
  T022 --> T023
  T010 --> T024
  T022 --> T024
  T023 --> T025
  T024 --> T025
  T014 --> T026
  T025 --> T026
  T025 --> T027
  T022 --> T028
  T012 --> T030
  T021 --> T030
  T025 --> T030
  T012 --> T031
  T013 --> T031
  T022 --> T031
  T025 --> T032
  T026 --> T033
  T027 --> T033
  T030 --> T033
  T031 --> T033
  T032 --> T033
  T045 --> T033
  T033 --> T034
  T035 --> T034
  T060 --> T034
  T011 --> T035
  T022 --> T035
  T033 --> T036
  T020 --> T037
  T026 --> T037
  T027 --> T037
  T030 --> T037
  T034 --> T038
  T055 --> T038
  T011 --> T040
  T022 --> T040
  T040 --> T041
  T031 --> T042
  T033 --> T043
  T041 --> T043
  T042 --> T043
  T035 --> T044
  T043 --> T044
  T012 --> T045
  T014 --> T045
  T020 --> T045
  T038 --> T046
  T034 --> T047
  T043 --> T047
  T052 --> T047
  T003 --> T050
  T022 --> T050
  T001 --> T051
  T002 --> T052
  T051 --> T052
  T040 --> T053
  T050 --> T053
  T052 --> T053
  T020 --> T054
  T022 --> T054
  T027 --> T055
  T028 --> T055
  T053 --> T055
  T054 --> T055
  T004 --> T056
  T020 --> T056
  T025 --> T056
  T055 -.-> T057
  T054 --> T060
  T043 --> T061
  T060 --> T061
  T044 --> T062
  T047 --> T062
  T055 --> T062
  T044 --> T063
  T047 --> T063
  T055 --> T063
  T061 --> T063
  T044 --> T064
  T047 --> T064
  T055 --> T064
  T056 --> T064
  T044 --> T065
  T047 --> T065
  T064 --> T066
  T065 --> T066
  T037 --> T067
  T047 --> T067
  T001 --> T070
  T015 --> T071
  T070 --> T071
  T071 --> T072
  T020 --> T073
  T021 --> T073
  T023 --> T073
  T070 --> T074
  T005 --> T075
  T071 --> T075
  T073 --> T075
  T074 --> T075
  T075 --> T076
  T076 --> T077
  T047 --> T078
  T055 --> T078
  T076 --> T078
  T078 -.-> T079
  T072 --> T080
  T080 --> T081
  T080 --> T082
  T080 --> T083
  T046 --> T090
  T077 --> T090
  T081 --> T090
  T037 --> T091
  T044 --> T091
  T082 --> T091
  T083 --> T091
  T090 --> T091
  T083 --> T092
  T090 --> T092
  T083 --> T093
  T090 --> T093
  T006 --> T099
  T016 --> T099
  T062 --> T099
  T063 --> T099
  T066 --> T099
  T067 --> T099
  T091 --> T099
  T092 --> T099
  T093 --> T099
  classDef crit stroke:#d62728,stroke-width:3px
  classDef cond stroke-dasharray:5 5
  classDef prior fill:#eeeeee,stroke:#999999
  class T001,T010,T011,T012,T013,T014,T015,T026,T030,T033,T034,T038,T043,T044,T046,T064,T066,T071,T075,T076,T077,T090,T091,T092,T093,T099 crit
  class T057,T079 cond
```

### 3.2 M1b

M1b 的任务（可以提前的除外）都隐含依赖 T099，图中只在没有其他前置时画出这条边。

```mermaid
flowchart LR
  subgraph H0["检查点与技术验证"]
    T101["T101<br/>SP-10 通知"]
    T102["T102<br/>技术验证 Q8 Codex"]
    T103["T103<br/>SP-6"]
    T104["T104<br/>Q29 审查文档"]
    T105["T105<br/>Q35 本机签名"]
  end
  subgraph H1["Codex"]
    T110["T110<br/>Codex 提取层"]
    T111["T111<br/>Codex 接入"]
    T112["T112<br/>Codex 集成测试"]
    T113["T113<br/>Codex 兼容与补录"]
    T114["T114<br/>codex-drift"]
    T119["T119<br/>Q8 后备 Codex（条件）"]
  end
  subgraph H2["Git、worktree 与文件"]
    T120["T120<br/>diff 与 status"]
    T121["T121<br/>暂存与还原"]
    T122["T122<br/>82 个 hunk 用例"]
    T123["T123<br/>提交与推送"]
    T124["T124<br/>worktree 创建"]
    T125["T125<br/>归档与恢复"]
    T126["T126<br/>文件搜索与打开"]
    T127["T127<br/>存储与清除快照"]
  end
  subgraph H3["审查面板"]
    T130["T130<br/>审查面板"]
    T131["T131<br/>审查文档 iframe"]
    T139["T139<br/>DiffHunksRenderer（条件）"]
  end
  subgraph H4["输入框、消息流与设置页"]
    T140["T140<br/>Steer"]
    T141["T141<br/>配置选择"]
    T142["T142<br/>@ 与拖入"]
    T143["T143<br/>高亮、计划与指引"]
    T144["T144<br/>新建会话页"]
    T145["T145<br/>图片、斜杠与发送键"]
    T146["T146<br/>折叠与晚到输出"]
    T147["T147<br/>设置页 II"]
  end
  subgraph H5["通知、打磨与其他"]
    T150["T150<br/>系统通知"]
    T151["T151<br/>设置页 I"]
    T152["T152<br/>三栏与主题"]
    T153["T153<br/>会话管理与快捷键"]
    T154["T154<br/>中英文"]
    T155["T155<br/>性能回归"]
    T156["T156<br/>防休眠"]
    T157["T157<br/>安全边界说明"]
    T158["T158<br/>主进程通知（条件）"]
    T159["T159<br/>SP-6 优化（条件）"]
  end
  subgraph H6["自用构建"]
    T160["T160<br/>自用构建"]
    T161["T161<br/>release-config"]
    T162["T162<br/>本机签名构建（条件）"]
  end
  subgraph H7["验收"]
    T170["T170<br/>S1–S2 e2e"]
    T171["T171<br/>S3–S4 e2e"]
    T172["T172<br/>NFR-2/9 e2e"]
    T173["T173<br/>S1–S4 检查表"]
    T174["T174<br/>Q16 复核"]
    T179["T179<br/>M1b 退出"]
  end
  T004(["T004 runtime-pack"])
  T005(["T005 host-bundle"])
  T053(["T053 Claude 提取层与 A 层"])
  T073(["T073 原生扩展"])
  T074(["T074 主进程 I 骨架"])
  T099(["T099 M1a 退出"])
  T099 --> T103
  T074 --> T104
  T005 --> T105
  T073 --> T105
  T053 --> T110
  T102 --> T110
  T099 --> T111
  T110 --> T111
  T111 --> T112
  T112 --> T113
  T004 --> T114
  T111 -.-> T119
  T099 --> T120
  T120 --> T121
  T121 --> T122
  T120 --> T123
  T099 --> T124
  T120 --> T125
  T124 --> T125
  T099 --> T126
  T099 --> T127
  T121 --> T130
  T123 --> T130
  T152 --> T130
  T104 --> T131
  T130 --> T131
  T104 -.-> T139
  T130 -.-> T139
  T111 --> T140
  T099 --> T141
  T126 --> T142
  T099 --> T143
  T111 --> T144
  T124 --> T144
  T141 --> T144
  T142 --> T145
  T143 --> T146
  T127 --> T147
  T151 --> T147
  T099 --> T150
  T101 --> T150
  T111 --> T151
  T125 --> T151
  T103 --> T152
  T125 --> T153
  T126 --> T153
  T131 --> T154
  T140 --> T154
  T144 --> T154
  T145 --> T154
  T146 --> T154
  T147 --> T154
  T150 --> T154
  T153 --> T154
  T103 --> T155
  T099 --> T156
  T113 --> T157
  T099 -.-> T158
  T101 -.-> T158
  T103 -.-> T159
  T099 --> T160
  T160 --> T161
  T105 -.-> T162
  T160 -.-> T162
  T144 --> T170
  T150 --> T170
  T131 --> T171
  T140 --> T171
  T111 --> T172
  T121 --> T172
  T124 --> T172
  T170 --> T173
  T171 --> T173
  T160 --> T174
  T154 --> T179
  T157 --> T179
  T161 --> T179
  T172 --> T179
  T173 --> T179
  T174 --> T179
  classDef crit stroke:#d62728,stroke-width:3px
  classDef cond stroke-dasharray:5 5
  classDef prior fill:#eeeeee,stroke:#999999
  class T103,T120,T121,T123,T130,T131,T152,T171,T173,T179 crit
  class T119,T139,T158,T159,T162 cond
  class T004,T005,T053,T073,T074,T099 prior
```

### 3.3 发布（推迟）

```mermaid
flowchart LR
  subgraph R0["发布（推迟，前提：用户开通账号）"]
    T180["T180<br/>Developer ID"]
    T181["T181<br/>发布流水线"]
    T182["T182<br/>许可证门禁 L2"]
    T183["T183<br/>自动更新"]
    T184["T184<br/>首次发布"]
  end
  T157(["T157 安全边界说明"])
  T161(["T161 release-config"])
  T161 --> T180
  T180 --> T181
  T161 --> T182
  T181 --> T183
  T157 --> T184
  T182 --> T184
  T183 --> T184
  classDef crit stroke:#d62728,stroke-width:3px
  classDef cond stroke-dasharray:5 5
  classDef prior fill:#eeeeee,stroke:#999999
  class T180,T181,T183,T184 crit
  class T157,T161 prior
```

### 3.4 关键路径

| 范围 | 路径 | 加权长度 |
|---|---|---|
| M1a | T001 → T010 → T011 → T014 → T026 → T033 → T043 → T044 → T064 → T066 → T099 | 27 |
| M1b（T099 之后） | T099 → T120 → T121 → T130 → T131 → T171 → T173 → T179 | 15 |
| 发布（推迟，T161 之后） | T161 → T180 → T181 → T183 → T184 | 10 |

M1a 里另外两条零松弛的链（桌面链、网关链）见第 0 节。关键链上的任务优先派发、优先评审；同一波里的其他任务用空档。

## 4. 派发波次与检查点

波次按依赖算出：同一波的任务前置都已在更早的波次完成，彼此可以并行（同区域的仍按 1.5 处理）。加粗的在关键链上。

### 4.1 M1a

| 波次 | 任务（可并行） | 说明 |
|---|---|---|
| W0 | **T001**、T003 | T001 与 T003 同时开始；T003 的结论是 K1 |
| W1 | T002、T004、**T010**、T020、T021、T022、T051、T070 | 协议、系统接口、宿主核心、假 agent、TS 库同时起步 |
| W2 | **T011**、T023、T024、T028、T050、T052、T054、T074 | T050 按 K1 的 Q9 结论实现扩展请求 |
| W3 | T006、**T012**、**T013**、**T014**、T025、T035、T040、T060、T073 | 纯核心（T040）与信任扫描（T035）不依赖存储，先做 |
| W4 | **T015**、**T026**、T027、**T030**、T031、T032、T041、T045、T053、T056 | T053 起 A 层金样例进入 `rust` 作业 |
| W5 | T005、**T033**、T037、T042、T055、**T071** | T055 合并后假 agent 可以经驱动跑通；K1 的 Q8 不通过时在此之后派 T057 |
| W6 | **T034**、T036、**T043**、T072、**T075** | T043 用进程内驱动替身，不等真实驱动的集成 |
| W7 | **T038**、**T044**、T047、T061、**T076**、T080 | T080 起界面任务，只依赖 ShellAPI 与 HostPort |
| W8 | **T046**、T062、T063、**T064**、T065、T067、**T077**、T078、T081、T082、T083 | T078 测量闸门（K2）；T077 含 MAN-keyboard |
| W9 | T016、**T066**、**T090** | T016 契约冻结（K3），T078 不达标时 T079 先于 T016；T066 用真实 Claude（第 6 节） |
| W10 | **T091**、**T092**、**T093** |  |
| W11 | **T099** | M1a 退出（K4）：20 次全绿 |
| 条件 | T057（K1 的 Q8 不通过，第 5 波后）、T079（K2 不通过，第 9 波） | 只在触发时派发 |

### 4.2 M1b

| 波次 | 任务（可并行） | 说明 |
|---|---|---|
| B0（M1a 期间可提前） | T101（M1a W0 起）、T102（M1a W0 起）、T104（M1a W3 起）、T105（M1a W6 起）、T110（M1a W5 起）、T114（M1a W2 起） | SP-10（K5）、Q8 Codex（K6）、Q35（K7）、Q29（K9）的结论都要在用到它们的任务之前出来 |
| B1 | **T103**、T111、**T120**、T124、T126、T127、T141、T143、T150、T156、T160 | T103 SP-6（K8）在 T099 的提交上测量，同波的界面任务不影响结果；T152 等它的结论 |
| B2 | T112、**T121**、**T123**、T125、T140、T142、T144、T146、**T152**、T155、T161、T174 |  |
| B3 | T113、T122、**T130**、T145、T151、T153、T170、T172 |  |
| B4 | **T131**、T147、T157 | T131 与 T139 只做其一（K9） |
| B5 | T154、**T171** |  |
| B6 | **T173** |  |
| B7 | **T179** | M1b 退出（K10） |
| 条件 | T119（K6 不通过）、T139（K9 不通过，代替 T131）、T158（K5 不通过，代替 T150 的宿主部分）、T159（K8 有不达标项）、T162（K7 通过） | 只在触发时派发 |

### 4.3 发布（推迟）

账号到位（K11）之后按下表派发。前置是 M1b 的 T161（T184 另需 T157）；这一组不影响 M1b 的退出。

| 波次 | 任务（可并行） | 说明 |
|---|---|---|
| R1 | **T180**、T182 | 前提：账号到位（K11） |
| R2 | **T181** |  |
| R3 | **T183** |  |
| R4 | **T184** |  |

### 4.4 检查点

| # | 检查点 | 位置 | 判定 | 通过 | 不通过时启用 | 受影响的任务 |
|---|---|---|---|---|---|---|
| K1 | 驱动层前的技术验证 | T003（第 0 波） | Q8：两个会话各 3 个 prompt、5 分钟内全部返回、不串话、3 轮（5.9.5）；Q9：扩展消息能否经 SDK 收发 | 按原设计写 T050、T055 | Q8：T057（全局串行或每会话一个适配器进程）；Q9：分接层在 stdin 侧发送并关联响应（写进 T050，不另开任务） | T050、T055、T063（IT-six-sessions-shared-adapter 的期望）、NFR-11 的含义 |
| K2 | 测量闸门 | T078（第 8 波） | 参考机器上主进程事件循环延迟 p99 < 5 ms，并且没有流重置 | 维持 control + 每窗口数据连接 | T079（utilityProcess 宿主桥 + 委托票据），编排者修订 5.7.1、5.8.1 和 ADR-03，交用户确认 | T075、T076 的连接持有方；T030 加票据认证；协议附加两项；T090 的断连钩子；T016 推迟；ui-core 不受影响 |
| K3 | 契约冻结 | T016（第 9 波） | `schema-compat` 与 ShellAPI 快照接入 CI | 之后只能附加 | — | M1b 新增的方法与成员（T127、T142、T147）只能附加 |
| K4 | M1a 退出 | T099 | NFR-1 套件在同一提交上连续 20 次全绿；契约已冻结；M1a 的 pending 登记清空 | 进入 M1b | 查明根因、修复后计数清零 | — |
| K5 | SP-10 通知身份 | T101（可提前） | helper 能申请授权、弹通知、点击后激活主 App | T150 | T158（主进程通知后备） | T150、T170 |
| K6 | Q8（Codex） | T102（可提前） | 同 K1 | 按原设计写 T111 | T119 | T111、T172（E2E-six-sessions） |
| K7 | Q35 本机签名 | T105（可提前） | 宿主按本机签名要求认出 UI；hardened runtime 与库校验下各组件可运行；CLI 能接入 | T162，并补设计和 ADR-05 | 维持退化模式，T162 不派发 | 见 7.3 |
| K8 | SP-6 | T103（M1b 第 1 波） | 5.9.6 与 5.9.7 的预算 | T152、T155 | T159（规模待定）；冷启动或内存经应用层优化仍不达标时，按 ADR-03 的改变条件讨论 | T152、T155，最坏情况影响整个桌面端 |
| K9 | Q29 审查文档 | T104（可提前） | 5.9.7 场景 9 与 5.7.5 的 8 点 | T131 | T139（DiffHunksRenderer） | T131 或 T139、T171、E2E-SEC-review-iso |
| K10 | M1b 退出 | T179 | 5.12.3 的退出标准 ①②③；M1b 的 pending 登记清空 | 进入 M2 | — | — |
| K11 | 账号到位 | 用户 | 用户开通 Apple Developer Program | 派发 T180–T184 | 发布组继续悬空 | 发布组 |

## 5. 覆盖核对

### 5.1 5.12.2 M1a 的每一项

对照 [5.12.2](design/12-milestones.md#5122-m1a宿主骨架)。

| 部分 | 要点 | 任务 |
|---|---|---|
| 工程 | 仓库结构、两套工作区、工具链锁定 | T001 |
| | CI：`rust`（含金样例） | T001（作业）、T053（A 层）、T065（B 层） |
| | CI：`protocol` | T015、T072（视图交叉验证）、T016（兼容比对） |
| | CI：`ts` | T001、T015 |
| | CI：`ui` | T080 |
| | CI：`license`（第一层） | T001（`cargo deny`）、T006 |
| | CI：`testdata-scan` | T002、T052、T064 |
| | CI：`e2e` | T090 |
| | CI：`security-macos14` | T021 |
| | CI：`compat-framework` | T064 |
| 核心骨架与测试基础设施 | `Clock`、`IdGen`、`disallowed-methods`、纯核心框架 | T022（clippy 配置在 T001） |
| | `AcpTransport`（stdio 和内存） | T050 |
| | 故障点、`--test-config` 与 stdin 载荷、`--build-info` | T023 |
| | 写线程的组提交 | T024 |
| 技术验证 | Q8（Claude）、Q9、SP-4b 遗留第 7 条 | T003 |
| | Q26 | T077 |
| | Q27 | T092 |
| `cupper-protocol` | v1 的全部类型（含 M1b 的方法） | T010–T014 |
| | schema 导出、注解、视图的序列化格式 | T010（注解机制）、T014（视图格式）、T015（导出） |
| | TS 生成链及后处理 | T015 |
| | 合并时冻结 | T016（时机见 P-1） |
| `cupper-sys` | `spawn`、进程组、libproc、`kern.bootsessionuuid`、`DARWIN_USER_TEMP_DIR` | T020 |
| | `LOCAL_PEERTOKEN`、`LOCAL_PEERPID`、SecCode、csops | T021 |
| 宿主 | 网关（握手、只接受一次的 `initialize`、三种认证与双向证明、角色） | T030、T033 |
| | 策略与账本（档位、真实输入证据、信任门控） | T031、T032 |
| | 事件存储（组提交、迁移与备份、检查点、回合快照、blob、原始帧、持久性、`data_epoch`） | T024、T025、T026、T027 |
| | 会话编排（纯核心、排队、中断兜底、自发回合与出队保护、崩溃后 resume、第一条消息落盘、准备阶段恢复、晚到输出、`config_unconfirmed`） | T040、T041、T043、T044 |
| | 项目信任（扫描、摘要、`settingSources`、stale） | T035、T034（添加与信任）、T044（门控与 stale） |
| | 交互代理（权限、提问、计划确认、未知 kind） | T042、T043、T053（对照表） |
| | 关注度（状态、未读、在场、通知意图） | T034 |
| | 驱动层（只有 Claude） | T050、T053、T054、T055 |
| | git 的快照档（`GitCmd` 从第一次调用起分三档） | T060、T061 |
| | 运行时安装器 | T056（清单由 T004 产出） |
| | 进程监护（清理、自检、空闲退出、有序退出） | T037 |
| | 日志与脱敏 | T028 |
| `cupper-cli` | NFR-1 需要的全部命令、`elevate`、`repl`、`--json`、`--build-info`、`--harness-key-fd` | T045、T046（宿主侧提权 T038） |
| 假 agent | 启动配置、剧本命令、原始行通道、全局不变式 | T051 |
| | 回放模式、录制格式、SP-1 录制转换 | T052 |
| | A 层金样例 | T053 |
| | 第一批 B 层金样例 | T065、T066 |
| 桌面端（最简） | 主进程：经原生扩展拉起宿主、两类连接、校验宿主与双向证明 | T073、T075 |
| | 数据连接生命周期、出方向解析与白名单 | T076 |
| | 真实输入、原生确认框 | T077 |
| | 基本加固（sandbox、CSP 与 Trusted Types、`app://cupper`、开关白名单、导航限制） | T074、T080（Trusted Types 策略） |
| | windowBridge 的测量闸门 | T078（不达标：T079） |
| | ShellAPI（M1a 的完整接口，冻结） | T070、T076、T077、T016 |
| | 界面：侧边栏、新建会话、添加项目与信任确认、各类横幅、弱认证标记、重连状态、Toaster | T081 |
| | 界面：消息流（按行虚拟化、streamdown、工具摘要、思考、错误、提示、带状态的回合标题） | T082 |
| | 界面：审批与提问卡片、排队条、输入框 | T083 |
| | 文案走 i18n 的键 | T070、T080（ESLint 规则） |
| | 只有开发构建和 e2e 构建 | T074、T005 |
| 测试 | 宿主集成测试（故障点、`data_epoch`、6 个会话共用假适配器） | T062、T063、T033 |
| | 性质测试 | T014、T026、T040、T041、T042 |
| | NFR-1 的端到端验收套件 | T091 |
| | 安全 e2e（e2e 构建能跑的部分） | T092、T093 |
| | 发布 CSP 下 `securitypolicyviolation` 为 0 | T080（UI-csp-zero）、T092（E2E-SEC-csp） |
| 退出标准 | 20 次全绿；契约已冻结 | T099、T016 |

### 5.2 5.12.3 M1b 的每一项（v1.1）

对照 [5.12.3](design/12-milestones.md#5123-m1b桌面-mvp)。

| 部分 | 要点 | 任务 |
|---|---|---|
| SP-6 | 只测渲染；Q29 | T103、T104（后续 T155，必要时 T159） |
| SP-10 的通知部分 | AP-3 依赖它 | T101 |
| Codex | Q8 的 Codex 部分 | T102 |
| | `AgentRuntime`、提取层与对照表、`threadStatus`、晚到输出、未信任项目拒绝启动、资源回收 | T110、T111、T112（晚到输出的宿主逻辑在 T044） |
| | 兼容矩阵跑 Codex，重录取消场景补 K3 的 B 层 | T113 |
| Git 与 worktree | git 服务全部方法（三档、Q32、LFS） | T120、T121、T123（快照档在 T061） |
| | 新建 worktree（CP-5） | T124 |
| | 归档、恢复与清理（PS-3、GT-5） | T125、T153 |
| | 审查面板（GT-1 至 GT-4，含行内评论） | T130、T131 或 T139、T126（评论排成文本） |
| | "清除快照对象" | T127、T147 |
| 输入框 | `@` 文件搜索、拖入文件 | T142、T126 |
| | 图片粘贴、斜杠命令菜单、↑ 恢复、Enter/⌘Enter | T145、T126 |
| | 模型、推理强度、权限模式（档位规则、完全访问的原生确认） | T141、T144 |
| | Steer | T140 |
| 消息流 | 代码高亮、计划与待办、错误的恢复指引 | T143 |
| | 按回合折叠与耗时、晚到输出的显示 | T146 |
| 关注度与通知 | 系统通知、`ui/navigate` | T150（不通过：T158） |
| | 跳到下一个需要处理的会话 | T153 |
| agent 状态 | 设置页的 agent 状态与重启 | T151（`agents/restart` 在 T111） |
| 界面打磨 | Codex 式三栏布局、亮暗主题、视觉（UI-6） | T152 |
| | 设置页各项（含存储、安装命令行工具） | T147、T151 |
| | 快捷键、UI-0 的"打开" | T153（`workspace/open` 在 T126） |
| | 中英文 | T154 |
| 自用构建 | 作者日常使用的本机构建 | T160 |
| | `release-config` 每晚构建（fuses、调试开关、asar 清单、产物扫描） | T161 |
| | 许可证门禁第一层（每个 PR） | T006（M1a 起一直运行） |
| | Q35 的技术验证 | T105（通过：T162） |
| 发布（推迟） | 发布构建、Developer ID 签名、公证 | T180 |
| | 签名后冒烟、RC 标签 | T181 |
| | electron-updater（安装空窗、安装位置检查）、升级演练 | T183 |
| | 许可证门禁第二层、第三方声明 | T182 |
| 其他 | 防休眠（NFR-13） | T156 |
| | 安全边界说明（MAN-security-doc） | T157 |
| 退出标准 | ① MAN-S1 至 MAN-S4 | T173、T179 |
| | ② E2E-NFR2-retry、IT-NFR2-epoch | T172、T033 |
| | ③ E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash、IT-NFR9-failpoints、IT-NFR9-crash-under-load | T172、T063、T121、T124 |

### 5.3 5.12.4 追溯矩阵：每条 P0 需求的验证用例由谁产出

对照 [5.12.4](design/12-milestones.md#5124-追溯矩阵m1-的全部-p0-需求)。原先依赖"发布"的用例改指"发布（推迟）"组或 Q35 的结果（标 †）。

| 需求 | 验证用例 → 产出任务 |
|---|---|
| AG-1 | MAN-compat → T066、T113；`license` 第一层 → T006；许可证门禁第二层 → T182 † |
| AG-2 | IT-auth-none、IT-K14-old-cli-auth → T062；UNIT-cli-resolver → T054、T111 |
| AG-3 | E2E-new-session-agent → T093（Claude）、T144（两种） |
| AG-4 | UNIT-capabilities → T053；E2E-new-session-before-first-message → T144 |
| AG-5 | UNIT-version-classify → T054；`testdata-scan` 的范围比对 → T064 |
| PS-1 | 截图 UI-layout → T152 |
| PS-2 | E2E-project-add-trust → T093；IT-trust-stale、IT-trust-rescan-before-prompt、IT-trust-settingsources → T063；COMPAT-untrusted-project → T066、T113 |
| PS-3 | IT-session-archive-restore → T125；E2E-archive-cleanup → T153 |
| PS-4 | UNIT-index-status → T034；E2E-next-needs-action → T153 |
| PS-8 | E2E-NFR1-5 → T091；IT-history-from-host → T062 |
| CP-1 | UNIT-composer-ime → T083；E2E-send-key-setting → T145；MAN-ime → T103 |
| CP-2 | E2E-image-paste → T145；E2E-file-ref → T142 |
| CP-3 | E2E-slash-commands → T145 |
| CP-4 | UNIT-tier-rules → T031；IT-config-unconfirmed → T063；E2E-full-access-native → T141 |
| CP-5 | IT-worktree-create → T124；E2E-S1 → T170 |
| CP-6 | PROP-queue-fifo → T040；IT-K10、IT-K11 → T140；E2E-NFR1-2 → T091 |
| CP-7 | PROP-interrupt-10s → T040；IT-K1-close-resume → T062；E2E-NFR1-2 → T091 |
| MS-1 | 截图回归 → T082、T143；Markdown 注入金样例 → T082（UNIT-md-injection） |
| MS-2 | IT-thought-claude → T062；GOLD-A-codex-s02 → T110；COMPAT-claude-thinking → T066 |
| MS-3 | GOLD-A（各 `tool_kind` 的必填字段）→ T053、T110；UNIT-final-items、UNIT-grouping → T041；截图回归 → T082、T146 |
| MS-4 | GOLD-A-claude-s05 → T053；GOLD-A-codex-s05 → T110；IT-plan-entries → T062 |
| MS-5 | IT-K6 → T062；IT-K7、IT-K17 → T112；E2E-error-guidance → T143 |
| AP-1 | E2E-SEC-real-input → T092；E2E-approve-enter-esc → T093；MAN-keyboard → T077 |
| AP-2 | GOLD-A（选项对照表）、UNIT-option-map → T053、T110；E2E-persistent-option-native → T093 |
| AP-3 | IT-notification-intent → T034；MAN-notification → T101、T150 |
| AP-4 | E2E-NFR1-3 → T091 |
| AP-5 | IT-form、IT-plan-confirm → T062；E2E-question-card → T093 |
| GT-1 | IT-git-diff-scopes → T120；IT-git-snapshot-parity → T061；E2E-S3 → T171 |
| GT-2 | E2E-review-comment → T131（Q29 不通过时 T139） |
| GT-3 | IT-git-hunk → T122 |
| GT-4 | IT-git-commit-push → T123 |
| GT-5 | IT-worktree-check-rules → T125 |
| UI-0 | E2E-workspace-open → T153 |
| UI-2 | 截图回归 → T151、T147 |
| UI-3 | 截图回归 → T152 |
| UI-4 | E2E-shortcuts → T153 |
| UI-5 | 截图回归（两种语言）→ T154 |
| UI-6 | MAN-ui6 → T152 |
| SEC-6 | E2E-SEC-egress → T092；宿主的 Seatbelt 外发检查 → T063（IT-host-egress-seatbelt） |
| SEC-8 | UNIT 策略矩阵（含无特性运行）→ T031、T023；IT-process-hygiene → T055；E2E-SEC-* → T092（csp、real-input、privileged、fake-host、egress）、T131（review-iso）、T161（switches）；SMOKE-SIGNED-* → T181 †（Q35 通过时，T162 的 MAN-local-signed 在作者机器上覆盖其中的握手与重连部分） |
| SEC-10 | IT-interaction-expire → T062；UNIT-digest-jcs → T010 |
| NFR-1 | E2E-NFR1-1 至 E2E-NFR1-5 → T091；连续 20 次全绿 → T099 |
| NFR-2 | E2E-NFR2-retry → T172；IT-NFR2-epoch → T033（决策表在 T024） |
| NFR-4 | E2E-SEC-egress → T092；代码审查"宿主只访问 npm 仓库" → T056 的评审 |
| NFR-5 | MAN-compat → T066、T113；`testdata-scan` 的范围比对与取值检查 → T064、T053 |
| NFR-6 | PERF（5.9.6）→ T067、T078、T061（PERF-snapshot）、T155；SP-6 → T103 |
| NFR-7 | IT-degrade-* → T140 |
| NFR-8 | E2E-single-instance、E2E-quit-confirm → T090；SMOKE-SIGNED-reattach → T181 †，或 Q35 通过时 T162 的 MAN-local-signed †；MAN-upgrade → T183 † |
| NFR-9 | E2E-NFR9-adapter-crash、E2E-NFR9-cli-crash → T172；IT-NFR9-failpoints → T063（M1a 部分）、T121、T124（git 部分）、T172（全集复核）；IT-NFR9-crash-under-load → T063 |
| NFR-10 | IT-migrations → T025；IT-n-minus-1-cli → 第二个发布版起（不在本计划，登记为"发布（推迟）"之后） |
| NFR-11 | IT-six-sessions-shared-adapter → T063；E2E-six-sessions → T172 |
| NFR-12 | UNIT-redaction → T028 |
| 第 8 节合规 | MAN-compat 报告里的配置哈希对比 → T066、T113（T003、T102 也做） |

### 5.4 规避清单与其他编号

对照 [5.6.9](design/06-agent-driver.md#569-已知问题的规避清单) 与 [5.9.4](design/09-testing.md#594-端到端测试与-nfr-1-验收)、[5.9.10](design/09-testing.md#5910-测试编号与人工验证清单)。

| 来源 | 用例 → 任务 |
|---|---|
| K1 | PROP-turn-single-prompt → T040；IT-K1-close-resume → T062；假 agent 的全局不变式 → T051；GOLD-A-claude-s06 → T053；GOLD-A-codex-s06 → T110 |
| K2 | GOLD-A-codex-s04、GOLD-A-codex-s04b → T110；IT-K2-reject-ends-turn → T112 |
| K3 | GOLD-A-codex-s07 → T110；IT-K3-late-output → T112；PROP-turn-snapshot-immutable → T041；B 层 → T113（P-10） |
| K4 | IT-K4-cancel-pending-approval → T112 |
| K5 | GOLD-A-claude-s07 → T053；IT-K5-withdraw → T062 |
| K6 | GOLD-A-claude-s07b → T053；IT-K6-crash-resume、IT-K6-output-then-crash → T062 |
| K7 | IT-K7-restartable → T112 |
| K8 | GOLD-A-claude-s09 → T053；IT-history-from-host → T062 |
| K9 | IT-K9-autonomous → T062；PROP-autonomous-dequeue-guard → T041 |
| K10、K11 | IT-K10-steer-explicit-only、E2E-composer-steer-default、PROP-steer-only-prompting、IT-K11-steer-idle-queued → T140 |
| K12 | GOLD-A-codex-s05 → T110；IT-K12-plan-confirm-idle → T112；PROP-prompt-inflight-no-end → T040 |
| K13 | UNIT-realpath → T054；IT-K13-echo-cwd → T055 |
| K14、K15 | UNIT-version-classify、UNIT-cli-resolver → T054（Codex 样例 T111）；IT-K14-old-cli-auth → T062 |
| K16 | GOLD-A-claude-s01 → T053；UNIT-redact-auth-status → T028 |
| K17、K18 | GOLD-A-codex-s10x → T110；IT-K17-text-error、IT-K18-upstream-retry → T112 |
| K19 | GOLD-A-unknown-enums → T053；IT-K19-unknown-kind-privileged → T062 |
| K20、K21 | IT-K20-reclaim → T111；IT-K21-reap-group → T055 |
| K22、K24、K25 | GOLD-A-claude-s03、GOLD-A-claude-s02c、UNIT-capabilities-plan → T053；GOLD-A-codex-s05 → T110 |
| K23 | 不测（5.6.9） |
| K26 | IT-K26-client-info → T055 |
| 端到端 | E2E-S2、E2E-S4（5.9.4 列了场景但没有编号，本计划补上）→ T170、T171 |
| 人工清单 | MAN-S1 至 MAN-S4 → T179；MAN-keyboard → T077；MAN-ime → T103；MAN-notification → T101、T150；MAN-tcc → M2 的 SP-10（不在本计划）；MAN-ui6 → T152；MAN-upgrade → T183 †；MAN-node-notarize → T180 †；MAN-compat → T066、T113；MAN-security-doc → T157 |
| 其他 | 差分测试 → T072；GOLD-B → T065、T066、T113；兼容报告校验 → T064 |

### 5.5 5.13 待定问题由谁负责

对照 [5.13.1](design/13-open-issues.md#5131-待定问题)。只列"仍待定"的项。

| # | 何时定（5.13） | 负责任务 |
|---|---|---|
| Q2 | M1b 开始前 | T101（不通过：T158） |
| Q7 | 实现时；上游 | T053；向上游反馈由用户决定（第 6 节） |
| Q8 | M1a 驱动层前；M1b Codex 前 | T003、T102；复核 T066、T113；后备 T057、T119 |
| Q9 | M1a | T003；落实在 T050，Steer 的使用在 T140 |
| Q10 | M1a | T041；验证 T062（IT-K9-autonomous）、T066（真实录制） |
| Q16 | M1b | T026、T027（参数可覆盖）、T033（记录订阅模式）、T174（复核） |
| Q17 | M2 | 不在本计划；M1b 的清除入口在 T127、T147 |
| Q18 | M1a | T054 |
| Q19 | M1a（Claude）、M1b（Codex） | T054、T111 |
| Q20 | M1b 发布任务 | T183 † |
| Q21 | SP-10（M2 前） | 安全说明里的警示 T157；disclaim 评估在 M2 |
| Q23 | M1b | T123 |
| Q24 | M1a 起 | T021（每晚与按路径过滤）；发布前的检查 T181 † |
| Q25 | M1b | T102（初查）、T110（未知取值按 full_access）、T113（实测） |
| Q26 | M1a | T077 |
| Q27 | M1a | T092 |
| Q28 | M1a 兼容套件 | T064（剧本）、T066（结论）、T157（写进说明） |
| Q29 | M1b 开头 | T104（不通过：T139） |
| Q30 | 改用隔离模式之前 | 不在本计划；兼容套件按 U-R4 用真实配置，报告记录运行模式（T064） |
| Q31 | M1b Codex 提取层 | T102（初查）、T110 |
| Q32 | M1a（快照档）、M1b（仓库档写操作、LFS） | T061、T121 |
| Q33 | 闸门不达标时 | T079 |
| Q34 | M1a 兼容套件（Claude）、M1b（Codex） | T064、T066、T113；结论写进 T157；需要调整扫描范围时另开缺陷修复任务改 T035 |
| Q35 | M1b 技术验证 | T105（通过：T162） |
| SP-4b #7 | M1a 驱动任务前 | T003（Claude）、T102（Codex） |
| SP-4b #6 | 实现时 | T021（`SCM_RIGHTS` 被丢弃） |

### 5.6 契约冻结清单

对照 [5.12.5](design/12-milestones.md#5125-契约冻结清单)。

| 组 | 内容 | 任务 |
|---|---|---|
| 第 1 组 | ① `initialize` 与绑定信道的证明 | T012、T030、T073 |
| | ② `stream/subscribe` 的 `epoch` 与重置 | T012、T014、T033 |
| | ③ `history/page` 的游标 | T012、T026 |
| | ④ `item.delta` 的 `field` 与 `output_state` | T011、T041 |
| | ⑤ 完整摘要与 RFC 8785 | T010 |
| | ⑥ 要批准的内容不截断 | T011、T042 |
| | ⑦ 策略语义（档位、信任门控、输入证据、未知 kind） | T031、T044 |
| | ⑧ ShellAPI | T070、T076、T077 |
| | ⑨ 宿主启动契约 | T023 |
| 第 2 组 | 时间字段、`final_item_ids`、活动组、`ToolCall` 字段与必填表、晚到输出、信任与 `project_config`、`confirm_config`、快照与 diff 的字段、`Option.effect` 的新字段、错误 reason 与 notice code、`ui/navigate`、`host/status.pid`、schema 注解与视图格式 | T010、T011、T013、T014（实现分布在相应的宿主任务） |
| 第 3 组 | 审查文档、仓库档写与用户档、Codex 提取层、安装空窗与签名后冒烟与许可证第二层、`pathForFile` | T131、T121、T123、T110–T112、T181–T183 †、T142 |
| 机制 | `released/v1-m1a.json`、`schema-compat`、ShellAPI 快照 | T016 |

### 5.7 CI 作业

对照 [5.1.6](design/01-repo-build-ci.md#516-ci-流水线)。

| 作业 | 建立 | 扩充 |
|---|---|---|
| `rust` | T001 | T022、T023（无特性运行）、T053、T065（金样例） |
| `protocol` | T015 | T072、T016 |
| `ts` | T001 | T015、T070 |
| `ui` | T080 | 各界面任务 |
| `license` | T001 | T006；第二层 T182 † |
| `testdata-scan` | T002 | T052、T064 |
| `e2e` | T090 | 各端到端任务 |
| `release-config` | T161 | — |
| `security-macos14` | T021 | — |
| `compat-framework` | T064 | — |
| `codex-drift` | T114 | — |
| `release` | T181 † | — |
| 每晚的其他检查（差分测试、性能趋势） | T072、T067 | T078、T155、T061 |

## 6. 需要用户参与或提供的事项（按时间顺序）

| # | 时间点 | 事项 | 任务 | 用户要做什么 |
|---|---|---|---|---|
| 1 | 计划评审（`gate/plan`）时 | 确认任务顺序和粒度；决定 U-P1（GitHub 仓库：何时建、公开或私有、放在哪个账号下）、P-1（契约冻结的时机）、P-14（真实 Codex 运行改写 `~/.codex/config.toml` 时怎样处理） | — | 回复决定 |
| 2 | M1a 第 0 波 | Q8（Claude）并发剧本，消耗少量额度（U-R5 已许可） | T003 | 知悉运行时段 |
| 3 | T001 前后（按 U-P1 的决定） | 在 GitHub 建仓库并推送、开启 Actions、设置 main 的保护规则和必需检查 | T001 | 建仓库、配置 |
| 4 | M1a 第 8 波 | MAN-keyboard：用实体键盘确认 Enter 批准、Esc 拒绝、输入法选词不触发（约 10 分钟） | T077 | 亲自操作 |
| 5 | M1a 第 8 波 | 测量闸门在作者机器上弹窗运行约 20 分钟；不达标时 ADR-03 的修订要用户确认 | T078、T079 | 运行期间不用电脑做别的；必要时确认修订 |
| 6 | M1a 第 9 波 | 真实 Claude 的兼容套件（多个剧本，思考流剧本用 sonnet 或 opus，额度高于 T003） | T066 | 知悉额度；查看配置哈希报告 |
| 7 | T053 之后任意时间 | 是否、以谁的名义向 claude-agent-acp 反馈 Q7（`allow-with-updates` 区分不了作用范围）和 K19（封闭枚举） | T053 | 决定 |
| 8 | M1a 期间任意时间，T150 之前 | SP-10：helper 申请通知授权（会弹系统授权框），观察通知和点击跳转 | T101 | 在场点"允许"，确认现象 |
| 9 | M1a 期间任意时间，T111 之前 | Q8（Codex）并发剧本，消耗额度；可能改写 `~/.codex/config.toml` | T102 | 知悉额度；按 P-14 的决定处理改动 |
| 10 | T005、T073 合并后任意时间 | Q35：在钥匙串里建代码签名证书，或用免费 Apple ID 登录 Xcode；通过后设计与 ADR-05 的补充要用户确认 | T105、T162 | 在场操作；确认修订 |
| 11 | M1a 退出 | M1a 同步会，确认退出 | T099 | 参会确认 |
| 12 | M1b 开头 | SP-6：作者机器跑 8 小时浸泡；MAN-ime 用拼音在流式输出期间输入（约 15 分钟）；Q29 的测量也在作者机器上 | T103、T104 | 让机器空出 8 小时；亲自输入 |
| 13 | M1b | 真实 Codex 的兼容套件，消耗额度 | T113 | 知悉额度；查看配置哈希报告 |
| 14 | M1b（可选） | Q23：用作者自己的 SSH agent、GPG 签名配置手动验证一次提交和推送 | T123 | 可选 |
| 15 | M1b | MAN-notification 在产品里复核一次 | T150 | 确认现象 |
| 16 | M1b | MAN-ui6：同尺寸窗口与 Codex 截图并排比对 | T152 | 比对并给意见 |
| 17 | M1b（建议） | 审阅安全边界说明 | T157 | 审阅 |
| 18 | M1b 后段 | 作者自用一段时间后，授权子 agent 读取本机日志 `~/Library/Logs/cupper`，用于 Q16 复核 | T174 | 授权 |
| 19 | M1b 退出 | 作者用自用构建、两种 agent 按检查表走完 S1–S4（真实额度，约半天）；M1b 同步会 | T179 | 亲自完成并参会 |
| 20 | 用户开通 Apple Developer Program 之后 | Developer ID Application 证书、App Store Connect API key、Team ID；GitHub 受保护环境与密钥；确认 bundle id 前缀（Q5 的最后机会）；MAN-node-notarize、MAN-upgrade 的演练；大范围公开前确认政策边界（ADR-11 第 5 节、PRD 第 8 节） | T180–T184 | 提供证书与凭据、配置、决定发布 |

## 7. 风险与应对

### 7.1 关键路径与大任务

| 风险 | 应对 |
|---|---|
| R1 M1a 有三条几乎等长的关键链（宿主核心链、桌面链、网关链），关键链上 L 任务占多数；任何一个 L 超期都会推迟 M1a | 关键链上的任务优先派发、优先评审；brief 写清接口和验收；T043 用进程内驱动替身，T055 直接驱动 `AgentRuntime`，减少互相等待；T076 用 TS mock 宿主做闸门的预测量，尽早暴露主进程转发的问题 |
| R2 L 任务实际超出 1,500 行 | 子 agent 超出时先汇报，按表后注里的拆分点拆；M1b 的 T182 等不在关键路径上的任务可以挪作缓冲 |
| R3 冻结前契约的反复修改波及 TS 生成、host-client 和界面 | 1.3 的规则：单独提交、同任务重新生成、汇报列出；评审时编排者逐条看契约差异 |
| R4 共享热点的合并冲突 | 1.5 的约定；编排者按波次合并，同一波内先合关键链上的任务 |
| R5 人工项集中在 M1b 退出前（MAN-S1–S4、MAN-ui6、T174 的日志） | T173 的检查表提前写好；第 6 节的时间点提前约 |

### 7.2 外部条件

| 风险 | 应对 |
|---|---|
| R6 GitHub 仓库未建（U-P1）：CI 跑不起来，`ui` 基线和"20 次全绿"做不了 | 计划评审时就定；在此之前用本机等价命令验收并存档输出（1.8） |
| R7 私有仓库的 macOS 分钟按 10 倍计费；公开仓库免费但仓库内容随之公开 | U-P1 一并决定；私有时可以先把 `e2e` 只放每晚、PR 只跑冒烟 |
| R8 真实 CLI 的额度和稳定性：Codex 走自定义网关、常过载（SP-1），结果可能"不确定" | 用最便宜的模型、限定剧本；"不确定"允许重跑，报告记下重试次数（5.9.5） |
| R9 `macos-14` 镜像下线（Q24），macOS 14 的对端校验没有地方跑 | 下线公告出来时找替代；在此之前 T021 的每晚作业照常 |
| R10 Electron 45 在 M1a 中途正式发布 | T003 复核，T074 定起始版本；M1a 中途不换大版本，M1b 开头按 5.10.6 跟进并做 MAN-keyboard |
| R11 Apple 账号推迟，发布组整体悬空 | 已按 v1.1 移出 M1b；T182 不需要账号，可以按需提前 |

### 7.3 检查点不通过时的返工范围

| 检查点 | 结果 | 返工范围 |
|---|---|---|
| K2 测量闸门（T078） | 不达标 | T079（L）；T075、T076 的数据连接改由 utilityProcess 持有；T030 加票据认证；协议附加 `auth/delegate` 和 `auth: ticket`；T090 的断连钩子改接；T016 推迟到 T079 之后；ADR-03 与 5.8.1 修订。ui-core（T080–T083）只依赖 ShellAPI 和 HostPort，不受影响 |
| K1 Q8（Claude，T003） | 不通过 | T057（M）；T055 的路由加串行闸门或改每会话一个进程；T063 的 IT-six-sessions-shared-adapter 改为断言串行下不串话；NFR-11 变成"6 个会话同时存在、prompt 轮流"；S1（两种 agent 各一个会话）不受影响 |
| K6 Q8（Codex，T102） | 不通过 | T119（S 或 M）；T111 的路由；T172 的 E2E-six-sessions 的期望 |
| K9 Q29（T104） | 不通过 | T139 替代 T131（同为 L）；E2E-SEC-review-iso 不再适用，审查视图改为主文档里没有 HTML 写入点的 React 渲染；T171 的 S3 走 T139 |
| K5 SP-10（T101） | 不通过 | T158 替代 T150 的宿主通知部分；宿主只写通知意图，通知只在 UI 运行时出现；T170 改依赖 T158 |
| K8 SP-6（T103） | 有不达标项 | T159（规模待定，按报告拆）；冷启动或内存经应用层优化仍不达标时，按 ADR-03 的改变条件讨论，最坏情况影响整个桌面端 |
| K7 Q35（T105） | 通过 | 派发 T162（L），之前由编排者补 5.7.9、5.8.1 和 ADR-05（用户确认）。受影响：T160（自用构建换身份与认证）、T045 与 T046（CLI 校验本机签名）、T073（签名要求来自本机配置）、T147（"安装命令行工具"在自用构建里可用）、T157（认证方式一节）；NFR-8 的"界面崩溃不影响会话"在自用构建上成立 |
| K7 Q35（T105） | 不通过 | 维持退化模式：自用构建里 CLI 连不上宿主，T147 隐藏"安装命令行工具"或注明原因；UI 崩溃后需要重启宿主（Q22 的局限扩展到作者的日常使用）；"本地鉴权较弱"标记常驻；T157 写明；T162 不派发 |

## 8. 待处理

设计里的遗漏或矛盾。本计划先按"建议"排，需编排者确认；涉及设计文字的由编排者修订设计，本任务不改设计。

| # | 问题 | 涉及 | 本计划的处理与建议 | 何时需要定 |
|---|---|---|---|---|
| P-1 | **契约冻结的时机自相矛盾**：5.12.5 写"M1a 协议 crate 的任务合并时冻结"；5.7.1 和 Q33 写"测量放在 M1a，后备方案引入的协议改动落在契约冻结之前"。测量闸门需要真实宿主、假 agent 和 windowBridge（T078，第 8 波），协议 crate 第 3–4 波就合并，两句不能同时成立 | 5.12.5、5.7.1、Q33 | 把 T016 排在 T078 之后：满足 5.7.1、Q33 和 5.12.2 的退出标准；协议合并到冻结之间，宿主、CLI、host-client 和界面会大量使用契约，这期间发现的问题还能改。Q33 的改动本身是附加式的，按 5.12.5 原文提前冻结也可行（T016 改为只依赖 T015、T070） | 计划评审时 |
| P-2 | **CI 的托管没有落地**：5.1.6 按 GitHub Actions 设计，M1a 退出要求"CI 的 macOS runner 上连续 20 次全绿"，`ui` 基线只由 CI 生成，但仓库没有远端；流程里也没写由谁推送分支触发 CI | 5.1.6、5.12.2、00-process | 用户决定 U-P1；编排者推送任务分支、看 CI 后再合并（1.8），建议写进 `00-process.md` | 计划评审时 |
| P-3 | **用例编号不规整**：5.12.4 混用完整编号与缩写（IT-K6 与 IT-K6-crash-resume；IT-K7、IT-K10、IT-K11、IT-K17）、通配（E2E-SEC-*、SMOKE-SIGNED-*、IT-degrade-*）、区间（MAN-S1 至 MAN-S4）和描述（"截图回归 UI-layout""UNIT 策略矩阵""GOLD-A（各 tool_kind 的必填字段）"）；5.9.4 的 S2、S4 没有编号；5.9.10 的层前缀里没有截图回归 | 5.9.10、5.12.4 | trace-ids（T002）的规则：缩写按前缀匹配完整编号；通配至少有一个匹配；区间展开；截图回归用 `UI-` 前缀；E2E-S2、E2E-S4 按本计划补上；本计划新增的编号也可以纳入抽取 | T002 派发前 |
| P-4 | **首个发布前表结构怎样变**：5.4.6 没说首个发布前的规则；并行任务各加一个 `NNNN` 脚本会撞号 | 5.4.6 | 首个自用构建（T160）交付之前，所有表结构变更直接改 `0001`（开发和测试数据可以重建）；T160 交付后每次变更加新脚本并保留样例数据库，因为作者开始有真实数据 | T025 派发前 |
| P-5 | **"自用构建"的种类没有定义**：5.7.9 只有发布、e2e、开发三种；v1.1 说 Q35 定案前自用构建"按开发构建的退化模式运行"，但开发构建不烧 fuses、带测试钩子，不适合日常使用 | 5.7.9、5.12.3（v1.1） | T160 假设：身份、数据目录、认证同开发构建（`dev.cupper.app.dev`、`cupper-dev/`、`dev_key`）；打包、fuses、"无测试钩子"同发布配置；宿主开 `dev-auth`、不开 `test-harness`。建议在 5.7.9 加"自用构建"一列（Q35 通过后再加"本机签名构建"），T161 也扫描自用构建的产物 | T160 派发前 |
| P-6 | **设置页的"存储"和"安装命令行工具"没有接口**：5.7.4、5.12.3 要求显示各类数据大小、"清除快照对象"、"安装命令行工具"，但 5.3.4 没有对应方法，5.7.3 的 ShellAPI 与 `PrivilegedAction` 也没有安装 CLI 的成员 | 5.3.4、5.7.3、5.7.4 | M1b 附加式加入（冻结后允许）。需补：方法名与 scope（清除快照对象会让旧回合的 diff 不可用，建议原生确认）；ShellAPI 成员（例如 `PrivilegedAction` 加 `install_cli`）；Q35 不通过时自用构建里怎样显示 | T127、T147 派发前 |
| P-7 | **Q8 后备方案的用例与"没人用的代码"冲突**：5.9.1 写"Q8 的后备方案也有用例"，若 Q8 对两种 agent 都通过，后备路径就是没人用的代码 | 5.9.1、CLAUDE.md | 做成条件任务 T057、T119，只在不通过时实现 | 计划评审时 |
| P-8 | **几项工作没有落到里程碑**：`xtask host-bundle`（e2e 构建需要）、`codex-drift` 作业、`tools/rec-shape-diff` 在 5.12 里都没有出现 | 5.12、5.1.6、5.9.5 | 分别放进 T005（M1a）、T114（M1b）、T064（M1a） | 计划评审时确认 |
| P-9 | **`--test-config` 的写法**：5.12.2、5.12.5 写成像命令行开关的 `--test-config`，5.8.1、5.2.16 写明经 stdin 的启动载荷传入 | 5.12.5 第 1 组第 9 条 | 以 stdin 载荷的 `test_config` 字段为准，不设 argv 开关（与"密钥不出现在 argv"一致） | T023 派发前 |
| P-10 | **晚到输出在 M1a 没有进程级用例**：5.12.2 把晚到输出放在 M1a，但 IT-K3-late-output 指定 codex 形态，Codex 提取层在 M1b | 5.12.2、5.6.9 | M1a 只有性质测试和进程内测试（T041、T044），进程级用例在 T112；如果希望 M1a 就有，可以在 T062 加一个 claude 形态的 `late_tool` 用例（新编号） | 计划评审时 |
| P-11 | **E2E-SEC-switches 的里程碑**：5.12.4 把 SEC-8 的 E2E-SEC-* 标为 M1a，但这一条只能在发布配置构建上跑，`release-config` 在 M1b | 5.12.4、5.9.4 | 放在 T161（M1b） | 计划评审时确认 |
| P-12 | **"M1a 早期"产出 B 层录制做不到**：5.9.3 写"M1a 早期由维护者用 Claude 跑一次兼容套件"，但兼容套件依赖完整的宿主、CLI 库和运行时安装器 | 5.9.3 | 真实录制放在 T066（第 9 波）；B 层运行器先用假 agent 经宿主产出的录制（T065） | 计划评审时确认 |
| P-13 | **v1.1 的"许可证门禁第一层"出现两次**：5.12.2 的 CI 一栏和 5.12.3 的自用构建一栏都有 | 5.12.2、5.12.3 | 只有一个任务 T006（M1a），M1b 只是继续每个 PR 运行 | — |
| P-14 | **真实 Codex 运行会改写 `~/.codex/config.toml`**：SP-1、SP-4b 都观察到；PRD 第 8 节要求不改写 agent 的配置，U-R4 只要求哈希对比并在报告里标出，没说发生后怎样处理 | U-R4、PRD 第 8 节 | 建议：运行前由用户自己备份一份（不入库），运行后报告列出差异，由用户决定保留还是恢复；子 agent 不自行改写用户配置 | T102 派发前 |
| P-15 | **MAN-security-doc 的归属**：v1.1 推迟发布后，5.12.3"其他"一栏仍写"发布前必须完成" | 5.12.3（v1.1） | T157 排在 M1b 退出前：作者日常使用前就应知道安全边界；按 5.9.10，M1b 退出时这一登记要清空 | 计划评审时确认 |

## 9. 进度

任务合并后由编排者打勾（[`00-process.md`](00-process.md) 的任务生命周期第 4 步）。条件任务在不需要时标"不派发"。

**M1a**

- [ ] T001 仓库与工程骨架
- [ ] T002 用例编号追踪 trace-ids
- [ ] T003 技术验证：Q8（Claude）、Q9、SP-4b #7 与版本复核
- [ ] T004 xtask runtime-pack 与运行时清单
- [ ] T005 xtask host-bundle
- [ ] T006 许可证门禁第一层
- [ ] T010 协议基础：信封、ID、错误码、规范 JSON 与注解
- [ ] T011 协议事件目录与领域类型
- [ ] T012 协议方法 I：握手、流、项目、agent、会话与交互
- [ ] T013 协议方法 II：git、文件、设置、认证、诊断与测试
- [ ] T014 协议视图与同步规则
- [ ] T015 schema 导出与 TS 生成链
- [ ] T016 契约冻结
- [ ] T020 cupper-sys I：进程与系统信息
- [ ] T021 cupper-sys II：对端校验与代码签名
- [ ] T022 宿主核心骨架 I：时钟与纯核心框架
- [ ] T023 宿主核心骨架 II：启动契约、注入点与故障点
- [ ] T024 存储 I：写线程、读池与持久性
- [ ] T025 存储 II：表结构、迁移与升级器
- [ ] T026 存储 III：流、检查点、回合快照与历史
- [ ] T027 存储 IV：blob、原始帧与保留清理
- [ ] T028 日志与脱敏
- [ ] T030 网关 I：连接、握手与本地认证
- [ ] T031 策略
- [ ] T032 命令账本
- [ ] T033 网关 II：分发、订阅、历史与 blob 传输
- [ ] T034 索引 actor 与关注度
- [ ] T035 项目信任扫描
- [ ] T036 设置
- [ ] T037 进程监护与宿主生命周期
- [ ] T038 CLI 提权（宿主侧）
- [ ] T040 会话纯核心 I：回合状态机与队列
- [ ] T041 会话纯核心 II：自发回合、合并器与最终回答
- [ ] T042 交互代理纯核心
- [ ] T043 会话 actor I：创建、发送、中断与持久化
- [ ] T044 会话 actor II：档位、信任门控、崩溃与晚到输出
- [ ] T045 cupper-cli I：客户端库与 Mirror
- [ ] T046 cupper-cli II：命令行
- [ ] T047 test-harness 测试接口与录制导出
- [ ] T050 驱动层 I：传输、线路分接与 ACP 连接
- [ ] T051 假 agent：剧本模式与 persona
- [ ] T052 录制格式、回放模式与 SP-1 录制转换
- [ ] T053 Claude 提取层、选项对照表与 A 层金样例
- [ ] T054 CLI 发现、登录 shell 环境与可信 PATH
- [ ] T055 驱动层 II：AgentRuntime 与 Claude 接入
- [ ] T056 运行时安装器
- [ ] T057 （条件）Q8 后备：同一种 agent 的 prompt 全局串行
- [ ] T060 Git 服务 I：GitCmd 三档与仓库档防护
- [ ] T061 Git 服务 II：快照档与回合快照
- [ ] T062 宿主集成测试 I：回合、交互与历史
- [ ] T063 宿主集成测试 II：信任、档位、并发、崩溃与外发
- [ ] T064 兼容套件框架
- [ ] T065 B 层金样例运行器
- [ ] T066 首批真实录制：Claude 兼容套件
- [ ] T067 宿主性能测量
- [ ] T070 shell-api 与 i18n
- [ ] T071 host-client I：连接、切帧、请求与续传
- [ ] T072 host-client II：视图 reducer 与交叉验证
- [ ] T073 cupper-desktop-native
- [ ] T074 桌面主进程 I：应用骨架、构建与加固基线
- [ ] T075 桌面主进程 II：拉起宿主、control 连接与退出确认
- [ ] T076 桌面主进程 III：windowBridge 与 preload
- [ ] T077 桌面主进程 IV：真实输入、特权操作与 Q26
- [ ] T078 测量闸门：主进程转发
- [ ] T079 （条件）utilityProcess 宿主桥加委托票据
- [ ] T080 ui-core I：骨架、状态层、安全规则与截图测试
- [ ] T081 ui-core II：侧边栏、新建会话与全局状态
- [ ] T082 ui-core III：消息流
- [ ] T083 ui-core IV：交互卡片、排队条与输入框
- [ ] T090 e2e 基础设施
- [ ] T091 NFR-1 验收套件
- [ ] T092 安全 e2e 与外发检查
- [ ] T093 交互与项目 e2e
- [ ] T099 M1a 退出验收

**M1b**

- [ ] T101 SP-10 通知身份（Q2）
- [ ] T102 技术验证：Q8（Codex）、SP-4b #7（Codex）与 Q31
- [ ] T103 SP-6 渲染压力测试
- [ ] T104 Q29 审查文档可行性（SP-6 场景 9）
- [ ] T105 技术验证：Q35 本机签名身份
- [ ] T110 Codex 提取层、选项对照表与 A 层金样例
- [ ] T111 Codex 接入：发现、运行时与回收
- [ ] T112 Codex 回合语义集成测试
- [ ] T113 Codex 兼容矩阵与 B 层补录
- [ ] T114 codex-drift 每日检查
- [ ] T119 （条件）Q8 后备（Codex）
- [ ] T120 Git 服务 III：diff、file_diff、status 与 branches
- [ ] T121 Git 服务 IV：暂存、取消暂存与还原
- [ ] T122 IT-git-hunk：迁移 SP-5 的 82 个用例
- [ ] T123 Git 服务 V：提交与推送（用户档）与 Q23
- [ ] T124 worktree 创建（CP-5）
- [ ] T125 worktree 检查、归档、恢复与重命名
- [ ] T126 文件搜索、提示内容映射与在外部应用打开
- [ ] T127 存储用量与清除快照对象
- [ ] T130 审查面板：主文档部分
- [ ] T131 审查文档（独立源 iframe）
- [ ] T139 （条件）审查面板后备：DiffHunksRenderer
- [ ] T140 Steer：宿主、驱动、CLI 与界面
- [ ] T141 配置选择：set_config 与模式选择器
- [ ] T142 输入框增强 I：@ 文件引用与拖入文件
- [ ] T143 消息流增强 I：代码高亮、计划与恢复指引
- [ ] T144 新建会话页：两种 agent、工作区与分支
- [ ] T145 输入框增强 II：图片、斜杠命令、↑ 与发送键
- [ ] T146 消息流增强 II：回合折叠与晚到输出
- [ ] T147 设置页 II：通用、通知、外观、存储与命令行工具
- [ ] T150 系统通知与 ui/navigate
- [ ] T151 设置页 I：agent 状态、连接与已归档会话
- [ ] T152 三栏布局与亮暗主题
- [ ] T153 会话管理界面、快捷键与"打开"按钮
- [ ] T154 中英文与截图基线
- [ ] T155 SP-6 场景改写为每晚性能回归
- [ ] T156 防休眠（NFR-13）
- [ ] T157 安全边界说明（用户文档）
- [ ] T158 （条件）主进程通知后备
- [ ] T159 （条件）SP-6 不达标项的优化
- [ ] T160 自用构建：发布配置与本机安装
- [ ] T161 release-config 每晚构建
- [ ] T162 （条件）本机签名构建种类
- [ ] T170 M1b 端到端：S1 与 S2
- [ ] T171 M1b 端到端：S3 与 S4
- [ ] T172 M1b 端到端：NFR-2、NFR-9 与容量
- [ ] T173 S1–S4 逐步检查表
- [ ] T174 Q16 保留参数复核
- [ ] T179 M1b 退出验收

**发布（推迟）**

- [ ] T180 Developer ID 签名与公证
- [ ] T181 发布流水线、签名后冒烟与 RC
- [ ] T182 许可证门禁第二层与第三方声明
- [ ] T183 自动更新与升级流程
- [ ] T184 首次正式发布

## 附录：修订记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-10-16 | 初稿（P01）：按设计 v1.1 拆分 M1a、M1b 与"发布（推迟）"组 |
