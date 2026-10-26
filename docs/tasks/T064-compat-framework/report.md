# T064 汇报

- 执行者：子 agent（Rust 工程师；T043、T055、T008、T116、T046、T090、T167、T091、T092、T185、T063 的执行者）
- 分支：`feat/T064-compat-framework`
- 完成日期：2026-10-26

T064 按收窄后的范围完成了：框架、`--fake` 和 `ci compat-framework`。`ci compat-framework` 和 `ci pr --record` 都通过，摘要已随分支提交。最终新增 1,126 行、删除 8 行，在停止线 1,150 以内。中途合计到过 1,155 行，超线 5 行，收紧写法后降回线内（见"规模"）。

## 做了什么

**执行者：cupper-host 的测试目标 `compat`**
- 文件是 `tests/compat.rs` 和 `tests/compat/scenarios.rs`，要求 test-harness 特性。
- 每个剧本单独起一个带假 agent 的宿主、用一个临时目录，在 tokio 任务里跑。某个剧本 panic 记为失败，后面的剧本照常跑。
- 步骤：
  - 等待请求；
  - 按选项种类作答（只用 `allow_once`、`reject_once`）；
  - 回答提问（取每个字段的第一个选项，或给定的文本）；
  - 暂停、中断，最后等回合结束。
- 步骤等待超时（30 秒）记为"不确定"，按计划的 `attempts` 重跑。
- 结构断言都读宿主库里本会话的事件：回合状态、条目种类、工具种类、非零退出码、请求的最终状态、会话能力、项目里没有某个文件。不看 agent 的文本。
- "基本"组 10 个剧本：
  - new-session、normal-turn、command-exit、approval、reject、question、plan、cancel、cancel-approval、thinking；
  - normal-turn、approval、cancel 标为 gold-b。
- 导出录制的做法沿用 T065 的一次性程序：改写头行，项目路径（链接路径和真实路径）换成 `/work/project`。

**编排：`cargo xtask compat --fake [--only <剧本>]... [--out <目录>] [--write [--words <文件>]]`**
- 先构建假 agent，再把计划交给执行者，读回 `run.json` 生成报告。报告先序列化，再严格读回、过语义检查，然后才写出。
- 没有 `--fake` 时直接报错，说明真实 CLI 还不能驱动。

**报告与 schema（`crates/xtask/src/compat/report.rs`）**
- 报告内容：提交、运行模式（fake、real_config、isolated_config）、agent、模型、适配器和 CLI 的版本、总耗时；每个剧本的结果（通过、失败、不确定）、原因、检查项、尝试次数、耗时、录制位置；配置文件的前后哈希；需要清理的 CLI 会话。
- 校验分三步：
  1. 严格反序列化（`deny_unknown_fields`）；
  2. `problems` 做语义检查：版本、完整提交哈希、fake 与真实模式各自的约束、剧本编号唯一、尝试次数和整数范围、结果与检查项一致；
  3. 入库的 schema 文件 `testdata/compat/report.schema.json` 不过期。
- schema 由 schemars 生成，键排好序。工作区里有别的 crate 打开了 serde_json 的 `preserve_order`，不排序的话，单独构建 xtask 和整个工作区构建得到的键顺序不同。

**`--write`（`crates/xtask/src/compat/record.rs`）**
1. 只取三个 gold-b 剧本的录制。
2. 用 `tools/sanitize` 脱敏：`--base` 传各临时根目录的两种写法，`--terms` 传敏感词清单。脱敏后还有残留就失败。
3. 写入 `testdata/recordings/<adapter>/<ver>/<cli-ver>/`。归一化后（去掉 `t`，id 按出现顺序重编号）与入库的相同时，保留原文件不重写。
4. 跑 `UPDATE_GOLDEN=1` 的 `gold_b` 测试重写金样例，再对 `testdata` 做通用扫描。
- 敏感词清单默认取主工作区的 `.private/sensitive-words.txt`，也可以用 `--words` 指定。两处都没有时，在跑任何东西之前就明确报错。

**`ci compat-framework`**
- 先跑 `cargo test -p xtask -- compat::`，再用当前的 xtask 二进制跑 `compat --fake`，输出放在日志目录下。
- 不跑真实 CLI。

**测试数据**：cancel 剧本的录制和金样例重新产出过（理由见"关键取舍"）。

## 关键取舍

- **执行者用"普通 libtest 目标里一个忽略的测试"，不是裁定里写的 `harness = false` 目标。** 这是和裁定的一处出入。
  - 原因：`cargo test --workspace` 会不带参数地运行 `harness = false` 的二进制；rust 作业的 `cargo test ... gold_` 还会把过滤词 `gold_` 当参数传给它。
  - 现在的做法：忽略的测试 `compat_driver` 在这两种情况下都不运行，只有 xtask 用 `--ignored --exact compat_driver` 点名并设好 `CUPPER_COMPAT_PLAN` 时才跑。
  - 裁定认可的其他结构不变：编排在 xtask，执行者复用 `tests/common`，xtask 不依赖 cupper-cli。
- **剧本表放在执行者里，xtask 只认报告。** 这样步骤只定义在执行它的地方，不用两边各写一套类型。xtask 只定义报告和 `run.json` 的类型，T188 在 ubuntu 上做校验时只需要这些。
- **cancel 剧本改成"命令执行中取消"**：先允许一条命令，1 秒后中断，对应 5.9.5 的定义。T065 的一次性程序是在回答进行中取消。
  - 为了确认生成器本身没问题，把脚本临时换回 T065 的版本跑了一次（没提交），结果与入库的相同。
  - 正式改动后，录制和金样例都更新了：工具调用以 `abandoned` 结束，回合 `cancelled`。
