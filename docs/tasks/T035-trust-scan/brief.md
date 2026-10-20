# T035 项目信任扫描

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T035-trust-scan`
- 依赖：T011、T022（都已合并）。T044（会话的档位与信任门控）依赖本任务，松弛 3 个单位

## 背景

经 ACP 运行时，CLI 不显示工作区信任对话框，仓库里的 hooks、`env`、`apiKeyHelper`、`.mcp.json` 照常生效。cupper 接手这一步：项目经用户在原生确认框里信任之后，会话才加载项目自带的 agent 配置（设计 5.2.6"项目信任"，安全 #2，纪要 D9，U-R1）。

本任务做 `trust` 模块：扫描、摘要、findings、状态转换。门控（Attach 和每次 prompt 之前按信任状态决定能否运行、怎样运行）归 T044。

## 范围

**做**（计划 T035 一行；设计 5.2.6"项目信任"、5.9.8）：
1. **扫描范围**：从工作区一直到它所在 git 仓库的根目录，**每一级目录**里的 `.claude/`、`.mcp.json`、`.codex/`；不是 git 仓库时只看工作区本身。git 根目录由调用方传入（T044 经 T060 取得），本模块不运行 git。
2. **符号链接一律跟随**（安全核对 1）：目录链接递归进入；摘要和 findings 都按解析后的目标内容计算，并记录链接本身和目标的路径。链接悬空、成环，或者目标不是普通文件和目录时，扫描失败。
3. **上限**：文件超过 2,000 个或合计超过 16 MB（含链接目标）时扫描失败，给出语义码，由确认框说明原因。
4. **信任摘要**：对扫描范围里的每个文件（链接按目标），按"相对路径、目标的 realpath、完整内容"做 sha-256，按相对路径排序后合并。按完整内容算，不按已知的键算。输出成协议的 `Digest` 形式（`d_` 加 64 位十六进制，交接，T018）。
5. **逐文件的哈希清单**（交接，T012 裁定）：`project/trust_info.diff_from_trusted` 要按文件列出差异，所以信任时要另存每个文件的哈希清单。按 P-4 加进 `0001` 的 `projects` 表，同时改设计 5.4.2 的 SQL（授权本任务改这两处；store 的测试会比对两者）。本模块提供"当前扫描结果与已信任清单的差异"函数。
6. **findings**：从上述文件里解析出 hooks、`env`、`apiKeyHelper`、`statusLine`、MCP 服务器、skills、agents、commands，以及 `.codex/` 里的配置项和 hooks。只用于在原生确认框里展示，解析不了的文件记一条"无法解析"的 finding，不让扫描失败（摘要照样覆盖它的完整内容）。
7. **工作区外的目标**：链接目标在工作区之外的，结果里标出来，并逐项列出目标里的 findings。不因此禁止信任。
8. **状态转换**：`untrusted`（默认）→ 信任 → `trusted`（记下摘要和清单）；重新扫描时摘要变了 → `stale`，按未信任处理，直到再次确认。扫描范围里一个文件都没有的项目，添加时直接记为 `trusted`（`trusted_by: auto_empty`）；以后出现了这些文件，同样转为 `stale`。做成纯函数，输入是旧状态和新扫描结果。

**不做**：
- 门控：Attach 时的 `settingSources`、Codex 的 `project_untrusted`、运行中变为 `stale` 时暂停队列（T044）；
- `project/trust`、`project/trust_info` 方法和原生确认框；添加项目的流程（索引和桌面的任务）；
- 能力探测目录（T055）。

## 验收标准

- [ ] 新增 UNIT-trust-scan，用临时目录造出的目录树覆盖：
  - 多级目录（工作区在 git 根目录下几层）、不是 git 仓库；
  - 文件链接、目录链接、链接到工作区外、悬空链接、成环；
  - 2,000 个文件和 16 MB 两种上限；
  - 摘要对内容、相对路径、链接目标三者都敏感，对目录遍历顺序不敏感；
  - findings 的各类条目，以及无法解析的文件；
  - 状态转换（含 `auto_empty` 之后出现文件转为 `stale`）和逐文件差异。
- [ ] `projects` 表的新列在 `0001` 和 5.4.2 的 SQL 里一致，store 的测试通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T035 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T035-trust-scan/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T012 裁定：信任时另存每个文件的哈希清单，按 P-4 加进 `0001` 的 `projects` 表。
- T018：`trust_digest` 在协议里是 `Digest` 类型（`d_` 加 64 位十六进制）。

## 注意

- **只读本任务自己造的临时目录**。不要扫描本机真实的项目、`~/.claude`、`~/.codex`，也不要读本仓库之外的任何配置文件。
- 文件读取是阻塞操作，模块本身同步实现即可；调用方会放进阻塞线程池（5.2.3），在 `notes.md` 里写明这一点。
- 代码约束：`cupper-host` 不用 `unsafe`；错误用 thiserror；时间只经 `Clock`（如果需要的话）。解析 `.codex/` 的 TOML 如需新依赖，锁精确版本，许可证在 ADR-11 的 allowlist 内。
- 样例目录树里不出现本机的用户名和路径。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍计。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,200 行时，开工前就停下来汇报，并给出拆分建议**（例如把"扫描、摘要与状态"和"findings 解析"分开）。

## 参考资料

- 设计：`02-host.md` 5.2.6"项目信任"、5.2.1 模块表里的 `trust`；`04-data-model.md` 5.4.2（`projects` 表）；`05-flows.md` 5.5.10；`09-testing.md` 5.9.8；`13-open-issues.md` Q34。
- 安全 #2、安全核对 1、安全确认 ①②；纪要 D9。
- `docs/tasks/T012-protocol-methods-core/`、`docs/tasks/T018-protocol-methods-session/`（`project/trust_info` 的类型）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,445 行（草估 1,015 已贴线），在写代码之前停下汇报，建议按任务书举的例子拆分。

**裁定：采用方案 A，拆分（计划 v1.3.15）。**
- **T035 收窄为范围、摘要与状态**：`trust/mod.rs`、`walk.rs`、`digest.rs`、`state.rs`，以及 `projects` 表的清单列（`0001` 与 5.4.2 两处）。验收 UNIT-trust-scan（范围、链接、上限、摘要、状态与差异）。规模改为 L，上限约 1,100 行，硬停止线 1,200。T034 的 `project/add`、`project/trust` 先把 findings 留成空列表。
- **新增 T096**（依赖 T035）：findings 解析（`findings.rs`、TOML 依赖、`FindingKind` 新增 `unparsed`），新增编号 UNIT-trust-findings。`tests/ids.toml` 已在 main 上改好。T068 改为也依赖 T096。

执行者列的 9 项待裁定：
1. `FindingKind` 加 `Unparsed => "unparsed"`，改 5.3.11 的那一行，重新生成 schema 和 TS：认可，归 T096。
2. TOML 依赖 `toml_edit = "=0.25.17"`（`default-features = false, features = ["parse"]`）：认可，归 T096，以 `cargo deny` 通过为准。
3. "工作区之外"以工作区为界，只有经过链接、且目标 realpath 不在工作区之下的才标 `outside_workspace`；上级目录里的文件本身不算：认可，与 5.2.6"链接目标在工作区之外的"一致。链接指向同一仓库里工作区之外的位置也会被标出，偏保守，接受。
4. `stale` 之后内容又改回原样，仍保持 `stale` 直到再次确认：认可，与 5.2.6"直到再次确认"一致。
5. 16 MB 按 16 MiB 计，上限只数文件：认可。
6. 清单列 `projects.trust_manifest TEXT`，JSON `{"<相对路径>": "<sha-256 十六进制>"}`：认可；序列化时键按相对路径排序（用有序 map），不依赖 serde_json 的 map 键顺序（代码约束）。
7. 摘要编码（每个字段 8 字节大端长度前缀、版本标签 `cupper-trust-v1`、没有文件时为固定值）：认可，写进模块文档。
8. `detail` 原样输出不截断，`project/add` 的结果可能超过帧上限：认可，交给 T034 处理，已写进交接。
9. git 根目录必须是工作区或它的上级（两边先取 realpath），否则扫描失败，`root_not_ancestor`：认可。
