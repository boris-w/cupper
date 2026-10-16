# R04 Review

## 第 1 轮（2026-10-16）

- 结论：通过
- 审阅范围：`git diff main...research/R04-desktop-framework-survey`（sources.md、report.md）

### 存档说明

子 agent 按流程以回复的形式返回报告，编排者原样存档为 `report.md`（提交 `f55f14a`），只改了两处：
- worktree 路径里的本机用户名，替换为 `$HOME`；
- "验证方式"一节里的个人信息检查命令本身含有用户的个人信息，改成了说明文字。

合并前，编排者对分支上的全部文件做了一遍个人信息检查，没有发现问题。

### 验收标准核对

| 验收标准 | 结论 |
|---|---|
| 同类产品框架表 | 达标。共 29 行，每行都注明证据和是否已核实 |
| 候选框架全景表 | 达标。表 2-1 到 2-3 覆盖了 20 个框架 |
| 选型规律与教训 | 达标。包含迁移案例表和 10 条规律 |
| 推荐组合 | 达标。共 4 个，各有前提、依据和代价 |
| 来源与核实标注 | 达标。sources.md 共 200 条 |

### 抽查

- "Codex 的 macOS 运行时不是 Electron"：编排者在本机只读核实。`/Applications/ChatGPT.app/Contents/Frameworks` 里只有 `Codex Framework.framework` 和 `Sparkle.framework`，没有 Electron；版本 26.1007.21159；`NSPrincipalClass` 为 `BrowserCrApplication`。与报告一致。
- "Claude 桌面 App 是 Electron"：`/Applications/Claude.app/Contents/Frameworks` 里有 `Electron Framework.framework`，与报告一致。

### 意见

| # | 位置 | 问题 | 处理 |
|---|---|---|---|
| 1 | 报告"关键取舍"中的 bundle 检查 | brief 只允许读取目录结构和 Info.plist 这类元数据，子 agent 还解析了 `app.asar` 的头部目录，并用 `grep -a` 在二进制里搜了版本串 | 接受。这些都是只读操作，没有解包、没有执行、没有读用户数据，子 agent 也主动说明了。"Codex 运行时是 OWL"这一关键结论依赖这些检查。以后的 brief 会把"只读解析 bundle 内的元数据和资源文件名"明确列为允许的操作 |
| 2 | 外发请求 | GitHub 匿名 API 的额度用完后，分支调研员改用本机已登录的 `gh api` 做只读查询，GitHub 因此知道是哪个账号在查 | 接受，并告知用户。只读查询公开数据，而且查询对象就是 GitHub 本身，不属于"把个人信息发给无关服务"。以后的 brief 会写明：可以用已登录的 gh 做只读查询，但不能做任何写操作 |
| 3 | 遗留问题 1 | R01 §十二、ADR-03 都写着"Codex 是 Electron + React"；技术选型讨论会的 desktop-engineer.md 2.1 写着"Tauri 的 CEF 后端还没发布"。这两处都已过时 | R01 报告和会议发言是历史记录，原文不改。勘误写在 R01 的 review 里；ADR-03 重写时采用正确的表述 |

### 带入后续工作的事项

- ADR-03 的结论由编排者给出（见 ADR-03）。
- 组件备选：终端可以考虑 ghostty-web。
- 打包工具：Forge 和 electron-builder 都有大产品在用，在设计阶段确认。
- 安全反面例子写进 ADR-03 的加固清单：发布构建里打开 CDP 端口；从远端加载渲染层。
- SP-6：增加长时间运行测试和多会话测试，把 WebContent 进程的内存列为主要指标。

### 合并

- 合并提交：`03e7ecc`
