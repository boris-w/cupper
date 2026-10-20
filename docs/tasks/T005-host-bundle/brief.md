# T005 xtask host-bundle

- 派发日期：2026-10-20
- 执行者：子 agent（工程师，构建与发布工具）。T004 的执行者接手，因为它刚做完 `runtime-pack` 的暂存目录
- 分支：`build/T005-host-bundle`
- 依赖：T004、T023、T045（都已合并）。T075（桌面拉起宿主）依赖本任务

## 背景

宿主以独立的 `CupperHost.app` 随 App 分发（ADR-01，设计 5.10.1）。e2e 构建要求"打包方式与发布构建相同"（5.9.4），自用构建的 CLI 要把同一产物里宿主的 cdhash 编进去（5.8.1，T045 的裁定）。`cargo xtask host-bundle` 负责按构建种类组装并签名 `CupperHost.app`，再编译并签名 CLI。

## 范围

**做**（计划 T005 一行；设计 5.10.1、5.10.2、5.7.9、5.8.1）：
1. **`cargo xtask host-bundle --kind <release|self-use|e2e|dev>`**，按 5.7.9 的表取每种构建的身份和特性：

   | 构建种类 | 宿主 bundle id | 宿主特性 | CLI identifier | CLI 特性 |
   |---|---|---|---|---|
   | release | `dev.cupper.app.host` | 无 | `dev.cupper.cli` | 无 |
   | self-use | `dev.cupper.app.local.host` | `dev-auth` | `dev.cupper.cli.local` | `self-use` |
   | e2e | `dev.cupper.app.e2e.host` | `dev-auth`、`test-harness` | `dev.cupper.cli.e2e` | `test-harness` |
   | dev | `dev.cupper.app.dev.host` | `dev-auth` | `dev.cupper.cli.dev` | `dev-auth` |

   核对这张表与 5.7.9、`crates/*/Cargo.toml` 的特性说明一致，不一致时以设计为准并在汇报里指出。
2. **组装 `CupperHost.app/Contents`**：
   - `Info.plist`：bundle id 按上表，`CFBundleExecutable` 为 `cupper-host`，`LSUIElement`，`LSMinimumSystemVersion` 为 14.0；**不写任何用途说明**（`NSAppleEventsUsageDescription` 等，安全 #13）；
   - `MacOS/cupper-host`：按构建种类的特性编译；
   - `Helpers/node` 和 `Resources/runtime/…`：从 `runtime-pack` 的暂存目录 `target/runtime-pack/Contents/` 拷入，布局按 5.10.1。Node 已由 `runtime-pack` 重签，这里**不要重签 Node**，拷入前后核对它的 cdhash 与清单一致。
3. **签名**，从内到外：
   - 宿主：ad-hoc 签名，identifier 按上表，没有 entitlements；hardened runtime 按 5.7.9 的"签名"一行（自用构建开，e2e 构建关；发布构建在 T180 之前也先用 ad-hoc，开 hardened runtime）。签完 `codesign --verify --strict`，取出 cdhash；
   - CLI：在宿主签完之后编译。自用构建用 `CUPPER_HOST_CDHASH=<宿主 cdhash> cargo build -p cupper-cli --release --features self-use`（交接，T045）；然后单独 ad-hoc 签名，identifier 按上表。
4. **产出位置**：`target/` 下按构建种类分目录，例如 `target/host-bundle/<kind>/CupperHost.app` 和 `target/host-bundle/<kind>/cupper`。命令最后打印两者的路径和 cdhash。
5. **esbuild-wasm 是否精简**（交接，T004）：整包里有用不到的浏览器版文件。由本任务决定精简与否；精简的话清单（`runtime/manifest.json`）要同步，`runtime-pack --verify` 仍要通过。不精简也可以，在汇报里说明理由。
6. **IT-host-bundle-layout**（`pr` 套件，已登记在 `tests/ids.toml`）：
   - 布局与 5.10.1 一致；
   - 宿主和 CLI 的 identifier 与构建种类一致；
   - 两个 Info.plist 都不含用途说明；
   - 自用构建的 CLI：`cupper --build-info` 输出的 `host_cdhash` 等于同一产物里宿主的 cdhash（交接，T045）。

**不做**：
- 自用构建的签名参数，例如本机证书（T160）；
- Developer ID 签名和公证（T180）；
- 运行时清单编译期内嵌进宿主、宿主侧的运行时安装器（T056）；
- `Cupper.app` 外层的组装（electron-builder，T075 之后的任务）。

## 验收标准

- [ ] `cargo xtask host-bundle --kind e2e` 和 `--kind self-use` 都能在本机产出签名有效的 `CupperHost.app` 与 CLI，`codesign --verify --strict` 通过。
- [ ] 新增 IT-host-bundle-layout，测试名以用例编号开头。
- [ ] **IT-host-bundle-layout 在 `pr` 里离线运行**：CI 的 `rust` 作业不能依赖网络下载，也不能依赖本机已有的 `runtime-pack` 下载缓存。用例里用一份测试用的假运行时目录（例如 ad-hoc 签名的小 Mach-O 充当 Node）。命令要能接受指定的运行时目录，默认仍取 `target/runtime-pack/Contents/`。
- [ ] 不明显拉长 `rust` 作业：新增耗时控制在 30 秒左右以内（例如复用已经编译好的产物、只用 debug 配置编译测试用的产物）。做不到时先停下来汇报。
- [ ] 不新增 CI 作业；用例放在 `rust` 作业已经覆盖的测试里。确实需要新作业时，先汇报，由编排者改设计 5.1.6。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T005-host-bundle/ci/` 通过，摘要随任务提交。

## 交接事项（已在 `handoff-notes.md`）

- T045：自用构建的 CLI 编译命令和 `CUPPER_HOST_CDHASH` 的格式（40 位小写十六进制；缺失或格式不对时编译失败）；产物核对 `host_cdhash`。
- T004：暂存目录的布局；esbuild-wasm 的精简由本任务定。

## 注意

- 签名只用 ad-hoc，不碰钥匙串里的任何证书，也不需要登录 Apple 账号。
- 不拉起真实的 Claude Code CLI，不登录。
- 产物都在 `target/` 下，不入库。
- ADR-11：`CupperHost.app` 里不能出现 `@anthropic-ai/claude-agent-sdk` 的任何部分和 Claude Code 二进制。claude-agent-acp 相关的只有打包脚本和锁文件。用例里可以顺带断言这一点。
- 依赖锁精确版本，许可证要在 ADR-11 的 allowlist 内。能用 macOS 自带的 `codesign`、`plutil`，就不要为此引入新 crate。

## 规模

S，计划估约 400 行以内。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 600 行时，必须先停下来汇报**。

## 参考资料

- 设计：`10-release.md` 5.10.1、5.10.2；`07-desktop.md` 5.7.9；`08-security.md` 5.8.1（自用构建一行）；`01-repo-build-ci.md` 5.1.5、5.1.6；`02-host.md` 末尾"T045 实现时的裁定"。
- `docs/tasks/T004-runtime-pack/`（`notes.md`、`review.md`）；`docs/tasks/T045-cli-client-mirror/`。
- ADR-01、ADR-05、ADR-11。

## 工作方式

在原来的 T004 worktree 里，从最新的 main 开新分支：`git checkout -b build/T005-host-bundle main`。T004 的分支已经合并，不要再往它上面提交。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
