# T036 设置

- 派发日期：2026-10-26
- 执行者：子 agent（Rust 工程师，宿主安全）。T054、T060、T098、T059、T129、T089、T169、T175、T177、T061、T186 的执行者接着做
- 分支：`feat/T036-settings`
- 依赖：T033（已合并）。T099 依赖本任务

## 背景

设置的协议方法（`settings/get`、`settings/update`）在契约里早已定义，网关也已按规则检查 `settings_update`（T033），但宿主还没有实现：
- 调用回 `unsupported`，桌面的特权操作 `settings` 也返回 `failed`；
- 几处依赖设置的功能拿到的都是空值，例如 `agents.env_passthrough`。

本任务把设置落地，并补上这几处接线。

## 范围

**做**（计划 T036 一行；设计 5.2.11 的设置）：
1. **`settings` 表与读取接口**：默认值、读取接口（宿主内部各模块从这里取设置）。表结构只改 `0001`（计划 P-4），设计 5.4.2 同步加行。
2. **`settings/get`、`settings/update`**：
   - 安全相关的键要求 `privileged`；
   - 登记进 `Routes`（`add::<M>`），并从 UNIT-unsupported-set 的允许表里删掉相应的行（T033 的交接）；
   - 变更后经索引流通知界面。通知方式照设计。
3. **接线**：
   - `agents.env_passthrough` 接进 `adapter_env` 的放行名单（T055 的交接）；
   - 桌面的特权操作 `settings`：照 `set_config` 的结构补上 `policy/describe`（`security_setting`）→ 原生确认框 → `settings/update`（`apps/desktop/src/main/privileged/actions.ts`，T134 的交接）；
   - 安全相关的 `settings/update` 已由 T163 记 `privileged.executed`，确认接上后照常记录。
4. **`AgentsUpdated.git`**：把 T094 的 git 版本检测结果接进来，让设置页能显示 git 与 git-lfs 的版本（T100 的交接）。范围小的话做；放不下就在汇报里提出。

**不做**：界面偏好（属于桌面端）；设置页的界面（M1b）；`apps.terminal`、`apps.editor` 的内置应用清单，设计里还没有（T013 的交接），汇报里提一句即可。

## 验收标准

- [ ] UNIT-settings：默认值；读取；`settings/update` 的校验与持久化；安全相关的键没有 `privileged` 时被拒；变更后的通知。
- [ ] UNIT-unsupported-set 里删掉了 `settings/*` 的行，用例照常通过。
- [ ] 桌面的特权操作 `settings` 经原生确认框走通，有用例。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T036 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T036-settings/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T036 的行）

- T033：登记 `settings/*`，从 UNIT-unsupported-set 删行。
- T055：`agents.env_passthrough` 还没接。
- T134：桌面特权操作 `settings` 的接法。
- T100、T094：`AgentsUpdated.git` 为空，接 git 版本检测。
- T013：内置应用清单没有设计。
- T016 正在冻结契约：设置的类型如有改动，以 T016 合并后的契约为准。`Settings.cli_path` 必填、`SettingsPatch` 的非空键不接受 `null` 这两点由 T016 处理，本任务不要自己改契约，有冲突时停下汇报。

## 注意

- 代码约束：每条事件流只有一个写者；时间只经 `Clock`；纯核心里不做 I/O。
- 设置里不存秘密。

## 规模

- 停止线：400；硬停止线：460

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计；每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T054 worktree 里从最新的 main 开新分支：`git checkout -b feat/T036-settings main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
