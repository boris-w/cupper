# T095 工作笔记

## 开工前的核对

- 12 个权限请求的工具调用都带 `name`（Write、Edit、Bash、ExitPlanMode），`_meta` 是 null；工具名按 5.6.4 先看 `_meta.claudeCode.toolName`，没有时看 `name`，都没有就判断不出来，`allow-with-updates` 按持久处理（Q7）。
- Bash 的 `allow-with-updates` 原文带前缀：`Yes, and don't ask again for <前缀> commands`，或 `Yes, and allow access to <目录> and <前缀> * commands`。前缀先从原文解析，解析不出时取 `rawInput.command` 的第一个词。
- 金样例里每份录制的 `session` 行都带配置项，档位表换上后 `permission_mode` 各取值的 `risk` 都会变，所以几乎每份金样例都会变。

## 规模估计（开工前）

上限约 700 行（含测试，不含金样例）。

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `driver/claude/options.rs` | 按工具区分的对照表、按 kind 兜底、未知 kind、前缀与 `prefix_risk` | 130 | 90 |
| `ask.rs` | 换成查表（净改动） | 20 | — |
| `capabilities.rs` | 档位表，`risk` 按表给 | 20 | 10 |
| `gold_a.rs` | "取值都在表里"的检查，含一份带未知取值的内联录制 | — | 90 |
| 合并器与 `SessionCore` | 按 `tool_call_id` 找条目，`write_held` 填 `item_id`，一条用例 | 15 | 35 |
| 合计 | | 185 | 225 |

合计约 **410 行**，在上限以内。
