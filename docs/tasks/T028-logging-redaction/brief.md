# T028 日志与脱敏

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主诊断）
- 分支：`feat/T028-logging-redaction`
- 依赖：T022（已合并）。T055（AgentRuntime）依赖本任务，松弛 3 个单位

## 背景

宿主现在没有正式的日志：T037 之后，发布构建的 stdio 指向 `/dev/null`，启动失败的原因、会话 actor 发现的 bug 都写进了 stderr，等于丢掉。适配器的 stderr 也还没有人读。本任务建立宿主的日志和脱敏层（设计 5.11.1、5.11.2），供之后所有宿主模块使用。

## 范围

**做**（计划 T028 一行；设计 5.11.1、5.11.2、5.9.8 的脱敏样例测试）：
1. **`host.log`**：tracing 输出 JSON Lines（时间、级别、模块、消息、结构化字段），写在 `~/Library/Logs/cupper/`（目录 0700），按大小轮转 10 MB × 5。默认级别 `info`；开发构建同时输出到终端。
   - 各构建种类的日志目录按 5.7.9 的数据目录区分（例如自用构建、开发构建不与发布构建混写），核对设计；设计没写清的，在汇报里提出，先按"和数据目录同名区分"实现。
   - 测试和 e2e 构建把日志写进每个用例的临时目录（经 `--test-config` 或启动载荷传入，用现有的注入点）。
2. **`info` 不记内容**：prompt、消息正文、命令输出、文件内容在 `info` 级别只记长度和类型。`diagnostics.verbose` 打开后为 `debug`，允许记录内容；设置项本身由 T036 做，本任务提供开关的接口。
3. **两档脱敏**（5.11.2）：
   - 结构化脱敏：用于原始帧，只处理账号字段和已知格式的凭据；
   - 完整脱敏：用于日志，下表全部规则；
   - **凭据规则只保留一份**（交接，T050）：现在在 `driver/tap/redact.rs`（`sk-`、`ghp_`、JWT、`Bearer`、URL 中的 userinfo 和查询参数，只匹配词首），挪到共用的位置（例如 `diag` 模块），原始帧的分接改为调用它，行为不变，原有用例照常通过。
4. **适配器 stderr 管道读取**：
   - 提供一个读取器：输入是适配器的 stderr 管道和实例 ID；每行加实例 ID 和时间，完整脱敏后写进 `adapter-claude.log` 或 `adapter-codex.log`（5 MB × 3）；
   - 在内存里为每个实例保留最后 64 KB，供 `error` 条目的错误详情（MS-5）使用；
   - 拉起适配器、把管道交给读取器，由 T055 接线。本任务用测试里的管道驱动。
5. **panic hook**：panic 的信息和位置写进 `host.log`（完整脱敏）。
6. **把现有的 stderr 输出改写进 host.log**（交接，T037、T043）：启动失败的原因（`main` 里，stdio 重定向之后）；会话 actor 在视图拒收事件时的 stderr（测试构建里仍然 panic）。顺带查一遍 `cupper-host` 里其余的 `eprintln!`，能改的一并改，改不了的列进汇报。

**不做**：
- 诊断包（M2）；
- `main.log`（桌面主进程，桌面任务）；
- `diagnostics.verbose` 的设置项和原生确认（T036）；
- 拉起适配器（T055）。

## 验收标准

- [ ] 新增 UNIT-redaction：5.11.2 表里每类规则都有样例，两档分别断言；长串规则不误伤宿主自己的 blob 哈希和 ID（按字段名放行）；结构化脱敏不改写 base64 图片和哈希。
- [ ] 新增 UNIT-redact-auth-status：`_auth/status_update` 只保留 `kind` 和套餐名。
- [ ] 轮转、64 KB 尾部缓冲、`info` 不记内容、panic hook 都有用例。
- [ ] 原有的原始帧脱敏用例照常通过，证明凭据规则挪动后行为不变。
- [ ] 不睡眠；时间只经 `Clock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T028 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T028-logging-redaction/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T050：凭据规则在 `driver/tap/redact.rs`，挪到共用的位置，避免两处各自改出差异。
- T037、T043：发布构建的 stdio 指向 `/dev/null` 之后，启动失败的原因和 actor 的视图拒收提示要改写进 host.log。

## 注意

- 用例里的日志目录一律用临时目录，**不要往本机真实的 `~/Library/Logs/cupper/` 写任何东西**。
- 代码约束：时间只经 `Clock`；`cupper-host` 不用 `unsafe`；错误用 thiserror。新增依赖（例如 `tracing-subscriber`、轮转用的 crate）锁精确版本，许可证在 ADR-11 的 allowlist 内；能自己写一个简单的按大小轮转就不必引入专门的 crate，取舍写进 `notes.md`。
- 样例里的"用户名""主机名"用明显虚构的值，不要用本机的。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍计。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,200 行时，开工前就停下来汇报，并给出拆分建议**（例如把"日志与轮转、panic hook"和"脱敏与 stderr 读取"分开）。

## 参考资料

- 设计：`11-diagnostics.md` 5.11.1、5.11.2；`09-testing.md` 5.9.8；`07-desktop.md` 5.7.9（构建种类与数据目录）；`02-host.md`（模块表里的 `diag`）。
- 安全建议 #15；ADR-06（原始帧）。
- `crates/cupper-host/src/driver/tap/`（T050）；`docs/tasks/T050-driver-transport/notes.md`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
