# T160 汇报

- 执行者：子 agent（构建与桌面工程师）
- 分支：`build/T160-selfuse-build`（PR #40：https://github.com/boris-w/cupper/pull/40 ，已推送到 b767fd4）
- 完成日期：2026-11-03。**还没收尾**：交回时两个运行都没跑完，没有任何结果可以报。这份汇报是在被要求立即交回时写的。

## 运行状态（交回时）

- **手动触发的 nightly，只跑 selfuse**：https://github.com/boris-w/cupper/actions/runs/41581457853
  - `selfuse (macos-15)` 还在排队；同一运行里其他没被选中的作业会空跑并显示为成功。
  - 三个用例 IT-selfuse-build、E2E-selfuse-coexist、E2E-selfuse-replace **都还没在 runner 上跑过**，通没通过不知道。
- **PR 的 ci**：https://github.com/boris-w/cupper/actions/runs/41581412488
  - protocol、ts、license、testdata-scan 已通过；
  - rust、e2e、ui 在跑；compat-framework 在排队。

这些用例还从没在真实的 Cupper Local 上跑过，第一次运行可能失败（风险最大的几处见"关键取舍"末尾和遗留第 7 条）。

**还差的验收项**：selfuse 在 runner 上通过；PR 的 8 个检查全绿。如果 selfuse 失败，需要我（或接手的人）看 `selfuse-macos-15-logs` 构件，修复后再推送。构件里有步骤日志，还有两个 App 的 `main.log`、`host.log`（文件名为 `selfuse-cupper-local.logs`、`selfuse-cupper-dev.logs`）。修复要控制在规模余量内，见"规模"。

## 做了什么

1. **构建配置**
   - `apps/desktop/electron-builder.release.yml`：发布构建的基础。
     - 5.7.8 的 fuses 全部烧录；开 hardened runtime；entitlements 只有 `allow-jit`。
     - 宿主和 CLI 放进 `Helpers/`，用 `signIgnore` 保留 `host-bundle` 给的签名；注册 `cupper://`。
     - Developer ID 签名、公证、发布留给 T180；没有签名身份时 electron-builder 产出不签名的 App，正好给 T161 用。
   - `apps/desktop/electron-builder.local.yml`：自用构建 Cupper Local。
     - 身份：`dev.cupper.app.local`，`cupper-local://`，显示名称 Cupper Local。
     - 宿主和 CLI 来自 `host-bundle --kind self-use`。
     - ad-hoc 签名，hardened runtime 开着；`entitlements` 和 `entitlementsInherit` 都是 `allow-jit` 加 `disable-library-validation`。
     - fuses 同发布构建，只关 `enableCookieEncryption`。
   - 两份 entitlements 在 `apps/desktop/build/`。
   - **构建入口**是 `pnpm build:local`（根目录和 apps/desktop 都有）。前提是先跑过一次 `cargo xtask runtime-pack`。
   - **Vite 的 mode**：Vite 不许用 `local` 当 mode 名（会和 `.env.local` 冲突），所以自用构建的 mode 是 `self-use`，在 `vite.config.ts` 里映射到 flavor `local`。`flavors.test.ts` 的测试入口扫描也加上了 `self-use`。
2. **宿主的 worktree 根目录**（5.7.9、5.4.1 要求每种构建一个）：以前所有构建都默认 `~/.cupper/worktrees`。自用构建和开发构建的宿主编译特性完全相同，宿主分不出自己是哪一种，所以改成按数据目录推出来，规则与 `log_dir` 相同：
   - `cupper-local` → `~/.cupper-local/worktrees`；
   - `cupper-dev` → `~/.cupper-dev/worktrees`；
   - 其他数据目录（测试用的临时目录）→ `<数据目录>/worktrees`。
   - 补了单元测试。
