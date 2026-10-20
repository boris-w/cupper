# T054 工作笔记

- 执行者：子 agent（Rust 工程师，宿主驱动层）
- 日期：2026-10-20

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock` 和生成物。"草估"是逐模块的估计；按 brief 引用的计划 1.1 的经验，实现按草估的 1.7 倍计，测试按 1.1 倍计（T004 的测试实际只比估计多约 15%）。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `manifest.rs`（宿主） | `include_str!` 内嵌 `runtime/manifest.json`，首次使用时解析出 `env.allow`、`env.deny` 和已验证范围；T056 以后在它上面加字段 | 45 |
| xtask 与清单数据 | xtask 的 `Manifest` 有 `deny_unknown_fields`，加 `cli` 要改它的结构体；`runtime/manifest.json` 加 `cli.claude`、`cli.codex` 两段（手工维护，`runtime-pack` 原样保留） | 25 |
| `driver/locate/trust.rs` | 可信 PATH：绝对路径、realpath 后不在项目、工作区、worktree 根目录和临时目录（`$TMPDIR`、`/tmp`、`/private/var/folders`）之下 | 45 |
| `driver/locate/resolve.rs` | 数据驱动的解析器：安装布局表；Mach-O 判定（魔数加可执行位）；按包内相对位置认出 npm 的入口，再按表找平台二进制；只有 JS 入口的判为 `unsupported_install` | 80 |
| `driver/locate/mod.rs` | `CliLocator`：设置里的 `cli_path` → 可信 PATH → 常见位置（Homebrew、`~/.local/bin`、npm 全局目录、nvm 当前版本）；`git`、`git-lfs` 只看可信 PATH；最终二进制的 realpath 也过一遍可信判定 | 85 |
| `driver/locate/run.rs` | 经 `cupper_sys::spawn` 拉起、读 stdout（有上限）、用 `Clock` 定时、超时后按进程组（含后代）发 SIGKILL 并回收 | 55 |
| `driver/locate/login.rs` | 登录 shell：`$SHELL -l -i -c`，带随机标记的脚本、`env -0` 输出的解析、只留需要的变量、`Debug` 只打印变量名；失败退回固定 PATH 并给出语义码 | 90 |
| `driver/locate/version.rs` | 取第一个语义化版本号、比较、四种分类；`--version` 走白名单环境和 5 秒超时；git 最低 2.39、git-lfs 是否存在及版本 | 75 |
| `driver/locate/env_rules.rs` | 配置目录校验（绝对路径、存在、属主、realpath 不在项目或工作区内；test-harness 的覆盖接口）；适配器环境变量规则（白名单、宿主设置、配置目录、放行与禁用清单、提示码） | 85 |
| **实现小计** | | **585**，乘 1.7 约 **1,000** |

**测试**

| 用例编号 | 内容 | 草估 |
|---|---|---|
| UNIT-cli-resolver-claude | 目录树样例的小工具（在临时目录里按表建文件、符号链接、假 Mach-O）；Mach-O、npm 入口加嵌套或提升的平台包、postinstall 没跑时的占位文件、只有 JS 入口、符号链接链、找不到；查找顺序（设置、可信 PATH、常见位置）、nvm 当前版本 | 170 |
| UNIT-realpath | 项目内、工作区内、worktree 根目录、临时目录、相对路径、经符号链接逃逸 | 80 |
| UNIT-version-classify | 版本号提取（带前后缀，如 `2.1.296 (Claude Code)`、`git version 2.39.5 (Apple Git-154)`、`git-lfs/3.4.0 (...)`）；四种结果；读不出版本；git 最低版本 | 80 |
| UNIT-env-rules | 白名单、宿主设置、配置目录校验四种拒绝、放行、禁用清单（前缀和名字，取内嵌清单的值）、`CUPPER_*`、白名单不可被覆盖 | 130 |
| 登录 shell（拟新增 UNIT-login-shell） | 假 shell 脚本：正常、超时（`ManualClock` 推进，不真的等 5 秒）、输出里没有标记；解析；失败时的退回 | 100 |
| **测试小计** | | **560**，乘 1.1 约 **620** |

**合计**：约 **1,620 行**（实现约 1,000，测试约 620），超出 brief 的停止线（约 1,200）。即使不乘系数（585 + 560 = 1,145）也贴着停止线，按以往任务的偏差（T004 多 11%，T020、T050 都接近各自 1,500 的上限）几乎一定超出。所以按 brief 和守则第 8 条，开工前停下来汇报，没有写代码。

## 拆分建议

brief 举的拆法（"登录 shell、可信 PATH、CLI 解析与版本" / "清单内嵌与环境变量规则"）前一半约 1,320 行，仍然超线，后一半只有约 400 行。建议按"拉起进程"和"文件系统与规则"拆，两半互不依赖，可以同时派给两个执行者，对只有 1 个单位松弛的关键路径更有利：

| 拆分 | 模块 | 用例 | 估计 |
|---|---|---|---|
| **T054a 登录 shell 与版本检测**（拉起进程的部分） | `run.rs`、`login.rs`、`version.rs`（含 git、git-lfs 版本） | UNIT-version-classify、登录 shell 用例（UNIT-login-shell） | 实现约 375，测试约 200，合计约 **575** |
| **T054b 可信 PATH、CLI 解析与环境变量规则**（文件系统与规则） | `manifest.rs` 与 xtask、清单数据；`trust.rs`、`resolve.rs`、`CliLocator`；配置目录校验、适配器环境变量规则 | UNIT-realpath、UNIT-cli-resolver-claude、UNIT-env-rules | 实现约 620，测试约 410，合计约 **1,030** |

两半之间只用普通类型交接，不互相 import，接线留给 T055（约多 20–30 行）：

- T054a 的登录 shell 要知道保留哪些变量：接收一个名字列表参数，T055 传内嵌清单的 `env.allow` 加放行名单；
- T054a 的版本分类接收已验证范围参数（版本号字符串），T055 从内嵌清单取；
- T054b 的 `CliLocator` 接收 PATH 字符串和家目录，环境变量规则接收登录 shell 的变量表（`BTreeMap<String, String>`），都不依赖 T054a 的类型；
- T054b 的配置目录校验和可信 PATH 共用同一个 realpath 包含判断，放在同一个任务里，不重复。

不拆也可以：上限提到约 1,700 行，由一个执行者顺序完成，工期约为拆分后的较大一半的 1.6 倍。

## 拆分后的估计（编排者裁定后，计划 v1.3.13）

T054 收窄为"可信 PATH、CLI 解析与环境变量规则"，登录 shell 和版本检测拆给 T094。上限约 1,100 行，硬停止线 1,200。下面是重新逐模块估的行数（实现、测试都按写出来的行数估，不再乘系数）：

| 模块 | 内容 | 实现 | 测试 |
|---|---|---|---|
| `manifest.rs` | 内嵌清单，解析 `env.allow`、`env.deny`、`cli` 范围 | 55 | 25 |
| xtask、清单数据 | `Manifest` 加 `cli`；测试样例补字段；`runtime/manifest.json` 加 `cli` 段 | 25 + 数据 15 | 3 |
| `driver/locate/trust.rs` | 可信 PATH、"在边界之内"判断 | 55 | 75 |
| `driver/locate/resolve.rs` | 布局表、Mach-O 判定、npm 入口认定 | 95 | 110 |
| `driver/locate/testkit.rs` | 目录树样例的小工具（只在测试里） | — | 35 |
| `driver/locate/mod.rs` | `CliLocator`：查找顺序、常见位置、nvm 当前版本、`git`、`git-lfs` | 110 | 90 |
| `driver/locate/env_rules.rs` | 配置目录校验、适配器环境变量规则、提示码 | 120 | 130 |
| **合计** | | **约 475** | **约 470** |

合计约 **950 行**，在上限之内，余量约 150 行。

## 安装布局的核实（Q19，Claude 部分）

只读地列了本机目录，另外读了 npm registry 的公开元数据（`registry.npmjs.org`、`data.jsdelivr.com` 的文件列表、`cdn.jsdelivr.net` 上的 `install.cjs`；curl 默认 User-Agent，不带任何个人信息）。没有运行 `claude`，没有安装任何包，没有读 `~/.claude`、`~/.codex`。

1. **Homebrew cask**（本机核实）：`/opt/homebrew/bin/claude` → `/opt/homebrew/Caskroom/claude-code@latest/<版本>/claude`，Mach-O arm64，约 240 MB。cask 名也可能是 `claude-code`。realpath 后直接是原生二进制。
2. **npm，旧布局**（registry 元数据核实，2.1.100 及以前；1.0.100、2.0.0、2.0.50、2.1.0、2.1.100 都是）：`bin: {"claude": "cli.js"}`，`cli.js` 以 `#!/usr/bin/env node` 开头，约 13 MB 的 JS。包里的 `vendor/` 有 `ripgrep/arm64-darwin/rg`、`audio-capture/*.node`、`seccomp/*` 等 Mach-O 文件，但**没有** Claude 的原生二进制，`optionalDependencies` 只有 `@img/sharp-*`。→ `unsupported_install`。解析器只能认表里列出的位置，不能"在包里找任意一个 Mach-O"，否则会误认成 `rg`。
3. **npm，新布局**（registry 元数据核实：2.1.100 之后、2.1.120 之前换成这种布局；查了 2.1.120、2.1.140、2.1.150、2.1.200、2.1.250、2.1.296，都相同）：
   - `bin: {"claude": "bin/claude.exe"}`；`bin/claude.exe` 在包里是约 500 字节、**没有 shebang** 的占位文件；
   - `optionalDependencies` 有 `@anthropic-ai/claude-code-darwin-arm64` 等平台包；平台包 4 个文件，原生二进制在包根目录的 `claude`；
   - `postinstall: node install.cjs`：把平台包的 `claude` 硬链接（失败时复制）到 `bin/claude.exe` 上。正常安装后 `<npm 前缀>/bin/claude` → `../lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe`，realpath 后**本身就是 Mach-O**；
   - 生命周期脚本没有运行时（`--ignore-scripts`；pnpm 10 起默认不运行依赖的脚本），`bin/claude.exe` 仍是占位文件，原生二进制在 `<包>/node_modules/@anthropic-ai/claude-code-darwin-arm64/claude`（嵌套），或包所在 scope 目录下的 `claude-code-darwin-arm64/claude`（提升安装，pnpm 的虚拟仓库也是这种兄弟目录）；
   - 包里另有 `cli-wrapper.cjs`（JS 后备，`require.resolve` 平台包后拉起），不是 `bin` 入口。
   - **与 brief 的出入**：brief 和设计写的是"`#!/usr/bin/env node` 的 npm shim 按包布局找平台二进制"。实测带 shebang 的入口只出现在旧的纯 JS 布局里；新布局的入口是没有 shebang 的占位文件或原生二进制本身。所以解析器要按"realpath 后的路径在哪个包的哪个入口位置"来认 npm 安装，而不是按 shebang；shebang 只用来把表里没有的 node 脚本判为 `unsupported_install`。
