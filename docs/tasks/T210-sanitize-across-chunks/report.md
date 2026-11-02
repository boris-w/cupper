# T210 汇报

- 执行者：子 agent（工程师）
- 分支：`test/T210-sanitize-across-chunks`（PR #25：https://github.com/boris-w/cupper/pull/25 ）
- 完成日期：2026-10-30

**CI 还没跑完，请你看结果。** 你原本要我等 8 个检查跑完再交汇报，但系统要求我现在就交。交的时候，运行 https://github.com/boris-w/cupper/actions/runs/41008639826 的 8 个检查都还是 pending。如果有检查失败，请把我叫回来修。

## 做了什么

**1. 跨 chunk 的脱敏和扫描（`tools/sanitize`）**

- **按消息归组**（新文件 `streams.ts`）。只看带 `params.update` 的帧，按"会话 + 更新类型 + 条目"归组：
  - 消息、思考、用户消息的 chunk（`content.text`）按 `messageId` 归组；
  - 命令输出的增量（`_meta.terminal_output.data`）按 `toolCallId` 归组；
  - 有 ID 的，中间夹着别的帧也照样归进同一组；
  - 没有 ID 的，按相邻的同类 chunk 分段。同一会话里来了一个类型不同的更新，就开始新的一段。
- **脱敏**（`sanitize.ts`）：
  - 原来 `#scrubString` 依次做的会话 ID、MCP 服务器名、机器规则、长串，现在整理成一张有序的规则表。规则本身没变，逐帧脱敏照旧按这张表做；
  - 逐帧做完之后，把每组文本折叠起来，按同一张表再替换一遍；
  - 命中的占位放进命中开头所在的那一块，命中的其余部分从后面几块里删掉，命中前后的文字留在原块。帧数和 `seq` 不变，和 T110 手工的改法一样；
  - 替换文本里的 `$1`（`Bearer <secret>` 用到）照 `String.replace` 的规则展开。
- **扫描**（`scan.ts`）：
  - 匹配器改为返回命中位置（规则名加起止偏移）。`scan`、`words` 和脱敏写完后的复扫，都是先逐行扫，再逐组折叠后扫；
  - 跨帧的命中报 `文件:起行-止行: 规则`，不报命中的文字；
  - 命令行的用法没变。

**2. UNIT-sanitize-across-chunks 通过**，编号原本就登记在 `tests/ids.toml` 的 `[T210]` 段。

- 合成的录制里有三种情况：
  - 一个临时路径切成 4 个消息 chunk，中间夹着思考；
  - 一个词表里的词跨两个没有 ID 的思考 chunk；
  - 一个 Bearer 令牌跨 3 段命令输出。
- 断言：
  - 脱敏后每一块的内容都和预期一致，帧数和 `seq` 不变，复扫干净，再跑一遍结果不变；
  - `scan` 和 `words` 报出正确的行号范围，输出里不带命中的文字。
- 变异检查做了 14 处，每一处都让用例失败：
  - 归组的 4 种来源和键（`messageId`、无 ID 的分段、chunk 来源、命令输出、更新类型）；
  - 替换的落点、命中其余部分的删除、`$1` 展开；
  - 脱敏和扫描各自去掉折叠这一遍；
  - 范围只报首行、止行多算一个字符；
  - `terms` 和 `patterns` 的命中位置。

**3. 全仓库复扫**（逐条结果在 `notes.md`，只记文件、行号和规则）

- **扫描范围**：`testdata`、`spikes`、`docs/tasks/*/data`、`docs/tasks/*/ci/*.json`，共 630 个文件，新旧工具各扫一遍：
  - 通用模式：新旧都是同样的 4 处，都在 spike 的脚本和说明文件里（扫描规则本身写着的家目录前缀，一个像邮箱的图标文件名），和 T204 记的一样，不是录制；
  - 敏感词清单：clean；
  - 用本机规则（家目录、用户名、主机名、临时目录）加词表再扫一遍：7 处，都在 5 个脚本和说明文件里，不是录制和金样例。
- **重新脱敏**：用新工具对已入库的 128 份录制重跑（`testdata` 71 份、SP-1 48 份、T003 5 份、T102 4 份），复扫没有命中，128 份逐字节不变。
- **结果**：没有要改的录制，所以金样例也不用重新生成。

## 关键取舍

- **只折叠增量文本。** `tool_call` 的 `content`、`rawInput`、`rawOutput` 每次都是整份，stderr 是一帧一行，所以不折叠。
- **两遍做。** 先逐帧脱敏，再折叠后补一遍。只在一块里的命中第一遍就已经换掉，第二遍只会碰到跨块的命中，工具对自己的输出仍然不变。
- **报精确的行号范围，不报整条消息的范围。** 为此匹配器要返回命中位置，这是行数超出估计的主要原因。你裁定保留精确范围，停止线放宽到 280。
- **用 T110 那处回放验证。** 取 T110 手工修改前的 4 份 s05：
  - 新 `scan` 报出 4 处 "per-user temporary path"，旧 `scan` 是 clean；
  - 新工具重新脱敏时，把那次运行的临时根目录作为 `--base` 传进去，输出和 T110 手工改后入库的文件逐字节相同。

## 验证方式

- [x] **UNIT-sanitize-across-chunks** 通过，已登记；14 处变异全部被测出。
- [x] **全仓库复扫**的结果写进了 `notes.md`，没有新的命中。
- [x] **A、B 层金样例**：本机跑 `cargo test -p cupper-host --features test-harness gold_` 通过，金样例没有变化。
- [ ] **PR 上的 8 个检查**：交汇报时都是 pending。运行链接：https://github.com/boris-w/cupper/actions/runs/41008639826
- 本机跑过的检查：

| 检查 | 结果 |
|---|---|
| `tools/sanitize` 的 vitest | 13 个通过 |
| tsc、eslint、prettier | 通过 |
| `cargo xtask ci testdata-scan` | 5 步通过（含 pii-scan、sensitive-words） |
| `cargo test -p xtask unit_sensitive_list` | 3 个通过 |
| `it_recording_export_two_instances_read_back_and_sanitize` | 通过（经命令行调用 sanitize） |
| `gold_` | 通过 |
| `git merge-tree` 对 main | 无冲突 |

- **规模**：`tools/sanitize` +262 −47。超过了原来的停止线 250，我当时停下来报告了你，你裁定停止线改为 280，现在在这条线以内。

## 提交列表

```
4481a37 docs(T210): notes on the grouping, the mutation checks and the rescan
b3ba656 fix(sanitize): redact and scan text streamed over several chunks
```

## 遗留问题 / 范围外发现

1. **折叠后的长串规则可能误伤。** `<opaque>` 规则（64 个以上连续的 base64 字符）现在也作用在折叠后的文本上，一条消息里被切开的长路径也可能被换成 `<opaque>`。这和逐帧时对完整字符串的行为一致，现有录制没有因此变化。T113 入库新录制时，如果在金样例里看到意外的 `<opaque>`，原因在这里。
2. **没有 ID 的 chunk 按相邻同类分段。** 两轮之间如果没有任何别的更新，两条没有 ID 的消息会被连成一组。对扫描和脱敏来说只会多匹配，不会漏。现有录制里没有 ID 的 chunk 只有 8 帧（假 agent 的 3 份、Codex 的 `s10x` 两份）。
3. **`leftovers()` 里 MCP 服务器名的匹配器没有单独的测试覆盖**（T204 起就是这样）：让它总是返回空，现有用例也不会失败。本任务只把它改成返回命中位置，没有补测试。
