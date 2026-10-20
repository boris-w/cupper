# T052 汇报

- 执行者：子 agent（工程师，测试基础设施）
- 分支：`feat/T052-recording-replay`
- 完成日期：2026-10-20

## 做了什么

T052 的第 1–4 项都已完成：格式库、SP-1 录制的转换、脱敏工具，以及接入 `testdata-scan` 的个人信息扫描。在干净的 worktree 里跑 `cargo xtask ci pr --record` 通过，摘要已提交。代码共 +1,515 −4 行，在 1,600 行的上限以内；这个数不含 `pnpm-lock.yaml`（+12）、转换后的录制和 notes。

1. **录制格式 v1**：放在 `crates/cupper-protocol/src/recording.rs`，442 行，其中测试约 108 行。
   - 公开的类型：`Header`、`Frame`、`Body`（对应 `dir` 的五种取值）、`Dir`、`Recording`。
   - 公开的函数：`Recording::parse`、`to_jsonl`、`segments()`，以及 `Header::to_line`、`Frame::to_line`，供 T047 逐行写出。
   - 不做 I/O，不派生 JsonSchema，不进 schema 导出。iOS 模拟器构建通过。
   - 校验规则按你认可的第 1 项实现：头行出现不认识的键就报错；负载与 `dir` 一一对应；`instance` 从 0 起、每次只能不变或加 1；每段以 `instance_started` 开头，它也不能出现在段中间；段内 `seq` 严格递增、`t` 不减；整数不超过 2^53−1。
   - 错误类型 `FormatError` 带行号和原因，用 thiserror 实现。
2. **SP-1 录制的转换**：
   - 一次性脚本在 `docs/tasks/T052-recording-replay/convert-sp1.ts`，102 行，运行方式写在 `notes.md` 里。48 份全部入库，三个目录分别是：

     | 目录 | 份数 |
     |---|---|
     | `testdata/recordings/claude-agent-acp/0.91.0/2.1.296/` | 18 |
     | `testdata/recordings/codex-acp/2.2.2/0.160.1/` | 16 |
     | `testdata/recordings/codex-acp/2.2.2/0.159.2/` | 14 |

   - 映射按你认可的第 2 项实现。
   - 头行的 `scenario` 取文件名。SP-1 保留的两次补跑（`claude/s07x`、`codex-bundled/s10x`）在 SP-1 自己的头里写的还是原场景名，所以不能用它。
   - `crates/cupper-protocol/tests/recordings.rs` 用格式库把入库的录制全部读一遍，并核对：路径与头行一致；SP-1 的录制共 48 份，`usage` 只有 `gold-a`；`s01` 每份 2 段、`s09` 每份 3 段、其余 1 段；每段恰好一个 `instance_exited`。
3. **`tools/sanitize`**：TS 包，结构照 `tools/trace-ids`。
   - `sanitize` 命令：SP-1 的规则原样迁移，只把输入输出改成格式 v1（`note` 是对象），按目录递归处理。写完重新扫描，查运行环境里的家目录、用户名、主机名、`--terms` 的词、会话 ID，以及下面的通用模式，命中就失败。`--terms` 指定的文件不存在时直接报错；SP-1 原来是静默忽略。
   - `scan` 命令：查通用模式，包括 `/Users/` 后跟非占位符的路径、`/var/folders/`、邮箱（`example.*`、`.invalid`、`.test` 这类占位域名除外）、几种 token 样式。
   - `words --list` 命令：查本机敏感词清单。清单一行一个词，`#` 开头是注释，匹配不区分大小写。
   - 所有命中都只输出 `文件:行号: 规则名或条目序号`，不输出命中的文字。
4. **`xtask ci testdata-scan`**：
   - 在 `trace-ids` 之后依次跑两步：`pii-scan` 和 `sensitive-words`。范围都是 `testdata/` 加 `docs/tasks/*/ci/*.json`。
   - 敏感词清单经 `git rev-parse --path-format=absolute --git-common-dir` 找到主仓库的 `.private/sensitive-words.txt`。
   - 清单不存在时，终端显示 `sensitive-words skipped: no local sensitive word list (...)`，结果汇总里有一行 `skipped steps: testdata-scan / sensitive-words`。摘要的每个作业条目新增 `skipped` 字段，这里是 `["sensitive-words"]`。摘要字段白名单和 `unit_ci_summary_holds_allowlisted_fields_only` 都同步改了。