4. **原生安装器**（本机没有，未核实）：公开资料是 `~/.local/bin/claude` → `~/.local/share/claude/versions/<版本>`，Mach-O。实现时按"realpath 后是 Mach-O 就直接用"处理，不依赖这个布局的细节。
5. **npm 全局前缀**：Node 官方安装包是 `/usr/local`；Homebrew 的 node 是 `/opt/homebrew`；nvm 是 `~/.nvm/versions/node/<版本>`（当前版本要读 `~/.nvm/alias/default`）。本机只有 `/usr/local/lib/node_modules`，里面没有 Claude Code。
6. **pnpm 全局安装**（未核实，留给实现时判断）：`~/Library/pnpm/claude` 一般是 `#!/bin/sh` 的脚本，不是符号链接，解析器认不出它指向的包，会判为不可用。

另外看了本机 codex 的 npm 布局，供 T111 参考：`/usr/local/bin/codex` → `../lib/node_modules/@openai/codex/bin/codex.js`（`#!/usr/bin/env node`），平台包嵌套在 `@openai/codex/node_modules/@openai/codex-darwin-arm64/`，原生二进制在 `vendor/aarch64-apple-darwin/bin/codex`（与 SP-2 记录的包内 `vendor/` 位置不同，新版本移进了平台包）。

