# T031 策略

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关与安全）
- 分支：`feat/T031-policy`
- 依赖：T013、T018、T022（均已合并）。M1a 第 4 波，**在关键路径上**：T013 → T031 → T033 → T043 → …。T042（交互代理）也依赖它。

## 背景

宿主对每个请求做两级判定（设计 5.2.5、5.8.2、5.8.3）：
- **方法级**：由协议常量 `SCOPE` 驱动，看 principal 有没有这个 scope；
- **资源级**：方法级允许之后，再按具体参数判定，规则在 `policy` 模块里注册。

T012、T013、T018 已经把 5.8.3 的权限矩阵写成了协议 crate 里的测试表 `MATRIX`。scope 决定不了的格子，用这几种格子标出，本任务负责实现它们：
- `Native`：原生确认；
- `ByMain`：仅由主进程代发；
- `Sec9`：scope 允许，但按 SEC-9 由策略拒绝；
- `Input`：window 的请求必须带 `input`；
- `Build`：只在特定构建里有；
- `CliOnly`：除 `cli` 以外一律拒绝。

账本（命令去重）不在本任务里，归 T032。

## 范围

**做**（计划 T031 一行；设计 5.2.5、5.8.2、5.8.3、5.7.8）：
1. **`policy` 模块**（`crates/cupper-host/src/policy/`）：纯函数或纯核心，不做 I/O；需要的时间作为参数传入。
2. **方法级**：按 principal 的 scope 集合和方法的 `SCOPE` 判定。
3. **资源级规则**：
   - **档位**：放宽档位的规则（5.8.2），手机不能放宽档位（SEC-9）；
   - **privileged 选项**：只能经原生确认；
   - **安全设置键**：`settings/update` 改安全相关的键时要 privileged（5.2.11 的键表）；
   - **window 角色的输入证据**：`interaction/respond`、`git/revert` 来自 window 时必须带 `input`。按键与选项相符，`digit` 与所选选项的序号相符，特权选项和效果未知的选项也占序号（v1.4）；
   - **400 ms 最短回答间隔**：window 发来的 `interaction/respond`，距请求创建不足 400 ms 时，回 `conflict/too_early`（5.2.4、5.8.3）；
   - **信任门控的钩子**：项目未受信任时拒绝的入口。真正的门控归 T044，本任务只留接口；
   - **SEC-9**：`Sec9` 格子的 remote_device 收窄，包括只能在已有项目里新建会话、只能改模型和推理强度；
   - **`CliOnly`**：`auth/elevate` 对除 `cli` 以外的 principal 一律拒绝。
4. **`policy/describe`**：主进程弹原生确认框之前，用它取得要确认的操作的语义描述，即 `{action, details}`。`action` 的 7 个取值见 5.3.11"T013 实现时的取值"；每种 `action` 带哪些 `details` 键，由本任务定下，不带文案。
5. **拒绝的结果**：语义码。`forbidden` 带 `reason`；`too_early` 等按 5.3.4 的错误码。

**不做**：
- 命令账本（T032）；
- 方法分发与网关的接线（T030 正在做网关；分发由 T032、T033 接）；
- 信任门控本身（T044）；
- 交互请求的状态（T042）。

## 验收标准

- [ ] 新增 UNIT-tier-rules：放宽档位、`mode_after`、手机不能放宽，每条都有允许和拒绝两个方向的用例。
- [ ] 新增 UNIT-policy-matrix：按 5.8.3 逐格断言。
  - 方法级与资源级合起来的最终结论，要与 `MATRIX` 一致：`MATRIX` 在 `cupper-protocol` 的测试模块里；如果要复用它，可以把表挪到能共享的位置，或者在宿主这边另写一张对照表并注明出处。
  - `Sec9`、`Input`、`CliOnly`、`Native`、`ByMain` 的格子都要覆盖。
  - `digit` 与序号的核对：特权选项和效果未知的选项也占序号。
- [ ] 400 ms 用 `ManualClock` 或传入的时间验证，不睡眠。
- [ ] 测试要覆盖拒绝的路径，不能只测允许的路径。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T031-policy/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **SEC-9 与输入证据**（T018）：
  - 要实现 `MATRIX` 里每个 `Sec9` 格子（scope 允许、按 SEC-9 拒绝，见 5.3.11 末尾"T018 实现时的取值"）；
  - `InputEvidence` 的 `key` 与 `digit` 要互相一致；
  - `digit` 要与所选选项的序号相符，特权选项和效果未知的选项也占序号。
- **T013 新增的策略点**：
  - git 写操作和 `settings/update` 的 `Sec9` 格子；
  - `auth/elevate` 的 `CliOnly`；
  - `git/revert` 的 `input` 证据（`Input` 格子）；
  - `settings/update` 修改安全相关的键时要 privileged；
  - `policy/describe` 每种 `action` 的 `details` 键。
- **惯例**：
  - 时间只经 `Clock` 获取，或者作为参数传入纯函数；
  - 错误类型用 thiserror；
  - `lib.rs` 只放 `mod` 声明；
  - 集成测试用到 `ManualClock` 的，声明 `required-features = ["test-harness"]`。

## 并行注意

T030 正在 `cupper-host` 里做 `gateway` 模块。本任务只建 `policy/`，不要改 `gateway/`；`lib.rs` 只加一行 `mod`。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按规则写行数估计，测试单独列一项**（逐格的矩阵测试会比较长）；动手后每完成一个模块统计一次（守则第 8 条）。预计会明显超出约 1,500 行时，先停下来汇报。

## 参考资料

- 设计：
  - `02-host.md` 5.2.4（400 ms）、5.2.5（策略与命令账本）、5.2.6（档位、信任）、5.2.11（设置键）；
  - `08-security.md` 5.8.2、5.8.3（权限矩阵及其下方的实现说明）；
  - `07-desktop.md` 5.7.8（RP 规则、输入证据）；
  - `03-protocol-v1.md` 5.3.11 末尾 T012、T013、T018 的取值。
- PRD 的 SEC-9；ADR-03（约 500 ms 的输入保护）。
- `crates/cupper-protocol/src/methods/catalog.rs` 的 `MATRIX` 和各格子的文档。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。