3. **三个用例**（`apps/desktop/selfuse/`，配置 `vitest.selfuse.config.ts`，脚本 `test:selfuse`）。自用构建关掉了 Node 调试开关这个 fuse，Playwright 驱动不了它，所以用例直接启动 App 的可执行文件，再看 `main.log`、`host.lock`、`host.log`、进程表、codesign 和 CLI 的输出。
   - **IT-selfuse-build**：
     - Info.plist 的身份、URL scheme、`LSMinimumSystemVersion` 15.0、`ElectronAsarIntegrity`，以及没有 `NSAppleEventsUsageDescription`；
     - 逐个 Mach-O 检查：都是 ad-hoc 加 runtime；可执行文件的 entitlements 按白名单（helper 在内，宿主、CLI 为空，Node 只有 `allow-jit`），库一律为空；主程序、4 个 helper、宿主、CLI、Node 的 identifier；没有通用二进制；
     - 用 `@electron/fuses` 读出 fuse 线逐项断言；
     - asar 里没有测试入口的标记串；
     - `--build-info`：宿主只有 `dev-auth`，CLI 是 `self_use`，CLI 内嵌的宿主 cdhash 等于产物里宿主的 cdhash；原生扩展是 `key_proof`。
   - **E2E-selfuse-coexist**：
     - 与开发构建同时运行，各有自己的宿主、`host.lock`、数据目录和日志目录；
     - 主程序、Renderer、GPU、base 三个 helper 和宿主都起来了；
     - control 连接和窗口连接都就绪，说明原生扩展加载成功；
     - 开发构建退出后，Cupper Local 的宿主还是原来那个，CLI 的 `status` 照常返回。
   - **E2E-selfuse-replace**：
     - 把 App 装到临时目录并运行，再按 Finder 的方式（先删后复制）换成"下一版"；
     - 新版的 CLI 拒绝旧宿主（退出码 4）；旧宿主写下 `host.pending_upgrade`，以 reason `upgrade` 有序退出；
     - 退出旧 App 后打开新 App，跑的是新宿主的 cdhash。
   - **防护**：不在 GitHub 托管的 runner 上时，`built()` 直接抛错，不启动任何东西。
4. **CI**
   - 新增 xtask 作业 `selfuse`，已加进 nightly 组合和 `nightly.yml`（macos-15，超时 60 分钟）。
   - 作业依次构建：Cupper Local；"下一版"（`host-bundle --kind self-use --debug` 后用 electron-builder 输出到 `dist/pack/local-next`）；开发构建。然后跑用例，把 App 的摘要值记进 CI 摘要，把两个 App 的日志复制到构件里。
   - 不在托管 runner 上时整个作业跳过。原因：作者机器上的 `cupper-local` 就是日常在用的那份数据。
   - `nightly.yml` 加了可选输入 `job`，手动触发时可以只跑一个作业；定时运行不受影响。
   - `e2e.rs` 抽出 `prepare` 和 `digest` 两个函数给新作业复用。
5. **用户文档**（交给你转给用户）
   - "自用前须知"：`docs/user/selfuse-notice.md`。
   - 安装与手动更新：`docs/user/selfuse-install.md`。
   - 须知覆盖计划 T160 注里的全部要点。Codex 部分只留了一句，按任务书交给 T157。

## 关键取舍

- **两份打包配置各写完整**：electron-builder 的 `extends` 会把数组拼起来，继承之后两个宿主路径、两个 URL scheme 都会进产物。自用配置的文件头列出了它与发布配置的差异，产物由 IT-selfuse-build 校验。
- **entitlements 白名单分"可执行文件 / 库"**：在本机已有的 e2e 产物上只读核对过，codesign 只把 entitlements 嵌进 MH_EXECUTE，framework、dylib 和原生扩展都没有。
- **"下一版"用 dev profile 编译宿主和 CLI**：这样它的 cdhash 和正式那版不同，又不用为测试新加任何构建参数。代价是 runner 上多一次 debug 编译。
- **本机只做了不启动 App 的事**：
  - 编译、clippy、fmt，`cargo test -p xtask`，宿主 settings 的单元测试，`apps/desktop` 的 vitest，tsc、eslint、prettier、electron-builder 配置的 schema 校验；
  - 不设 `RUNNER_ENVIRONMENT` 跑 `test:selfuse`：防护生效，什么都没启动；
  - 把 IT-selfuse-build 指向本机旧的 e2e 产物：5 条断言都按预期失败，说明每条检查都看得见它要找的东西。
  - 本机没有构建、安装或启动过 Cupper Local。
- **第一次在 runner 上最可能出问题的几处**：
  - 是否每个 Mach-O 的签名标志都正好是 `adhoc,runtime`；
  - 冷启动时宿主能否在启动器的 10 秒预算内起来；
  - E2E-selfuse-replace 里，旧宿主退出后旧界面会从磁盘拉起新宿主（遗留第 7 条），这会不会让旧 App 退出得不干净。

## 验证方式（对照验收标准）

- [ ] 自用构建在 runner 上构建出来，三个用例在 runner 上通过：**还没结果**，nightly 41581457853 的 selfuse 在排队。
- [x] "自用前须知"和安装说明写好：见上面两个路径。
- [x] 设计、计划要补的地方：见遗留第 1–3 条。
- [ ] PR 的 8 个检查：**还没跑完**（ci 41581412488），已通过 4 个。
- **规模**：`cargo xtask size` 为 +740 −67，离停止线 800 还剩 60 行。
  - 其中测试 424 行，实现 316 行，比估计的 705 多 5%。
  - runner 上的修复要很省，超过 60 行就会越过停止线。

