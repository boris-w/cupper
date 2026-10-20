# T052 工作笔记

## 方案草图（开工前）

### 录制格式 v1（`cupper-protocol` 的 `recording` 模块）

- 头行：`format`、`v`、`source`（`sp1`、`host`）、`adapter`、`adapter_version`、`cli`、`cli_version`、`scenario`、`usage`（`gold-a`、`gold-b`、`replay`），不认识的键报错。
- 帧行：`seq`、`t`、`instance`、`dir`，外加与 `dir` 对应的一个负载：`send`、`recv` 带 `msg`；`recv_nonjson`、`stderr` 带 `data`（字符串）；`note` 带 `note`（对象，必须有字符串 `event`）。负载缺失、多余或类型不对都报错。
- 按实例分段：`instance` 从 0 开始，与假 agent 的 `--segment` 编号一致；每段第一帧是 `instance_started` 的 note，`instance_started` 也只出现在段首；`instance` 只能不变或加 1。
- 段内 `seq` 严格递增，`t` 不减（SP-1 的 `seq` 每个进程从头数，`t` 换算到段内后不减，已用 jq 核对 48 份）。整数不超过 2^53−1。
- 错误类型用 thiserror，`line` 是文件里的行号（从 1 起），加上原因。
- 公开的类型：`Header`、`Frame`（`seq`、`t`、`instance`、`body`）、`Body`（五种取值的枚举）、`Recording`（`parse`、`to_jsonl`、`segment(n)`）；`Header`、`Frame` 都能单独序列化成一行，供 T047 逐行写出。不派生 `JsonSchema`，不进 schema 导出。

### SP-1 的转换（`convert-sp1.ts`，一次性）

- 目录：`claude` → `claude-agent-acp/0.91.0/2.1.296/`；`codex-bundled` → `codex-acp/2.2.2/0.160.1/`；`codex-local` → `codex-acp/2.2.2/0.159.2/`（版本取自 SP-1 头里的 `versions`，三组互不冲突）。头行的 `adapter` 是 `claude-agent-acp` 或 `codex-acp`，`cli` 是 `claude` 或 `codex`。
- SP-1 每个进程开头都写一行 `header` note（`s01`、`s09` 各有 2、3 个进程）：第一行的版本信息进头行；每个 `header` note 改成该段的 `instance_started`，其余字段（日期、客户端能力）留在 note 里；`exit` 改成 `instance_exited`；其余 note 改成 `{event: <原 note>, ...data}`。
- `t` 换算成相对段首；`seq` 原样保留（段内递增）；`recv-nonjson` 改名为 `recv_nonjson`（SP-1 实际没有出现）。
- 转换输出先写到临时目录，再经 `tools/sanitize` 写进 `testdata/recordings/`（工具会重新扫描）；最后用格式库把 48 份全部读一遍。

### 脱敏与扫描（`tools/sanitize`，TS）

- `sanitize --in <目录> --out <目录> [--base <目录>]... [--terms <文件>]`：SP-1 的规则原样迁移，只把输入输出改成格式 v1（头行、`note` 对象），目录按相对路径递归。写完后重新扫描：运行环境里的家目录、用户名、主机名、`--terms` 的词，加上通用模式。
- `scan <路径或 glob>...`：通用模式（`/Users/` 后跟非占位符、邮箱（`example.*`、`.invalid` 等占位域名除外）、token 样式、`/var/folders`），命中只报文件、行号和规则名。
- `words --list <文件> <路径或 glob>...`：本机敏感词清单（一行一个，`#` 开头是注释），不区分大小写；命中只报文件、行号和条目序号。
- `xtask ci testdata-scan`：在 `trace-ids check` 之后加两步，范围是 `testdata/` 和 `docs/tasks/*/ci/*.json`。清单经 `git rev-parse --git-common-dir` 找到主仓库的 `.private/sensitive-words.txt`；不存在时这一步不失败，输出"没有本机敏感词清单，已跳过"，摘要的作业条目里加 `skipped: ["sensitive-words"]`（摘要字段白名单相应加一项）。

### 回放模式与 persona 比对（第 5、6 项）

- 假 agent 加 `--replay <录制>`、`--replay-speed <倍数>`（默认 100，即时间间隔压到 1/100），走自己的原始行循环，不经 SDK。按 `--segment`/`--segment-file` 选段。
- 比对：方法相同；宿主请求的 id 与录制里的 id 建立映射，回发响应时换回；`cwd`、`path` 等路径字段按首次出现建立映射；`_meta` 在各层都忽略；其余（含 prompt 文本、审批应答的选项）精确比较。失败时在 stderr 打印逐字段的差异，以专用退出码退出。
- 段内遇到 `instance_exited` 就按录制的退出码退出；宿主关闭 stdin 时还有未比对的 `send`，按失败处理。
- 设计里"默认精确比较""默认要求选项一致"的非默认档，现在没有使用方，不实现。
- persona 比对：把 T051 里按常量核对的两个单测改成从 `testdata/recordings/` 读录制，按 T051 notes 里 jq 命令的口径摘取后比较。

## 规模估计（开工前）

按格式化后的写法估（rustfmt 宽度 100，prettier 宽度 100）。SP-1 的 `sanitize.ts` 260 行，用 prettier 排版后 308 行，迁移部分按这个基数估。转换后的录制和 `pnpm-lock.yaml` 不计入。

