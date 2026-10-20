# T115 运行时安装器 II：按锁文件下载与本机打包

- 派发日期：2026-10-20
- 执行者：子 agent（工程师）。T004、T056 的执行者接手
- 分支：`feat/T115-runtime-download`
- 依赖：T056（合并中）。T066（首批真实录制）依赖本任务

## 背景

T056 开工前拆成两份（计划 v1.3.20）：T056 做了运行时目录、复制、原子提交与 `--self-test`；本任务按锁文件从 npm 下载 claude-agent-acp 的依赖、在本机用随包的 Node 和 esbuild-wasm 打包，并核对产物哈希（设计 5.2.12，ADR-08、ADR-11）。

## 范围

**做**：
1. **新组件 `ClaudeAgentAcp`**：失败原因加 `Network`、`HashMismatch`；核心改为实现 `pure::Core`，加退避定时器（驱动带 `Clock`）；要等 node 和 esbuild-wasm 都 `ready` 之后再开始；打包脚本和锁文件同样经 `Bundle` 读取并核对哈希（T056 的交接）。
2. **HTTP 客户端**：`reqwest` 加 rustls，加密实现用 `ring`，根证书用系统的（`rustls-native-certs`），不用 `aws-lc-rs` 和 `webpki-roots`；以 `cargo deny` 对 ADR-11 白名单通过为准，不过就停下汇报。User-Agent 为 `cupper-runtime-installer/<版本>`，不带任何用户信息；遵守登录 shell 里的 `HTTPS_PROXY`；只访问 `registry.npmjs.org`，registry 可经 `test_config.runtime.registry_url` 注入。
3. **按锁文件下载**：解析锁文件；逐包下载、SRI 校验；解包只取普通文件、路径越界就失败；**不运行任何包脚本**；8 个含 Claude Code 二进制的 optional 平台包永远不下载（ADR-11）。网络错误按 `Clock` 退避重试 3 次。
4. **本机打包**：用随包的 Node 跑打包脚本，环境只有 `PATH`（只放随包 Node 所在的目录，T004 的交接）；打包产物的 sha-256 与清单一致才提交；用完删掉下载目录（含 SDK）。
5. **两处衔接**（T055、T056 合并后编排者发现）：
   - **拉起路径改到安装器的布局**：T055 的 `launch::production` 现在从 `<数据目录>/runtime/node` 和 `<数据目录>/runtime/bundle-claude-acp.mjs` 拉起；T056 定下的布局是 `<数据目录>/runtime/<组件>/<版本>/`。改成从安装器装好的版本目录拉起（Node、打包产物）。
   - **拉起前的核对只留一份**：T055 在 `launch` 里写了一份按清单核对文件的函数，T056 有 `runtime::files::verify`。合成一份（从 `runtime` 导出），`launch` 用它。

**不做**：codex-acp 的复制（T111）；接进宿主启动的后台校验（T059 之后，写清接入点）。

## 验收标准

- [ ] 新增 IT-runtime-download：网络失败按退避重试 3 次、SRI 不符、越界的 tarball、打包产物哈希不符、经本地 registry 的完整安装；全部离线（自己用 `tokio` 起一个最小的 HTTP registry，用自己造的小 tarball 和假的打包器）。
- [ ] 不下载真实的 SDK；测试里不出现真实的 npm 包内容。
- [ ] `launch::production` 改到新布局后，T055 的拉起用例照常通过；核对函数只剩一份。
- [ ] 不睡眠；时间只经 `Clock`。
- [ ] 测试名以用例编号开头；编号已在 `tests/ids.toml` 的 T115 段里。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T115-runtime-download/ci/` 通过，摘要随任务提交。

## 注意

- **ADR-11 红线**：Anthropic 的 SDK 和 Claude Code 二进制绝不进入仓库和产物。
- 用例里的运行时目录、下载缓存都用临时目录；不访问网络。
- 代码约束：子进程一律经 `cupper_sys::spawn`；时间只经 `Clock`；有状态的决策逻辑写成纯核心。

## 规模

计划估为 M，T056 的执行者估约 530 行，加上第 5 项。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**推算会超出约 800 行（包括贴线）时，必须先停下来汇报**，不要指望后面压回去。

## 参考资料

- 设计：`02-host.md` 5.2.12；`10-release.md` 5.10.3；ADR-08、ADR-11；SP-2 的报告。
- `docs/tasks/T004-runtime-pack/`、`T056-runtime-installer/`、`T055-agent-runtime/` 的 `notes.md`。
- `crates/xtask/src/runtime_pack/`（下载、解包、打包的做法可以参考）、`crates/cupper-host/src/runtime/`、`driver/runtime/launch.rs`。

## 工作方式

T056 合并之后，在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b feat/T115-runtime-download main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
