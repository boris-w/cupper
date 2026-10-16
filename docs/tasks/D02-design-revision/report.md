# D02 汇报

- 执行者：子 agent（架构师）
- 分支：`docs/D02-design-revision`（基于 main 的 `9c9da46`）
- 完成日期：2026-10-16

## 做了什么

按设计评审的结论修订了详细设计，共 10 个提交，16 个文件，新增 1,722 行、删除 628 行。修订依据按以下顺序取用：纪要的 D1–D16、三份 `*-confirm.md` 的附加条件、`architect-response.md`、第一轮原文。编排者的补充说明也已落实：

- 待定的 10 项一律写成"已定（2026-10-16）"，出处是 `docs/meetings/2026-10-16-gate-design-decisions/minutes.md`；
- `<前缀>` 全部换成 `dev.cupper`，并注明"M1b 首次签名发布前可以无成本修改"；
- 所有提到 PRD 的地方按 v1.2 写。

**各文件的修订**

| 文件 | 主要变化 |
|---|---|
| `docs/design/02-host.md` | 新增 5.2.16"时间与可测性"：Clock/IdGen、禁用清单、纯核心、`AcpTransport`、`test/clock_advance` 的返回时机。<br>5.2.6 新增三个小节："晚到的工具输出"、"项目信任"、"配置项"（档位核对和 `config_unconfirmed`）；第一条消息随受理事务落盘；自发回合的出队保护。<br>5.2.10 重写为三档调用：快照档、仓库档、用户档。<br>5.2.8：组提交加 savepoint、持久性定案、`data_epoch`。<br>5.2.5：档位规则和 `input` 证据。<br>5.2.13：启动载荷、退出码 6、`DARWIN_USER_TEMP_DIR`。<br>5.2.14：CLI 移到 `Helpers/`。<br>模块表增加 `trust` 和 `clock`。 |
| `docs/design/03-protocol-v1.md` | 全文重写。<br>握手：`initialize` 只接受一次；dev_key 证明绑定信道。<br>同步：`data_epoch` 与 `cursor_ahead`；`history/page` 改用回合内游标。<br>事件：`item.delta.field`、`output_state`；时间字段；`final_item_ids` 与活动组规则；`late_tool_output`。<br>方法：`project/trust`、`session/confirm_config`、`ui/navigate`。<br>交互请求：要批准的内容不截断；未知 kind 按最严；`effect.rule` 与确认框文案规则；JCS 与 256 位摘要。<br>文件头加冻结说明。 |
| `docs/design/04-data-model.md` | `projects` 加信任相关列；`sessions` 加 `initial_prompt_json`、`mode_set_by`、`full_access_confirmed_at`；`meta` 加 `data_epoch`；迁移改为每个脚本在自己的事务里更新版本号；存储位置增加快照档的 GIT_DIR、探测目录，日志和 worktree 目录改为 0700 |
| `docs/design/05-flows.md` | 更新启动、新建会话、审批、中断（晚到输出）、宿主崩溃（epoch）、升级（安装空窗）、审查各图；新增 5.5.10"添加项目与信任" |
| `docs/design/06-agent-driver.md` | 档位表；环境变量规则（禁用清单、适配器控制变量、`CLAUDE_CONFIG_DIR`/`CODEX_HOME` 的校验）；`settingSources`；SETSID；`fs`/`terminal` 永不声明；K 表的回归列改成用例编号 |
| `docs/design/07-desktop.md` | 全文重写。<br>连接：数据连接的生命周期、按字节块转发、"应用后确认"的流控、按订阅上限定义的测量闸门。<br>ShellAPI：完整合同，含审查文档地址、开放枚举的命令 ID、`PrivilegedAction` 增加 `add_project`、`trust_project`、`confirm_config`。<br>界面：折叠规则和卡片规则。<br>审查文档：消息协议与桌面确认的 8 点。<br>安全：两个文档的 CSP、Trusted Types、合并后的真实输入规则（含 D7 全部补充）。<br>构建：`dev.cupper` 的各种构建、URL scheme、e2e 签名方式。 |
| `docs/design/08-security.md` | 全文重写。启动载荷；绑定信道的双向证明；档位规则表；权限矩阵（档位、`input` 证据、`project/trust`、`test/*`）；提权的全局限频与确认框布局；进程卫生；审计事件；安全边界说明（信任、git、TCC 警示） |
| `docs/design/09-testing.md` | 全文重写。<br>测试分层与编号；三部分的假 agent；录制格式规格；A、B 两层金样例与规范化。<br>NFR-1 用例；按构建种类标注的安全 e2e。<br>兼容矩阵：只在本机运行，报告有 JSON schema。<br>测量方法；SP-6 补到 9 个场景；故障点与 epoch 用例。<br>新增 5.9.9（注入点）、5.9.10（编号与人工清单）。 |
| `docs/design/01-repo-build-ci.md` | CI 表重写：ubuntu 与 macOS 分工、去掉自托管 runner、Actions 加固、`release-config` 每晚构建、无特性测试、`compat-framework`；5.1.7 增加 Clock、Trusted Types、`dialog`、spawn 四条约束 |
| `docs/design/10-release.md` | CLI 和随 App 公证的 Node 放进 `Helpers/`；签名身份；清单覆盖范围；更新代理端口的描述、安装空窗、安装位置检查；发布流水线：按 crate 分别构建、产物检查、许可证门禁第二层、签名后冒烟、RC 标签、protected environment |
| `docs/design/11-diagnostics.md` | 原始帧只做结构化脱敏；诊断包里的 minidump 和原始帧默认不勾选 |
| `docs/design/12-milestones.md` | 全文重写。M1a 的第一个任务是核心骨架，之前先做技术验证；新增界面元素；退出标准改为"连续 20 次全绿"的口径。M1b 的退出改为检查表。追溯矩阵的"验证"列改为引用用例编号。**新增 5.12.5 契约冻结清单**，含第 1、2、3 组和冻结机制。 |
| `docs/design/13-open-issues.md` | 全文重写。关闭 11 项：Q1、Q3、Q4、Q5、Q6、Q11、Q12、Q13、Q14、Q15、Q22。更新其余各项，新增 Q26–Q33。5.13.2 改写成逐份 ADR 的修订依据；5.13.3 改为"已在 PRD v1.2 落实" |
| `docs/02-design.md` | 版本改为 v0.7，依据改为 PRD v1.2；第 1 节的两张组件图加入审查文档、项目信任、时钟、git 的三档；约束表加 5 行；第 3 节更新 SP-6、SP-10；第 5 节的索引和"主要结论"重写 |
| `docs/tasks/D02-design-revision/notes.md` | 独立 GIT_DIR 方案的实验过程与结论 |
| `docs/tasks/D02-design-revision/checklist.md` | 逐行列出落点：4 条阻断、45 条重要、三份确认的附加条件、D1–D16、10 项用户决定，以及采纳的建议级意见 |

