# T094 登录 shell 环境与版本检测

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T004、T005 的执行者接手
- 分支：`feat/T094-login-shell-version`
- 依赖：T020、T022（都已合并）。T055（AgentRuntime 与 Claude 接入）依赖本任务和 T054。与 T054 互不依赖，并行进行

## 背景

T054 开工前估计约 1,620 行，按执行者的建议拆成两半（计划 v1.3.13，见 `docs/tasks/T054-cli-discovery/brief.md` 末尾的补充裁定）：T054 做"文件系统与规则"（可信 PATH、CLI 解析、环境变量规则），本任务做"拉起进程"那一半：从登录 shell 取环境，以及运行 `--version` 做版本检测。

GUI 进程自己的 PATH 里没有 Homebrew、nvm 等目录（SP-2），所以宿主要从登录 shell 取环境（设计 5.6.6，待定问题 Q18）。

## 范围

**做**（设计 5.6.6"登录 shell 环境""版本检查"、5.2.10 末尾的 git 版本检测；T054 的 `notes.md` 里已有的方案草图可以直接参考）：
1. **带 `Clock` 超时的拉起辅助**：经 `cupper_sys::spawn` 拉起一次性的子进程，收集 stdout，超时由 `Clock` 驱动，超时后按进程组结束它。本任务的其余部分都用它。
2. **登录 shell 取环境**：
   - 运行 `$SHELL -l -i -c '<在两个标记之间打印 env>'`，stdin 为 `/dev/null`，超时 5 秒（登记进 `durations.rs`）；用 `env -0` 一类不受换行影响的输出格式，按标记截取（rc 文件可能自己输出东西）；
   - 取出 `PATH`、代理变量、`SSH_AUTH_SOCK`、`CLAUDE_CONFIG_DIR`、`CODEX_HOME`，以及调用方给出的变量名（`agents.env_passthrough`）对应的值，结果是普通的 `BTreeMap<String, String>`；
   - **取到的环境只放在内存里，不写日志，不进诊断包**；
   - 失败（超时、没有标记、shell 不存在）时退回 `/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`，并给出语义码；
   - 已有的 `TestConfig.login_env`（test-harness 的注入点）怎么接，按现有代码核对，在 `notes.md` 里写明。
3. **版本分类**：运行 `<binary> --version`（超时 5 秒，环境变量走白名单），取第一个语义化版本号，与已验证范围比较，得出 `verified`、`newer_untested`、`older_untested`、`unsupported`。已验证范围以版本号字符串的形式由调用方传入（数据在 `runtime/manifest.json`，由 T054 内嵌）；`cli_too_old` 由 T055 判定。
4. **git 与 git-lfs 检测**：暂定最低 2.39。git 的绝对路径由调用方传入（T054 解析）。**是 `/usr/bin/git` 时先防占位程序**（设计 5.2.10，2026-10-20 补充）：先运行 `xcode-select -p`，确认开发者目录存在、且其下的 `usr/bin/git` 存在，才运行 `/usr/bin/git --version`；否则判为 git 缺失并给出"需要安装命令行工具"的语义码，**绝不运行占位程序**（它会弹安装对话框）。

**不做**：
- 可信 PATH、CLI 解析、清单内嵌、环境变量规则（T054）；
- 接进宿主启动和 `agents/refresh`，写进索引流（T055、T034）。

## 验收标准

- [ ] UNIT-version-classify（由 T054 移来）：四种结果；版本号提取覆盖带前后缀的输出，例如 `2.1.296 (Claude Code)`、`codex-cli 0.160.1`、`git version 2.39.5 (Apple Git-154)`。
- [ ] 新增 UNIT-login-shell：用测试自己的假 shell 脚本，覆盖正常、rc 文件自己有输出、超时、输出里没有标记、shell 不存在；超时由 `ManualClock` 驱动，不真的等 5 秒；断言取到的环境没有进入任何日志。
- [ ] git 检测有用例：版本低于 2.39、git-lfs 不存在、`/usr/bin/git` 而开发者目录不存在时不运行它（用可替换的路径和假的 `xcode-select` 验证，不依赖本机是否装了命令行工具）。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T094 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T094-login-shell-version/ci/` 通过，摘要随任务提交。

## 注意

- 代码约束：子进程一律经 `cupper_sys::spawn`；时间和定时只经 `Clock`；`cupper-host` 不用 `unsafe`；错误用 thiserror。
- **不运行真实的 Claude Code、Codex CLI（包括 `--version`），不登录**；用例里的 CLI 和 shell 都是测试自己写的假脚本，不运行用户真实的 shell 配置。版本输出的格式以 SP-1 录制头里记下的为准。
- 不读、不改 `~/.claude`、`~/.codex`。
- 用例和 notes 里不出现本机的用户名和路径。
- 新的模块放在 `cupper-host` 的 `driver` 下，与 T054 的 `CliLocator` 相邻但不互相 import；文件名先和 T054 的执行者错开（看 main 上 T054 的 `notes.md` 里的草图），避免合并冲突。

## 规模

计划估为 M，T054 的执行者估这一半约 575 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 700 行时，开工前就停下来汇报**；硬停止线 800。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.6；`02-host.md` 5.2.10（git 版本检测）；`13-open-issues.md` Q18。
- ADR-08；SP-2 的报告（GUI 进程的 PATH）。
- `docs/tasks/T054-cli-discovery/`（`brief.md` 的补充裁定、`notes.md` 的方案草图）。
- `crates/cupper-sys/src/env.rs`、`spawn`；`crates/cupper-host/src/clock/`、`durations.rs`。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T094-login-shell-version main`。T005 的分支由编排者合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
