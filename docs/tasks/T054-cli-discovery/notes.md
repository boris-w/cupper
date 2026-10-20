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
