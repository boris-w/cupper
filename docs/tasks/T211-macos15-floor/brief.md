# T211 最低支持版本提到 macOS 15

- 派发日期：2026-11-02
- 执行者：子 agent（工程师）
- 分支：`ci/T211-macos15-floor`
- 依赖：无。T099 依赖本任务：退出候选上的 nightly 要能通过

## 背景

GitHub 在 2026-11-02 停用了 macOS 14 的托管 runner。nightly 里对端校验作业 `security-macos14` 在 `macos-14` 上的那一份，从 10-30 起就因停用前的限流被取消，以后也跑不了。用户决定只支持最近两三年的 macOS，最低支持版本从 14 提到 15（`docs/meetings/2026-11-02-macos-floor/`）。编排者已经改了设计、PRD 的界面规格、计划、`CLAUDE.md` 和 `tests/ids.toml`：IT-sys-peer-macos14 标为不适用，新增 IT-sys-peer-macos26。本任务改代码、workflow 和构建配置。

## 范围

**做**：
1. **先核实 GitHub 的 `macos-26` runner 可用**：在分支上跑一次，看它的系统版本。不可用的话，先停下告诉 `main`。
2. **作业改名**：`security-macos14` 改为 `security-peer`。涉及 xtask 的作业表、`ci/summary.rs`、`.github/workflows/nightly.yml`、`.github/workflows/cupper-sys.yml`，以及作业的文档注释。
   - nightly：在 `macos-15`、`macos-26` 上各跑一次；
   - `cupper-sys.yml`：改动 `cupper-sys` 的 PR 在 `macos-26` 上跑（macOS 15 由每个 PR 的 `rust` 作业覆盖，设计 5.1.6）；
   - 摘要按 runner 的 macOS 大版本记作 IT-sys-peer-macos15、-macos26；其他版本（例如本机）照旧只记 IT-sys-peer-current。现有的单元测试跟着改。
3. **构建产物的最低系统版本**：`crates/xtask/src/host_bundle.rs` 的 `LSMinimumSystemVersion`（连同它的测试），`apps/desktop/electron-builder.*.yml` 的 `minimumSystemVersion`，改成 `15.0`；仓库里其他写着 14.0 的最低版本也一并找出来。
4. **README**：最低系统要求改成 macOS 15。
5. 在分支上手动触发一次 nightly，确认 `security-peer` 在两个 runner 上都通过、摘要记到了正确的编号。要触发时告诉 `main`。

**不做**：改设计、计划、`CLAUDE.md`、`tests/ids.toml`（编排者已改）；代码里为了 macOS 14 写的兼容分支，如果发现了，列进汇报，不在本任务里删。

## 验收标准

- [ ] `macos-26` runner 可用的证据（运行链接和系统版本）。
- [ ] 分支上的 nightly 里，`security-peer` 在 `macos-15`、`macos-26` 上都通过，摘要记作 IT-sys-peer-macos15、-macos26。
- [ ] 构建产物的最低系统版本都是 15.0。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：120；硬停止线：160

## 参考资料

- 会议记录：`docs/meetings/2026-11-02-macos-floor/minutes.md`。
- 设计：5.1.6（作业表、runner 标签）；02-design 的平台约束；5.13 的 Q24。
- 卷宗：T197（GitHub Actions、nightly、按 runner 版本记编号）；T021（对端校验）。

## 注意

- workflow 只改 runner 标签和作业名，权限仍是只读，action 照旧锁 SHA。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
