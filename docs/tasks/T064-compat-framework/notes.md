# T064 工作笔记

- 执行者：子 agent（T043、T055、T008、T116、T046、T090、T167、T091、T092、T185、T063 的执行者接着做）

## 开工前读过的

- 任务书；设计 5.9.5、5.1.6 的 compat-framework 与 testdata-scan 两行、5.6.6 的已验证范围；计划 T064、T066、T113 三行和 Q8、Q28、Q34、Q43 的跟踪行。
- `handoff-notes.md` 第一列含 T064 的七行：T002、T004、T053、T116、T029、T067、T065。
- `tests/ids.toml` 的 T064 段，有 UNIT-compat-report 和 UNIT-compat-guards 两个编号。另有六个 COMPAT 编号，属于 T066 和 T113，`--compat` 读报告时要认得它们。
- T065 的 `generate.rs` 和它的用法。入库的三份录制里有 `t` 和 UUID 形式的请求 id，重跑出来的录制不会逐字节相同（见下面的确认点 4）。

## 做法（准备采用，待裁定）

- **两段结构。**
  - **编排者在 xtask 里**：参数、自我约束、临时仓库、配置哈希、报告与 schema、录制入库流程。testdata-scan 在 ubuntu 上跑，要用报告类型，所以这些都放在 xtask。
  - **执行者是 cupper-host 的一个测试目标**：`harness = false`、`required-features = ["test-harness"]`，复用 `tests/common` 的宿主、harness 客户端、读库等待。它从 xtask 收一份运行计划（JSON），执行剧本的步骤和结构断言，导出录制，再把每个剧本的结果（JSON）交回。
  - **不让 xtask 直接依赖 cupper-cli 的原因**：xtask 要能在 ubuntu 上编译。另外它带着 `test-harness` 依赖 cupper-cli 的话，工作区构建时这个特性会合并进普通的 `cupper` 二进制。
- **schema**：用 schemars 从报告的 Rust 类型生成，入库为一个文件。校验分三步：严格反序列化（`deny_unknown_fields`）、语义检查（剧本编号已知、结果取值合法、哈希前后都有）、入库的 schema 与生成的一致。不引入 JSON Schema 校验库。
- **`--fake`**：全部剧本都跑，出报告。只有标了 gold-b 的三个剧本（T065 的 normal-turn、approval、cancel）在加 `--write` 时写进 `testdata/`，然后按 `UPDATE_GOLDEN=1` 重写金样例。
- **`ci compat-framework`**：跑 `--fake`（不写）和单元测试。

## 规模估计

实现按 1.15 倍、测试按 1.1 倍放大（沿用 T185 的系数，xtask 的代码经 rustfmt 展开后常比草估多）。

| 模块 | 内容 | 草估 |
|---|---|---|
| 编排 `xtask compat` | 参数（`--fake`、`--dry-run`、`--agent`、`--only`、`--write`）；流程：约束检查、临时仓库、哈希、跑执行者、汇总报告、录制入库 | 150 |
| 剧本表 | 类型约 40 行，加 5.9.5 的全集约 21 个剧本（每个约 10 行），见下 | 250 |
| 自我约束 | 每个剧本显式的权限模式；拒绝 privileged 和放宽档位，除非剧本声明了、而且只作用于临时仓库；临时目录必须在本用户自己的临时目录下、0700 | 80 |
| 配置哈希 | 遍历 `~/.claude`、`~/.codex` 里列出的文件，逐个算 sha256，比较前后 | 60 |
| 报告 | 类型（提交、运行模式、模型、版本、剧本结果、耗时、重试、哈希前后、待清理的 CLI 会话）、schema 导出、校验、写出 | 120 |
| 录制入库 | 改写头行、项目路径换成占位、跑 `tools/sanitize`（链接路径和真实路径都作为 `--base`）、过敏感词清单（没有清单就报错）、写入 `testdata/`、重写金样例 | 100 |
| `ci compat-framework` 与入口 | 作业、`main.rs` 接入 | 40 |
| 执行者（cupper-host 测试目标） | 计划和结果的类型 50；宿主启动（假 agent 或真实配置）40；步骤：发送、等事件、允许与拒绝、回答提问、确认计划、中断、steer、图片、杀适配器、重启宿主、resume、关闭、回合中改文件，共 170；结构断言 90；并发（N 个会话各发 3 个 prompt，查串话）40；进程计数与后代退出 40；按会话导出录制 30 | 460 |
| `lsof` 断言 | 适配器的后代都不持有宿主的管道和套接字 | 40 |
| testdata-scan | 读入报告做校验，从报告重算已验证范围，与 `runtime/manifest.json` 比对 | 70 |
| `tools/rec-shape-diff`（TS） | 方法、update 种类、`_meta` 键路径、`option_id`、`stopReason`、错误码，以及差异输出 | 130 |
| `trace-ids --compat` | 读报告，把 COMPAT 编号的结果并入 `status`、`verify` | 35 |
| | 实现小计 | 约 1535，×1.15 ≈ 1765 |
| 测试 | UNIT-compat-guards 80；UNIT-compat-report 70；哈希 35；录制改写 30；范围重算 50；rec-shape-diff 60；`--compat` 30；结构断言 40 | 395，×1.1 ≈ 435 |

