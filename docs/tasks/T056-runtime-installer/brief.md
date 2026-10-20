# T056 运行时安装器

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T004、T005、T094、T028、T075、T007 的执行者接手，因为运行时清单、暂存目录和 claude-agent-acp 的本机打包都是它在 T004 里做的
- 分支：`feat/T056-runtime-installer`
- 依赖：T004、T020、T025（都已合并）。T064（兼容套件框架）依赖本任务；离剩余关键路径有 3 个单位的松弛

## 背景

App 带着一份运行时（Node、codex-acp、esbuild-wasm、claude-agent-acp 的锁文件和打包脚本，设计 5.10.1），但宿主运行期间不读 App 包里的文件，唯一的例外就是本任务：运行时安装器按编译期内嵌的清单核对哈希后，把包内文件复制进用户数据目录里的运行时目录；claude-agent-acp 按锁文件从 npm 下载、在本机打包并核对哈希（ADR-08、ADR-11：SDK 和 Claude Code 二进制不进 App）。适配器从这个运行时目录拉起（T055 已留好"运行时目录"的拉起分支）。

## 范围

**做**（计划 T056 一行；设计 5.2.12、5.10.3、ADR-08）：
1. **清单在编译期内嵌**：T054 已在 `cupper-host` 里建了内嵌清单模块（`manifest.rs`，只解析了 `env`、`cli`），在它上面加本任务要用的字段（文件哈希、Node 的 cdhash、claude-agent-acp 的打包哈希、锁文件等）。
2. **组件状态机**：每个组件（Node、claude-agent-acp、esbuild-wasm）的状态与转换，纯核心。
3. **自检后按清单复制包内文件**：先核对包内文件的哈希，再复制；复制中包被替换（文件变了）要能发现并失败。
4. **按锁文件下载 npm tarball**：SRI 校验；路径检查（解包不越界、只取普通文件）；**不运行任何包脚本**；User-Agent 用项目占位（例如 `cupper-research`），不含用户信息；只访问 `registry.npmjs.org`。registry 可注入（测试用本地夹具）。
5. **本机打包 claude-agent-acp 并核对哈希**：用随包的 Node 和 esbuild-wasm 跑打包脚本；`PATH` 只放随包 Node 所在的目录，其余环境变量清空（交接，T004）；打包产物的 sha-256 与清单一致才算成功。
6. **原子提交与旧版本回收**：先装到临时目录，校验通过后原子改名提交；旧版本在没有适配器使用时回收。
7. **`--self-test`**：宿主的 `--self-test` 参数（现在会被当作未知参数，以退出码 1 退出，交接 T023），核对已安装的运行时是否完整。

**不做**：codex-acp 的复制（T111）；接进宿主启动的后台校验和 `AgentRuntime`（T055 之后的接线，在汇报里写清接入点）。

## 验收标准

- [ ] 新增 IT-runtime-install：SP-2 记下的 5 种故障加"复制中包被替换"，全部用本地夹具离线运行（注入的 registry、造的 tarball、假的打包脚本或测试用 Node），不访问网络。
- [ ] 不运行任何 npm 包脚本；下载与解包有路径越界、SRI 不符的用例。
- [ ] `--self-test` 有用例。
- [ ] 不睡眠；时间只经 `Clock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T056 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T056-runtime-installer/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T023：`--self-test` 还没加。
- T004：打包脚本的 `PATH` 只放随包 Node 所在的目录。
- T054：内嵌清单模块在 `manifest.rs`，在它上面加字段。

## 注意

- **ADR-11 红线**：Anthropic 的 SDK 和 Claude Code 二进制绝不进入仓库和产物。测试用的 tarball 是自己造的小包，不下载真实的 SDK。8 个含 Claude Code 二进制的 optional 平台包永远不下载。
- 用例里的运行时目录、下载缓存都用临时目录；不碰本机真实的数据目录。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；有状态的决策逻辑写成纯核心；宿主运行期间只有本安装器读 App 包内的文件，而且要先核对哈希。
- 解包用 T004 已引入的 `tar`、`flate2`（锁精确版本）；新依赖许可证在 ADR-11 的 allowlist 内。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计（测试单独列），推算超线（包括贴线）就开工前停下汇报，并给出拆分建议**（例如"复制与原子提交、`--self-test`"和"下载与本机打包"分开）。**每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。

## 参考资料

- 设计：`02-host.md` 5.2.12；`10-release.md` 5.10.1、5.10.3；`06-agent-driver.md` 5.6.6。
- ADR-08、ADR-11；SP-2 的报告（5 种故障）。
- `docs/tasks/T004-runtime-pack/`（`notes.md`）、`T005-host-bundle/`、`T054-cli-discovery/`。
- `crates/xtask/src/runtime_pack/`（下载、解包、打包的做法可以参考，宿主侧另写）、`crates/cupper-host/src/manifest.rs`。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b feat/T056-runtime-installer main`。T007 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。

## 补充裁定（2026-10-20，编排者）

执行者开工前估计约 1,360–1,630 行，贴线，在写代码之前停下汇报，建议按任务书举的拆法拆分。

**裁定：拆分（计划 v1.3.20）。**
- **T056 收窄为"运行时目录：复制、原子提交与 `--self-test`"**：`manifest.rs` 加字段、组件状态机与驱动、从包内复制（边读边算 sha-256，Node 另核对 cdhash）、`<数据目录>/runtime/<组件>/<版本>/`、staging、`.installed.json`、原子提交、坏了挪开重装、旧版本回收、`--self-test`。验收 IT-runtime-install 中不需要下载的部分。上限约 1,000 行，硬停止线 1,100。T064 只依赖这部分。
- **新增 T115"按锁文件下载与本机打包"**（依赖 T056）：`reqwest`、锁文件、注入的 registry、下载与 SRI、解包路径检查、退避重试、本机打包与哈希核对；新增编号 IT-runtime-download。T066 改为也依赖 T115。`tests/ids.toml` 已在 main 上改好。

其余三点：
1. **HTTP 客户端**（归 T115）：`reqwest` 加 rustls，加密实现用 `ring`，根证书用系统的（`rustls-native-certs`），不用 `aws-lc-rs` 和 `webpki-roots`；以 `cargo deny` 对 ADR-11 白名单通过为准，不过就停下汇报。
2. **User-Agent 以设计 5.2.12 为准**：`cupper-runtime-installer/<版本>`。本任务书写成 `cupper-research` 是编排者的笔误（那是调研任务用的），产品名加版本不含个人信息。
3. **`--self-test`**：退出码只反映"随包组件与内嵌清单一致"（设计 5.10.7 发布流水线要的语义，构建时还没有已安装的运行时）；已安装运行时的状态作为一个字段写进那行 JSON，不影响退出码。

"复制中包被替换"用边读边算 sha-256 发现、不在产品代码里加测试钩子：认可。registry 的注入点用已有的 `test_config.runtime.registry_url`：认可。
