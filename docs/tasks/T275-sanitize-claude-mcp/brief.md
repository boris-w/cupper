# T275 sanitize：Claude 的 MCP 名与绝对路径；rec-shape-diff 过滤

- 派发日期：待定（T274 之后）
- 执行者：待定
- 分支：`fix/T275-sanitize-claude-mcp`
- 依赖：无。由 T113 拆出；T113 依赖本任务

## 范围

**做**（T113 估算的块 3、块 4）：

1. `tools/sanitize` 对 Claude 的 MCP 服务器名改成先取名字再全局替换，与 T204 对 Codex 的做法一致；覆盖 "Some MCP servers are still connecting: <名字>" 这类文本（T204 的交接）。
2. `words`、`scan` 不再把绝对路径拼到工作目录后面（T066 的交接）。
3. `rec-shape-diff` 过滤 `toolResponse` 下随工具不同的键（T066 的交接）。
4. 测试。

## 验收标准

- [ ] 上面三项各有测试；已入库的录制重新扫描不出新的命中，或命中逐条说明。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 估约 120。停止线：160；硬停止线：200。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
