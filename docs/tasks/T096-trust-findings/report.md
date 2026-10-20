# T096 汇报

- 执行者：子 agent（Rust 工程师，宿主安全）
- 分支：`feat/T096-trust-findings`（按指示在原 T037 worktree 里从 main 建出）
- 完成日期：2026-10-20

## 做了什么

T096 已完成：信任扫描现在能列出确认框要展示的 findings，协议增加了 `unparsed` 这一类。UNIT-trust-findings 通过，`cargo xtask ci pr` 全部通过。改动约 398 行（实现约 220、测试约 178），不含 `Cargo.lock` 和 `pnpm gen` 的生成物，在约 600 行的上限之内。开工前在 notes.md 里的估计是约 520 行（草估 368）。

1. **协议**：`FindingKind` 加了 `Unparsed => "unparsed"`，5.3.11 列取值的那一行同步修改，`pnpm gen` 重新生成了 schema 和 TS，随提交一起提交。
2. **`Scan::findings()`**（`trust/findings.rs`）：只从 `Scan.files` 的 `path`、`content` 解析，不再读文件。按路径分派时先去掉开头的 `../`：

   | 文件 | 解析出的 findings |
   |---|---|
   | `.claude/settings*.json` | 每条 hook 命令；`env` 的每个键；`apiKeyHelper`；`statusLine` |
   | `.mcp.json` | `mcpServers` 里的每个服务器 |
   | `.claude/skills/<名>/SKILL.md` | 技能 |
   | `.claude/agents/**/*.md` | 代理 |
   | `.claude/commands/**/*.md` | 命令（`/<名>`） |
   | `.codex/**/*.toml` | 每个叶子配置项，写成 `<键路径> = <原文>`；原文按 `toml_edit` 的 span 从源文本截取，数组表写成 `key[i].` |

   - **解析失败**：不是 UTF-8、JSON 不合法或顶层不是对象、TOML 不合法时，各记一条 `unparsed`，`detail` 写原因；扫描照常进行，摘要仍覆盖该文件的完整内容。
   - **形状不合预期**：结构合法但某个键的形状不对时，把这个值整个按规范 JSON 展示，不记为 unparsed。
   - **target 与 outside_workspace**：`target` 只在 `linked` 时取 `ScannedFile.target`；`outside_workspace` 原样带过来，所以工作区外的目标会逐项列出。
   - **detail 不截断**：字符串原样输出；其他 JSON 值用 `cupper_protocol::canonical` 输出 RFC 8785 形式，键有序。
3. **TOML 依赖**：`toml_edit = "=0.25.17"`，配 `default-features = false, features = ["parse"]`。新拉进来的是 `toml_parser`、`toml_datetime`、`winnow`；`cargo deny` 的 licenses 和 bans 都通过。

## 关键取舍

- **JSON 先按键排序再遍历**：workspace 里 serde_json 依赖 indexmap，可能开了 `preserve_order`。先排序，map 键顺序就不会影响输出，符合代码约束。TOML 按书写顺序输出，这个顺序由 `toml_edit` 保证。
- **TOML 原文用 span 截取**：这样不需要 `toml_edit` 的 `display` 特性，`detail` 也就是"文件怎么写就怎么显示"。
- **一个配置项一条 finding**：`.codex` 按叶子配置项列；MCP 服务器只展示命令和参数（或 url），不展示 `env`。
- **测试在内存里构造 `ScannedFile`**：不碰文件系统，样例里的名字、命令、路径都是虚构的（`acme-*`、`/work/...`、`*.invalid`）。

## 验证方式

- [x] **UNIT-trust-findings**：5 个用例，名字都以 `unit_trust_findings_` 开头。
  - 同一个 `settings.json` 里的四类（hooks 含 matcher、无 matcher、无 command 三种，`env` 按键排序，`apiKeyHelper`，`statusLine`），外加上级目录的 `settings.local.json`；
  - `.mcp.json` 的命令型、URL 型和形状不合预期的服务器；skills、agents、commands；与 findings 无关的文件不出 finding；
  - `.codex/config.toml`：标量、数组、点分表、内联表、数组表，按原文输出；
  - `unparsed`：不是对象、不是 UTF-8、TOML 不合法、JSON 不合法；之后的文件照常解析；
  - 链接到工作区外的目标：`target` 和 `outside_workspace` 都正确。
- [x] `FindingKind::Unparsed` 已进入 `protocol/schema/v1.json` 和 `packages/protocol/src/gen/*`；5.3.11 那一行已改。
- [x] UNIT-trust-findings 已在 `tests/ids.toml` 的 T096 段，`testdata-scan` 通过。
- [x] 在干净的 worktree 上跑了 `cargo xtask ci pr --record docs/tasks/T096-trust-findings/ci/`：rust 1475、protocol 185、ts 323、license、testdata-scan 全部通过，摘要 `ci/pr-fbc8f95fa3d3.json` 已提交。只有 `sensitive-words` 一步跳过，因为本机没有敏感词表，这是 CI 本身的行为。

## 提交列表

```
1b12e80 docs(task): T096 CI summary (pr at fbc8f95fa3d3)
fbc8f95 docs(task): T096 notes: actual lines and parsing points
504f30f feat(host): findings of the trust scan for the native confirmation
92d9a76 feat(protocol): the `unparsed` kind of trust findings
53e77ff docs(task): estimate T096 by module
```

## 遗留问题 / 范围外发现

- **交给 T034**：`project/add`、`project/trust_info` 现在可以把 `scan.findings()` 填进 `TrustScan.findings`。`detail` 不截断，帧上限的问题按裁定第 8 条已交给 T034。
- 文件很多时，`.codex` 按叶子配置项列可能产生很多条 finding；这也一并由 T034 的帧上限处理覆盖。