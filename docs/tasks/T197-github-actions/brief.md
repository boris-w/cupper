# T197 启用 GitHub Actions

- 派发日期：2026-10-26
- 执行者：子 agent（工程师）。T093、T189 的执行者接着做（机器锁、e2e 的两阶段、`@focus` 的前提都是它做的）
- 分支：`ci/T197-github-actions`
- 依赖：T001、T093、T189（都已合并）。T099 依赖本任务

## 背景

用户决定建 GitHub 公开仓库 `boris-w/cupper`，CI 放到 GitHub Actions 上跑，每个任务一个 PR（`docs/meetings/2026-10-26-github-remote/minutes.md`，计划 v1.3.65）。仓库已经建好，main 和阶段 tag 已推送。Actions 目前是**关闭**的：现有的 5 个 workflow 是 T001 写的"休眠"版本，没配好依赖。本任务把它们配到能在 GitHub 上跑通，之后编排者打开 Actions、给 main 设保护规则，PR 以 GitHub CI 为准。

公开仓库的标准 runner 免费、不限量。

## 范围

**做**（设计 5.1.6 的作业表；计划 1.8）：
1. **`pr` 组合里的每个作业一个 workflow**，都只调用 `cargo xtask ci <作业>`，runner 照设计 5.1.6 的作业表：
   - `rust`、`ui`、`e2e`、`compat-framework` 在 `macos-15`；`protocol`、`ts`、`license`、`testdata-scan` 在 ubuntu。设计写 ubuntu 的作业要是在 ubuntu 上跑不通，说明原因，改用 macOS；
   - 触发：`pull_request` 和推送到 main。不用 `pull_request_target`。`permissions: contents: read`；
   - 作业内部已经有的按路径跳过（ui、compat-framework、e2e 冒烟）照旧由 xtask 判断，workflow 不另写 `paths` 过滤，免得两处规则不一致。xtask 判断时要和 main 比较，checkout 的深度要够。
2. **工具链与缓存**：
   - Rust 照 `rust-toolchain.toml`；Node 照 `.node-version`；pnpm 照 `package.json` 的 `packageManager`；
   - cargo 的注册表与 target、pnpm store、`runtime-pack` 与 git-lfs 的下载缓存；
   - 所有 action 都锁定到提交 SHA，注释写版本号（照现有写法）。编排者打开 Actions 时会一并开启"必须锁定到 SHA"。
3. **e2e 在 runner 上**：
   - `@focus` 两阶段照旧；`setup.ts` 对锁屏和睡眠显示器的检查在 runner 上要能通过；
   - workers 现在固定为 2，是按作者的机器定的，按 runner 的核数重新定（T090 的交接），并说明理由；
   - 失败时上传 Playwright 的结果和截图作为构件。
4. **摘要**：`--record` 的结构化摘要作为构件上传；原始日志也作为构件，保留期设短一些。CI 上的摘要不再提交进仓库，卷宗里改为记 PR 和运行的链接。
5. **每晚**：定时运行 nightly 里已经实现的作业：`differential`；`security-macos14` 在 `macos-14` 和 `macos-15` 上各跑一次，这样能补上 R7 的缺口 IT-sys-peer-macos14、IT-sys-peer-macos15；e2e 全集；`compat-framework`。
   - `perf` 先不放上 CI：runner 不是参考机型，历史也要写回仓库，在汇报里给出建议；
   - `release-config`、`codex-drift` 还没实现，不放。
6. **`workflow_dispatch` 跑单个用例 N 次**：参数是 spec、用例名、次数，一次调用跑完（`--repeat-each`）。用它补完 T189 的 E2E-SEC-real-input 50 次连跑，结果写进 `notes.md`，并附运行链接。
7. **`ui` 的截图基线**：设计 5.1.6 写的是"基线只由 CI 的专用工作流生成"。现在的基线是在作者机器上生成的，macOS 大版本和 runner 的不同。照设计加一个手动触发的基线生成工作流，把产出作为构件；基线入库仍要经 PR。runner 上的比对失败时，先查是不是基线环境不同造成的，在汇报里说明。
8. **计划 1.8 的改写建议**：哪些行在有远端后要改，写进 `notes.md`，由编排者改计划。

**不做**：签名、发布流水线（T180、T181）；`perf` 上 CI；branch protection 与仓库设置，由编排者做。

## 怎样在 GitHub 上试

子 agent 不 push（`CLAUDE.md`）。要在 GitHub 上试 workflow 时：
1. 先在本机提交；
2. 用 SendMessage 发给 `main`："请推送 `ci/T197-github-actions`，提交 `<sha>`"；
3. 编排者推送，第一次时开一个草稿 PR 并打开 Actions，然后回复你。

你可以用只读的 `gh` 命令看结果：`gh run list`、`gh run view --log-failed`、`gh pr checks`、`gh run download`。**不要**用 `gh` 改任何东西：不 push、不开或合并 PR、不改仓库设置、不用 `gh api -X` 写入。

每次推送都会在公开仓库里跑一整轮，所以先在本机把能查的查清楚（`actionlint` 之类的静态检查可以用），再请求推送；一次请求尽量把能改的都改了。

## 验收标准

- [ ] 草稿 PR 上，`pr` 组合的全部作业都在 GitHub 上通过，并附运行链接。
- [ ] 推送到 main 时同样的作业能运行（编排者合并后核对）。
- [ ] 夜间 workflow 至少手动触发跑通一次，`security-macos14` 在两个 macOS 版本上都跑过。
- [ ] `workflow_dispatch` 跑完 E2E-SEC-real-input 50 次，没有失败。
- [ ] 所有 action 都锁定到提交 SHA；没有 `pull_request_target`；权限只读。
- [ ] 本机的 `cargo xtask ci pr` 照旧能跑，不因本任务的改动而变。

## 交接事项（`handoff-notes.md` 第一列含 T197 的行）

- T189：E2E-SEC-real-input 50 次连跑的做法；所有 `@focus` 用例都要求运行期间没人操作。
- T090：workers 按 runner 的核数重新定，在 CI 上重跑一轮 20 次。
- T001：workflow 的写法（现有 5 个文件）；`xtask ci` 运行前检查 git 状态干净。
- T006：license 作业取 RustSec 数据库失败时重试并回退到缓存。
- T188：compat-framework 在 `ci pr` 里按路径判断，要和 main 比较。

## 注意

- workflow 里不放任何密钥，也不需要。
- runner 上的日志是公开的：不要让作业打印本机路径以外的个人信息；摘要的字段白名单照旧。

## 规模

- 停止线：600；硬停止线：700

<!-- 执行者开工前在 `notes.md` 里按模块写行数估计。每写完一个模块就跑一次 `cargo xtask size`，不要等全部写完；推算或实际越过停止线就停下汇报。 -->

## 工作方式

在原来的 T004 worktree 里从最新的 main 开新分支：`git checkout -b ci/T197-github-actions main`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
