# T002 工作笔记

## 方案

### 数据流

1. 测试运行器产出原始报告：现在是 libtest 的文本输出和 Vitest 的 JSON；以后是 nextest 的 JUnit（改用 nextest 时）和 Playwright 的 JSON（T090）。
2. `xtask ci` 的作业把原始报告解析成测试用例，`--record` 时写进摘要的逐编号结果清单（T001 已有）。
3. `tools/trace-ids` 读登记表 `tests/ids.toml`、文档里的编号、结果和 MAN 证据，给出已完成和待完成清单，并在里程碑退出时把关。结果来源有两种：
   - `xtask ci` 的摘要（入库的逐编号结果清单，里程碑退出只认这一种）；
   - 原始报告：nextest 的 JUnit 和 Playwright 的 JSON，由调用方指明套件。

### 放在哪里

- `tools/trace-ids` 是 TS 工具，与 `tools/gen-protocol-ts` 一样由 Node 直接运行源码（设计 5.1.2 把它列在 `tools/` 下，计划 1.5 把它划在区域 C）。它的 Vitest 用例由 `ts` 作业自动运行，UNIT-trace-ids 因此进入摘要。
- `xtask ci testdata-scan` 只在自己的文件 `jobs/testdata_scan.rs` 里调用 `trace-ids check`（brief 的"并行注意"）。
- xtask 另有一处一行的改动：摘要匹配编号时，除了文档，也读 `tests/ids.toml`。否则任务自己新增、计划还没写进去的编号，摘要里永远没有结果，trace-ids 会一直把它算作待完成。

### 不改用 nextest

- 计划 1.6 写的是 trace-ids 读 nextest 的 JUnit。本机没有装 cargo-nextest；装它要改作者机器上的 `~/.cargo/bin`，编排者合并前跑 `xtask ci pr` 也离不开它，应当由用户决定。另外 nextest 每个用例一个进程，不跑文档测试，`rust`、`protocol`、`security-macos14` 三个作业都要跟着改。验收标准里也没有这一项。
- 所以本任务只让 trace-ids 能读 nextest 的 JUnit 报告（按格式实现，用样例测试，同 Playwright），xtask 继续解析 libtest。改用 nextest 时，`ctx.rs` 的 `parse_libtest` 按交接事项替换，摘要格式不变，trace-ids 不用改。

### 登记表的格式

- 每个产出任务一段 `[Txxx]`，段里的 `milestone` 是这一段的默认里程碑，其余每个键都是一个编号。
- 值是字符串时就是套件；需要更多字段时写成内联表：`suite`、`milestone`（覆盖段的默认值）、`status` 与 `reason`、`parts`（子编号）、`versions` 与 `max_age_days`（MAN 证据）。
- 没有产出任务的编号放在 `[unplanned]`；文档里出现、但不是用例编号的写法放在 `[not-ids]`，值写原因；MAN 证据追加在文件末尾的 `[[evidence]]`。
- 说明性的备注一律写 TOML 注释，不进字段。

### 套件

套件就是 `xtask ci` 的调用目标，摘要里每条结果的 `suite` 记的也是它：

| 套件 | 含义 | 用于 |
|---|---|---|
| `pr` | 每个 PR | UNIT、PROP、GOLD、IT、UI，以及 5.12.4 标〔PR〕的 E2E |
| `nightly` | 每晚 | PERF、5.12.4 标〔晚〕的 E2E、差分测试 |
| `release` | 发布流水线 | SMOKE-SIGNED |
| `compat` | 兼容套件（维护者本机手动运行） | COMPAT；结果来源（兼容报告）由 T064 接入 |
| `man` | 人工 | MAN；靠证据，不靠测试结果 |

结果的套件必须与登记的完全相同。`nightly` 包含 `pr` 的全部作业，但 e2e 在 PR 上只跑冒烟子集，所以在 `nightly` 里通过不能代替 `pr`。

### 状态

| 状态 | 含义 | 挡不挡里程碑退出 |
|---|---|---|
| （默认） | 由结果或证据推导 | 没完成就挡 |
| `conditional` | 条件任务的编号，`reason` 写触发条件 | 挡；检查点通过后改为 `na` |
| `na` | 不适用，`reason` 写原因，例如"K1 已通过" | 不挡 |
| `gap` | 缺口：本机替代不了 | 挡，直到用户确认 |
| `gap-confirmed` | 缺口（用户已确认），`reason` 写确认记录 | 不挡 |

