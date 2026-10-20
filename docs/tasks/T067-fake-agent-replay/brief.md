# T067 假 agent 回放模式与 persona 比对

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T052、T165、T168 的执行者接着做（T052 做过录制格式与录制导出）
- 分支：`feat/T067-fake-agent-replay`
- 依赖：T052（已合并）。T099 依赖本任务；T065（B 层金样例运行器）会用到回放

## 背景

T052 拆分时（计划 v1.3.12），回放模式归了本任务，UNIT-replay 也随之移来。你在 T052 做了录制格式库（`cupper-protocol::recording`）。SP-1 的 48 份录制已经入库。T051 做的假 agent 有 `--segment`、`--segment-file`，persona 与录制形态的核对目前只用测试里的常量逐条比。

## 范围

**做**（计划 T067 一行；设计 5.9.2 的回放模式）：
1. **假 agent 的 `--replay <录制>`**：
   - 收到宿主的帧时，与录制里下一个 `send` 帧比对：
     - 方法相同；
     - JSON-RPC id 重映射；
     - 路径换成占位；
     - `_meta` 忽略；
     - prompt 精确比较。
   - 比对通过后，按序发出后面的 `recv` 帧；比对失败时以明确的错误退出，指出哪一帧、哪个字段。
   - 进程级回放按 1/100 压缩时间。
   - 适配器发起的请求（例如权限请求）要等宿主应答，并要求应答的选项与录制一致。
   - 多实例录制按段回放，接 `--segment-file`（每次启动段号加 1）。
2. **persona 与 SP-1 录制形态的自动比对**：比对能力、configOptions、默认模式、命令列表、权限选项 ID，替换 T051 里的常量核对。jq 摘取命令在 `docs/tasks/T051-fake-agent-scripts/notes.md` 里。

**不做**：宽松比对档（没有使用方）；B 层回放（T065）。

## 验收标准

- [ ] UNIT-replay：
  - 每条比对规则各有正反用例（方法、id 重映射、路径占位、`_meta`、prompt）；
  - 适配器发起的请求等应答且选项一致；
  - 时间压缩；
  - 按段回放；
  - 比对失败时的错误信息。
- [ ] 至少用一份 SP-1 的 claude 录制做端到端回放：宿主 → 假 agent `--replay` → 回合完成。
- [ ] persona 的自动比对覆盖上面列的五项，与入库的录制对不上时失败。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T067 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T067-fake-agent-replay/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T067 的行）

- T052：录制格式库的用法（`Recording::parse`、`segments()`、`Body::event()`、`INSTANCE_STARTED`、`INSTANCE_EXITED`）；录制在 `testdata/recordings/` 下的位置。
- T051：
  - `--segment-file` 的语义；
  - persona 与录制的 jq 摘取命令；
  - 假 agent 的配置只经命令行参数，因为宿主的环境变量规则会过滤掉我们自己的变量。
- T055：拿假 agent 路径的做法。

## 注意

- 不运行真实的 Claude Code、Codex CLI；录制是入库的数据，只读不改。
- 产品代码不依赖 `spikes/`。

## 规模

M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 850 行（包括贴线）就先停下汇报**；硬停止线 950。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条；T168 是写完才统计的，这次请按模块统计）。

## 工作方式

T168 合并之后，在原来的 T052 worktree 里从最新的 main 开新分支：`git checkout -b feat/T067-fake-agent-replay main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
