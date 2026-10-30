# T210 工作笔记

- 执行者：子 agent（工程师）

## 开工前读过的

- 任务书；设计 5.9.3；计划 T210 一行。
- 卷宗：T110 的 `notes.md`（"录制里没脱敏的临时路径"一节）；T204 的任务书、笔记、汇报、评审。
- 代码：`tools/sanitize` 全部；`crates/xtask/src/ci/jobs/testdata_scan.rs`（`pii-scan`、`sensitive-words` 两步怎样调用工具）。

## 录制里哪些文本是分块流过来的

把 `testdata/recordings/`、`spikes/` 的录制和 T102 卷宗里的录制按 `session/update` 的类型数了一遍：

| 类型 | 文本在哪 | 有没有 ID |
|---|---|---|
| `agent_message_chunk` | `content.text` | 2167 帧有 `messageId`，7 帧没有（假 agent 3 份录制、Codex 的 `s10x-model-unsupported` 两份） |
| `agent_thought_chunk` | `content.text` | 355 帧有 `messageId`，1 帧没有（假 agent） |
| `user_message_chunk` | `content.text` | 11 帧，都有 `messageId` |
| `tool_call_update` | `_meta.terminal_output.data`（命令输出的增量） | 按 `toolCallId` |

`tool_call` 和 `tool_call_update` 的 `content`、`rawInput`、`rawOutput` 每次给的是整份，不是增量，不折叠。stderr 和非 JSON 行一帧一行，也不折叠。

## 行数估计（只算 `tools/sanitize`）

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `streams.ts`（新） | 按会话加条目给分块的文本归组；把一组折叠成一段文本，并记下每个字符来自哪一块 | 45 | — |
| `scan.ts` | 匹配器返回命中的位置（规则名加起止偏移），`scan` 先逐行再逐组扫，跨块的命中报行号范围 | 35 | — |
| `sanitize.ts` | 逐帧规则整理成一张替换表；逐帧脱敏之后，同一组的文本再按同一张表折叠起来替换，替换结果落回原块 | 40 | — |
| `scan.test.ts` | 匹配器改返回位置后，原有断言改取规则名 | — | 5 |
| `sanitize.test.ts` | UNIT-sanitize-across-chunks：合成录制（路径切成多块、词表的词跨两块、令牌跨三块），脱敏、扫描、再跑一遍不变 | — | 90 |
| 合计 | | 120 | 95 |

预计约 215 行，停止线 250。

实际：第一次提交后 `cargo xtask size` 报 +262 −47，比停止线多 12。这时停下来报告了编排者。编排者选择照现在的写法继续，停止线改为 280，硬停止线 320 不变，保留精确的行范围（任务书"补充裁定"）。多出来的行主要有两处：匹配器从"返回规则名"改成"返回命中位置"，`patterns`、`terms`、`scan` 都跟着改；另外 prettier 把几处长行折开了。

## 工具的改法（`tools/sanitize`）

- **归组**（`streams.ts`）：
  - 只看带 `params.update` 的帧，键是"会话 + 更新类型 + 条目"；
  - 条目是 `messageId`（消息、思考、用户消息的 chunk）或 `toolCallId`（`_meta.terminal_output.data`）。有 ID 的按 ID 归组，中间夹着别的帧也照样归进同一组；
  - 没有 ID 的按"相邻的同类 chunk"归组：同一会话里只要来了一个类型不同的更新，就算新的一段。
- **折叠**：把一组的文本连起来，同时记下每个 UTF-16 单元来自哪一块。脱敏和扫描用的是同一个函数。
- **脱敏**（`sanitize.ts`）：
  - 原来 `#scrubString` 依次做四件事：会话 ID、服务器名、机器规则、长串。现在整理成一张有序的规则表，逐帧规则照旧按这张表做；
  - 逐帧做完之后，每组的文本折叠起来，按同一张表再替换一遍；
  - 一处命中的替换文本放进命中开头所在的那一块，命中的其余部分从后面几块里删掉，命中前后的文字留在原来的块里。帧数和 `seq` 不变；
  - 替换文本里的 `$1` 照 `String.replace` 的规则展开（`Bearer <secret>` 要用到）；
  - 只在一块里的命中，逐帧那一步已经换掉了，第二遍不会再命中，所以对工具自己的输出再跑一遍结果不变。
