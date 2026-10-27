# T199 时序依赖的用例与首次执行的扫描

- 派发日期：2026-10-27
- 执行者：子 agent（Rust 工程师，宿主核心）。T037、T117、T196 的执行者接着做
- 分支：`fix/T199-timing-tests`
- 依赖：无。T099 依赖本任务

## 背景

T099 要求同一提交上连续 20 次全绿，不允许重试。10-26 陆续冒出几处与各自任务无关、在负载下才失败的用例，计划 v1.3.67 起归到本任务。

## 范围

**做**，每一处都照 T196 的做法：先在高负载下复现、量出实际等待，再修，不加重试。
1. **`it_gw_sync_reconnect_replays_without_gaps`**（`tests/gateway_sync.rs:372`，"no range events live"）：断言合并窗口 200 ms 内实时流里出现 range 事件。PR #3 的 rust 作业失败过一次（job 121550735981），main 上同一测试通过。要分清是用例的时序，还是合并窗口在负载下真的变了。
2. **E2E-NFR1-4**（"a window whose connection dropped resumes from its cursor"）：负载约 47 时，等 "the window connects again" 超时（20 s）。和第 1 处同属断线重连。
3. **`it_nfr9_crash_under_load_kill9_again_and_again`**（`tests/crash_concurrency.rs:341`，"no load in round 4"）：宿主重启后，在等待上限内没等到 200 条新 delta（PR #4，run 40322065826）。先看是不是重启后宿主或假 agent 的首次执行扫描。
4. **cargo 重新链接后 `cupper-host` 的第一次执行**：它要等 macOS 扫描新二进制，这一次会落在最先启动宿主的那个用例上，`connect` 只等 30 s。照 T196 的 `first_start`，在测试二进制第一次启动宿主之前先跑一次。
5. **`fake_agent_host_from` 开头的 `connect` 失败（panic）时，已拉起的宿主没人回收**：在 `connect` 之前就把它包进 `Running`。

**不做**：其他偶发失败，发现了记下来在汇报里提出。原因在产品里、单处超过 100 行或要改接口的，停下汇报。

## 验收标准

- [ ] 每一处的复现数据、根因、修法写进本目录的 `notes.md`；第 1–3 处修后在负载下各连跑至少 200 次无失败。第 2 处是 e2e，要开窗口：在本机跑会占用作者的桌面，改为请编排者用 `e2e-repeat` 在 runner 上跑，次数照写。
- [ ] 第 4、5 处有能证明修法生效的核对（例如每次先重新链接再跑）。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 交接事项（`handoff-notes.md` 第一列含 T199 的行）

- T196：负载的造法与 pid 回收；macOS 首次执行新二进制的扫描与 `first_start`；"只放宽等待时要说明为什么是对的"。
- T069：NFR9 崩溃压力用例的写法与 `PATIENCE`。

## 规模

- 停止线：260；硬停止线：300

<!-- 执行者开工前在 `notes.md` 里按处写估计。每写完一处就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T037 worktree 里从最新的 main 开新分支：`git checkout -b fix/T199-timing-tests main`。要在 GitHub 上跑 CI 或 `e2e-repeat` 时照守则第 9 条请编排者推送和触发。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充（2026-10-27，编排者）

用户的规则：本机不制造很高的负载，高负载场景的测试放到 CI 上跑；普通的检查与单份测试照常在本机跑（`CLAUDE.md` 守则第 10 条）。

1. **新增手动 workflow `test-repeat.yml`**（`workflow_dispatch`），给 cargo 与 vitest 的单测在 runner 上做"重复 N 次、可选并行加压"：
   - 参数：种类（cargo 或 vitest）；包或项目；用例过滤；次数；并行份数（加压用，默认 1）；runner（默认 `macos-15`，vitest 可选 ubuntu）。
   - 照 `e2e-repeat.yml` 的写法：权限只读；输入经环境变量传进脚本；action 锁定到 SHA；复用 `.github/actions/setup`；结果作为构件上传。
   - 并行份数大于 1 时，每份都在 runner 上独立循环跑，用来造负载，不跑到作者机器上。
   - 规模算在本任务里。
2. 第 1、3 处修后"负载下连跑 200 次"，改用 `test-repeat` 在 runner 上做，请编排者触发。修前的失败以 CI 上已出现的那几次为证据。
3. T198 也会用这个 workflow 做它的验证。

- 停止线：330；硬停止线：380
