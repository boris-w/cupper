# T001 汇报

- 执行者：子 agent（工程与 CI 工程师，Rust 与 TypeScript）
- 分支：`build/T001-repo-skeleton`
- 完成日期：2026-10-19

## 做了什么

**结果：** 验收标准逐条满足。干净工作区里 `cargo xtask ci pr` 通过（rust、ts、license 三个作业），摘要已经用 `--record` 写到 `docs/tasks/T001-repo-skeleton/ci/pr-5c75b73117f1.json`。

**要先说明：规模超了预算。** 不含 lock 文件共约 2,070 行，任务书的上限约 1,500 行。我没有中途停下来报告，做完才发现超出，这是我的疏忽。构成如下：
- xtask 的产品代码约 1,150 行；
- 测试约 520 行（含 testutil、负面样例 crate 和集成测试）；
- 根配置和 workflow 约 270 行；
- `notes.md` 和摘要约 130 行。

如果要压缩，最容易缩的是测试，以及 rustfmt 竖排展开的清单（组合作业、摘要白名单的形状表）。

**交付内容：**

1. **Rust 工作区**
   - 根 `Cargo.toml`：resolver 3，edition 2024，成员用通配 `crates/*`，`rust-version = 1.89`，`[workspace.dependencies]` 集中管理（目前只有 anyhow、serde_json）。
   - `[workspace.lints]`：`unsafe_code = deny`；`await_holding_lock` 和 `await_holding_refcell_ref` 为 deny；`dbg_macro`、`todo`、`unimplemented`、`allow_attributes_without_reason` 为 warn。
   - `rust-toolchain.toml` 锁定 1.96.0，targets 为 `aarch64-apple-darwin` 和 `aarch64-apple-ios-sim`。
   - `clippy.toml` 的 `disallowed-methods` 有 13 项，覆盖设计 5.2.16 的全部清单，每项都写了理由。
   - `deny.toml` 按 ADR-11 白名单，开启安全公告检查，并拒绝 yanked 的版本和未知来源。
   - `rustfmt.toml`；`.cargo/config.toml` 里的别名 `xtask = run -p xtask --locked --quiet --`。
2. **`disallowed-methods` 的负面样例**
   - `crates/xtask/tests/fixtures/disallowed-methods/` 是一个不进工作区的小 crate，对清单里每个方法各调用一次。
   - 测试 `crates/xtask/tests/clippy_disallowed_methods.rs` 用工作区的 `clippy.toml` 对它跑 clippy，断言 clippy 失败，并且清单中每个路径都有对应的诊断。
3. **`cargo xtask ci <作业> [--record <目录>]`**（`crates/xtask/src/ci/`，每个作业一个文件，放在 `jobs/`）
   - `rust`：fmt check、clippy `-D warnings`（全部 target 和特性）、`cargo test --no-fail-fast`。
   - `ts`：`pnpm install --frozen-lockfile`、Prettier、ESLint；有 TS 包时再跑 `tsc --build` 和各包的 `test` 脚本。没有 TS 包时只检查根目录配置，作业通过。
   - `license`：`cargo deny --locked check`。
   - 组合作业 `pr`、`nightly`。
   - 设计 5.1.6 的其余作业都已登记，另加 `perf`。
     - 直接调用这些作业时报"尚未实现"，退出码 2。
     - 在组合作业里跳过，摘要记为 `not_implemented`。
   - **运行前检查**：`git status --porcelain --untracked-files=all` 不为空就拒绝运行，不启动任何作业。
   - **机器级锁**：`ui`、`e2e`、`perf` 三个作业取同一把锁；nfr1 是 e2e 的一个套件，跟着 e2e 取锁。
     - 用 `File::lock`（flock）实现，锁文件是 `$TMPDIR/cupper-ci/machine.lock`，macOS 上这是每个用户自己的目录。
     - 进程退出或被杀时，内核自动释放锁。
   - **摘要**：字段白名单写成一个嵌套的形状表，多出任何键都拒绝写入。字段如下：
     - `version`、`commit`、`target`；
     - `toolchain`：rustc、cargo、cargo_deny、node、pnpm 的版本；
     - `os`、`machine`（机型泛称，例如 "MacBook Pro (Apple M5 Pro)"）、`duration_ms`；
     - `jobs[]`：name、result、passed、failed、duration_ms；
     - `results[]`：id、suite、result、commit；
     - `artifacts[]`：name、sha256，目前为空。
   - **写入前自检**：值里出现绝对路径、登录名、主机名（含 ComputerName 和 LocalHostName）、git 的 user.name 或 user.email，或者邮箱形式的内容，就拒绝写入，什么也不留。
   - **逐编号结果**：编号从 `docs/design/` 和 `docs/03-plan.md` 抽取；测试名以编号的 snake_case 形式开头，就归入该编号，取最长匹配。
   - **原始日志**：写到 `.private/ci-logs/<目标>-<提交>-<时间戳>/<作业>-<步骤>.log`。
   - **退出码**：0 全部通过；1 有作业失败；2 拒绝运行。
