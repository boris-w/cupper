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

（每完成一个模块补一行。）

## 登记时拿不准的地方

（登记过程中补充。）
