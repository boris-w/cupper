# T188 工作笔记

- 执行者：子 agent（Rust 工程师）
- 日期：2026-10-26

## 读到的现状（开工前）

- `crates/xtask/src/compat/report.rs`：`Report` 类型、`parse`（严格读入）、`problems`（类型表达不了的规则）。schema 由类型生成，比对它的函数只在测试里（`unit_compat_report_schema_file_is_current`），`testdata-scan` 要用得先挪出测试。
- `testdata-scan`（`ci/jobs/testdata_scan.rs`）的步骤全是外部命令（`trace-ids check`、`sanitize scan`/`words`）。报告校验得在 Rust 里做（`report::parse`），做法和 `compat-framework` 一样：用 `std::env::current_exe()` 起一个 xtask 子命令作为一步。
- xtask 的依赖（anyhow、base64、flate2、schemars、serde、serde_json、sha2、tar）都跨平台；只有 `ci/facts.rs` 用 `cfg!(target_os = "macos")` 分支，没有 macOS 专用的 crate。本机只装了 `aarch64-apple-darwin` 和 `aarch64-apple-ios-sim` 两个 target。
- 报告的 CLI：`cli: {name, version}`。假 agent 的报告是 `{name: "claude", version: "fake"}`，`agent: "claude"`；适配器是 `adapter: {name, version}`。
- 剧本表在 `crates/cupper-host/tests/compat/scenarios.rs`，现有 10 个：new-session、normal-turn、command-exit、approval、reject、question、plan、cancel、cancel-approval、thinking。T187 正在加其余 11 个，并且会定新剧本的 `id`。
- COMPAT 编号有 6 个带套件的：
  - `COMPAT-claude-thinking`（剧本 thinking，已有）；
  - `COMPAT-untrusted-project-claude`、`COMPAT-claude-six-sessions`、`COMPAT-claude-close-running`、`COMPAT-claude-resume-procs`（剧本由 T187 加）；
  - `COMPAT-untrusted-project-codex`（T113，Codex）；
  - 另有一个由子编号组成的 `COMPAT-untrusted-project`。
- 录制格式：首行是头（`format: "cupper-rec"`），其余每行 `{seq, t, instance, dir: send|recv|note, msg}`，`msg` 是 JSON-RPC 消息。
- `ci pr` 组合作业里没有 `compat-framework`；`ui` 作业用 `git::changed_since_main` 按路径跳过。

## 打算的做法（供裁定）

1. **报告校验与已验证范围**：新子命令 `cargo xtask compat-reports [--write]`。
   - 不带参数时做三项检查：`testdata/compat/reports/` 下每个 `.json`（含子目录）过 `report::parse` 和 `report::problems`；schema 文件不过期；从报告算出的范围和 `runtime/manifest.json` 的 `cli.*.verified` 一致，不一致时提示运行 `cargo xtask compat-reports --write`。
   - `--write` 只重写清单的 `cli.*.verified`。
   - `testdata-scan` 加一步 `compat-reports`。
2. **计入范围的报告**（汇报里要提的取舍）：
   - 真实模式（`real_config` 或 `isolated_config`）；
   - 适配器版本等于清单里锁定的版本（claude-agent-acp 或 codex-acp）。理由是设计 5.10.3 写的"每个适配器版本一份"：旧适配器的报告不能证明新适配器能和这个 CLI 版本配合；
   - 每个剧本都通过。`fail` 和 `inconclusive` 都让整份报告不计入：剧本不确定说明结构没有被验证到，要整份重跑通过后才计入。T187 加的"不适用"按通过算，因为它与 CLI 版本无关；"观察"类不判通过与否，也不阻止计入；
   - CLI 版本必须是 `x.y.z`，否则判失败，不静默跳过；
   - 已知局限：`--only` 跑出的部分报告看上去也"全部通过"。建议 T187 让真实模式的 `--write` 只接受全量运行，或者在报告里记下选了哪些剧本。