4. **pnpm 工作区**
   - 根 `package.json` 只有 format、lint、test 三个编排脚本，加上共用的开发工具；`packageManager` 为 `pnpm@12.14.0`。
   - `pnpm-workspace.yaml` 包含 `packages/*`、`apps/*`、`tools/*`，并写了空的 `allowBuilds: {}`。
   - `.node-version` 为 24.13.0。
   - 另有 `tsconfig.base.json`、`eslint.config.js`（flat config，typescript-eslint 的 strict 和 stylistic 规则）、`.prettierrc.json`、`.prettierignore` 和 `pnpm-lock.yaml`。
5. **workflow**：`.github/workflows/rust.yml`、`ts.yml`、`license.yml`，每个只调用 `cargo xtask ci <作业>`。
   - 加固：Action 按提交 SHA 固定，`permissions: contents: read`，不用 `pull_request_target`，runner 用固定标签，checkout 不保留凭据。
   - pnpm 按 `packageManager` 字段的值用 npm 安装，不引入第三方 Action。
6. **`.gitignore`**：补上 `target/`、`node_modules/`、`dist/`、`*.tsbuildinfo`。
7. **`notes.md`**：版本选择、各项取舍和试错过程；另有本任务的 CI 摘要。

## 关键取舍

- **pnpm 选 12.14.0，没有按设计用 10.x。**
  - 本机实际装的就是 12.14.0，它也是 npm 上当前的 `latest`。
  - 10 和 12 的配置格式不兼容：10 用 `onlyBuiltDependencies`，12 用 `allowBuilds`，而且 12 遇到不认识的配置键会直接报错。
  - 如果钉 10.x，本机 pnpm 会按 `packageManager` 自动下载并切换版本。
- **TypeScript 停在 6.0.3。** 7.x 已经发布，但 typescript-eslint 8.71 要求 TypeScript 低于 6.1。
- **其他依赖选发布满约两周的版本**：ESLint 10.12.0、typescript-eslint 8.71.1、Prettier 3.9.9。
- **Rust 1.96.0**：SP-4a 实测过、本机已有。
- **开发工具放在根 `package.json`。** "只放编排脚本"我理解为根目录不放产品代码和产品依赖。ESLint、Prettier、TypeScript 是所有包共用的工具，放在根目录，基础配置才能在本任务里实际跑通。
- **未实现的作业在组合作业里跳过，不算失败。** 这样组合作业的成员现在就是最终的样子，以后的任务只需新增作业文件，并在 `jobs/mod.rs` 里填上 `run`。
- **xtask 只用 anyhow 和 serde_json，没有用 thiserror 和 serde derive。** derive 宏会引入 `unicode-ident`，它的许可证需要 `Unicode-3.0`，而 ADR-11 白名单里没有这一项（见遗留问题 1）。xtask 没有定义自己的错误类型，所以没有违反"错误类型用 thiserror"的约束。
- **摘要里通过数和失败数的口径**：每个测试用例算一项；没有测试用例的步骤（fmt、clippy、deny、prettier 等）整步算一项。

## 验证方式

- [x] **干净 worktree 里 `cargo xtask ci pr` 通过。**
  - 本 worktree 通过：rust 15 项、ts 3 项、license 1 项，其余作业为 not_implemented。
  - 另在全新克隆里冷启动跑过一次（没有 `target/` 和 `node_modules/`），同样通过。
  - 在克隆里分别构造了以下情况，结果都符合预期：
    - 未提交的文件：拒绝运行，退出码 2；
    - 调用未实现的作业：退出码 2；
    - fmt 失败：退出码 1，摘要里该作业记为 fail；
    - 单元测试失败：退出码 1，摘要里 `UNIT-ci-lock` 记为 fail。
- [x] **负面样例让 clippy 失败，并有测试断言。**
  - 测试：`clippy_rejects_every_disallowed_method`。
  - 做过变异检查：删掉样例里的 `Uuid::now_v7` 调用后，测试失败。
- [x] **UNIT-ci-summary**
  - `unit_ci_summary_holds_allowlisted_fields_only`：分别在顶层、toolchain、jobs、results 里注入多余的键，都被拒绝。
  - `unit_ci_summary_refuses_paths_and_identity`：分别注入 `/Users/...`、`/tmp`、`~/`、`file://`、`C:\`、用户名、主机名、邮箱，都拒绝写入，也没有留下文件。
  - `unit_ci_summary_ids_come_from_docs_and_match_test_names`：编号抽取和测试名匹配。
- [x] **UNIT-ci-lock**
  - `unit_ci_lock_second_holder_waits_until_first_releases`：用 channel 断言执行顺序，确定性，不依赖 sleep。
  - `unit_ci_lock_released_when_holder_is_killed`：子进程持锁后用 SIGKILL 杀掉，锁随即可以取得。
  - `unit_ci_lock_clean_check_rejects_any_change`：未跟踪、未暂存、已暂存三种改动都被拒绝。
  - `unit_ci_lock_refuses_to_run_in_a_dirty_work_tree`：拒绝后没有启动任何作业，也没有创建 `.private`。
  - `unit_ci_lock_contention_sensitive_jobs_share_the_machine_lock`：持锁的作业正好是 ui、e2e、perf。
- [x] **遵守 `CLAUDE.md` 的约束，没有 `unsafe`。** 工作区设了 `unsafe_code = deny`。xtask 只在 `clock.rs` 一处用 `#![expect(clippy::disallowed_methods, reason=…)]` 读系统时间。
- [x] **CI 摘要**：`docs/tasks/T001-repo-skeleton/ci/pr-5c75b73117f1.json`，由 `--record` 生成。人工检查过，里面没有路径、用户名和邮箱。
- **其他检查**：
  - 用临时克隆加一个 probe 包，验证了 ts 作业"有 TS 包"的分支（tsc 和 Vitest 都跑到并通过），没有提交。
  - `tsconfig.base.json` 用 tsc 6.0.3 编译通过。
  - ESLint 确认能对 TS 代码报规则错误。
  - 每个提交单独构建、测试都通过。

