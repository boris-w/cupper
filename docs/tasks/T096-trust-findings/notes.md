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