- **扫描**（`scan.ts`）：
  - 匹配器改为返回 `[规则, 起, 止]`，`terms` 改用不分大小写的正则来定位；
  - `scan` 先逐行扫，再把每个文件里的行当成帧，逐组折叠后扫；
  - 跨几帧的命中报 `文件:起行-止行: 规则`，落在一行里的命中和逐行扫的结果合并去重；
  - `words` 和 `sanitize` 写完后的复扫也走这条路，因为它们都调用 `scan`。

### 变异检查（UNIT-sanitize-across-chunks）

每次只改一处，只跑这一个用例。下面 14 处改动都让用例失败：
1. 归组不看 `messageId`；
2. 没有 ID 的 chunk 不分段；
3. 不收消息和思考的 chunk；
4. 不收命令输出；
5. 键里没有更新类型；
6. 替换文本放进最后一块；
7. 命中的其余部分不删；
8. `$1` 不展开；
9. 脱敏不做折叠这一遍；
10. 扫描不折叠；
11. 跨帧的命中只报第一行；
12. 范围的止行取命中之后的那个字符；
13. `terms` 的命中位置错；
14. `patterns` 的命中位置错。

用例里专门安排了几种情况，有的变异只靠它们才测得出来：
- 词表的词正好结束在一块的末尾，用来测第 12 条；
- 令牌用 `Bearer` 的形式，用来测第 8 条；
- 消息的各块之间夹着思考，用来测第 1、5 条；
- 最后一段思考单独成组，用来测第 2 条。

## 全仓库复扫

范围是 `testdata`、`spikes`、`docs/tasks/*/data`、`docs/tasks/*/ci/*.json`，共 630 个文件，包括全部录制和金样例。新旧工具（旧的取自 main）各扫一遍。这里只记文件、行号和规则，不记命中的文字。

| 扫描 | 新工具 | 旧工具 |
|---|---|---|
| `scan`（通用模式） | 4 处：SP-1 的 `README.md`、SP-1 的 `sanitize.ts`、T003 的 `sanitize-t003.ts` 里，是扫描规则本身写着的家目录前缀；T101 的 `build.sh` 里，是一个像邮箱的图标文件名 | 同样 4 处 |
| `words`（主仓库的敏感词清单） | clean | clean |

- **本机规则**：
  - 用本机的家目录、用户名、主机名、临时目录，再加上敏感词清单，组成 `leftovers()` 的匹配器，扫同一范围；
  - 结果是 7 处，分布在上面那 4 个文件和 SP-1 的 `scenarios.ts`。`scenarios.ts` 里那处是一行注释里的 `/private/var/...`。都不是录制，也不是金样例。
- **重新脱敏**：
  - 用新工具对已入库的 128 份录制重跑脱敏：`testdata/recordings/` 71 份、SP-1 48 份、T003 5 份、T102 卷宗 4 份；
  - 机器取本机，`--terms` 用敏感词清单，四组放进同一个暂存目录一次跑完；
  - 写完后的复扫没有命中；128 份逐字节和入库的一样，没有要改的录制。
- **金样例**：录制没有变化，所以不重新生成。`gold_` 测试照常通过：B 层 12 个，A 层连同其他名字以 `gold_` 开头的测试 49 个。
- **用 T110 那处验证**：
  - 取出 T110 手工修改之前的 4 份 s05（96b8428 的父提交）；
  - 新的 `scan` 报出了 4 处 "per-user temporary path"。行号范围分别是 78-82、81-85（testdata 的两份，第 1 行是头），77-81、80-84（SP-1 的两份）。也就是从 `"(/"` 那一块到 `/var/folders/` 结束的那一块；
  - 旧的 `scan` 是 clean；
  - 新工具重跑脱敏时，把那次运行的临时根目录作为 `--base` 传进去。目录名是从折叠后的文本里取的，没有打印出来。结果 4 份和 T110 手工改后入库的文件逐字节相同，与 T110 的改法一致。

## 本机跑过的检查

- `tools/sanitize` 的 vitest（13 个）、tsc、eslint、prettier：通过。
- `cargo xtask ci testdata-scan`：5 步通过（trace-ids、compat-reports、pii-scan、sensitive-words 等）。
- `cargo test -p xtask unit_sensitive_list`：3 个通过。
- `cupper-host` 的 `it_recording_export_two_instances_read_back_and_sanitize`：通过（它经命令行调用 `sanitize`）。
- `cargo test -p cupper-host --features test-harness gold_`：通过。