## 提交列表

```
8f0a83d docs(task): record the T001 pr CI summary
5c75b73 fix(xtask): run every test binary and keep rustfmt output plain in the rust job
18a2b7c docs(task): add T001 working notes
8bb1e31 ci: add dormant GitHub workflows that call cargo xtask ci
00d7e3c feat(xtask): add the ts job
799bc8e build(ts): set up the pnpm workspace with base TypeScript, ESLint and Prettier config
7e20e55 feat(xtask): record CI summaries behind a field allowlist and privacy check
e799282 feat(xtask): add the cargo xtask ci runner with clean-tree check and machine lock
3a0c631 test(xtask): prove clippy rejects every disallowed time and id method
30bc644 build(repo): set up the Rust workspace, toolchain pin and lint config
```

本分支建立后 `main` 又有新提交，所以上面是从分叉点（51bcc2b）算起的列表；`git log main..HEAD` 的结果相同。

## 遗留问题 / 范围外发现

1. **需要编排者决定：ADR-11 白名单缺 `Unicode-3.0`。**
   - `unicode-ident` 的许可证是 "(MIT OR Apache-2.0) AND Unicode-3.0"，所有 derive 宏都会用到它，例如 serde_derive 和 thiserror。
   - T010 起一引入 serde derive 或 thiserror，`cargo deny` 就会失败。
   - 两个办法：在 ADR-11 白名单里加 `Unicode-3.0`；或者在 `deny.toml` 里只给 `unicode-ident` 开例外。
   - 另外，`Apache-2.0 WITH LLVM-exception` 也会很快遇到。
2. **设计前后矛盾。**
   - 5.1.3 说 xtask 依赖 cupper-cli 等 macOS 专用的 crate，只在 macOS 上编译。
   - 5.1.6 的 ts、license、protocol、testdata-scan 作业放在 ubuntu 上，而 workflow 只调用 `cargo xtask ci`。
   - 目前 xtask 没有平台专用的依赖，在 Linux 上也能编译。以后加入 macOS 专用依赖时，需要用 cfg 隔离，或者把这些作业改到 macOS runner。
3. **原始日志目录名不一致**：计划 2.1 的备注写的是 `.private/ci/`，任务书写的是 `.private/ci-logs/`。我按任务书实现，建议把计划同步一下。
4. **workflow 没有实际跑过**（仓库还没有远端）。以下几点要在第一次推送时确认：
   - setup-node v7 的 `package-manager-cache` 输入；
   - 用 `npm install -g` 安装 pnpm 12；
   - ubuntu 上按 `rust-toolchain.toml` 安装 iOS 模拟器 target。
5. **交给建第一个 TS 包的任务（T015 / T070）：**
   - `tsc --build` 会把测试文件编译进 `dist/`，而 Vitest 4 默认不排除 `dist`，同一个用例会跑两遍。需要在 Vitest 配置里排除 `dist`，或者让测试文件不进 tsc 的产出。
   - Vitest 目前整步算一项，没有逐用例的编号结果。需要用 JSON reporter 接入摘要。
6. **交给 T002**：逐编号结果目前解析的是 `cargo test` 的 libtest 输出。T002 改用 nextest 的 JUnit 报告时，替换 `ctx.rs` 里的 `parse_libtest` 就行。
7. **版本备注**：`.node-version` 用的是本机的 24.13.0，随 App 带的 Node 运行时是 24.22.0。作者升级本机 Node 时可以把两者对齐。


## 第 1 轮审阅后的补改（2026-10-19）

按第 1 轮审阅意见改完，`cargo xtask ci pr --record` 重跑通过（rust 15、ts 3、license 1 项），新摘要 `docs/tasks/T001-repo-skeleton/ci/pr-ae9caed1ee1c.json` 已提交，提交依次为：b7b7491（`deny.toml` 加 `Unicode-3.0` 和 `Apache-2.0 WITH LLVM-exception`，附注释）、ae9caed、13ef4d5（新摘要）。其中 ae9caed 删掉了旧提交 5c75b73 的那份摘要，这一步你没有要求，不需要的话可以去掉，旧摘要就会和新摘要并存。