### 第一部分：格式库、SP-1 转换、脱敏与扫描（第 1–4 项）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `cupper-protocol/src/recording.rs` | 类型、逐行解析与序列化、分段校验、带行号的错误 | 260 | 170 |
| `cupper-protocol/tests/recordings.rs` | 48 份转换后的录制全部能读，段数与 SP-1 一致 | — | 35 |
| `convert-sp1.ts` | 一次性转换脚本（任务目录下的物料） | 120 | — |
| `tools/sanitize` 包骨架 | `package.json`、`tsconfig.json`、`vitest.config.ts` | 35 | — |
| `tools/sanitize/src/sanitize.ts` | SP-1 规则迁移，环境参数可注入，格式 v1 | 300 | 110 |
| `tools/sanitize/src/scan.ts` | 通用模式、敏感词清单（UNIT-sensitive-list 的 TS 部分） | 110 | 130 |
| `tools/sanitize/src/main.ts` | 命令行：`sanitize`、`scan`、`words` | 90 | — |
| `xtask` | `testdata-scan` 加两步、找主仓库的清单、跳过记录进摘要 | 90 | 70 |
| 小计 | | 1,005 | 515 |

第一部分约 **1,520 行**。

### 第二部分：回放模式与 persona 比对（第 5、6 项）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `cupper-fake-agent/src/replay.rs` | 选段、stdin 读取、比对（id、路径、`_meta`）、差异输出、按比例延时、适配器发起的请求、按 note 退出 | 340 | 110 |
| `config.rs`、`main.rs` | `--replay`、`--replay-speed` | 25 | 10 |
| `tests/replay.rs` | 进程级：正常回放、比对失败的差异、多实例分段、审批应答一致与不一致、时间压缩 | — | 260 |
| persona 比对 | 从录制摘取后比较，替换 T051 的常量单测（约删 110 行） | — | 130 |
| 小计 | | 365 | 510 |

第二部分约 **875 行**（另删约 110 行）。

### 合计

约 **2,400 行**，超出约 1,500 行的上限约 60%。按 brief 和守则第 8 条，开工前停下来汇报，建议按 brief 的拆法分成两部分：第一部分（第 1–4 项，约 1,520 行，T053 只依赖它）先做先合并；第二部分（第 5、6 项，约 875 行）随后。

## 编排者裁定（2026-10-20）

拆分：T052 只做第 1–4 项，上限约 1,600 行，硬停止线 1,700；回放模式和 persona 比对归 T067。上面的 6 项取舍全部认可。

## SP-1 转换的运行方式

在仓库根目录运行，`<临时目录>` 用任意空目录：

```sh
node docs/tasks/T052-recording-replay/convert-sp1.ts spikes/SP-1-adapter-behavior/recordings <临时目录>
node tools/sanitize/src/main.ts sanitize --in <临时目录> --out testdata/recordings
diff -r <临时目录> testdata/recordings      # 脱敏没有改动任何一行
node tools/sanitize/src/main.ts scan testdata 'docs/tasks/*/ci/*.json'
cargo test -p cupper-protocol --test recordings
```

- 转换得到 48 份：`claude-agent-acp/0.91.0/2.1.296/` 18 份，`codex-acp/2.2.2/0.160.1/` 16 份，`codex-acp/2.2.2/0.159.2/` 14 份。
- 头行的 `scenario` 取文件名。SP-1 保留了两次补跑（`claude/s07x-sleep-backgrounded`、`codex-bundled/s10x-model-unsupported`），它们的 SP-1 头里写的还是被补跑的场景名。
- `tools/sanitize` 的重新扫描通过（`clean: 48 files`），输出与转换结果逐字节相同：SP-1 的录制已经脱敏过，这次没有发现漏网的内容，规则没有改。
- 通用扫描覆盖 `testdata/` 和 72 份入库的 CI 摘要，共 120 个文件，没有命中。本机还没有敏感词清单，这一项在 CI 里会显示为跳过。
- 用格式库读全部 48 份：都能读；`s01` 每份 2 段、`s09` 每份 3 段、其余 1 段，每段都有一个 `instance_exited`。

## 进度统计

不含 `pnpm-lock.yaml`、转换后的录制和本笔记。

| 提交 | 内容 | 行数 | 累计 |
|---|---|---|---|
| 1 | 格式库 `recording.rs`（实现约 335，测试约 108） | +443 | 443 |
| 2 | `tools/sanitize`（实现 524，测试 250） | +774 | 1,217 |
| 3 | 转换脚本 102、读全部录制的测试 58 | +160 | 1,377 |
| 4 | xtask：`testdata-scan` 加两步、`git::common_dir`、跳过记录进摘要（实现约 45，测试约 55，另删 4） | +138 −4 | 1,519 |

格式库的实现比估计多约 75 行（错误类型和分段校验），测试少约 60 行（错误用例改成一张表）；`tools/sanitize` 与估计持平；xtask 比估计少约 20 行。合计约 1,520 行，在 1,600 行的上限以内。

## xtask 的接入

- `testdata-scan` 在 `trace-ids check` 之后依次跑 `pii-scan`（通用模式）和 `sensitive-words`（本机清单），范围都是 `testdata` 和 `docs/tasks/*/ci/*.json`。
- 清单经 `git rev-parse --path-format=absolute --git-common-dir` 找到主仓库，取它下面的 `.private/sensitive-words.txt`。清单不存在时，终端上显示 `sensitive-words skipped: no local sensitive word list (...)`，作业不失败，摘要的作业条目里 `skipped` 为 `["sensitive-words"]`，结果汇总里也列出跳过的步骤。
- UNIT-sensitive-list 分两处：TS（`tools/sanitize/src/scan.test.ts`）测清单格式、命中时退出码为 1、输出只有文件、行号和条目序号；Rust（`testdata_scan.rs`）测从链接的 worktree 找到主仓库的清单、清单缺失时跳过并记进摘要、在本仓库的 `testdata/` 上用临时清单跑这一步，命中时步骤失败、日志里没有命中的词。
