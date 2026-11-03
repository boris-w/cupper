# T160 自用构建

- 派发日期：2026-11-03
- 执行者：子 agent（构建与桌面工程师）
- 分支：`build/T160-selfuse-build`
- 依赖：T099（M1a 已退出，`gate/m1a`）

## 背景

M1b 的第一波里，T160 交付作者日常使用的本机构建"Cupper Local"：在真实项目上运行真实 agent。T160 交付之前，不用开发构建在真实项目上运行真实 agent（P-15）；交付之后，真实项目只用自用构建，开发构建和测试只用临时目录或 `cupper-dev/`。

自用构建的种类已经定好（计划 8.1 的 P-5，设计 5.7.9 的"自用构建"一列）：
- 单独一套身份和数据目录：`dev.cupper.app.local`、`dev.cupper.app.local.host`、`dev.cupper.cli.local`、`cupper-local/`（userData 在其下的 `ui/`）、`~/.cupper-local/worktrees/`、`cupper-local://`，显示名称 Cupper Local。
- 宿主开 `dev-auth`、不开 `test-harness`，界面提示"本地鉴权较弱"。
- 打包和 fuses 同发布构建，只关闭 `enableCookieEncryption`。
- ad-hoc 签名并**开启 hardened runtime**；entitlements 在发布构建的基础上只多 `com.apple.security.cs.disable-library-validation`，同时写进 `entitlements` 和 `entitlementsInherit`，不带 `get-task-allow`、`allow-dyld-environment-variables`。
- 没有测试钩子（`__CUPPER_FLAVOR__` 门控）；没有自动更新，手动替换 App，旧宿主经自检进入"待升级"（5.5.7）。

计划第 3 节 T160 的注写了"自用前须知"要包含的内容和剩余风险。T160 的合并要把配置、签名和它的校验一起带上，否则 main 上会有一个没经校验、却每天在真实项目上使用的构建。

## 范围

**做**（计划 T160 一行和注，5.7.9，5.12.3，5.10.1）：
1. **构建配置**：`apps/desktop/electron-builder.release.yml`（发布构建的基础）和自用构建的配置，按 5.7.9 的自用构建一列；产物里的宿主、CLI、原生扩展、Node 运行时都用自用构建的身份。构建入口（例如 `cargo xtask` 或 `pnpm build:local`）由你定，写进 5.1.5 的命令列表建议里（汇报中给出，由编排者改设计）。
2. **用例**（编号已登记，挂在 T160 下）：
   - **IT-selfuse-build**：逐个 Mach-O 比对 entitlements 白名单（helper 也在内）；`@electron/fuses read` 逐项断言；产物里没有测试钩子的标记串；`--build-info` 显示自用构建；CLI 内嵌的宿主 cdhash 与产物里的宿主相符。
   - **E2E-selfuse-coexist**：在 T160 产出的那份 App 上运行：与开发构建同时运行互不干扰（各自的 `host.lock`、数据目录），全部 helper 都能起来，原生扩展能加载。
   - **E2E-selfuse-replace**：手动替换 App 时，旧宿主进入"待升级"。
   - 这三个用例都在 GitHub 的 runner 上跑（nightly，必要时加进合适的 workflow），**不在作者机器上跑**。
3. **文档**（放在 `docs/` 下合适的位置，由你提议，例如 `docs/selfuse.md`）：
   - 本机安装与手动更新说明：怎么构建、装到哪里、怎么替换、第一次打开时 macOS 会问什么。
   - 一页"自用前须知"，内容按计划 T160 的注：退化认证意味着什么；不要因为 agent 的操作给 Cupper 授予辅助功能或自动化权限；开启 agent 自带的沙箱；在 cupper 里提交会运行仓库的 hooks；剩余风险（`disable-library-validation`、每次替换 App 要重新确认 TCC 授权、两者的根本解决是 Q35）；关闭 `enableCookieEncryption` 的原因。Codex 部分留给 T157。
4. **P-4**：T160 之后每次改表结构都要附样例数据库和迁移用例。确认迁移框架和测试已经能满足这一点，汇报里说明；缺的部分列出来，不在本任务补。

**不做**：Developer ID 签名、公证、自动更新（推迟，T180）；Q35 的本机签名认证（T105 / T162）；`release-config` 每晚构建与产物扫描（T161）。

## 验收标准

- [ ] 自用构建能在 runner 上构建出来，三个用例在 runner 上通过，附运行链接。
- [ ] "自用前须知"和安装说明写好，交编排者转给用户。
- [ ] 设计、计划要补的地方（构建命令、5.1.5、5.7.9 若有出入）列进汇报。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：800；硬停止线：1000

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.7.9（构建种类）、5.7.7 的 Fuses 一行、5.5.7（升级）、5.10.1、5.12.3、5.1.5（命令）、ADR-05（退化认证）、ADR-09（Node 运行时）。
- 计划：T160 一行和第 3 节的注、8.1 的 P-4、P-5、P-15、1.10。
- 卷宗：T005（`xtask host-bundle`、先签宿主再编译 CLI）、T075（dev、e2e 构建的打包配置）、T105（本机签名的技术验证，进行中）、T211（最低系统版本 15.0）。

## 注意

- 用户决定不在自己的电脑上跑压测或长时间的测试（`docs/meetings/2026-11-02-perf-local/`）。构建和三个用例都放在 runner 上；本机只做普通的编译、单元测试和 lint。本机不要安装或启动自用构建：用户会按你写的说明自己装。
- 不运行真实的 Claude Code、Codex CLI；不读也不改 `~/.claude`、`~/.codex`。
- `gh` 只用只读命令；要推送、触发 workflow 时告诉 `main`。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
