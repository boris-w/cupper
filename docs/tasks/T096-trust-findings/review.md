# T096 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：由 T035 拆出（计划 v1.3.15）。合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-a293e8a35179.json`。执行者那次的 `pr-fbc8f95fa3d3.json` 一并保留。
- 核对：
  - 新增依赖是任务书认可的 `toml_edit =0.25.17`（`default-features = false, features = ["parse"]`），带进 `toml_parser`、`toml_datetime`、`winnow`，`cargo deny` 的 licenses 和 bans 都通过；
  - 协议改动只有 `FindingKind::Unparsed`（附加式），`pnpm gen` 的生成物随提交，5.3.11 那一行已改；
  - 用例在内存里构造 `ScannedFile`，不碰文件系统，样例里的名字、命令、路径都是虚构的；没有个人信息。
- 审阅：
  - **只从 `Scan.files` 的内容解析**，不再读文件；解析失败记 `unparsed`，扫描照常、摘要照常覆盖完整内容。
  - **JSON 先按键排序再遍历**：workspace 里 serde_json 可能开了 `preserve_order`，先排序就不依赖 map 键顺序，符合代码约束。
  - **TOML 的原文按 span 截取**：不需要 `display` 特性，`detail` 就是"文件怎么写就怎么显示"。
  - 形状不合预期但结构合法的值整个按规范 JSON（RFC 8785）展示，不记为 unparsed：认可，确认框照样能看到内容。
  - MCP 服务器只展示命令和参数（或 url），不展示 `env`：认可，`env` 里常有密钥。
- 规模：约 398 行（实现约 220、测试约 178），在约 600 的上限以内。
- 转交事项已写进 `handoff-notes.md`：T034（`scan.findings()` 填进 `TrustScan.findings`；`.codex` 按叶子配置项列可能产生很多条，由帧上限的处理一并覆盖）。
