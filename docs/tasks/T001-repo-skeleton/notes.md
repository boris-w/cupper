# T001 工作笔记

## 工具链与依赖版本（2026-10-19 复核）

设计 5.1.4 要求"M1a 开工时按当天复核"。原则：优先用本机已有、已实测过的版本；新装的依赖选发布满约两周的版本。

| 项 | 选定 | 理由 |
|---|---|---|
| Rust | `1.96.0`（`rust-toolchain.toml`，targets `aarch64-apple-darwin`、`aarch64-apple-ios-sim`，components clippy、rustfmt） | SP-4a 的实测版本，本机已有；钉精确版本，不写 `stable`。`rust-version = 1.89`（`File::try_lock`，设计 5.1.4） |
| cargo-deny | 0.20.2 | 本机已有；workflow 里用 `cargo install --locked cargo-deny@0.20.2` |
| Node（开发与工具） | `24.13.0`（`.node-version`） | 本机实际版本。随 App 带的运行时是 24.22.0（runtime 清单），两者无关；作者升级本机 Node 时可以一起对齐 |
| pnpm | `12.14.0`（`packageManager`） | 本机实际版本，也是 npm 的 `latest`。设计写 10.x，但 10 和 12 的配置格式不同（10 用 `onlyBuiltDependencies`，12 用 `allowBuilds`，并且 12 对 `pnpm-workspace.yaml` 里不认识的键直接报错），按 10.x 写配置再用 12 跑会不一致；钉 10.x 则本机 pnpm 会按 `packageManager` 自动下载切换。12.0.0 发布于 2026-08-26，已有 14 个小版本 |
| TypeScript | `6.0.3` | `latest` 已是 7.0.2（Go 实现），但 typescript-eslint 8.71 的 peer 依赖是 `typescript <6.1.0`，所以停在 6.0.x |
| ESLint / @eslint/js | `10.12.0` / `10.0.1` | 10.13.0 才发布 3 天，取上一个小版本 |
| typescript-eslint | `8.71.1` | 同上，发布满两周 |
| Prettier | `3.9.9` | 同上 |
| GitHub Actions | `actions/checkout` v7.0.1 `3d3c42e5…`、`actions/setup-node` v7.0.0 `82076278…` | 按提交 SHA 固定，均发布于 2026-07。没用 `pnpm/action-setup`（第三方），改为按 `packageManager` 的值 `npm install --global` |

## 关键取舍