## 实现时要注意的点（供执行者）

- `runtime/manifest.json` 的键顺序由 xtask 结构体的字段顺序决定（`unit_runtime_pack_envscan_the_committed_manifest_is_canonical_and_classified` 比对全文），`cli` 加在 xtask 结构体哪里，清单里就放在哪里。`runtime-pack` 写回时用 `pinned.clone()` 起步，新字段自然原样保留。
- 宿主的 `TestConfig` 已经有 `login_env`（T023），登录 shell 模块要给它留一个构造入口，T055 接线。
- `/usr/bin/git` 在没装命令行工具的机器上是 xcode-select 的占位程序，运行 `git --version` 会弹出安装对话框。这一点超出本任务范围，记在汇报的范围外发现里。

## 实际行数（逐模块）

`git diff --numstat`，不含本文件。实现、测试按文件里 `#[cfg(test)] mod tests` 前后分开统计。

| 完成的模块（提交） | 新增 / 删除 | 实现 | 测试 | 估计（实现 + 测试） |
|---|---|---|---|---|
| 内嵌清单、xtask 结构体、清单的 `cli` 段（`feat(host): embed the runtime manifest`） | 143 / 0 | 99 + 数据 15 | 29 | 80 + 数据 15 + 28 |
| 可信 PATH 与目录树小工具（`feat(host): trusted PATH entries`） | 202 / 0 | 85 | 117 | 55 + 110 |
| 解析器与布局表（`feat(host): data-driven resolver`）；目录树改成文本表 | 264 / 30 | 142 | 92 | 95 + 110 |
| `CliLocator`（`feat(host): CliLocator finds ...`） | 293 / 1 | 173 | 120 | 110 + 90 |
| 可信 PATH 用例的样例改成文本（`test(host)`） | 7 / 16 | — | −9 | — |
| 环境变量规则与配置目录校验（`feat(host): adapter environment rules`） | 303 / 0 | 186 | 116 | 120 + 130 |

