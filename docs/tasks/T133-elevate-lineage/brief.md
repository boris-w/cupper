# T133 CLI 提权 II：血缘、核实信息与 IT-elevate

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T055、T008、T116 的执行者接手（T008 做过假 agent 的 setsid 孙进程和按进程组回收）
- 分支：`feat/T133-elevate-lineage`
- 依赖：T038（并行进行中，见"工作方式"）。T046 依赖本任务

## 背景

T038 开工前拆成两份（计划 v1.3.24，见 `docs/tasks/T038-cli-elevate/brief.md` 末尾的补充裁定）：
- T038 做提权的状态与授权：纯核心、`IndexActor` 接线、网关共享的已授 scope。
- 血缘和核实信息经一个窄接口取。T038 给的生产实现 fail closed：一律判为"无法排除是适配器的后代"，返回 `forbidden`（`lineage`）。

本任务做这个接口背后的真实实现，并写进程级的 IT-elevate。

## 范围

**做**（计划 T133 一行；设计 5.8.4、5.8.5、5.8.1）：
1. **`cupper-sys` 补充**（`unsafe` 只在这里）：
   - 取进程的控制终端；
   - 取可执行文件的签名身份（identifier 和 Team ID，未签名时说明）；
   - `setsid` 包装，给 IT 里要自成会话的探针进程用。
2. **血缘三信号**：对端与某个适配器同一个会话、同一个进程组，或者祖先链里有适配器。
   - 适配器取 `adapters` 表里未结束的行（pid、pgid、`start_us`）；用 `start_us` 防 pid 复用；适配器重启后的新实例自然是新的一行。
   - 信号的期望按 T003 的结论（设计 5.8.4 v1.6）：Claude 的工具命令在 CLI 新建的会话和进程组里，只有祖先链会命中；CLI 本身拉起的 MCP、hooks 留在适配器的进程组里，前两个信号能命中。经 nohup、双重 fork 或 disown 脱离的命令三个信号都失效，这是已知局限。
   - 写成纯函数（输入是造出来的进程信息），UNIT-lineage-signals 覆盖每个信号、pid 复用和祖先链断在 launchd 的情况。
3. **核实信息**（`auth/elevation_get`）：可执行文件路径、签名身份或"未签名"、pid、父进程链；父进程链断在 launchd、或没有控制终端时标"无法确认来源"；"是否本 App 自带的命令行工具"（路径与 cdhash 都相符）。协议的 `VerifiedRequester` 加这个字段（例如 `bundled_cli: bool`），重新生成 TS 类型和 schema。
4. **接上 T038 的窄接口**，替换 fail closed 的占位实现。
5. **IT-elevate 的工具**：
   - 一个只在测试构建里存在的提权探针：用 `cupper-cli` 的客户端库以 CLI 身份连上宿主，发 `auth/elevate`，把结果写进文件；
   - 假 agent 扩展：能拉起这个探针，传入数据目录、结果文件、是否 `setsid` 等参数（现有的 `spawn_group_child` 只传固定参数）。

**不做**：提权状态与授权（T038）；原生确认框（T077）；`responsibility_*` 私有 SPI（SP-10）。

## 验收标准

- [ ] **IT-elevate**（进程级，真实宿主加假 agent），覆盖：
  - 适配器的后代被拒：
    - 经祖先链识别的工具命令：假 agent 拉起探针，探针自己 `setsid`；
    - 与适配器同一进程组的 MCP 或 hooks：假 agent 拉起探针，不 `setsid`；
  - 非后代的探针进入未决，`elevation_get` 给出核实信息（含"是否自带 CLI"），`grant` 后这条连接能调用 write 类方法；
  - 限频（全局一个未决、被拒 2 次后静默）、2 分钟过期、断开即失效：经 `test/clock_advance` 推进时钟，不靠缩短的真实时长等待。
- [ ] UNIT-lineage-signals（纯函数）。
- [ ] 用例只向测试自己拉起的进程取信息、发信号；结束后确认没有残留进程。新增的 IT 连跑 10 次全绿（计划的基本验收）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T133 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T133-elevate-lineage/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T038 的行也适用于本任务；开工前逐行读一遍）

- T020：`parent_chain` 能经过 root 进程走到 launchd；进程名是内核里的 16 字节短名，要完整路径需另取。
- T038：窄接口的形状见 T038 分支的 `notes.md`。

## 注意

- 不运行真实的 Claude Code、Codex CLI；用例一律用假 agent。
- 探针只在测试构建里存在，不能进入发布产物（照 test-harness 特性的做法）。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；错误用 thiserror。

## 规模

计划估为 M，T038 的执行者估约 700 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 800 行（包括贴线）时，必须先停下来汇报**；硬停止线 900。

## 工作方式

- 在原来的 T043 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T133-elevate-lineage main`。T116 的分支由编排者合并，不要再往它上面提交。
- **T038 还在进行中**。先做不依赖它的部分：`cupper-sys`、血缘纯函数和 UNIT-lineage-signals、核实信息的采集、协议字段、探针和假 agent 扩展。T038 的窄接口定下之后，可以在 T038 的 worktree（`.claude/worktrees/T054`）里只读查看它分支上的 `notes.md` 和代码。不要合并它的分支。
- T038 合并进 main 之后，由编排者把 main 合进你的分支并通知你，你再接上接口、写 IT-elevate。在那之前不要执行 merge 或 rebase。
- 不依赖 T038 的部分做完了而 T038 还没合并时，停下来汇报进度，等通知。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