## 关键取舍

- **用例编号的归属**：UNIT-sensitive-list 分两处写。
  - TS 部分测清单格式、命中时退出码为 1、输出里没有命中的文字。
  - Rust 部分测三件事：从链接的 worktree 能找到主仓库的清单；清单缺失时跳过并记进摘要；在本仓库真实的 `testdata/` 上用临时清单跑这一步，命中时步骤失败，日志里没有命中的词。
- **跳过提示用英文**：与 xtask 其余输出一致。
- **占位域名不算命中**：`user@example.invalid` 这类邮箱在通用扫描里放行。脱敏本身仍按 SP-1 的规则，把所有邮箱替换掉。
- **通用模式不含长 base64 串**：录制里有测试图片的 base64，会误报。脱敏仍按 SP-1 把长串换成 `<opaque>`，只跳过图片数据。

## 验证方式

- 我在合入前的 `de0e404` 上，用干净的 worktree 跑 `cargo xtask ci pr --record docs/tasks/T052-recording-replay/ci/`，全部通过。摘要是 `pr-de0e404e3921.json`，提交为 `8c50efd`。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,171 通过 / 0 失败 |
  | protocol | 185 / 0 |
  | ts | 323 / 0 |
  | license | 1 / 0 |
  | testdata-scan | 3 / 0，`sensitive-words` 跳过 |
  | ui、e2e | 还没实现 |

  license 作业这次没有遇到网络问题。
- 对照验收标准：
  - UNIT-rec-format、UNIT-sensitive-list 在摘要里都是 pass，测试名都以编号开头。✅
  - `testdata/` 和入库的 CI 摘要通过通用扫描，共 120 个文件；新提交的摘要我也单独扫过一遍，没有命中。本机还没有敏感词清单，这一项是"跳过"。✅
  - 48 份录制已转换入库。`tools/sanitize` 重新扫描通过（`clean: 48 files`），而且它的输出与转换结果逐字节相同，即脱敏没有改动任何内容，也没有发现漏网的个人信息，规则没有改。✅
  - 编号都在 `tests/ids.toml` 的 T052 段里，`trace-ids check` 通过。按你的说明我没有改这一段，UNIT-replay 在我的分支上是待完成，不影响作业结果。✅
  - persona 比对已按裁定移出本任务，归 T067。
- 每个提交后都跑过对应的 fmt、clippy（`-D warnings`）和测试；TS 部分跑过 prettier、eslint 和 `tsc --build`。

## 提交列表

```
8c50efd docs(task): record the T052 pr CI summary
de0e404 feat(xtask): scan test data for personal data in testdata-scan
0f3680d test(recordings): convert the 48 SP-1 recordings to format v1
33a9b17 feat(sanitize): migrate SP-1's redaction and add the personal data scans
4840e67 feat(protocol): add the recording format v1
89a03d2 docs(task): record the T052 design sketch and size estimate
```

## 遗留问题 / 范围外发现

1. **录制里的 uid**：两份 Claude 录制（`s06`、`s07x`）里有 Claude CLI 后台任务的输出路径，形如 `/private/tmp/claude-<uid>/-tmp--repos-claude/...`。其中 uid 是 macOS 第一个用户的默认值，不是用户名或主机名；路径的其余部分已被 SP-1 脱敏成 `-tmp-`。我判断它不算个人信息，没有改规则，请你定夺。
2. **敏感词清单还没建**：本机没有 `.private/sensitive-words.txt`，所以这次 `sensitive-words` 是跳过的。计划第 6 节第 4 行要求在 T066 之前由用户建好；建好后 `testdata-scan` 会自动启用这一步。
3. **给 T053、T067 的接口说明**：
   - 格式库读文件靠 `Recording::parse(&str)`；分段用 `segments()`，按 `instance` 切分，从 0 起。
   - 假 agent 的 `--segment` 编号与 `instance` 一致。
   - 判断 note 事件用 `Body::event()`，常量是 `INSTANCE_STARTED`、`INSTANCE_EXITED`。
4. **本机一个统计数据**：转换后的录制约 3.1 MB。`prettier --check .` 不处理 `.jsonl`，不需要改 `.prettierignore`。
5. **worktree 已保留**，T067 可以接着在里面做。