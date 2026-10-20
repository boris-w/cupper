# T052 录制格式、回放模式、SP-1 录制转换与敏感信息扫描

- 派发日期：2026-10-20
- 执行者：子 agent（工程师，测试基础设施）
- 分支：`feat/T052-recording-replay`
- 依赖：T002、T051（都已合并）。**本任务现在在关键路径上，没有松弛**：T052 → T053（Claude 提取层与 A 层金样例）→ T055 → T047 → T064 → T066 → T099

## 背景

驱动层的测试靠两样东西（设计 5.9.2、5.9.3）：
- **录制**：适配器与 CLI 之间的真实 ACP 帧，统一成格式 v1，存在 `testdata/recordings/` 下。A 层金样例（T053）用它回放提取层，B 层用它回放会话层。
- **假 agent 的回放模式**：读一份录制，扮演适配器，供进程级测试使用。

SP-1 留下了 48 份真实录制（`spikes/SP-1-adapter-behavior/recordings/`），格式是 SP-1 自己的。本任务把它们一次性转成格式 v1 入库，作为 A 层的第一批输入。录制来自真实环境，所以入库之前要过脱敏和个人信息扫描。

## 范围

**做**（计划 T052 一行；设计 5.9.2"回放模式"、5.9.3）：
1. **录制格式 v1 的读写库**（规格见 5.9.3）：头行、帧行、`dir` 的五种取值、按实例分段。格式错误要给出行号和原因。
   - **放在哪里（编排者裁定）**：格式的类型和逐行的解析、序列化放在 `cupper-protocol` 里的一个模块，不做文件 I/O，也不进 schema 导出和 TS 生成。这样 T047 的 `diag/recording_export`（宿主）、本任务的回放模式（假 agent）、T053 的 A 层运行器（宿主测试）都用同一份定义。文件读写由各使用方自己做。不新增 crate。
2. **SP-1 录制一次性转换**：48 份全部转成格式 v1，放进 `testdata/recordings/<adapter>/<adapter-ver>/<cli-ver>/<scenario>.jsonl`（版本取自 SP-1 录制头里的 `versions`）。转换规则见 5.9.3：
   - note 行的 `note`、`data` 统一成 `note`；
   - `recv-nonjson` 改为 `recv_nonjson`；
   - 补上头行，`source` 为 `sp1`，`usage` 只有 `gold-a`（SP-1 的录制只进 A 层，计划第 4 节）。

   转换脚本只用一次，**不进产品代码和工具链**（工程原则：没人用的代码不进 main）。脚本作为物料放在本任务目录下，例如 `docs/tasks/T052-recording-replay/convert-sp1.ts`，`notes.md` 里写明怎么运行。转换后用第 1 项的库把 48 份全部读一遍，作为校验。
3. **`tools/sanitize`**：由 SP-1 的 `sanitize.ts` 迁移，规则不变（路径、用户名、主机名、邮箱、密钥、账号字段、用户自己的 skill 名和 MCP）。写完之后重新扫描，命中就失败。转换后的录制都要过一遍。
4. **个人信息扫描接入 `xtask ci testdata-scan`**：
   - **通用模式**（5.9.3）：`/Users/` 后跟非占位符的路径、邮箱、token 样式、`/var/folders`，命中就失败。范围是 `testdata/`；入库的 CI 摘要（`docs/tasks/*/ci/*.json`）也一并扫描（计划 1.8"验收记录"一行：T052 之后摘要再过 `testdata-scan`）；
   - **本机敏感词清单**：由用户在主仓库的 `.private/sensitive-words.txt` 里维护（一行一个，`#` 开头是注释）。CI 是在 worktree 里跑的，所以要经 `git rev-parse --git-common-dir` 找到主仓库，再读它下面的 `.private/`。**清单不存在时不让作业失败**（用户在 T066 之前才建清单，计划第 6 节第 4 行），但要在输出里明确提示"没有本机敏感词清单，已跳过"，摘要里也要能看出这一项被跳过；
   - **清单内容和命中的内容都不能进仓库、摘要和汇报**。命中时输出文件名、行号和清单里的条目序号，不输出命中的文字本身。