### 逐编号的推导

1. `na`、`gap-confirmed`：已了结。`gap`、`conditional`：待完成，附原因。
2. 有子编号的：全部子编号了结才算完成；否则列出没完成的子编号。
3. MAN：看证据。没有证据是待完成；证据日期晚于今天、超过 `max_age_days`、或与绑定的组件版本对不上（例如 MAN-keyboard 绑定 Electron 的大版本，从 `apps/desktop/package.json` 读当前版本）都算过期。
4. 其余看结果：任何一处失败（含 flaky）是"失败"；在登记的套件里通过是"完成"；只在别的套件里通过是"套件不符"；只有被跳过的用例是"被跳过"；什么都没有是"没有测试"。
5. 结果里出现未登记的编号、或者父编号自己有结果，都报出来。

### 命令

- `trace-ids check`：登记表的一致性检查（`testdata-scan` 运行它）。
- `trace-ids status [来源]`：列出已完成、待完成和问题，只报告，不判失败。
- `trace-ids verify --milestone <M> | --task <T> [来源]`：所选编号全部了结才通过。`--milestone` 包含更早的里程碑，只接受入库的摘要，并且要求所有摘要是同一个提交。

## 行数估计（开工前）

`tests/ids.toml` 是数据，不计。测试按计划 1.1 的经验单独估（工具类任务的测试和夹具往往比实现多）。

| 模块 | 内容 | 代码 | 测试 |
|---|---|---|---|
| 包骨架 | `package.json`、`tsconfig.json`、`vitest.config.ts` | 40 | — |
| `registry.ts` | 读 `ids.toml`，校验段、字段、状态、子编号、证据 | 170 | 150 |
| `docs.ts`、`match.ts` | 从文档抽编号；测试名到编号的精确匹配（与 `ids::owner` 同一规则） | 60 | 50 |
| `results.ts` | 读摘要、nextest 的 JUnit、Playwright 的 JSON | 140 | 120 |
| `evaluate.ts` | 逐编号推导、MAN 证据、子编号、里程碑与任务筛选 | 150 | 150 |
| `main.ts` | 命令行：`check`、`status`、`verify` | 110 | 30 |
| xtask | `jobs/testdata_scan.rs`、注册、`ids.rs` 读登记表 | 40 | — |
| **合计** | | **约 710** | **约 500** |

**总计约 1,210 行**，超过计划的 M（400–900 行），在约 1,500 行的上限以内。每完成一个模块统计一次；按实际偏差推算会超过约 1,500 行时，停下来汇报。到时可以先拿掉的部分：`--task` 筛选、父编号自己有结果的检查。

## 实际行数

`git show --shortstat <提交>`，不含 `pnpm-lock.yaml` 和 `tests/ids.toml`：

| 完成的部分（提交） | 新增 / 删除 | 其中测试 | 估计（代码 + 测试） | 累计 |
|---|---|---|---|---|
| `tools/trace-ids` 全部模块与测试（`b21215f`） | 1,418 | 509 | 约 1,170 | 1,418 |
| `tests/ids.toml`（`40e2332`，数据，941 行） | 不计 | — | — | 1,418 |
| xtask：`testdata-scan` 作业与登记表进摘要匹配（`3ac4012`） | 25 / 4 | — | 约 40 | 1,443 / 删 4 |
| 用法文字的排版（`281c1bc`） | 3 / 3 | — | — | **1,446 / 删 7** |

- 工具的五个模块是一起写完的，提交前统计了一次（1,430 行，含格式化后多出的行），比估计多约 20%：Prettier 把校验逻辑展开得比预估长（`registry.ts` 313 行，估 170）。按剩下的 xtask 部分（估 40 行）推算总数约 1,470，在约 1,500 的上限以内，所以没有停下；当时把 `registry.ts` 开头重复登记表头部说明的注释删掉，留了一点余量。
- 合计约 1,450 行，超过计划的 M（400–900），在上限以内。超出的主要原因是测试（509 行）：五种报错情况、三种结果来源和登记表的各种格式错误各要一组夹具。

## 登记时拿不准的地方

以计划 v1.3.6 的任务表为准。以下是表里没有直接写明、由我判断的：

