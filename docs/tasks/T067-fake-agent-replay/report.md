# T067 汇报（假 agent 回放模式与 persona 比对）

- 执行者：子 agent（Rust 工程师）
- 分支：`feat/T067-fake-agent-replay`
- 完成日期：2026-10-20

T067 已完成。假 agent 有了 `--replay`，persona 改成和 SP-1 录制自动比对，SP-1 的一份 claude 录制经真实宿主回放到回合完成。在干净的 worktree 里跑 `cargo xtask ci pr --record` 全部通过，摘要 `pr-2c5963452092.json` 已提交。规模：新增 760 行、删除 112 行，开工前估计约 670，在约 850 的停止线以内。这次每完成一个模块统计一次，数字记在 `notes.md`。

有一处需要你确认：设计 5.9.2 说 SP-1 录制只用于 A 层，任务书又要求用 SP-1 录制做宿主端到端回放。我的处理写在"关键取舍"第一条。

## 做了什么

**回放核心**（`cupper-fake-agent/src/replay.rs`、`config.rs`、`main.rs`）
- 新参数 `--replay <录制>`，回放的段由 `--segment` 和 `--segment-file` 选，录制读进来后用 `Recording::parse` 解析。回放不经 SDK，直接逐行读写 stdio。
- 宿主每发来一条消息，就和这一段里下一个 `send` 帧比对：
  - 方法和各字段都要相同；
  - JSON-RPC id 重映射：宿主请求的 id 代表录制里的 id，录制的响应换回宿主的 id 再发出；适配器自己发起的请求保留录制里的 id，宿主应答时 id 必须一致；
  - `cwd`、`path` 字段按占位比较：录制里的路径第一次出现时，和宿主发来的路径对上；之后要一致，发出的帧里也换成宿主的路径；
  - 各层的 `_meta` 都忽略；
  - 其余字段（包括 prompt 文本、应答里的选项）精确比较。
- 比对通过后，按顺序发出后面的 `recv`、stderr 和非 JSON 行，直到下一个 `send`。适配器发起的请求因此会等宿主应答，应答也要和录制一致。
- 时间间隔压到 1/100，从宿主最近一条消息到达时重新计起。
- 遇到 `instance_exited` 就按录制里的退出码退出。
- 比对失败时在 stderr 写出帧号和字段路径，例如 ``frame 9: `result.outcome.optionId` is "reject", the recording has "allow-once"``，并以 71 退出。

**persona 自动比对**（`tests/persona.rs`，替换 `persona.rs` 里原来按常量核对的两个单测）
- 从入库录制里读出下面五项，和 persona 比对，claude 和 codex 两种都比：
  - 能力：`agentCapabilities` 和顶层 `_meta`；
  - configOptions：id、category、currentValue，以及模式的选项；
  - 默认模式；
  - 命令列表；
  - 权限请求的选项 ID 和 kind，按工具类型 `edit`、`execute`、`switch_mode` 分别比。
- 对不上就失败。我把一个选项 ID 临时改错验证过，用例确实失败。另有断言保证三种工具类型都比到了，避免空比对。

**宿主端到端**（`cupper-host/tests/lifecycle.rs`）：SP-1 的 claude `s02b-plan` 由假 agent `--replay` 扮演适配器，真实宿主跑到回合 `completed`。

## 关键取舍

**SP-1 录制与宿主请求不一致**
- 设计 5.9.2 写明 SP-1 录制只用于 A 层，因为 SP-1 客户端的请求和宿主不同。实际差两处：
  - `initialize`：`clientInfo` 不同，能力也不同；
  - SP-1 的安全策略在建会话后固定发两次 `set_config_option`。
- 我没有放宽比对规则（宽松档按任务书不做）。端到端用例在测试里把这两处换成宿主自己的请求：`initialize` 的参数换成宿主发出的那份；安全策略的两次请求连同响应删掉。其余帧原样回放，逐帧比对：`session/new` 的路径占位、UUID 形式的请求 id 重映射、prompt 原文、`recv` 帧按序发出、退出码都实际走过一遍。
- 如果你认可这个做法，5.9.2 那句"只用于 A 层"可能要补一句说明。

**其他取舍**
- **宿主的 `initialize` 参数写在用例里**：第一次跑失败，回放写进原始帧的报错指出宿主比我照代码写的多一项 `auth.terminal: false`，补上后通过。宿主的 `initialize` 以后再变，这个用例会带着清楚的字段名失败，等于多了一道守卫。
- **命令列表按"原样出现、顺序一致"比**：录制机器上装了插件和 skill，Claude 录制里有 54 条命令，persona 只取了 5 条内置命令。所以规则是 persona 的每条命令都按原样出现在录制里，而且顺序一致；Codex 两边正好全等。
- **模型的当前值不比**：它取决于录制账号，persona 本来就有意替换掉了。
- **不做 `--replay-speed`**：没有使用方，1/100 写成常量 `SPEED`。这和我在 T052 拆分前写的方案相比少了这一项。
- **段末的处理**：段里没有 `instance_exited` 时，假 agent 等宿主关闭 stdin；之后宿主再发消息就算失败。

