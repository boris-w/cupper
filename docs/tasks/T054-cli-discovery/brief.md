# T054 CLI 发现、登录 shell 环境与可信 PATH

- 派发日期：2026-10-20
- 执行者：子 agent（Rust 工程师，宿主驱动层）
- 分支：`feat/T054-cli-discovery`
- 依赖：T020、T022（都已合并）。T055（AgentRuntime 与 Claude 接入）依赖本任务。**本任务离关键路径只有 1 个单位的松弛**（T052 → T053 → T055 那条），请优先保证进度

## 背景

宿主要找到用户机器上的 `claude`、`git`、`git-lfs`，并决定拉起适配器时传哪些环境变量（设计 5.6.6、5.2.10"程序的路径"、ADR-08、安全 #9）。

GUI 进程自己的 PATH 里没有 Homebrew、nvm 等目录（SP-2），所以要从登录 shell 取环境。取到的 PATH 不能全信：agent 可以在工作区里放一个同名程序，所以只在可信条目里找。

## 范围

**做**（计划 T054 一行；设计 5.6.6、5.2.10 第 82–87 行附近的"程序的路径""环境"、ADR-08）。整体是设计里的 `CliLocator`，放在 `cupper-host` 的 `driver` 下：
1. **登录 shell 取环境**：
   - 运行 `$SHELL -l -i -c '<在两个标记之间打印 env>'`，stdin 为 `/dev/null`，超时 5 秒；
   - 从输出里取 `PATH`、代理变量、`SSH_AUTH_SOCK`、`CLAUDE_CONFIG_DIR`、`CODEX_HOME`，以及 `agents.env_passthrough` 列出的变量名对应的值；
   - **取到的环境只放在内存里，不写日志，不进诊断包**；
   - 失败时退回 `/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin`，并给出可供 agent 状态展示的语义码。
2. **可信 PATH 规则**：条目必须是绝对路径，不在任何项目、工作区、worktree 根目录下，也不在临时目录（`$TMPDIR`、`/tmp`、`/private/var/folders`）下。比较之前先 `realpath`。项目和工作区根目录由调用方传入。
3. **`claude`、`git`、`git-lfs` 的绝对路径**：按 5.6.6 的查找顺序。设置里的 `cli_path` 在前；登录 shell 的可信 PATH 其次；常见位置最后（Homebrew、原生安装器 `~/.local/bin`、npm 全局目录、nvm 当前版本的 bin 目录）。git 和 git-lfs 只看可信 PATH。
4. **数据驱动的 CLI 解析器**（Q19 的 Claude 部分）：
   - 先 `realpath`。是 Mach-O 就直接用；
   - 是 `#!/usr/bin/env node` 的 npm shim，按包布局找到包内的平台二进制。不能直接把 shim 交给适配器，GUI 环境里没有 node；
   - 只有 JS 入口、没有原生二进制的，判为 `unsupported_install`；
   - 各种安装方式的目录布局做成数据（表），用目录树样例做单元测试。
5. **版本分类**：
   - 运行 `<binary> --version`，超时 5 秒，环境变量走白名单，取第一个语义化版本号；
   - 与"已验证范围"比较，得出 `verified`、`newer_untested`、`older_untested`、`unsupported`。`cli_too_old` 要等适配器报"需要认证"才能判定，由 T055 处理。
   - **已验证范围放在哪里（编排者裁定）**：放进 `runtime/manifest.json`，字段名由你定，例如 `cli.claude.verified`、`cli.claude.unsupported_below`。起始值取 ADR-08（claude 2.1.200 到 2.1.296；codex 0.150.1 到 0.163.0，codex 低于 0.140 判为 unsupported）。它和 `env.allow`、`env.deny` 一样是手工维护的数据：`cargo xtask runtime-pack` 写回清单时要原样保留它，`--verify` 照常通过。以后由兼容报告生成（T064、T066）。