**D8 独立 GIT_DIR 方案的评估结论：可行，快照路径采用它**（写在 5.2.10，实验见 `notes.md`）。

- **与 SP-5 方案对比的结果**：以下场景里两者得到的 tree 都相同：
  - 普通改动；
  - 被忽略但已跟踪的文件、`info/exclude`；
  - autocrlf；
  - 锥形稀疏检出加 sparse index；
  - split index（需要复制 `sharedindex.*`）；
  - index 带 UNTR 和 FSMN 扩展；
  - linked worktree；
  - sha256 仓库。
- **恶意配置仓库**：只做无条件覆盖时，SP-5 方案会执行三个 clean 过滤器，其中包括 include 进来的配置和 `info/attributes` 引用的过滤器；独立 GIT_DIR 一个都不执行。
- **快照档生成的补丁**可以直接 `apply --cached` 到用户仓库，结果与 `git add` 相同。
- **性能**：快照本身与 SP-5 方案相当；每次多出约 15–25 ms，用来重新生成配置。

## 关键取舍

1. **独立 GIT_DIR 的适用范围扩大**。除了回合快照，`unstaged`、`staged` 两种 diff 范围、`git/status` 的文件状态、`@` 引用的文件列表也都放进快照档。在用户仓库上执行的自动调用只剩只读引用类命令和少数写操作，后者用"每次调用前读配置并覆盖"的做法。理由是让会执行仓库程序的面尽量小。
2. **实验中发现的两个问题，已写进设计**：
   - 复制 index 时如果换成新的 mtime，同一秒内改过、大小又没变的文件会被当成未修改，快照里留下旧内容。普通复制 12 次全错，保留 mtime 后 12 次全对。SP-5 的方案同样有这个问题，改为复制时保留 mtime。
   - `write-tree` 这类辅助命令也会触发过滤器、fsmonitor 和 hook，所以仓库档的防护必须覆盖每一次调用。