剧本全集按 5.9.5 和计划 T064 一行算，共 21 个：

| 组 | 剧本 |
|---|---|
| 基本 | 新建会话与能力、普通回合、执行命令（含非零退出码）、审批允许、审批拒绝、结构化提问、计划确认、取消（命令执行中）、取消（审批挂起时）、思考流 |
| 恢复 | CLI 被杀后恢复、宿主重启后 resume |
| 输入 | 图片输入、Steer |
| 并发与进程 | Q8（两个会话各 3 个 prompt，重复 3 轮）、6 个会话并发、关闭时有命令正在运行、多次 resume 后的进程数 |
| 配置边界 | 未信任项目、Q28（回合中改写 hooks）、Q34（上级目录和 git 根目录之外的配置） |

**合计约 2200 行**（区间约 1900–2300），超过停止线 1300，也超过硬停止线 1450。还没写代码，先停下汇报，附拆分建议。

## 拆分建议（交编排者裁定）

**方案一（推荐，三个任务；后两个可以并行）**

| 任务 | 范围 | 估计 | 用例 |
|---|---|---|---|
| T064（缩小） | 框架与 `--fake`：编排、剧本表的类型和"基本"组 10 个剧本、执行者核心（发送、等事件、允许与拒绝、回答、确认计划、中断）、结构断言、报告与 schema、录制入库（脱敏、敏感词清单）、`--fake` 重新产出 T065 的三份录制和金样例、`ci compat-framework` | 约 1050 | UNIT-compat-report |
| 新任务 A：真实模式与剧本补全 | 自我约束、配置哈希、`--dry-run`、真实配置的宿主启动；其余 11 个剧本和它们的步骤（杀适配器、重启宿主、resume、关闭、steer、图片、回合中改文件、并发、进程计数）；`lsof` 断言 | 约 740 | UNIT-compat-guards |
| 新任务 B：审阅与校验工具 | testdata-scan 的报告校验和已验证范围重算、`tools/rec-shape-diff`、`trace-ids --compat` | 约 430 | 可能要新增编号（形态差异、范围重算），由编排者定 |

- 依赖：A 和 B 都依赖缩小后的 T064（剧本表类型、报告格式），彼此不依赖。T066 依赖三者。
- 真实模式的安全约束（自我约束、配置哈希）都在 A 里，T066 之前必须合并。

**方案二（两个任务）**：T064（缩小）同方案一，约 1050；其余合成一个任务，约 1170。两段都在 1300 以内，但只能串行。

## 需要确认的

1. **结构**：编排在 xtask，执行者是 cupper-host 的 `harness = false` 测试目标，理由见上面"做法"。
2. **schema**：用 schemars 生成并入库；"符合 schema" 的检查是严格反序列化加语义检查加 schema 文件不过期，不引入 JSON Schema 校验库。xtask 新增 schemars 依赖（工作区已有）。
3. **`--fake` 写入的范围**：只有标了 gold-b 的三个剧本、而且加了 `--write` 时才写 `testdata/`；其他剧本只跑、只出报告。
4. **"与入库的一致"怎么判**：录制里有 `t`（毫秒）和 UUID 形式的请求 id，重跑不会逐字节相同。判据定为两条：去掉 `t` 并把 id 按出现顺序重编号后相同；`gold_b` 测试在不设 `UPDATE_GOLDEN` 时对新录制通过（金样例不变）。
5. **M1a 还没有的功能**：Steer 是 M1b 的功能（`SendMode::Steer` 会回落到排队），图片输入要先上传 blob。这两个剧本在报告里按"不适用"记，带原因，还是要求断言回落行为？
6. **已验证范围的起点**：报告还没有入库时（T066 之前），重算没有数据。准备把 ADR-08 的起始值作为"无报告时的基线"，有报告后取基线与通过的报告的并集。也可以改成"没有报告就跳过比对"。

## 裁定之后（计划 v1.3.55）

按方案一收窄为框架与 `--fake`，停止线 1150、硬停止线 1300。UNIT-compat-guards 移到 T187；审阅与校验工具（新增 UNIT-compat-tools）归 T188。六点确认都认可；Steer 和图片归 T187，已验证范围的基线归 T188。

## 实际做法