**合计**：1,166 行，其中实现约 686（宿主 655、xtask 29、模块登记 2），测试约 465，清单数据 15。比估计（约 960）多约 21%，**超出 1,100 的上限约 66 行，在 1,200 的硬停止线以内**。超出的部分都在实现：文档注释和 rustfmt 对结构体字面量的展开比估计多；测试与估计基本相符（样例目录树和期望值改成文本表，省了约 60 行）。写 `CliLocator` 之后推算总数约 1,120（872 加环境变量规则估计的 250），已经贴线；当时判断这一模块写紧一些可以落在上限内，就没有停下，结果多出约 50 行。汇报里写明了。

## 实现时的取舍

1. **CLI 的查找**：设置里的 `cli_path` 一旦设置就是唯一候选，用不了也不退回其他位置（用户经原生确认指定的，悄悄换成别的程序反而意外），也不套位置规则。其余候选（可信 PATH、常见位置、nvm）里，第一个能解析出原生二进制的胜出；只有 JS 入口、占位文件或 shell 脚本的安装只在全部候选都没有原生二进制时才报 `unsupported_install`（设计"只找到 JS 入口"）。解析出的二进制和 npm 平台包里的二进制也要过位置规则（防止可信目录里的符号链接指向工作区）。
2. **npm 安装按路径认**：realpath 以 `@anthropic-ai/claude-code/<入口>` 结尾就认定是这个包（`Path::ends_with` 按路径分量比较），不读 `package.json`。平台二进制只在表里列出的两处找：嵌套的 `node_modules/...-darwin-arm64/claude`，和同一 scope 下的兄弟目录（提升安装、pnpm 的虚拟仓库）。只支持 arm64：随包的 Node 只有 darwin-arm64。
3. **Mach-O 判定**：普通文件、有执行位、前 4 字节是 Mach-O（32、64 位）或通用二进制的魔数。不判断 CPU 架构。
4. **常见位置**：`/opt/homebrew/bin`、`~/.local/bin`、`/usr/local/bin`、`~/.npm-global/bin`，再加 nvm 的默认版本。nvm 读 `~/.nvm/alias/default`，沿别名文件最多走 4 步（`lts/*` → `lts/jod` → `22`），得到版本号或前缀，取已安装版本里匹配的最新一个；`node`、`stable` 取最新。不读 `NVM_DIR`（登录 shell 正常时 nvm 的目录已经在 PATH 里，常见位置只是退路）。用例用只含家目录下位置的样例布局，避免受本机安装的影响。
5. **可信 PATH 返回 realpath**，去重后按原顺序。git 子进程的 PATH（T060）直接用它，避免条目被换成符号链接。
6. **配置目录**：传给适配器的是登录 shell 给出的原值，不是 realpath，让 CLI 看到和终端里相同的路径（登录态等可能按路径区分）。为此除了 realpath，还检查"路径本身所在的位置"（父目录的 realpath 加上最后一段）也不在项目或工作区内：放在项目里、指向外面的符号链接会被拒绝，因为 agent 以后可以改它的指向。临时目录允许（兼容套件的隔离配置目录在临时目录下，Q30）。test-harness 的覆盖接口是 `ConfigDir::from_test_config`，只在 `test-harness` 特性下存在。
7. **环境变量**：固定白名单的值以登录 shell 的为准，没有时取宿主自己的；`PATH` 只取登录 shell 的（T094 失败时给的是退回值）。放行名单依次检查名字格式、禁用清单、固定白名单，再取登录 shell 的值；跳过的名字带语义码返回（`passthrough_invalid_name`、`passthrough_denied`、`passthrough_fixed`、`passthrough_unset`）。宿主自己设置的变量最后写入。禁用清单和白名单都只来自 `manifest::embedded()`，用例逐条取内嵌清单的禁用项验证。
8. **语义码**：`Missing::code`（`cli_not_found`、`unsupported_install`）、`ConfigDirRejected::code`（`config_dir_not_absolute`、`config_dir_missing`、`config_dir_not_owned`、`config_dir_inside_project`）、`EnvNote::code`。这些是结果状态，不是错误，没有用 thiserror。
9. **目录树样例**写成文本表（`<路径> dir | macho | exec <内容> | plain <内容> | -> <目标>`），在临时目录里建出来，名字都是占位符。