3. **自定义过滤器**（如 git-crypt）在快照档里不执行，相应路径标为 `filter_unsupported`，不提供 hunk 操作，避免把明文暂存进去。LFS 只放行标准取值，本机没有 git-lfs，未实测，记为 Q32。
4. **`config_unconfirmed` 选择"暂停"**。D10 允许两种做法：暂停，或者自动降到 normal。我选了暂停，不悄悄改用户的模式；解除有两条路：桌面端经原生确认，或者有权限的一方把档位降下来。
5. **扫描范围里没有任何文件的项目，直接记为已信任**，不弹确认框。以后出现了项目配置，摘要变化，照样转为 stale。这样减少无谓的确认。
6. **晚到的输出到达时，如果没有进行中的回合**，开一个自发回合承载它，等这个条目结束时自发回合也结束。命令确实还在运行，会话显示"运行中"是准确的。
7. **`data_epoch` 的判定依据**：上次异常退出，并且 `kern.bootsessionuuid` 变了。`kill -9` 不触发重新同步，断电则会。两个输入都可以注入。
8. **测量闸门的负载数字**：每条会话流每秒 25 帧、每帧约 2 KB。这是按 40 ms 写入合并换算出的上限，设计里写明 M1a 开工时按 SP-1 录制里的最高速率复核。
9. **流式性能预算的口径**：从"6 个会话"改为"4 个订阅中的会话"，与订阅上限一致；6 个会话的并发改由宿主集成测试覆盖（NFR-11）。
10. **安装器依赖由 `lzma-rs` 改为 `flate2`**：Node 改为随包的 Mach-O 后不再需要解 xz，而 npm 的 tarball 是 gzip。

## 验证方式

- **mermaid**：在会话临时目录装了 mermaid 11.4.1 和 jsdom 25.0.1（不在仓库里，用完已删），解析 `docs/02-design.md` 和 `docs/design/` 下全部 21 张图，没有失败；故意写错的一张图能被检出。
- **链接**：自写脚本按 GitHub 规则生成锚点，检查 314 个跨文档链接和锚点，0 个失效。
- **个人信息**：对分支 diff 和全部提交信息扫描用户名、邮箱、本机路径、临时目录、claude.ai 链接，没有命中。唯一命中的一处是 5.9.3 描述扫描规则的那句话。
- **独立 GIT_DIR 实验**：在每个用户自己的临时目录下新建目录，只调用 git；`HOME` 和全局配置都指向实验目录里的空文件，没有读写本机的 git 配置；结束后整个删除，确认没有残留的 fsmonitor 守护进程。
- **外部核对**：从 npm 下载 `electron` 44.8.0 的类型定义（User-Agent 为 `cupper-research`），确认 `webContents.focusedFrame` 存在，用于"只记录主框架上的输入"。读完即删。
- **对照验收标准**：
  - [x] 4 条阻断、45 条重要、三份确认的附加条件、D1–D16，在 `checklist.md` 里都有落点；修订完我又通读了一遍，并对 02-host、05、06、07、08 做了一轮一致性修正，即最后一个提交。
  - [x] 契约冻结清单写在 5.12.5。
  - [x] 5.13 已更新：问题的关闭和新增、ADR 修订建议、PRD 影响。
  - [x] 待用户决定的事项都按已定的结论写，统一标"已定（2026-10-16）"，正文里没有"待用户确认"的标记。
  - [x] D8 的独立 GIT_DIR 方案有评估结论（5.2.10、`notes.md`）。
  - [x] mermaid 全部通过解析，链接和锚点没有失效。
  - [x] 新内容里没有个人信息。

## 提交列表

```
46d763c docs(design): align details from the read-through and add D02 checklist
d76a99b docs(design): update milestones, contract freeze list and open issues
1fbd7cd docs(design): harden CI, release pipeline and diagnostics
4e92a30 docs(design): rework test strategy around seams, goldens and failpoints
a74e655 docs(design): specify desktop contracts, review document and real input
85bacdd docs(design): complete protocol v1 contract before the M1a freeze
db125f9 docs(design): define approval tiers, env passthrough and elevation limits
6e35583 docs(design): add project trust before loading project agent config
c0a01fe docs(design): stop host git calls from running repo-configured programs
945543c docs(design): add clock, pure cores and test seams to host design
```

## 遗留问题 / 范围外发现

1. **合并时需要编排者处理**：
   - 正文按 PRD v1.2 写，并引用 `docs/meetings/2026-10-16-gate-design-decisions/minutes.md`。这两样都在 main 的 `df9e447` 上，本分支看不到。引用写成路径文字，不是 Markdown 链接，所以链接检查不受影响。
2. **ADR 修订**：按 5.13.2 逐份处理 ADR-01 至 ADR-11（ADR-01 也有细化项）。其中三处是"替换"原有写法：
   - ADR-06："原始帧的脱敏要求和日志相同"，以及"持久性 M1 实测再定"；
   - ADR-07："丢弃晚到的工具输出"；
   - ADR-08："Node xz 压缩后约 24 MB"，以及"首次运行时复制"。
