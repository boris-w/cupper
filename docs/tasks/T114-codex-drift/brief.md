# T114 codex-drift 检查

- 派发日期：2026-10-28
- 执行者：子 agent（工程师）
- 分支：`ci/T114-codex-drift`
- 依赖：T004（已合并；可以提前，计划 1.7）。M1b 的 T179 依赖本任务

## 背景

codex-acp 经 `codex app-server` 驱动 Codex。codex CLI 发布很频繁，新版本可能改掉 codex-acp 依赖的 app-server 方法，而 codex-acp 锁定在 2.2.2（ADR-08）。设计 5.1.6 的 `codex-drift` 作业就是来发现这种漂移的：每天检查有没有新的 codex 版本，有的话，比对 codex-acp 实际引用到的 app-server 方法有没有变。发现变化时，兼容矩阵要立即重跑（5.9.5）。

SP-2 已经写了原型脚本 `spikes/SP-2-adapter-distribution/codex-schema-drift.mjs`：
- 对每个 codex 二进制运行 `app-server generate-json-schema`，带和不带 `--experimental` 各一次；
- 只比较适配器里以字符串字面量出现的方法和通知；
- 报告每个版本里这些方法在不在、是 stable 还是 experimental、参数 schema 有没有变。

T102 刚发现 codex CLI 0.165.0 已经超出已验证上限 0.163.0，codex-acp 2.3.0 也发布了。

## 范围

**做**（计划 T114 一行；设计 5.1.6、ADR-08）：
1. **把原型迁移成 `tools/codex-drift`**，照仓库现有 TS 工具的惯例（`tools/` 下其他工具的写法，包的 `test` 跑 Vitest）。
   - 适配器取锁定的 codex-acp：从 `runtime/manifest.json` 记录的产物来，核对 integrity；
   - codex 二进制从 npm 取：`@openai/codex` 对应平台的包，同样核对 integrity；
   - 只运行 `app-server generate-json-schema`，不登录、不发 prompt、不读写 `~/.codex`。
2. **基线入库**：在 `testdata/` 下放一份基线，内容是锁定的 codex-acp 引用到的方法，以及它们在已验证上限那个 codex 版本里的 schema 摘要，并记下基线对应的版本。
   - 检查时取 npm 上最新的 codex 版本，和基线比；没有新版本就直接通过；
   - 有新版本时，比对这些方法：消失了、从 stable 变成 experimental、参数 schema 变了，都判失败，并列出变化的方法和定义；
   - 更新基线用一条显式的命令（例如 `--write`），经 PR 入库。
3. **接进 `xtask ci nightly`**：作为夜间的一步，设计写的 runner 是 ubuntu。
   - 设计写的是"失败开 issue"，但 workflow 现在只有只读权限。这次不开 issue，作业失败即可（夜间运行失败本来就会通知）。要不要给这一个作业 `issues: write`，在汇报里给出建议，由编排者定，因为这要改仓库的权限设置。
   - 网络失败、npm 取不到包，判为"不确定"并重试，不判为漂移；规则照 license 作业取 RustSec 数据库失败时的做法（T006）。
4. **UNIT-codex-drift**：用合成的 schema 和合成的适配器片段，覆盖下面几种情况：
   - 没有变化；
   - 方法消失；
   - 方法从 stable 变成 experimental；
   - 参数加了字段、删了字段；
   - 适配器里出现了基线里没有的方法。

   不联网。

**不做**：升级 codex-acp 或已验证范围（T113）；兼容矩阵本身。

## 验收标准

- [ ] UNIT-codex-drift 通过，登记在 `tests/ids.toml`。
- [ ] 本机用当天 npm 上的真实版本跑一次 `cargo xtask ci nightly` 里的这一步，把结果写进 `notes.md`。当天的最新版本已经超出基线时，照实写出比对结果，不要为了通过去改基线。
- [ ] 在 GitHub 上手动触发一次 nightly，或者只跑这一步，附运行链接。
- [ ] 所有 action 锁定到提交 SHA，权限只读，不新增密钥。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上 PR 和运行的链接（守则第 9 条）。

## 规模

- 停止线：300；硬停止线：380

<!-- 基线数据不计入。执行者开工前在 `notes.md` 里按模块写行数估计，测试单独列一项。每次提交后跑一次 `cargo xtask size`，推算或实际越过停止线就停下汇报。 -->

## 参考资料

- 设计：5.1.6（CI 作业表，`codex-drift`）；5.9.5（兼容矩阵的重跑时机）；ADR-08。
- 原型：`spikes/SP-2-adapter-distribution/codex-schema-drift.mjs` 和它的 README。
- 卷宗：SP-2（脚本的用法和当时的结论）；T004（运行时清单）；T006（license 作业的重试与回退）；T197（GitHub Actions 的写法、nightly）；T102（codex 0.165.0、codex-acp 2.3.0）。
- `docs/03-plan.md`：T114 一行。

## 注意

- 不运行 Codex 的会话，不登录，不读写 `~/.codex`。`generate-json-schema` 只是让 codex 二进制输出自己的 schema。
- 外发请求只有 npm，不带个人信息。
- 不在本机造高负载。

## 工作方式

编排者会告诉你用哪个 worktree，以及分支是否已经开好。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