3. **`tools/rec-shape-diff <旧录制> <新录制>`**：
   - 六类形态是：方法（带方向）；update 种类（`sessionUpdate`）；`_meta` 键路径（带所在的方法和 params、result、error，数组下标记成 `[]`）；`option_id`（带 kind）；`stopReason`；错误码（带方法）。
   - 应答按 `instance` 和 `id` 对回请求的方法；`note` 行跳过。
   - 输出每类的增（`+`）删（`-`）；没有差异时退出码 0，有差异时 1，用法错误时 2。
4. **`trace-ids --compat <报告>`**：照任务书的建议，在 `tests/ids.toml` 里登记，例如 `COMPAT-claude-thinking = { suite = "compat", scenario = "thinking", cli = "claude" }`。
   - 规则：`scenario`、`cli` 只能写在 COMPAT 编号上；套件为 compat 的 COMPAT 编号必须写；同一个 `(cli, scenario)` 只能对应一个编号。
   - 报告按 `cli.name` 和剧本 `id` 找编号：`pass` 记通过，`fail` 记失败，`inconclusive` 记跳过（evaluate 判为 problem，提示要重跑）。
   - 假模式的报告直接报错：假 agent 证明不了 COMPAT 编号。
   - 没有登记的剧本（基本组的大部分）单独列一节提示，不判失败。
   - `--milestone` 允许同时给 `--compat`，前提是报告已提交且未改动。
   - 剧本编号已由编排者定下、不再改：T064 的 10 个，以及新增的 11 个（crash-resume、host-restart、image、steer、concurrent-two、six-sessions、close-running、resume-procs、untrusted-project、hooks-reload、config-scope；其中 10 个由 T192 补，计划 v1.3.60）。
   - 6 个 COMPAT 编号的登记：thinking 对应 `COMPAT-claude-thinking`；six-sessions、close-running、resume-procs 对应 `COMPAT-claude-*`；untrusted-project 对应 `COMPAT-untrusted-project-claude`（cli claude）和 `-codex`（cli codex）。
5. **compat-framework 进 `ci pr`**：
   - 相关路径：`crates/xtask/src/compat/`、`crates/xtask/src/ci/jobs/compat_framework.rs`、`crates/cupper-host/tests/compat.rs`、`crates/cupper-host/tests/compat/`、`testdata/compat/`。
   - 假 agent 和宿主本身的改动不算（每晚的 `compat-framework` 覆盖），否则几乎每个 PR 都要跑一遍。

## 开工前的规模估计

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 报告校验与已验证范围（`compat/reports.rs`） | 子命令与参数；读目录、`parse` + `problems`；schema 检查（`report.rs` 里的生成函数挪出测试）；版本解析与比较；计入条件；算范围；与清单比对、`--write`；`main.rs` 分发、`manifest.rs` 注释、`testdata-scan` 一步 | 165 | 85 |
| compat-framework 进 `ci pr` | 路径表、按改动跳过、组合作业加一项 | 25 | 12 |
| `tools/rec-shape-diff` | 包的样板（package.json、tsconfig、vitest 配置，约 33）；六类形态的提取、对比、输出（约 110）；命令行（约 35） | 180 | 75 |
| `trace-ids --compat` | 登记表的 `scenario`、`cli` 字段与校验；读报告、映射到编号；命令行、`--milestone` 的放行、未登记剧本的提示；`ids.toml` 里 6 个 COMPAT 编号改成表 | 95 | 55 |
| **合计** | | **约 465** | **约 227** |

- 直接相加约 **690**；按我最近的偏差（T036 估 320、实际 452）乘 1.2，约 **830**。超过停止线 520，也超过硬停止线 600。按守则先不写代码，停下汇报。

## 拆分建议

| 拆分 | 内容 | 估计（直接相加） | 乘 1.2 |
|---|---|---|---|
| **T188a 报告校验、已验证范围与 compat-framework 进 `ci pr`**（Rust） | 上表第 1、2 行 | 约 287 | 约 345 |
| **T188b 审阅工具：`rec-shape-diff` 与 `trace-ids --compat`**（TS） | 上表第 3、4 行 | 约 405 | 约 485 |

