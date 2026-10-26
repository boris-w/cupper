# T064 兼容套件框架

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师）。T043、T055、T008、T116、T046、T090、T167、T091、T092、T185、T063 的执行者接手（录制导出 T116、xtask 的作业都是它做的）
- 分支：`feat/T064-compat-framework`
- 依赖：T044、T047、T056、T116（都已合并）。T066（首批真实录制）依赖本任务

## 背景

兼容套件（设计 5.9.5）用真实的 Claude Code、Codex CLI 跑一组剧本，检查 cupper 和它们的兼容性，产出录制和报告；"已验证 CLI 范围"由报告生成。真实运行会消耗额度，只由维护者手动跑（T066），CI 里不跑。本任务做框架，以及不消耗额度的 `--fake` 模式。

## 范围

**做**（计划 T064 一行；设计 5.9.5、5.1.6 的 compat-framework 与 testdata-scan）：
1. **`cargo xtask compat`**：
   - 剧本全集：5.9.5 列出的全部剧本，含 Q8、未信任项目、Q28、Q34、思考流；
   - 只断言结构，不断言 agent 的具体文本；
   - 真实配置的前后哈希对比：运行前后对 `~/.claude`、`~/.codex` 等真实配置做哈希，只读不改，变了就判失败（安全 #8）；
   - 报告与它的 JSON schema；
   - 录制脱敏（`tools/sanitize`），过本机敏感词清单（`.private/sensitive-words.txt`，没有时明确报错）后才入库。
2. **真实模式的自我约束**（这几条要有单元测试，即 UNIT-compat-guards）：
   - 每个剧本显式设权限模式：Claude 用 `default`，Codex 用 `read-only` 或 `workspace-write`；
   - 拒绝 privileged 选项和放宽档位，除非剧本声明了、而且只作用于临时仓库；
   - 临时仓库只建在每个用户自己的临时目录。
3. **`--fake`**：同一套剧本用假 agent 经宿主跑，不消耗额度。
   - 可重复地产出 B 层录制与金样例，替换 T065 首批录制用的一次性程序（计划 v1.3.47）；
   - 用法见 `docs/tasks/T065-gold-b-runner/notes.md`。
4. **`xtask ci compat-framework`**：跑 `--fake` 和单元测试，不跑真实 CLI。
5. **`testdata-scan`** 加报告校验和已验证范围的重算。
6. **`tools/rec-shape-diff`**：比较两份录制的结构差异。
7. **`lsof` 断言**：剧本跑完没有遗留的文件描述符或进程。
8. **`tools/trace-ids` 加读兼容报告的入口**（`--compat <报告>`）：在那之前 COMPAT 编号一直是待完成（T002 的交接）。

**不做**：用真实 CLI 跑（T066，维护者手动）。

## 验收标准

- [ ] `cargo xtask ci compat-framework` 通过。
- [ ] UNIT-compat-report：报告的生成与 schema 校验。
- [ ] UNIT-compat-guards：真实模式的各条自我约束。
- [ ] `--fake` 能重新产出 T065 的三份 B 层录制与金样例，结果与入库的一致（或者差异可以解释）。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T064 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T064-compat-framework/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T064 的行；开工前逐行读一遍）

- T116：录制导出 `diag/recording_export` 的用法与注意（头行改写、`--base`、`max_blob_bytes`、孤儿 blob）。
- T067、T065：回放规则；B 层只用宿主驱动的录制；首批录制的一次性程序。
- T004、T053：清单里还没有"已验证 CLI 范围"；`compaction_update` 要用兼容剧本核对。
- T029：CLI 的 `Client` 能收发 blob 分片。
- T002：`tools/trace-ids` 的 `--compat`。

## 注意

- **本任务里不运行真实的 Claude Code、Codex CLI**，只用 `--fake`；真实模式的代码路径靠单元测试和 dry-run 验证。
- 不读、不改用户的 `~/.claude`、`~/.codex`；前后哈希对比的代码在测试里用临时目录代替。
- 入库的录制与报告不含个人信息。

## 规模

- 停止线：1300；硬停止线：1450

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报，并给出拆分建议。 -->

## 工作方式

在原来的 T043 worktree 里从最新的 main 开新分支：`git checkout -b feat/T064-compat-framework main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