- **执行者**：cupper-host 的测试目标 `compat`（`tests/compat.rs` 和 `tests/compat/scenarios.rs`），要求 test-harness 特性。
  - 和裁定的出入：执行者不是 `harness = false` 的目标，而是普通 libtest 目标里一个忽略的测试 `compat_driver`，从 `CUPPER_COMPAT_PLAN` 读计划。原因是 `cargo test --workspace` 会不带参数地运行 `harness = false` 的二进制，rust 作业的 `cargo test ... gold_` 还会把过滤词 `gold_` 当参数传给它。忽略的测试在这两种情况下都不运行，只有 xtask 用 `--ignored --exact compat_driver` 点名时才跑。
  - 每个剧本单独起一个宿主、用一个临时目录，在 tokio 任务里跑，任务 panic 记为失败，后面的剧本照常跑。
  - 步骤在等待时超时（30 秒）记为"不确定"，按计划的 `attempts` 重跑（`--fake` 只跑 1 次）。
  - 结构断言都读宿主库里本会话的事件：回合状态、条目种类、工具种类、非零退出码、请求的最终状态、能力、项目里没有某个文件。
  - 剧本表只有假 agent 的脚本。真实提示词、权限模式、准备文件都要到真实模式才用得上，留给 T187；现在写进去就是没人用的代码。
- **编排**（`crates/xtask/src/compat/`）：`mod.rs` 负责参数和流程，`report.rs` 是报告类型、校验和 schema，`record.rs` 是 `--write` 的入库流程。
  - 没有 `--fake` 时直接报错，说明真实 CLI 还不能驱动。
  - 报告先序列化，再严格读回、过 `problems`，然后才写出。
  - schema 只在测试里生成：`unit_compat_report_schema_file_is_current` 比对入库的 `testdata/compat/report.schema.json`，设了 `UPDATE_GOLDEN=1` 时改写它。
  - 生成 schema 时把键排好序。工作区里有别的 crate 打开了 serde_json 的 `preserve_order`，不排序的话，`-p xtask` 单独构建和整个工作区构建得到的键顺序不同。两种构建都试过，都通过。
- **`--write`**：
  1. 只取三个 gold-b 剧本的录制。
  2. 用 `tools/sanitize sanitize` 脱敏：`--base` 传各剧本临时根目录的链接路径和真实路径，`--terms` 传敏感词清单。脱敏后还有残留就失败。
  3. 归一化后与入库的相同（去掉 `t`，id 按出现顺序重编号），就保留原文件，不重写。
  4. 跑一遍 `UPDATE_GOLDEN=1` 的 `gold_b` 测试，再跑 `tools/sanitize scan testdata`。
  - 敏感词清单默认取主工作区的 `.private/sensitive-words.txt`，也可以用 `--words` 指定；两处都没有时，在跑任何东西之前就报错。
- **`ci compat-framework`**：先跑 `cargo test -p xtask -- compat::`，再用当前的 xtask 二进制跑 `compat --fake --out <日志目录下>`。

## `--fake` 与 T065 首批录制的对照

| 剧本 | 结果 |
|---|---|
| normal-turn | 与入库的相同（去掉 `t`、重编号 id 后），文件和金样例都没动 |
| approval | 同上 |
| cancel | 剧本改成"命令执行中取消"：先允许一条命令，1 秒后中断。录制和金样例都随之更新，新的金样例里工具调用以 `abandoned` 结束，回合 `cancelled`。另外把脚本临时换回 T065 的 `say working; wait_cancel` 跑了一次（没提交），结果与入库的相同 |

- 更新后不设 `UPDATE_GOLDEN` 跑 `gold_b`：9 个都通过。
- 再跑一次 `--write`：三份都是 unchanged，文件逐字节不变。
- 本机主工作区没有敏感词清单。验证 `--write` 时用的是草稿目录里的一份临时清单，只有一个不会出现的词。真实录制入库前仍要维护者自己的清单（T066）。

## 规模

- 执行者第一版 554 行，经 rustfmt 展开后比估计多。收紧写法、删掉真实模式才用得上的字段后是 492 行。
- xtask 部分写完时合计 1155 行，超过新的停止线 1150 共 5 行。改用 JSON 构造测试用的报告、简化提交哈希的检查等，降到 1124 行。
- 最终新增 1124、删除 8（`cargo xtask size`；不含 `Cargo.lock`、`testdata/`、`docs/`）：cupper-host 492，xtask 632。

## 留给后续任务的

- **T187**：
  - 每个剧本的真实提示词、权限模式（Claude `default`；`plan` 剧本以 `plan` 模式开始）、准备文件，以及自我约束。
  - 给 `session/create` 加显式权限模式后，三份 B 层录制的 `config` 会变。到时用 `--fake --write` 重新产出，差异可以解释。
  - `cancel` 剧本在真实 CLI 上靠"允许后等 1 秒再中断"，`sleep 60` 不输出，是否足够要实际跑一次才能确认。
- **T188**：报告的读入用 `report::parse` 加 `report::problems`；schema 文件在 `testdata/compat/report.schema.json`。
- **compat-framework 进 PR**：设计 5.1.6 写的是"改动 xtask 兼容套件的 PR"也要跑。本任务没把它加进 `ci pr`；要加的话照 e2e 的做法按路径判断，由编排者决定放进哪个任务。
