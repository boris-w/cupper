# T023 宿主核心骨架 II：启动契约、注入点与故障点

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主核心）
- 分支：`feat/T023-host-core-startup`
- 依赖：T022（已合并）。M1a 第 2 波；T025、T037、T047 等任务依赖它。

## 背景

宿主进程的启动契约属于冻结清单第 1 组第 9 条（设计 5.12.5），必须在 M1a 定下来。它包括四部分：
- 命令行参数；
- 经 stdin 传入的启动载荷（`dev_key`、`harness_key`、`test_config`）；
- 退出码；
- `--build-info`。

另外，测试要用到的注入点和故障点，在 test-harness 构建里需要一个统一的结构（设计 5.9.9）。本任务只建结构，各个注入点由各自的使用方任务接上。

## 范围

**做**（计划 T023 一行）：
1. **命令行**：`--data-dir`、`--build-info`。`--build-info` 输出启用了哪些编译特性等构建信息；发布流水线和 T099 要靠它断言没有开启 `dev-auth`、`test-harness`。
2. **stdin 启动载荷**（5.8.1）：
   - 读一行 JSON，`{dev_key?, harness_key?, test_config?}`，2 秒超时；
   - 读完之后，把 stdin 重定向到 `/dev/null`；
   - 只有开启 `dev-auth` 的构建才读 `dev_key`，只有开启 `test-harness` 的构建才读 `harness_key` 和 `test_config`，用编译期门控；
   - 发布构建不读 stdin；
   - 格式错误或超时时，以退出码 1 退出；
   - `test_config` 只能经这个载荷传入，没有对应的命令行开关（计划 P-9）。
3. **`test_config` 的结构**：包含 5.9.9 注入点表里的全部字段，例如 `durations`、`clock`、`boot_session_uuid`、agent 命令覆盖、registry 地址等。本任务只定义结构和解析，使用方以后接入。
4. **故障点框架**：在 test-harness 构建里，执行到某个具名故障点就让进程退出。具体做法类似 `fail` crate；可以直接用它，也可以自己写一个，写明理由即可。由测试指定哪些故障点生效；故障点的名字统一登记。
5. **宿主 ID**：`h_` 加 UUIDv4（5.4.4）。实现时自己取随机源（见下面"交接事项"）。也可以留给 T025 的 `meta` 表，在汇报里写明你的选择。
6. **退出码**：按 5.2.13 定义成常量。

**不做**：
- 注入点的使用方，比如时钟、agent 命令、registry 地址的实际接入；
- `test/*` 方法（T047）；
- 存储（T024、T025）。

## 验收标准

- [ ] 新增 UNIT-startup-payload，覆盖：
  - 格式正确；
  - 格式错误时退出码为 1；
  - 2 秒超时（用 `Clock` 或注入的时长来测，不真等 2 秒）；
  - 读完后 stdin 指向 `/dev/null`；
  - 不同特性组合下忽略对应的字段。
- [ ] 新增 UNIT-failpoint：故障点生效时进程按约定退出，不生效时完全没有副作用。
- [ ] UNIT-release-features（5.9.1"无特性运行"）：在不带任何特性的构建里：
  - `--build-info` 不含 `dev-auth`、`test-harness`；
  - 不读 `test_config`，也不读 `harness_key`；
  - 没有故障点。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T023-host-core-startup/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头（计划 1.6）。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- 宿主 ID 是 `h_` 加 UUIDv4，不归 `IdGen` 管；`cupper-host` 的 `uuid` 依赖没有开 `v4` 特性，需要自己取随机源（T022）。
- `tests/` 下的集成测试只要用到 `ManualClock`、`CoreDriver`、`IdGen::seeded`，就要声明 `required-features = ["test-harness"]`（T022）。

## 注意

- 遵守 `CLAUDE.md` 的约束：时间只经 `Clock` 获取，`unsafe` 只能出现在 `cupper-sys` 里。T020 正在并行开发 `cupper-sys`，本任务需要的 stdin 重定向这类系统调用，优先用标准库实现。实在需要 `cupper-sys`，就在汇报里说明，等 T020 合并后再接。
- 规模上限约 1,500 行。明显会超出时，先停下来汇报。

## 参考资料

- 设计：`02-host.md` 5.2.13、5.2.16；`08-security.md` 5.8.1（启动载荷）；`09-testing.md` 5.9.1、5.9.8、5.9.9；`04-data-model.md` 5.4.4；`12-milestones.md` 5.12.5。
- 计划：`docs/03-plan.md` 1.6，T023 一行，P-9。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为 `report.md`。不要自己创建 `report.md`。