3. **`gate/design` 之后合并进 `CLAUDE.md` 的约束**：5.1.7 新增了 4 条，分别是 Clock/IdGen、Trusted Types 与样式、`dialog` 在调用时才取函数、统一经 `cupper-sys::spawn` 拉起子进程。
4. **`docs/tech-stack.md` 可能过时**，不在本任务范围内，没有改。可能过时的几处：Node 改为随 App 公证、git 分三档、审查文档改为独立源的 iframe。
5. **未核实、已记入待定问题的事项**：
   - Q26：Electron 44 的输入事件，以及用 `focusedFrame` 判断主框架是否可靠；
   - Q27：整个 App 的外发检查方法；
   - Q28：Claude Code 是否热加载项目配置；
   - Q29：审查文档的可行性，包括 constructable stylesheet 与 CSP；
   - Q30：OAuth 刷新令牌是否轮换；
   - Q31：codex-acp 是否透出 phase；
   - Q32：其余可执行键的覆盖效果，以及 LFS；
   - Q33：委托票据的草案。
6. **测量闸门的负载数字**是按合并窗口换算的上限，M1a 开工时要按录制复核（5.7.1 已写明）。
7. 主仓库的工作区没有改动；实验和检查用的临时目录都已删除。

## 补充修订（评审者核对后，2026-10-16）

安全、测试两位核对时提出的 10 处问题都已在 D02 分支上改完，分 3 个提交。逐条落点写在 `checklist.md` 第 7 节"核对后的补充"。

**安全**
1. **信任扫描（合并前必须改，已改，`02` 5.2.6）**：
   - 符号链接一律跟随，摘要和 findings 都按链接目标的内容计算；目标被改写时，项目转为 stale。
   - 目标在工作区外的，选择"醒目标出"而不是"禁止信任"：项目配置链接到 dotfiles 很常见，而目标内容已计入摘要，每次发 prompt 前都会重扫。
   - 扫描范围扩大到"工作区到 git 根目录的每一级"。CLI 是否还会越过 git 根目录加载配置没有核实，记为新增的 Q34。
2. **git 和 git-lfs 的路径（`02` 5.2.10）**：宿主启动时把两者解析成绝对路径，只在 PATH 的可信条目里查找（绝对路径，并且不在任何项目、工作区、临时目录下）。快照档的 LFS 过滤器写成绝对路径；git 子进程的 PATH 只留可信条目，工作目录设为 cupper 自己的目录。CLI 发现用同一条规则（`06` 5.6.6）。
3. **`filter_unsupported`（`02` 5.2.10）**：快照档复制 `info/attributes`，检测覆盖全部属性来源。这类路径在快照里只存占位和哈希，不存明文；`git/stage` 之前再用 `check-attr` 核对一次。
4. **回合中途档位越权（`02` 5.2.6"配置项"）**：先把档位改回原值（最多等 2 s），改不回就中断当前回合，然后再进入 `config_unconfirmed`。
5. **仓库档第 2 条**：写明"仓库自己的 git 目录除外"，这部分由覆盖和指纹兜底。

第 1、2、4 条（以及第 3、5 条）已写进 5.13.2 的 ADR-05、ADR-08、ADR-10 三行。

**测试**
6. **racy 用例**：显式设置文件和 index 的 mtime 来构造；另加单元断言：复制出的临时 index 的 mtime 等于原文件。
7. **负载工具**：
   - 假 agent 加 `flood_rate <每秒片段数> <秒> <长度>`；
   - 宿主加 `test/seed {profile}`，写进 5.2.16 和 5.9.9 的注入点表；
   - 测量闸门、冷启动数据集、SP-6 夹具都改为引用这两个工具。
8. **SP-6 的场景定义**：
   - 场景 8：用不透明的置顶窗口遮住 App，测前断言 `visibilityState === 'hidden'`；
   - 场景 1：定义冷启动和热启动；
   - 场景 2：输入延迟预算 p95 ≤ 50 ms；
   - 场景 4：锚点位移不超过 1 px。
9. **`protocol` 作业**：schema 导出改为 `cupper-protocol` 自己的 bin `export-schema`，不依赖 `cupper-sys`，能在 Linux 上编译；`xtask` 只在 macOS 上编译（`01` 5.1.1–5.1.6）。
10. **用例引用**：
    - MS-2 改为引用 IT-thought-claude 和 GOLD-A-codex-s02；真实的 Claude 思考流留给兼容矩阵补。
    - 顺带修正一处：MS-4 原来引用的 GOLD-A-codex-s02b 里没有计划帧，改为 s05。
    - 新增 `tools/trace-ids` 检查：文档里引用的用例编号，要么有同名测试，要么登记在 `tests/pending-ids.toml` 里并写明里程碑。

**提交列表**
```
1e3874f docs(design): record Q34, ADR notes and checklist for the verification
fbe0a33 docs(design): close testability gaps found in the revision check
1bed8cb docs(design): close security gaps found in the revision check
```

**验证结果**
- mermaid 11.4.1 解析 21 张图，0 失败，反向对照能检出错误。
- 链接检查 323 个，0 失效。
- 个人信息扫描：本轮的 diff 和提交信息都没有命中。
- 临时工具目录已删除。
- 主仓库的工作区里有 ADR 文件的改动，不是我做的，应是编排者在 main 上的修订，我没有动。