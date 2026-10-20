# T067 工作笔记

## 开工前的核对

- `handoff-notes.md` 里第一列含 T067 的行：
  - T051：persona 与 SP-1 录制形态的自动比对，替换常量核对；`--segment-file` 每次启动段号加 1；jq 摘取命令在 `docs/tasks/T051-fake-agent-scripts/notes.md`。
  - T052：`Recording::parse`、`segments()`、`Body::event()`、`INSTANCE_STARTED`、`INSTANCE_EXITED`；录制在 `testdata/recordings/` 下。SP-1 的 `codex-bundled` 是 `codex-acp/2.2.2/0.160.1/`，persona 按它。
  - T055：宿主测试用 `launch::fake_agent()` 拿假 agent 的路径（`xtask ci` 先构建）；`test_config.agents.claude` 支持 `args`。
- `tests/ids.toml` 的 T067 段：UNIT-replay。persona 比对原来属于 UNIT-fake-scripts 的两个常量单测，替换后改名到 UNIT-replay 下。
- 我在 T052 拆分前写过第二部分的方案（T052 的 notes "回放模式与 persona 比对"），编排者认可；这次照它做，去掉 `--replay-speed`（没有使用方，1/100 写成常量）。

## 一处需要取舍的地方：SP-1 录制与宿主的请求不一致

设计 5.9.2 写明 SP-1 的录制只用于 A 层，因为 SP-1 客户端的请求和宿主不同；任务书又要求至少用一份 SP-1 的 claude 录制做"宿主 → 假 agent → 回合完成"的端到端回放。实际差别：

- `initialize`：SP-1 的 `clientInfo` 是 `cupper-sp1-client`，能力也多几项（`session.configOptions`、`elicitation.url`、`auth.terminal`）；
- SP-1 的安全策略在 `session/new` 之后固定发 `model=haiku`、`effort=low` 两次 `set_config_option`。

做法：回放的比对规则不放宽（宽松档不做）。端到端用例在测试里把这两处换成宿主自己的请求：`initialize` 的参数换成宿主发出的；安全策略的两次 `set_config_option`（连同回复和 note）删掉。其余帧原样，按规则逐帧比对。选不含审批的 `s02b-plan`，prompt 原文由用例经 `session/create` 发出。

## 规模估计（开工前）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| 回放核心 `src/replay.rs` | 选段；逐帧比对（方法、id 重映射、路径占位、`_meta` 忽略、其余含 prompt 精确比较）；按序发出 `recv`、stderr、非 JSON 行；时间按 1/100 压缩；遇到 `instance_exited` 按录制的退出码退出；带帧号和字段路径的错误 | 220 | 90（比对规则的正反用例） |
| 启动配置与入口 | `--replay <录制>`；`main` 分派 | 25 | 5 |
| 进程级用例 `tests/replay.rs` | 适配器发起的请求等应答且选项一致；时间压缩；`--segment-file` 按段回放；比对失败的错误信息和退出码 | — | 150 |
| persona 自动比对 | 从录制摘取能力、configOptions、默认模式、命令列表、权限选项 ID 后比较，替换 T051 的常量单测（约删 85 行） | — | 110 |
| 宿主端到端 | SP-1 的 claude `s02b-plan` 经宿主回放到回合完成 | — | 70 |
| 合计 | | 245 | 425 |

合计约 **670 行**（新增），停止线约 850，硬停止线 950。每完成一个模块用 `git diff --numstat` 统计一次，记在下面。

## 按模块统计