## 验证方式

- **UNIT-replay**，测试名都以 `unit_replay_` 开头：
  - 比对规则（`replay.rs` 的单测）：方法、id 重映射、路径占位、`_meta`、prompt 各有正反用例；另覆盖适配器请求的应答 id 和选项、未知 id 的响应。
  - 进程级（`tests/replay.rs`，3 条）：
    - 权限请求发出后，宿主不应答，回合就不继续；
    - 录制里 3 秒的间隔，回放时在 30 ms 到 1.5 s 之间；
    - 应答的选项和录制不同时，以 71 退出，并写出帧号和字段；
    - 用 `--segment-file` 启动三次：第一次演第 0 段，退出码 3；第二次演第 1 段，退出码 0；第三次报"没有第 2 段"。
  - persona（`tests/persona.rs`）：上面的五项。
  - 端到端（`unit_replay_a_host_runs_an_sp1_claude_turn`）：回合完成，宿主正常退出。
- **CI**：在干净的 worktree 里跑 `cargo xtask ci pr --record docs/tasks/T067-fake-agent-replay/ci/`。
  - 第一次 rust 作业失败：宿主的 `it_process_hygiene_no_inherited_fds_allowed_env_own_session` 看到假 agent 报告继承了 fd 3–8。这是我引入的回归：我在 `main` 里把 tokio 运行时提前到两种模式之前统一构建，运行时自己打开的描述符被 `record_inherited_fds` 当成了继承来的。fix 提交把顺序改回先记录、再构建运行时。
  - 重跑全部通过。跑的期间没有往工作区写文件。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,897 通过 |
  | protocol | 185 通过 |
  | ts | 468 通过 |
  | license | 通过 |
  | testdata-scan | 3 通过（本机没有敏感词表，那一步跳过） |
  | ui | 按路径筛选跳过（没有界面改动） |
  | e2e | 4 通过 |

- **试合并**：用 `git merge-tree` 试合并最新的 main，没有冲突。
- **规模**（不含 notes、`Cargo.lock`、CI 摘要）：

  | 模块 | 新增 | 删除 | 估计 |
  |---|---|---|---|
  | 回放核心与入口（含比对规则单测 113 行） | 410 | 8 | 340 |
  | 进程级用例 | 176 | 1 | 150 |
  | persona 自动比对 | 115 | 105 | 110 |
  | 宿主端到端 | 61 | 1 | 70 |
  | fix 提交 | 5 | 4 | — |

  分支相对开工点合计新增 760、删除 112（fix 改的是前面新增的行，所以合计比逐项相加少）。超出估计最多的是回放核心：rustfmt 把错误类型和结构体拆成了多行。写完后把文件读取挪到 `main`、把差异并进错误类型，压掉了约 37 行。

## 提交列表

```
1b279bb docs(T067): record the ci pr summary and the descriptor regression
2c59634 fix(fake-agent): note the inherited descriptors before the runtime starts
c63c2ee docs(T067): record the size per module and what the runs showed
bacc082 test(host): replay an SP-1 Claude turn end to end
30403a2 test(fake-agent): compare the personas with the SP-1 recordings
e0d0634 test(fake-agent): replay over the stdio pipes
6218c13 feat(fake-agent): replay a recording's segment with --replay
2adad56 docs(T067): record the checks, the SP-1 choice and the size estimate
```

## 遗留问题 / 范围外发现

- **设计措辞**：5.9.2 那句"SP-1 录制只用于 A 层"和这次的端到端做法需要你定。可以在设计里补一句说明，也可以改成只认宿主自己录的录制。
- **段末退出的时机**：段末的 `instance_exited` 按压缩后的时间退出，不区分这次退出是不是因为客户端先关了 stdin。SP-1 录制在退出前有一条 `stop-requested` note，说明是客户端关的；宿主导出的录制（T047）里没有这类标记。如果宿主驱动的录制里，退出是因为宿主关闭了 stdin，回放可能比宿主更早退出，宿主会看到一次适配器意外退出。T065 或真实的 `xtask compat` 录制用上之后，可能要在录制里标出"由客户端关闭"，回放据此等 stdin 关闭再退出。
- **路径字段只认 `cwd` 和 `path`**：`additionalDirectories`、`file://` 形式的 `uri` 现在没有请求用到，用到时再加。
- **Codex 0.159.2 的录制没有比对**：persona 按 0.160.1 做，所以只和 0.160.1 比。
- **用例编号的位置**：端到端用例放在 `cupper-host/tests/lifecycle.rs`，复用那里的宿主夹具；按 T067 段登记的编号命名为 `unit_replay_…`。
- **原来的两个 persona 单测改了编号**：它们原属 UNIT-fake-scripts，现在由 UNIT-replay 下的 `tests/persona.rs` 接替。UNIT-fake-scripts 的其他用例不变。