## 交接

- **T055**：
  - 构造 `Boundaries::new(全部项目和工作区根目录加 worktree 根目录, system_temp_dirs($TMPDIR))`；
  - 用登录 shell 的 PATH（T094 的结果）和家目录构造 `CliLocator`，`locate(&resolve::CLAUDE, 设置里的 cli_path)`；
  - `check_config_dir(登录 shell 的 CLAUDE_CONFIG_DIR, cupper_sys::current_uid(), &boundaries)`；
  - `adapter_env(env_rules::CLAUDE, 宿主环境, 登录 shell 的变量表, binary, config_dir, 放行名单)` 得到 `Env`；
  - 版本分类的已验证范围从 `manifest::embedded().cli.claude` 取，交给 T094 的分类函数。
  - `test_config.login_env` 给的配置目录照样走 `check_config_dir`；要跳过校验时用 `ConfigDir::from_test_config`。
- **T094**：`CliLocator::tool("git")`、`tool("git-lfs")` 给出绝对路径；是 `/usr/bin/git` 时的 xcode-select 检查归 T094。登录 shell 要保留的变量名：`manifest::embedded().env.allow` 匹配的、`CLAUDE_CONFIG_DIR`、`CODEX_HOME`、放行名单。
- **T060**：git 子进程的 PATH 用 `CliLocator::trusted_path()`。
- **T111**：Codex 的布局加一个 `resolve::Layout`（`CODEX`），环境变量名已有 `env_rules::CODEX`。