- 两半互不依赖，可以先后做，也可以并行。
- T188b 改 `tests/ids.toml` 的 COMPAT 行，和 T187 的剧本 `id` 有关，放在 T187 之后做冲突最少。
- T066 依赖本任务；T066 真正需要的是 T188a（报告入库后 CI 校验、生成范围）。T188b 是评审和编号的工具，T066 在评审升级时才用得上。

## 裁定后（计划 v1.3.62）的范围与估计

只做 Rust 的一半：停止线 400，硬停止线 460。`rec-shape-diff`、`trace-ids --compat` 和 COMPAT 编号的登记移到 T194。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `compat/reports.rs` | 子命令与参数；读目录、`parse` + `problems`；schema 检查；版本解析；计入条件；算范围；与清单比对、`--write` | 140 | 85 |
| 接线 | `report.rs` 的 schema 生成挪出测试；`main.rs` 分发与用法；`manifest.rs` 注释；`testdata-scan` 加一步 | 25 | — |
| compat-framework 进 `ci pr` | 路径表；按改动跳过（与 ui 作业共用一个判断，放进 `ci/git.rs`）；组合作业加一项 | 25 | 12 |
| **合计** | | **约 190** | **约 97** |

直接相加约 287，乘 1.2 约 345。

- 计入条件里，剧本的结果用穷尽的 `match` 判断。T187 加"不适用"一类的取值时，这里会编译不过，逼合并的人按裁定决定它算不算通过，不会悄悄落进某一边。

## 实际行数（两个代码提交之后）

`cargo xtask size`：**+550 −46**，超过停止线 400，也超过硬停止线 460。按守则停在这里，没有跑记录 CI。

| 文件 | 内容 | 实际（新增行） | 估计 |
|---|---|---|---|
| `compat/reports.rs` | 子命令、三项检查、范围、`--write` | 203 | 140 |
| `compat/reports/tests.rs` | UNIT-compat-tools 三个用例与夹具 | 209 | 85 |
| `ci/jobs/compat_framework.rs` | 路径表、跳过、用例（含造分支的辅助函数） | 76 | 25 + 12 |
| `compat/report.rs` | schema 生成挪出测试（搬移的 21 行也按新增算） | 24 | — |
| `ci/git.rs`、`ui.rs`、`jobs/mod.rs`、`testdata_scan.rs`、`main.rs`、`manifest.rs`、`compat/mod.rs` | 共用的路径判断、接线、注释 | 38 | 25 |

估计偏低的原因：
- 测试夹具：真实模式的报告要带配置文件哈希、模型等字段，每个剧本的检查项和说明也要和结果一致。用结构体字面量写，rustfmt 每个字段占一行，约 45 行；
- 三个用例各覆盖多种情况：计入与不计入的五类、两种错误、版本解析、不合格的报告不写清单、写后再查；
- `compat-framework` 的用例要造一个从 main 分出的临时仓库；
- 搬移的 schema 函数按新增行计。

另外，我没有在写完 `reports.rs`、开始写测试之前统计一次行数，没照守则第 8 条做到每个模块后统计，否则能早一步发现。

### 可选的处理

1. 上限提到约 560（硬停止线 600），照现在的代码跑记录 CI。
2. 压缩测试：夹具改用 `json!` 写（参照 `report.rs` 测试里的写法），用例合并重复的搭建步骤。估计可以压到约 450，覆盖不变，但夹具会失去类型检查。

### 验证（已做）

- xtask 全部测试通过（57 个）；clippy（本机与 `x86_64-unknown-linux-gnu`）无警告；fmt 通过。
- `rustup target add x86_64-unknown-linux-gnu` 后，`cargo check -p xtask --all-targets --locked --target x86_64-unknown-linux-gnu` 通过。构建脚本不用交叉编译 C 代码，`rust-toolchain.toml` 没改。
- 在本仓库上运行 `cargo xtask compat-reports`：通过（没有报告，清单等于基线）。
- 变异检查：去掉"适配器版本等于锁定版本"、把 `inconclusive` 算作通过、有问题时仍写清单，各有一个用例失败。
