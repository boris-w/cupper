# T036 评审

- 评审人：编排者
- 日期：2026-10-26
- 结论：**通过，合并**

## 核对

- **规模**：+452 −20。开工前估约 540（乘 1.2 约 650），执行者在写代码前停下，编排者裁定收窄为宿主的设置（v1.3.56）；写完后越过停止线 400，执行者再次停下，编排者裁定提高到 480、硬停止线 500。
- **CI**：执行者的摘要 `pr-0145ad672fb0.json` 通过（rust 2135、protocol 185、ts 473、license、testdata-scan、e2e 30）。之后 main 进了 T069（宿主的测试与会话 actor 的测试挂钩），编排者合进分支后编译检查与设置的用例都通过，直接合并。
- **变异检查**：去掉回复前的重新发现、去掉写行，第二个用例都会失败。

## 逐项意见

1. **启动时读表要宽容**：宿主不认识的键、类型不对的值都跳过，不让启动失败。
2. **`settings/update` 的事务**：校验不通过回 `invalid_params`、账本票据记为拒绝、什么都不写；补丁里的键与账本记录同一事务提交，提交成功后才改内存。从校验到提交都在锁内，两个补丁不会互相覆盖。
3. **安全相关的键要求 `privileged`**：沿用网关已有的 `settings_update` 规则与规则矩阵的用例，没有重复。
4. **接进 agent 发现**：`launch::production` 原来写死的 `None` 与 `&[]` 改为从设置取；`cli_path`、`env_passthrough` 变了，回复之前先重新发现，界面收到回复时 `agents.updated` 已发出。重新发现做成回调，端口不带真实运行时也能测。
5. **不改契约**：`cupper-protocol` 没有改动，`cli_path` 暂用 `Option<WirePath>`。

## 记下的点

- **T016 改 `CliPath` 时**，要一并改 `settings.rs` 里的 CLI 路径校验和 `launch.rs` 里读 `claude_cli_path` 的那一行（已告诉 T016 的执行者）。Codex 的 CLI 路径要等 Codex 启动接进 `production` 才生效。
- **移走的三项**：`settings.updated` 与索引视图的 `settings`（T151）、桌面特权操作 `settings`（T147）、`AgentsUpdated.git`（T151）。
- **内置应用清单没有设计**（T013），`apps.terminal`、`apps.editor` 暂用 macOS 自带的 Terminal 和 TextEdit。记交接给 T126。