5. **假 agent 的回放模式**（5.9.2"回放模式"）：
   - `--replay <录制>`：收到宿主的请求时，与录制里的下一个 `send` 帧比对。比对规则：方法相同；JSON-RPC 的 id 重新映射；`cwd` 等路径换成占位符；`_meta` 忽略；prompt 文本默认精确比较。比对失败就让测试失败并打印差异；
   - 比对通过后，按顺序发出其后的 `recv` 帧，直到下一个需要等待宿主的 `send` 帧。进程级回放按比例压缩时间间隔，默认 1/100；
   - 适配器发起的请求（审批、提问）要等宿主应答，默认要求应答的选项与录制一致；
   - **多实例录制**：一段以"进程退出"的 note 结束时，假 agent 照样退出；宿主重新拉起后从下一段继续（用 T051 的 `--segment-file`，见交接）。
6. **persona 自动比对**（交接，T051）：假 agent 的 persona（能力、configOptions、默认模式、命令列表、权限选项 ID）与 SP-1 录制里的形态一致，改成用例自动比对。

**不做**：
- `diag/recording_export`（T047）；
- A 层运行器和金样例（T053）；
- B 层回放（进程内宿主核心，按虚拟时钟推进）；
- `tools/rec-shape-diff`。

## 验收标准

- [ ] 新增 UNIT-rec-format（格式 v1 的读写和错误报告；48 份转换后的录制全部能读）、UNIT-replay（比对规则、比对失败的差异输出、多实例分段、适配器发起的请求）、UNIT-sensitive-list（用测试里临时写的清单：命中时作业失败、输出里没有命中的文字；清单缺失时跳过并提示）。测试名以用例编号开头。
- [ ] `testdata/` 和入库的 CI 摘要通过个人信息扫描。
- [ ] 48 份 SP-1 录制已转换入库，并且都过了 `tools/sanitize` 的重新扫描。
- [ ] persona 自动比对的用例通过。
- [ ] 编号已在 `tests/ids.toml` 的 T052 段里，`testdata-scan` 通过。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T052-recording-replay/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T051：回放模式落地时，加 persona 与 SP-1 录制形态的自动比对（T051 里只用测试常量逐条核对）。
- T051：`--segment-file <路径>` 每次启动段号加 1，重启后的实例接着演下一段。persona 与 SP-1 录制的 jq 摘取命令在 `docs/tasks/T051-fake-agent-scripts/notes.md` 里。
- T002：编号登记在 `tests/ids.toml`，只在本任务那一段里追加。

## 注意

- **个人信息**：SP-1 的录制已经脱敏过，但转换入库前仍要重新过 `tools/sanitize` 和通用扫描。发现漏网的个人信息时，修规则，不要手工改录制；在汇报里只写"第几条规则补了什么类型"，不写命中的具体内容。
- **只读 `spikes/`**：不改 SP-1 的任何文件。
- **不运行真实的 Claude Code 或 Codex CLI**，不登录，不联网。
- **依赖**：锁精确版本，许可证在 ADR-11 的 allowlist 内。
- `tools/sanitize` 是 TS 包，写法参照 `tools/trace-ids`（同样由 `pnpm` 工作区管理）。

## 规模

计划估为 M。范围较宽，**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍、组合测试按 1.5 倍计。转换后的录制是数据，不计入规模；转换脚本计入。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

**推算会超出约 1,500 行时，开工前就停下来汇报，并给出拆分建议。** 建议的拆法：
- 第一部分：格式库、SP-1 转换、脱敏与扫描（第 1–4 项）。T053 只依赖这部分，先合并可以缩短关键路径；
- 第二部分：回放模式与 persona 比对（第 5、6 项）。

不拆分时，也请按第 1–4 项、再第 5–6 项的顺序提交。

## 参考资料

- 设计：`09-testing.md` 5.9.2、5.9.3；`01-repo-build-ci.md` 5.1.2（`testdata/`、`tools/sanitize`）、5.1.6（`testdata-scan`）。
- 计划：第 4 节 T052 的注、1.8（验收记录）、第 6 节第 4 行。
- `spikes/SP-1-adapter-behavior/`（`sanitize.ts`、`recordings/`、README）；`docs/tasks/SP-1-adapter-behavior/`。
- `docs/tasks/T051-fake-agent-scripts/notes.md`；`docs/tasks/T002-test-ids/`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径，也不要写任何扫描命中的内容。其他物料（`notes.md`、转换脚本）由执行者在任务分支上提交。