## 提交列表

```
b767fd4 docs(task): record T160 findings and sizes so far
c74099a docs(user): what to know before using Cupper Local, and how to install it
23dea21 ci: build and check Cupper Local every night, on GitHub-hosted runners
16ac69e test(desktop): check Cupper Local as built: IT-selfuse-build and its two E2E tests
9a6d805 build(desktop): package Cupper Local, the self-use build, beside a release config
88a8e97 feat(host): default the worktree root by the build kind's data directory
2dd7ec2 docs(task): estimate T160 by module before starting
```

## 遗留问题 / 范围外发现

1. **5.1.5 的命令列表建议补两行**：
   - `pnpm build:local`：构建 Cupper Local，前提是先跑 `cargo xtask runtime-pack`，产物在 `apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app`；
   - `pnpm --dir apps/desktop run test:selfuse`：只经 `cargo xtask ci selfuse` 在托管 runner 上运行。
2. **5.1.6 和 5.1.2 建议同步**：
   - CI 表加 `selfuse` 作业（macos-15，每晚，失败开 issue），并写上 `nightly.yml` 的 `job` 输入；
   - 5.1.2 里 electron-builder 配置现在有 release、local、e2e、dev 四份，另有 `apps/desktop/selfuse/`。
3. **5.7.8、5.7.9、5.10 的出入**：
   - Electron 44 的 fuse 线有 9 根。第 9 根（Electron 的 `WasmTrapHandlers`）设计没列，@electron/fuses 1.8.0 和 electron-builder 26.17 都设不了，出厂是开启。用例按"保持出厂"断言它，建议 5.7.8 写明，T161 照此断言。
   - 5.7.8 的 Fuses 一行还写着"`asarUnpack` 只有 `cupper_native.node`"，与 5.1.6、5.10.7 的"为空"矛盾。
   - 5.7.9 可以注明自用构建的 Vite mode 名是 `self-use`。
   - 5.10.2 可以注明：库不带 entitlements；`chrome_crashpad_handler`、`app_shim_loader`、`ShipIt` 这几个可执行文件拿的是 `entitlementsInherit`。
   - 5.10.1 的 `LSMinimumSystemVersion 14.0` 已经过时（T211 也提过）。
4. **Electron 自带的用途说明**：Info.plist 里有摄像头、麦克风、蓝牙（两项）、音频采集的用途说明。agent 触发这类授权提示时会显示成 Cupper，须知里已经写了"拒绝"。是否在打包时删掉它们（需要 `afterPack` 钩子），建议交 T161、T180 或安全工程师决定。
5. **P-4 的现状**：迁移框架本身满足需要（脚本按序执行、每个脚本一个事务、崩溃后续跑、备份、太新时拒绝；IT-migrations 能注入测试脚本），但"T160 之后每次改表结构都附样例数据库和迁移用例"还缺下面几样，没有在本任务补：
   - (a) 没有按版本存放的样例数据库，例如 `testdata/` 下 v1 的 SQL 转储；
   - (b) 没有通用用例：把每个样例库迁到最新，检查 `schema_version`、`integrity_check`、`foreign_key_check`、数据保留，再让宿主在上面启动；
   - (c) 没有防止原地修改 `0001_initial.sql` 的检查。它至今已被原地改过很多次；T160 合并、用户开始用 Cupper Local 后，再原地改就会破坏用户的真实数据。建议对已发布的脚本固定哈希；
   - (d) `migrate.rs` 的注释"Until T160 … changes in place"要改；
   - (e) 需要一个新编号，例如 IT-migrations-samples。
6. **自用构建的 CLI 安装**：设置页的"安装命令行工具"还没做，安装文档里暂时用 `ln -s`。
7. **手动替换后的版本混用**：E2E-selfuse-replace 的流程里，旧宿主因待升级退出后，旧界面的 control 连接会立刻从磁盘拉起新宿主，出现旧界面配新宿主。安装文档要求先退出再替换，也写了忘记退出时该怎么办。设计 5.5.7 只写了发布构建的路径，建议考虑：收到 `host/shutdown(upgrade)` 时，界面不再自动拉起宿主，而是提示重启 App。
8. **其他提交**：worktree 根目录的改动在 `crates/cupper-host`，是按 5.4.1 的表补齐，没有改协议。用户文档放在新目录 `docs/user/`，T157 可以接着写在这里。