6. **清单在编译期内嵌**：宿主运行期间不读 App 包里的文件（代码约束），所以用 `include_str!` 等方式把 `runtime/manifest.json` 编进宿主，解析出本任务要用的字段（已验证范围、`env.allow`、`env.deny`）。做成一个小模块，T056 的安装器以后在它上面加字段。
7. **配置目录校验**：`CLAUDE_CONFIG_DIR`、`CODEX_HOME` 只从登录 shell 读取。必须是绝对路径、目录存在、属主是当前用户，并且 realpath 不在任何项目或工作区之内。不通过就不设置，并给出语义码。test-harness 构建可以经 `--test-config` 覆盖，只留接口，用法由测试决定。
8. **适配器环境变量规则**（UNIT-env-rules），按 5.6.6 的表：
   - 固定白名单；
   - 宿主自己设置的 `CLAUDE_CODE_EXECUTABLE`、`CODEX_PATH`；
   - 校验后设置的配置目录；
   - 用户放行：名字匹配 `^[A-Z][A-Z0-9_]*$`，不在禁用清单里。命中禁用清单的忽略，并给出提示码；固定白名单里的变量也不能经放行覆盖；
   - **禁用清单只取内嵌清单的 `env.deny`**，代码里不另抄一份（5.6.6，T004 之后的裁定）；
   - 去掉所有 `CUPPER_*`。
9. **git 版本检测**：暂定最低 2.39，同时检测 git-lfs 是否存在、版本多少。

**不做**：
- 接进宿主启动和 `agents/refresh`，写进索引流的 `agents` 状态（T055 拉起适配器时接线，T034 做索引）；
- Codex 的安装布局（T111）；
- 运行时安装器（T056）。

本任务交付的是可以单独测试的库代码：输入是登录 shell 的输出、项目根目录、设置、目录树，输出是解析结果和语义码。

## 验收标准

- [ ] UNIT-cli-resolver-claude：用目录树样例覆盖 Mach-O、npm shim、只有 JS 入口、符号链接链、找不到；样例里不出现真实的用户名和路径。
- [ ] UNIT-version-classify：四种结果和版本号提取（含带前后缀的输出）。
- [ ] UNIT-realpath：可信 PATH 的判定，覆盖项目内、工作区内、worktree 根目录、临时目录、相对路径，以及经符号链接逃逸的情况。
- [ ] 新增 UNIT-env-rules：白名单、宿主设置、配置目录校验、放行、禁用清单（前缀和名字）、`CUPPER_*`、白名单不可被覆盖。禁用清单的用例要从内嵌清单取值，证明没有另抄一份。
- [ ] 登录 shell 取环境有用例：用一个测试用的假 shell 脚本，覆盖正常、超时、输出里没有标记三种情况；超时由 `Clock` 驱动，不真的睡 5 秒。
- [ ] `cargo xtask runtime-pack --verify` 在加了已验证范围之后仍然通过。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T054 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T054-cli-discovery/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T020：环境直接用 `cupper_sys` 的 `Env::from_allowed` 加 `set`。`CUPPER_*` 已在 `cupper-sys` 里拒绝，这里不用再处理，但规则表里仍要体现。
- T004：禁用清单以 `runtime/manifest.json` 的 `env.deny` 为唯一来源。

## 注意

- **拉起子进程一律经 `cupper_sys::spawn`**；时间和定时只经 `Clock`（代码约束）。
- **核实真实的安装布局**（Q19 写明"实现时逐个核实"）：可以只读地列出本机 Claude 安装所在的目录结构。**不运行真实的 CLI（包括 `--version`），不登录**；`--version` 的输出格式以 SP-1 录制头里记下的为准（例如 `2.1.296 (Claude Code)`）。看到的布局写进 `notes.md` 时，把用户名、主目录换成占位符。样例目录树里同样不能出现真实路径。
- 不读、不改 `~/.claude`、`~/.codex` 里的任何文件。
- 用例里的登录 shell 用测试自己的假脚本，不运行用户真实的 shell 配置。
- 依赖锁精确版本，许可证在 ADR-11 的 allowlist 内。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项**；计划 1.1 的执行经验是实现按草估的 1.7 倍计。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 1,200 行时，开工前就停下来汇报，并给出拆分建议**（例如把"登录 shell、可信 PATH、CLI 解析与版本"和"清单内嵌与环境变量规则"分开）。

## 参考资料

- 设计：`06-agent-driver.md` 5.6.6；`02-host.md` 5.2.10（"程序的路径""环境"、git 版本检测）；`10-release.md` 5.10.3；`13-open-issues.md` Q19。
- ADR-05、ADR-08；安全 #9；纪要 D14。
- SP-2（适配器分发）的报告：GUI 进程的 PATH、各安装方式的布局。
- `crates/cupper-sys/src/env.rs`、`spawn`；`docs/tasks/T004-runtime-pack/`（清单的结构、`env` 的分类）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径和用户名。其他物料由执行者在任务分支上提交。