1. **IT-handshake**：T030 产出 peer、codesign 和错误码部分，T039 补密钥部分，计划没有拆子编号。按"实际产出"登记在 T030，注释写明 T039 的部分。摘要里的通过结果不能说明密钥部分也在；需要的话可以拆成 `-peer`、`-key` 两个子编号。
2. **跨任务的同一编号只登记一次**：PERF-bridge-gate 在 T078（T079 达标后重测同一编号）；MAN-keyboard 在 T077，绑定 Electron 的大版本，T107 升级后补新证据；E2E-review-comment、E2E-SEC-revert-confirm 在 T131（K9 不通过时由 T139 产出）；E2E-notification-navigate 在 T150（K5 不通过时由 T158 产出）。
3. **父编号放在完成它的任务里**：UNIT-proto-events（T017）、UNIT-proto-methods-core（T018）、IT-sys-peer（T021）、GOLD-A-tool-fields、GOLD-A-option-map、UNIT-option-map（T110）、UNIT-cli-resolver（T111）、MAN-compat、COMPAT-untrusted-project（T113）、E2E-new-session-agent（T144）、UI-settings（T147）、MAN-ui6（T148）、MAN-notification（T150）、IT-NFR9-failpoints（T172）。
4. **5.12.4 没有标套件的编号**：
   - 记为 `nightly`：E2E-S2、E2E-S4（同 S1、S3）、E2E-notification-navigate、E2E-selfuse-coexist、E2E-selfuse-replace、IT-selfuse-build（要构建自用版 App）、UNIT-views-differential（计划写明接入 nightly）、全部 PERF（含 PERF-bridge-gate、PERF-snapshot）；
   - 记为 `pr`：E2E-SEC-no-key-client、E2E-SEC-double-initialize、E2E-SEC-cli-fake-host（同 E2E-SEC-fake-host）；
   - 记为 `release`：UNIT-signing-check（检查签名后的产物）；
   - 缺口 IT-sys-peer-macos14 记为 `nightly`（`security-macos14` 作业），IT-sys-peer-macos15 记为 `pr`（5.1.6："macOS 15 由 `rust` 作业覆盖"）。
5. **COMPAT 的套件是 `compat`**：5.12.4 把它们标为〔人〕，但它们的结果是兼容报告里机器可读的剧本结果，不是 MAN 那样的证据。trace-ids 还没有读兼容报告的入口，T064 定下报告格式时补一个 `--compat <报告>`，在此之前 COMPAT 一直是待完成。
6. **金样例的"其余录制"**：T053、T110 的验收写了"及其余 Claude（Codex）录制"，没有逐个列出，由它们各自追加。
7. **没有产出任务的编号**：IT-n-minus-1-cli（第二个发布版起）记为里程碑 `later`；MAN-tcc（M2 的 SP-10）记为 `M2`。
8. **条件任务的编号**：T079、T119、T139、T158、T162 的编号记为 `conditional`，原因写触发条件；检查点有结论后改为 `na` 或去掉状态。T057 的 IT-q8-fallback-claude 已是 `na`（K1 已通过）。
9. **不是用例编号的写法**（`[not-ids]`）：PRD 的需求编号 UI-0 至 UI-7；旧名 E2E-question-card、UI-cards；计划 T140 的注里写的 IT-K11（应为 IT-K11-steer-idle-queued，计划改成完整编号后可以从 `[not-ids]` 删掉）。

## 合并时与计划 v1.3.9 同步

本分支从计划 v1.3.6 开出。main 上的计划已到 v1.3.9，拆出了 T019、T029、T058、T087，并新增 IT-gw-blob。合并 main 后 `trace-ids check` 会报 IT-gw-blob 未登记。按 v1.3.9 的任务表，`tests/ids.toml` 要这样改：

```toml
# [T033] 去掉 IT-gw-subscribe

# 网关 III：订阅、历史与发送合并
[T019]
milestone = "M1a"
IT-gw-subscribe = "pr"

# 网关 IV：blob 上传与下载
[T029]
milestone = "M1a"
IT-gw-blob = "pr"

# [T041] 去掉 PROP-autonomous-dequeue-guard、PROP-turn-snapshot-immutable

# 会话纯核心 II-b：自发回合判定与组合
[T058]
milestone = "M1a"
PROP-autonomous-dequeue-guard = "pr"
PROP-turn-snapshot-immutable = "pr"

# [T071] 去掉 UNIT-hc-sync

# host-client I-b：订阅与同步
[T087]
milestone = "M1a"
UNIT-hc-sync = "pr"
```

各段放在任务表里相应的位置（T019、T029 跟在 T033 后，T058 跟在 T041 后，T087 跟在 T071 后）。