- **根 `package.json` 的"只放编排脚本"**：理解为根目录不放产品代码和产品依赖。ESLint、Prettier、TypeScript 这些所有包共用的开发工具放在根的 `devDependencies`，否则基础配置没法在本任务里实际跑通。
- **pnpm 不运行依赖脚本**：pnpm 12 默认就不运行；`pnpm-workspace.yaml` 里写了空的 `allowBuilds: {}` 表示白名单为空，以后需要的包（electron、esbuild）逐个加并写理由。
- **`ts` 作业**：`pnpm install --frozen-lockfile` → `prettier --check .` → `eslint --max-warnings=0 .` → 对 `packages/*`、`apps/*`、`tools/*` 中有 `tsconfig.json` 的包跑 `tsc --build` → `pnpm -r --if-present run test`（各包的 `test` 脚本跑 Vitest）。没有 TS 包时只检查根目录的配置文件。按包目录扫描，以后加包不用改根清单。
- **`rust` 作业**：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、`cargo test --workspace --all-features --locked`。cargo 用运行 xtask 的同一个工具链（`$CARGO`）。
- **`license` 作业**：`cargo deny --locked check`。`deny.toml` 的 `graph.targets` 只列 macOS 和 iOS 模拟器两个目标（我们构建和分发的）；`licenses.private.ignore = true` 跳过自己的 `publish = false` crate；`unused-allowed-license = "allow"`，白名单是策略，不必每项都用到。
- **未实现的作业**：设计 5.1.6 的全部作业都已登记（另加 `perf`，对应计划 1.8 的"性能"）。直接调用未实现的作业时报"尚未实现"并以退出码 2 结束；在组合作业 `pr`、`nightly` 里则跳过，摘要里记 `not_implemented`。这样以后的任务只需新增作业文件并在 `jobs/mod.rs` 里填上 `run`，组合作业的成员已经是最终的。
- **退出码**：0 全部通过；1 有作业失败；2 拒绝运行（参数错误、作业未实现、工作区不干净、摘要自检不通过）。
- **干净检查**：`git status --porcelain=v1 --untracked-files=all` 必须为空，否则在启动任何作业之前拒绝。
- **机器级锁**：`std::fs::File::lock`（Unix 上是 `flock`），锁文件是 `$TMPDIR/cupper-ci/machine.lock`（macOS 的 `$TMPDIR` 是每个用户自己的目录）。进程退出或被杀时内核随文件描述符一起释放锁。持锁作业是 `ui`、`e2e`、`perf`；nfr1 是 e2e 的一个套件，跟着 e2e 持锁。被挡住时打印一行"等待机器锁"，然后阻塞等待。
- **原始日志**：`.private/ci-logs/<目标>-<提交前 12 位>-<毫秒时间戳>/<作业>-<步骤>.log`（按任务书写 `ci-logs`；计划 2.1 的备注写的是 `.private/ci/`）。终端上成功的步骤只打一行，失败的步骤打印日志最后 40 行。
- **结构化摘要**（`--record <目录>` 写 `<目录>/<目标>-<提交前 12 位>.json`）：
  - 字段白名单：`version`（摘要格式版本）、`commit`、`target`（调用的作业或组合作业）、`toolchain`（rustc、cargo、cargo_deny、node、pnpm 的版本号）、`os`（如 `macOS 26.4`）、`machine`（机型泛称，取 `system_profiler` 的 Model Name 和 Chip，如 `MacBook Pro (Apple M5 Pro)`，不取型号标识和序列号）、`duration_ms`、`jobs[]`（name、result、passed、failed、duration_ms）、`results[]`（id、suite、result、commit）、`artifacts[]`（name、sha256；e2e 出现后由 T090 填）。白名单写成一个嵌套的形状表，写入前逐键校验，多一个键就拒绝。
  - 通过数和失败数的口径：每个测试用例算一项；没有测试用例的步骤（fmt、clippy、cargo deny、prettier……）整步算一项。`cargo test` 编译失败、一个用例都没跑时记 1 项失败。
  - 自检：任何字符串值里出现绝对路径（以 `/`、`~` 开头的词，或 `X:\`、`\\`）、登录名（`$USER`、`$LOGNAME`、`$HOME` 的末段）、主机名（`hostname` 全名和短名，macOS 再加 ComputerName、LocalHostName）、git 的 `user.name`、`user.email`，或者邮箱形式的词，就拒绝写入，什么也不留。名字按整词、不区分大小写匹配。机型泛称本身包含的词（比如主机就叫 "MacBook Pro"）不算个人信息，从名单里去掉。
- **逐编号结果清单**：编号从 `docs/design/*.md` 和 `docs/03-plan.md` 里抽取（按 5.9.10 的层前缀，`MAN-` 除外），保留文档里的原样大小写；一个测试的名字（或它路径上的某个模块名）以编号的 snake_case 形式开头、后面紧跟 `_` 或结束，就属于这个编号，取最长匹配。某编号的测试全部通过才记 `pass`。`suite` 记调用目标（`pr`、`nightly` 或单个作业名）。目前只解析 `cargo test` 的 libtest 输出；T002 改读 nextest 的 JUnit 时可以替换这一处。
- **`disallowed-methods` 的负面样例**：`crates/xtask/tests/fixtures/disallowed-methods/` 是一个不进工作区的小 crate（自带 `[workspace]` 和 lock），对清单里的 13 个方法各调用一次。集成测试用工作区的 `clippy.toml`（`CLIPPY_CONF_DIR`）对它跑 clippy，断言失败，并且清单里的每个路径都有一条对应的诊断。清单新增一项而样例没跟上，或者路径写错，测试都会失败。用变异检查确认过：删掉样例里的 `Uuid::now_v7` 调用，测试失败。
- **xtask 自己读时间**：xtask 没有 `Clock`，只在 `clock.rs` 一个模块里用 `#![expect(clippy::disallowed_methods, reason = …)]` 读系统时间（步骤耗时、日志目录名）。
- **错误类型**：xtask 是二进制，用 `anyhow`，没有定义自己的错误类型，所以没用 thiserror（另见下面许可证一节）。
- **`[workspace.lints]`**：`unsafe_code = "deny"`（以后 `cupper-sys` 在 crate 根用 `allow` 放开）；clippy 的 `await_holding_lock`、`await_holding_refcell_ref` 为 deny；`allow_attributes_without_reason`、`dbg_macro`、`todo`、`unimplemented` 为 warn（CI 里 `-D warnings`）。
- **workflow**：只写了已实现的三个作业（`rust`、`ts`、`license`），其余作业由实现它们的任务补。通用加固照 5.1.6：Action 按 SHA 固定、`permissions: contents: read`、不用 `pull_request_target`、runner 固定标签（`macos-15`、`ubuntu-24.04`）、checkout 不保留凭据。

## 试错

- 锁的"持有者被杀后释放"测试第一次卡住：子进程是测试二进制自己（`--exact` 只跑这个用例），libtest 先打印 `test <名字> ... ` 不换行，子进程的就绪标记跟在同一行，父进程按整行比较永远等不到。改为先换行再打印一个专用标记。
- 用临时克隆加一个 probe 包验证了 `ts` 作业"有 TS 包"的分支（tsc 和 Vitest 都跑到并通过），没有提交。发现 `tsc --build` 会把测试文件编译进 `dist/`，Vitest 4 默认不排除 `dist`，同一个用例会跑两遍。这是包自己的配置问题，交给建第一个 TS 包的任务（T015 / T070）：在 Vitest 配置里排除 `dist`，或者测试文件不进 `tsc` 的产出。
