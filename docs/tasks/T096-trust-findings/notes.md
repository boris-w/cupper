# T096 工作笔记

- 执行者：子 agent（Rust 工程师，宿主安全）

## 开工前的规模估计

按模块估计，行数指 rustfmt 展开后的行，含文档注释和空行。读过任务书、5.2.6"项目信任"、5.3.11 的 `FindingKind`、T035 的补充裁定和 notes、`trust` 模块与 `cupper-protocol` 的 `Finding`。

| 模块 | 内容 | 草估 |
|---|---|---|
| `trust/findings.rs` | 按路径分派（去掉开头的 `../`）：`.claude/settings*.json` 的 hooks（每条命令一项）、`env`（每个键一项）、`apiKeyHelper`、`statusLine`；`.mcp.json` 的每个服务器；`.claude/skills/<名>/SKILL.md`、`agents/**/*.md`、`commands/**/*.md` 各一项；`.codex/**/*.toml` 的每个叶子配置项（键路径加原文，取自 `toml_edit` 的 span）；解析不了的记 `unparsed`；`target`、`outside_workspace` 取 `ScannedFile` | 200 |
| `trust/mod.rs` | `Scan::findings()`、模块文档 | 10 |
| 协议 | `FindingKind::Unparsed`、5.3.11 的一行；`pnpm gen` 的生成物不计 | 5 |
| `Cargo.toml` | `toml_edit` | 3 |
| **实现小计（草估）** | | **218** |
| **乘 1.7** | | **约 370** |
| UNIT-trust-findings | 同一个 `settings.json` 里的四类；`.mcp.json` 的命令和 URL 两种服务器；skills、agents、commands；`.codex/config.toml` 的叶子与原文；`unparsed`（JSON 不合法、不是对象、TOML 不合法、不是 UTF-8）；链接到工作区外的目标 | 150 |
| **合计** | | **约 520**（草估 368） |

在约 600 行的上限之内，开工。

细节约定：
- JSON 对象的键先排序再遍历，不依赖 serde_json 的 map 键顺序（workspace 里 serde_json 可能开了 `preserve_order`）。结构不合预期时，`detail` 用 RFC 8785 的规范 JSON（`cupper_protocol::canonical`）。
- TOML 的 `detail` 用 `toml_edit` 解析后的 span 从原文截取，所以不需要它的 `display` 特性。

## 实际行数

按提交统计（`git diff --numstat`），不含 `Cargo.lock` 和 `pnpm gen` 的生成物。

| 提交 | 内容 | 增 | 删 | 估计 |
|---|---|---|---|---|
| `feat(protocol): the unparsed kind …` | `FindingKind::Unparsed`、5.3.11 的一行；生成物另有 schema 3/2、TS 4/2 | 4 | 1 | 5 |
| `feat(host): findings of the trust scan …` | `findings.rs`（实现约 210，测试约 178）、`mod.rs`、`Cargo.toml` | 394 | 2 | 213 + 150 |
| **合计** | | **398** | 3 | 约 520（草估 368） |

## 实现要点

- **入口**：`Scan::findings()`，按文件顺序给出 `Vec<Finding>`。`target` 只在 `linked` 时给（取 `ScannedFile.target`），`outside_workspace` 原样带过来。
- **分派**：先去掉路径开头的 `../`，再按路径判断：
  - `.mcp.json`；
  - `.claude/settings*.json`（`.claude/` 下的直接子文件）；
  - `.claude/skills/<名>/SKILL.md`；
  - `.claude/agents/**/*.md`、`.claude/commands/**/*.md`；
  - `.codex/**/*.toml`。

  其他文件没有 finding，但仍计入摘要。
- **detail**：
  - **字符串**：原样。
  - **其他 JSON 值**：用 `cupper_protocol::canonical`，按 RFC 8785 输出，键有序。
  - **hooks**：`<事件> <matcher>: <命令>`，没有 matcher 时省掉 matcher；没有 `command` 的 hook（例如 prompt 类型）整条按规范 JSON 输出。
  - **MCP 服务器**：`<名>: <命令> <参数…>` 或 `<名>: <url>`，两者都没有时整条输出。
  - **commands**：`/<相对名>`。
  - **TOML**：`<键路径> = <原文>`，原文按 `toml_edit::Document` 的 span 从源文本截取；数组表写成 `key[i].`。
- **unparsed**：以下情况记一条 `unparsed`，`detail` 写原因，其余文件照常解析：
  - 文件不是 UTF-8；
  - JSON 不合法，或顶层不是对象；
  - TOML 不合法（`detail` 是 `toml_edit` 的错误信息）。

  结构合法但某个键的形状不合预期时，不算 unparsed，而是把那个值整个展示出来。
- **键顺序**：JSON 对象先按键排序再遍历，不依赖 serde_json 的 map 顺序。workspace 的锁文件里 serde_json 依赖 indexmap，可能开了 `preserve_order`。TOML 按文件里的书写顺序，这个顺序由 `toml_edit` 自己保证。
