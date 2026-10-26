# T188 兼容套件 III：审阅与校验工具

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师）。T054、T060、T098、T059、T129、T089、T169、T175、T177、T061、T186、T036 的执行者接着做
- 分支：`feat/T188-compat-tools`
- 依赖：T064（已合并）。T066 依赖本任务

## 背景

兼容套件每次运行产出报告和录制（设计 5.9.5）。报告要在 CI 里校验，"已验证 CLI 范围"要从报告重新生成、与清单比对，不手填；升级适配器时评审看录制的形态差异，不看事件全文；COMPAT 编号要从报告推导完成状态。T064 做了报告与 schema，计划 v1.3.55 把这几件工具拆给本任务。

## 范围

**做**（计划 T188 一行；设计 5.9.5、5.1.6 的 testdata-scan 与 compat-framework、5.10.3）：
1. **`testdata-scan` 加报告校验**：`testdata/compat/reports/` 下的每份报告都用 `report::parse` 严格读入、过 `report::problems`，schema 文件不过期。目录为空或不存在时通过。
2. **已验证范围重算**：
   - 没有报告时以 ADR-08 的起始值为基线：claude 2.1.200–2.1.296，codex 0.150.1–0.163.0；
   - 有报告后，取基线与计入的报告的 CLI 版本，下限取最小、上限取最大；
   - 计入的条件：真实模式，没有失败的剧本。"不确定"和"不适用"怎么算由你提出，写进汇报；
   - 与 `runtime/manifest.json` 的 `cli.*.verified` 比对，不一致就失败，提示该用哪条命令重写；
   - 提供重写清单这部分的命令（命令名由你定），让清单里的范围始终由报告生成；`manifest.rs` 里"手工维护到 T064、T066"的注释相应改掉。
3. **`tools/rec-shape-diff`**：对比两份录制里的方法、update 种类、`_meta` 键路径、`option_id`、`stopReason`、错误码，输出形态差异报告。与 `tools/sanitize`、`tools/trace-ids` 一样用 TS 写。
4. **`tools/trace-ids --compat <报告>`**（可重复）：通过的剧本让对应的 COMPAT 编号算作完成。
   - 剧本与编号的对应要显式，不按名字猜：现有编号的写法不统一（`COMPAT-claude-thinking` 与 `COMPAT-untrusted-project-claude`）。
   - 建议在 `tests/ids.toml` 里给 COMPAT 编号登记剧本和 CLI，例如 `COMPAT-claude-thinking = { suite = "compat", scenario = "thinking", cli = "claude" }`，这样不用改报告的 schema，也不碰 T187 正在改的剧本表。另有更好的做法也可以，先写进 `notes.md` 再动手。
5. **compat-framework 进 `ci pr`**（T064 的交接）：设计 5.1.6 要求改动兼容套件的 PR 也跑。照 ui 作业的做法用 `git::changed_since_main` 按路径判断，没有改动相关路径时跳过。相关路径由你列出并写进注释。

**不做**：真实模式、剧本补全、`lsof` 断言（T187）；用真实 CLI 跑（T066）。

## 验收标准

- [ ] UNIT-compat-tools：
  - 报告校验：合格的报告通过，多余字段、语义问题、schema 过期各自报错；
  - 已验证范围：没有报告时等于基线；计入与不计入的报告各有用例；与清单不一致时失败；
  - `rec-shape-diff`：相同形态时没有差异，六类差异各有用例；
  - `trace-ids --compat`：通过的剧本让编号完成，失败的不算，没有登记的剧本报错或提示。
- [ ] `testdata-scan` 在 ubuntu 上也能编译和运行（设计 5.1.3 对 xtask 的要求：不依赖 macOS 专用的 crate）。
- [ ] `ci pr` 在没有兼容套件改动的分支上跳过 compat-framework，有改动时运行。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T188 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T188-compat-tools/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T188 的行；开工前逐行读一遍）

- T064：读报告用 `report::parse` 加 `report::problems`；schema 在 `testdata/compat/report.schema.json`；`testdata_scan.rs` 的 `WORD_LIST`、`word_list` 已改成 `pub`；compat-framework 还没进 `ci pr`。
- T002：`trace-ids` 的 `--compat`，在那之前 COMPAT 编号一直是待完成。
- T004：清单里还没有由报告生成的"已验证 CLI 范围"。
- T187 同时在做：它会给报告加"不适用"之类的结果取值，并让真实模式的 `--write` 把报告写进 `testdata/compat/reports/`。你按现在的 schema 写，测试用的报告放在测试夹具里；与它的改动在合并时由编排者处理。

## 注意

- 不运行真实的 Claude Code、Codex CLI。
- 测试夹具里的报告与录制不含个人信息。

## 规模

- 停止线：520；硬停止线：600

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T188-compat-tools main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