- **真实模式要用的东西都没写进来**：真实提示词、权限模式、准备文件、自我约束。现在写进去就是没人用的代码，留给 T187。plan 剧本已经以 `plan` 模式开始，这一点两种模式通用。
- **报告里有 `config_files`、`cli_sessions` 两个字段**，fake 模式下为空。语义检查要求真实模式必须有配置哈希。这样 T187 不用再改 schema。

## 验证方式

- **`cargo xtask ci compat-framework --record`**：通过，8 项（7 个单元测试加 compat-fake）。摘要 `ci/compat-framework-0f4212edc59d.json`。
- **UNIT-compat-report**，共 4 个测试：
  - 往返并符合约束；
  - 多余字段或未知取值时读入失败；
  - 语义检查能查出 7 类问题，真实模式缺配置哈希也会报；
  - schema 文件不过期。
  - 单独构建 xtask 和整个工作区构建（`--workspace --all-features`）下都通过。
- **`--fake` 重新产出 T065 的三份录制**：

  | 剧本 | 结果 |
  |---|---|
  | normal-turn、approval | 归一化后与入库的相同，文件和金样例都没动 |
  | cancel | 剧本改了，差异就是上面说的那一处 |

  - 不设 `UPDATE_GOLDEN` 跑 `gold_b`，9 个都通过。
  - 再跑一次 `--write`，三份都是 unchanged，文件逐字节不变。
  - 本机主工作区没有敏感词清单。验证 `--write` 时用的是草稿目录里的一份临时清单，只有一个不会出现的词；不带 `--words` 时按预期报错。
- **结构断言不是永真**：把几项检查临时改成相反的期望再跑，它们按预期失败；改动没提交。
- **测试命名**：带编号的测试都以 `unit_compat_report_` 开头。其余是不带编号的辅助测试：参数解析、归一化、清单缺失。
- **`cargo xtask ci pr --record`**：在干净的 worktree 里，提交 eb12d67 上 7 个作业全部通过。摘要 `ci/pr-eb12d67291c5.json`。
  - 跳过的步骤：testdata-scan 的 sensitive-words（本机没有清单）、ui 的 test-ui（ui 作业自己的判断）。
- **规模**：
  - 执行者第一版是 554 行，收紧后 492 行。
  - xtask 部分写完时合计 1,155 行，超过停止线 1,150 共 5 行。改用 JSON 构造测试用的报告、简化提交哈希的检查等，降到 1,124 行。
  - 最终新增 1,126、删除 8（不含 lock 文件、`testdata/`、`docs/`）：cupper-host 492，xtask 632，`.prettierignore` 2。
  - `cargo xtask size` 还按 main 上旧的 1,300 显示。分支上的任务书在合 main 之前读不到新数，所以按你说的手工对照了 1,150 和 1,300。

## 提交列表

```
35c6adc docs(task): T064 notes on the final size and a hang seen in CI
fbcab18 docs(task): T064 PR CI summary
eb12d67 fix(xtask): prettier leaves the generated compat report schema alone
e56db73 docs(task): T064 compat-framework CI summary
0f4212e docs(task): T064 notes on the runner, the report and the GOLD-B check
cc2a392 ci: compat-framework runs xtask compat --fake and its unit tests
115257b test(gold-b): the cancel recording interrupts a running command
be25f66 feat(xtask): cargo xtask compat --fake and its report
a496efc test(compat): the runner and the basic scenarios
4202347 docs(task): T064 size estimate and split proposal
```

就地修正一处（2 行，不改接口）：eb12d67。第二次跑 `ci pr` 时，ts 作业的 prettier 检查报了生成的 schema 文件。照协议 schema 的做法，把它加进了 `.prettierignore`。

## 遗留问题 / 范围外发现

1. **已有测试偶发卡死（范围外，没有修）。**
   - 位置：第一次跑 `ci pr` 时，`test-host-release` 步骤（不带特性的 `cargo test -p cupper-host`）里的 `orchestrator::session::trust_tests::it_orch_trust_gating_two_reasons_hold_until_both_go`，28 分钟没有结束。
   - 栈：测试的 current-thread 运行时在等待，宿主的写者线程空闲。看起来是在等一个不会来的事件，而这个进程内夹具等待时没有超时。
   - 处理：用 SIGTERM 结束了本次运行拉起、核实过的那个测试进程。单独跑这个测试 40 次都通过，后两次完整的 `ci pr` 里也通过。
   - 建议：另开任务查竞态，并给夹具的等待加上限。
2. **compat-framework 还没进 `ci pr`。** 设计 5.1.6 写的是"改动 xtask 兼容套件的 PR"也要跑这个作业。本任务没有加；要加的话照 e2e 的做法按路径判断，由你决定放进哪个任务。
3. **留给 T187 的：**
   - 每个剧本的真实提示词、权限模式（Claude `default`）、准备文件，以及自我约束。
   - 给 `session/create` 加显式权限模式后，三份 B 层录制的 `config` 会变。到时用 `--fake --write` 重新产出，差异可以解释。
   - cancel 剧本在真实 CLI 上是"允许后等 1 秒再中断"；`sleep 60` 不输出，是否可靠要实际跑一次才能确认。
4. **留给 T188 的：** 读报告用 `report::parse` 加 `report::problems`，schema 文件在 `testdata/compat/report.schema.json`。`testdata_scan.rs` 开头的注释已改为"报告校验归 T188"；`WORD_LIST`、`word_list` 改成了 `pub`，`--write` 也用它们。
5. **`tests/ids.toml`**：分支上的 T064 段还是旧的，里面有 UNIT-compat-guards。main 上已经移到 T187，合 main 时以 main 为准。本任务的测试只用 UNIT-compat-report。

notes.md 里另有做法、对照表和规模的明细。
