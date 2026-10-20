# T046 cupper-cli II：命令行

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师）。T043、T055、T008、T116、T133 的执行者接手（T133 里用 `cupper-cli` 的客户端库写过提权探针）
- 分支：`feat/T046-cli-commands`
- 依赖：T038、T133、T044、T049、T100（都已合并）。T090（e2e 基础设施）依赖本任务；本任务离剩余关键路径有 2 个单位的松弛

## 背景

`cupper-cli` 的库部分已经有了：`Client`、`Mirror`、构建种类、宿主签名校验、`HarnessKey`（T045、T039、T049）。宿主那边，提权（T038、T133）、索引的状态与在场（T100）、会话命令（T043、T044）都已合并。

本任务把设计 5.2.14 的命令行做出来，NFR-1 的验收（T091）和 e2e（T090）都要用它。

## 范围

**做**（计划 T046 一行；设计 5.2.14）：
1. **命令**：`status`、`sessions [--watch]`、`watch <session>`、`history <session>`、`new --agent <a> --project <path> <message>`、`send <session> <text> [--wait]`、`interrupt <session>`、`pending [<session>]`、`approve`/`reject <session> <request> [--option <id>]`、`answer <session> <request> --field k=v ...`、`elevate --scopes write,approve`、`repl`。协议方法和需要的权限按 5.2.14 的表。
2. **输出**：默认是人读的格式；`--json` 输出 JSON Lines，供测试断言。人读格式里，"宿主未经签名校验"的标记用 `HostCheck::is_weak(&auth)`（T039 的交接）。
3. **`--harness-key-fd <n>`**：只在测试构建里有（编译期门控），参数解析接 `cupper_cli::harness::HarnessKey::read_fd`。`--build-info` 照旧。
4. **`elevate` 与 `repl`**：
   - 写操作要先 `elevate`，授权只在这条连接上有效，所以连续的写操作用 `repl`；
   - `elevate` 的各种回复（`granted`、`granted: []`、`timeout`、`rate_limited` 带 `retry_after_ms`、`forbidden`/`lineage`）都要有清楚的输出和退出码。
5. **在场**：交互式的 `watch`、`repl` 可以经 `Client::notify::<ClientPresence>` 上报，但索引不采信 CLI 的在场（T100 的交接）。是否上报由你定，写进 `notes.md`。
6. **退出码与错误**：协议错误按语义码输出；宿主已退出时，`Refused(PeerExited)` 和 `Eof` 当作同一种情况报告（T049 的交接）。

**不做**：`cupper diff`（T120）；`--steer`（T140）；`--worktree --base`（M1b 的 worktree 任务）。

## 验收标准

- [ ] **IT-cli-commands**（真实宿主加假 agent）：逐个命令断言 `--json` 输出，包括：
  - 写操作没有 `elevate` 时被拒；
  - `elevate` 由用例经 control 连接发 `auth/elevation_respond` 批准后（照 IT-elevate 的做法），`repl` 里的写操作成功；
  - `approve`/`reject`/`answer` 对假 agent 的提问生效；
  - `watch` 能看到回合的事件。
  - 时钟只经 `test/clock_advance` 推进，不睡眠。
- [ ] 新增的 IT 连跑 10 次全绿（计划的基本验收）；跑完没有残留进程。
- [ ] 测试名以用例编号开头；编号在 `tests/ids.toml` 的 T046 段里（需要单元测试编号时补进该段）。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T046-cli-commands/ci/` 通过，摘要随任务提交。

## 交接事项（`handoff-notes.md` 第一列含 T046 的行；开工前逐行读一遍）

- T049：客户端读循环每次读到数据都先查对端令牌；宿主写完就关闭连接时，关闭原因是 `Refused(PeerExited)`，与 `Eof` 当作同一种情况报告。
- T039：`--harness-key-fd` 接 `HarnessKey::read_fd`；"宿主未经签名校验"用 `HostCheck::is_weak(&auth)`。
- T100：`Client::notify::<ClientPresence>`；索引不采信 CLI 的在场。
- T038、T133：`auth/elevate` 的回复；IT-elevate 的探针做法可以参考（`cupper-elevate-probe`、`host.db` 里 `elevation.*` 的轮询）。

## 注意

- 不运行真实的 Claude Code、Codex CLI；用例一律用假 agent。
- 用例里批准提权照 IT-elevate 经 control 连接发 `auth/elevation_respond`；不能在生产代码里留任何绕过原生确认的路径。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；错误用 thiserror。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项；推算超过约 900 行（包括贴线）就开工前停下汇报，并给出拆分建议。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.14；`08-security.md` 5.8.1、5.8.4；`09-testing.md` 5.9.4（NFR-1 的用法）、5.9.9。
- `docs/tasks/` 下 T045、T049、T039、T038、T133 的 `notes.md` 和 `review.md`。
- `crates/cupper-cli/src/`。

## 工作方式

在原来的 T043 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T046-cli-commands main`。T133 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,435 行（实现约 1,035、测试约 400），在写代码之前停下，建议拆成"骨架与读命令""写命令、提权与 repl"两份。

**裁定：不拆，放宽上限（计划 v1.3.30）。**
- 理由：T090 一开始就要用 CLI 提权和写操作，两份都得排在它前面。拆成串行的两份，会让本任务从松弛 2 变成零松弛；整体做，改为 L 后还剩约 1 个单位的松弛，也省一轮派发与合并。
- 规模改为 L，上限约 1,450 行，硬停止线 1,550；推算贴线也先停下。
- 建议的提交顺序：先骨架与读命令，再写命令、`elevate`、`repl`。这样万一中途贴线，可以在模块边界停下。

**三条确认**：
1. 手写参数解析、不加依赖：认可。解析写成纯函数，单元测试补一个编号（例如 UNIT-cli-args），补进 `tests/ids.toml` 的 T046 段。
2. `repl` 的切词只支持空白分隔和双引号：认可，写进 `cupper repl` 的帮助文本。
3. 不上报在场：认可。
