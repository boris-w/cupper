# P02 第 2 轮：落实评审者的确认条件（03-plan.md v0.3）

- 派发日期：2026-10-19
- 执行者：子 agent（架构师，兼任规划；P01、P02 的执行者）
- 分支：`docs/P02-plan-revision-2`
- 依赖：P02 第 1 轮（已合并，`21c2378`）；设计 v1.3（`65d0394`）

## 背景

三位评审者已确认 v0.2：
- 测试工程师：两条阻断都可关闭；
- 桌面客户端工程师：12 条可关闭，4 条附条件关闭；
- 安全工程师：部分接受的三条都接受。

三人都没有分歧，但都附了条件。编排者已经把其中涉及设计的部分写进设计 v1.3：
- 5.8.1：`dev_key` 只接受拉起宿主的那个进程；按构建种类分别规定 UI 和 CLI 怎样校验宿主；
- 本机 CI 的措辞、用例登记、四种构建的目录清单；
- 更正 Q35。

本轮把其余条件落实到 `docs/03-plan.md`，升到 v0.3。

## 必读

- 三份确认：`docs/meetings/2026-10-16-plan-review/test-engineer-confirm.md`、`desktop-engineer-confirm.md`、`security-engineer-confirm.md`；
- 设计 v1.3 的改动：`git show 65d0394`，重点是 5.8.1 和 13-open-issues 的 Q35。

## 要落实的条件

### 测试

1. T063、T068、T034 补上依赖 T037；也可以把 IT-project-add-trust 改成 crate 内的用例。
2. 本机跑的 `security-macos14` 实际运行在作者的 macOS 26 上。摘要要记下系统版本；trace-ids 不能把它算作 macOS 14 的覆盖。IT-sys-peer 在 macOS 14、15 上保持"缺口"。
3. 入库的摘要里加一份逐编号的结果清单，包括编号、套件、结果和提交，里程碑退出时可以直接从仓库复核。
4. e2e、`ui`、性能、nfr1 这几类作业加一把机器级的锁，串行运行。T090 写出 Playwright 并行度的具体数字。
5. 截图元数据：Chromium（Electron）的版本严格比对；macOS 只比对大版本。小版本变了只给提示，出现像素差异时才重新生成基线。
6. 写清"干净 worktree"的含义：
   - git 状态干净，构建缓存可以共享；
   - e2e 用的 App 和 `CupperHost.app` 必须由当前提交构建，摘要记下产物哈希；
   - 性能趋势的"历史中位数"要用入库的历史数值序列（脱敏）。

### 桌面

7. B3 里先合并 T152。同波的 T130、T145、T151 合并前，要在 T152 的 token 取值下重新生成各自新增的截图基线。
8. M1b 的所有界面任务都依赖 T106（视觉规格），包括 T141、T143、T150 的界面部分，以及 T140、T142、T146 等。依赖图和波次要相应调整。
9. **P-20（编排者裁定）**：T106 由子 agent 执行。brief 会写明主仓库里参考图的路径（`.private/references/`），子 agent 只读不改。产出是文字规格：尺寸、间距、字号、圆角、颜色、各区域的结构。参考图不复制进 worktree，不入库，规格里也不嵌入图像。
10. 1.11 中复刻度审阅用的截图，在 e2e 构建里用系统字体（SF Pro、苹方）截取，不用开源字体的回归基线；写进 1.11 和界面任务的验收说明。
11. **T160**：
    - `disable-library-validation` 同时写进 `entitlements` 和 `entitlementsInherit`；IT-selfuse-build 逐个 Mach-O 比对 entitlements 白名单，helper 也在内。
    - **编排者裁定**：自用构建关闭 `enableCookieEncryption`。理由：界面不使用 cookie（`connect-src 'none'`），而 ad-hoc 签名下每次替换 App 都会触发钥匙串提示。这一项写进 5.7.9 的 Fuses 一行，本轮授权你改这一格。
    - E2E-selfuse-coexist 在 T160 产出的那份 App 上运行，确认所有 helper 都能起来、原生扩展能加载。
12. 剩余风险写进 T160 的"自用前须知"和 T157：
    - 开了 `disable-library-validation`，包内的库被替换后仍会被加载；
    - 每次手动替换 App，TCC 授权都要重新确认；
    - 两者的根本解决都是 Q35。

### 安全

13. T092 的 E2E-SEC-no-key-client 加一条断言：出示 `auth: dev_key` 但证明错误或缺失时，返回 `unauthenticated` 并断开，不会降级成只读。
14. P-14 的细节写进 T102：
    - 备份覆盖 `~/.codex/config.toml` 和 `hooks.json`，放在仓库外，权限 0600；
    - 差异报告单独列出新增的 `[projects]` 信任项；
    - spike 只在临时仓库里运行，不读写 `auth.json`。
15. P-19（设计 v1.3 的 5.8.1）：
    - T021 实测 `PROC_PIDUNIQIDENTIFIERINFO`；
    - T030 实现"只接受拉起宿主的那个进程"，加上启动时 ppid 为 1 就退出；
    - "孤立"状态放进 T037 或 T030，选一个写明；
    - T030、T033、T073 的集成测试里，`dev_key` 都由拉起宿主的测试进程自己出示；
    - 新增相应的用例编号。
16. CLI 校验宿主（设计 v1.3 的 5.8.1 表格）：
    - T005：构建顺序是先签宿主、取 cdhash，再编译并签名 CLI；
    - T045：CLI 按构建种类校验宿主，并加 UNIT 用例；
    - T092：新增 E2E-SEC-cli-fake-host；
    - 可选：T038 的确认框醒目标出"申请方不是本 App 自带的命令行工具"。
17. 更正 7.3 中 K7 不通过那一行，以及 T147、8.1 的 P-6：退化模式下 CLI 照常按 uid 只读接入，`install_cli` 不隐藏。K7 不通过的实际影响只有两点：NFR-8 不成立，以及 TCC 授权每次重新确认。Q35 收益的写法与设计 v1.3 保持一致。

## 范围与要求

- 修订 `docs/03-plan.md` 到 v0.3：
  - 版本、日期（2026-10-19）、修订记录；
  - 任务表、依赖图、波次和进度清单保持一致；
  - 新增用例写进覆盖核对；
  - 第 8 节更新 P-16 到 P-20 的状态：P-16 到 P-19 已由设计 v1.3 处理，P-20 已裁定。
- 只授权改设计的一处：5.7.9 自用构建一列的 Fuses 格（第 11 条）。其余设计、PRD、ADR、`CLAUDE.md`、`00-process.md` 都不改。
- 在 `docs/tasks/P02-plan-revision/responses.md` 末尾加一节"第 2 轮"，逐条写出上面 17 条的落点。
- 只在指定的 worktree 里工作，不执行 merge、rebase、push；提交带 `Refs: P02`；不运行真实的 CLI；不改用户配置；外发请求不带个人信息。
- 跑一遍校验：mermaid、链接与锚点、环检查、四处一致、个人信息。

## 汇报

按 `docs/templates/task-report.md` 的结构返回简短汇报（不超过 60 行）。编排者会把它追加到 `report.md`。
