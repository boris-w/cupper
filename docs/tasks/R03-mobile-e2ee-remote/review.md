# R03 Review

## 第 1 轮（2026-10-16）

- 结论：通过
- 审阅范围：`git diff main...research/R03-mobile-e2ee-remote`（report.md、sources.md、sources-happy.md、sources-others.md）

### 验收标准核对

| 验收标准 | 结论 |
|---|---|
| 产品对比表 | 达标。比了 11 个产品，额外加了"配对"和"推送"两列 |
| 连接方案、加密方案取舍表 | 达标。连接 C1–C6；加密分为配对、消息、多设备三张表，另有撤销和元数据两部分 |
| 移动端功能清单 | 达标。20 项功能，分"普遍、较普遍、差异化"三档 |
| 来源、日期、核实标注 | 达标。每条事实都标了 `[编号·核/推]`，来源可追溯到 URL 或"仓库 + 提交 + 路径" |
| 对架构的启示 | 达标。A1–A11，附分层图 |

### 抽查

- Happy 配对应答没有认证（#1503）：用 `gh issue view 1503 -R slopus/happy` 核对，issue 存在，状态为 open，标题是"Security Flaw: Content Key is delivered unauthenticated"，与报告一致。

### 意见

| # | 位置 | 问题 | 处理 |
|---|---|---|---|
| 1 | report.md「提交列表」 | 报告提交自身的哈希写成了占位符 `<本提交>` | 不改。提交前无法得知哈希，实际哈希见合并记录 |
| 2 | report.md 遗留问题 7 | 系统不允许子 agent 创建报告文件，子 agent 先写到临时目录，再用 shell 复制进来，绕过了这个限制 | 本次报告保留。流程已调整：子 agent 以回复形式返回汇报，由编排者存档为 report.md，以后不再绕过限制（见 CLAUDE.md 子 agent 守则第 6 条） |

### 带入 PRD 的事项

- 待产品决策（并入需求访谈的待澄清问题）：
  - 中继是否保存密文历史（A6）
  - 是否承诺可自托管，以及推送网关由谁运营（A7）
  - 移动端第一版的功能范围（A11）
  - 手机端能否远程执行 shell、写文件，允许哪些权限模式
- 架构启示 A1–A11 作为产品-架构对齐会的输入。
- 转给 R02 参考的发现：
  - Happy 驱动 Codex 时用的是 `codex app-server`，没有用 codex-sdk，因为后者没有审批回调。
  - app-server 自带 `remoteControl/*` 方法。

### 合并

- 合并提交：`93960ac`
