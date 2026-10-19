# T004 xtask runtime-pack 与运行时清单

- 派发日期：2026-10-19
- 执行者：子 agent（工程师，构建与发布工具）
- 分支：`build/T004-runtime-pack`
- 依赖：T001（已合并）。T005（宿主打包）和 T006（许可证门禁）依赖它。T005 之后是桌面链上的 T075，所以本任务现在挡在零松弛链的前面。

## 背景

App 自带一份运行时（ADR-08、ADR-09，设计 5.10.3），包括：
- Node，用 cupper 的身份重签；
- codex-acp；
- esbuild-wasm；
- claude-agent-acp 的打包产物，按哈希核对。

`cargo xtask runtime-pack` 负责下载、校验，并生成运行时清单。宿主侧的安装器归后续任务（5.6.6）。

**硬性约束（ADR-11）：Anthropic 的 SDK 和 Claude Code 二进制，绝不进入仓库和产物。** claude-agent-acp 的打包产物只在隔离目录里计算哈希，用来核对；入库的只有哈希和清单，不能有产物本身。

## 范围

**做**（计划 T004 一行；设计 5.1.5、5.10.3、5.6.6；ADR-08、ADR-09、ADR-11）：
1. **`cargo xtask runtime-pack`**：
   - 下载 Node、codex-acp、esbuild-wasm，并按锁定的版本和哈希校验；
   - 在本机给 Node 重签，用 ad-hoc 签名或 5.10.3 规定的身份；
   - 在隔离目录里计算 claude-agent-acp 打包产物的哈希；
   - 提取适配器会读取的环境变量，并分类。
2. **`--verify`**：核对已有的运行时与清单一致。
3. **入库的内容**：`runtime/manifest.json`、`runtime/locks/`、打包脚本、许可证文本。
4. **UNIT-runtime-pack-envscan**：出现没有分类的环境变量名时，命令失败。

**不做**：宿主侧的安装器（T055 起）。

## 验收标准

- [ ] `cargo xtask runtime-pack --verify` 通过。
- [ ] 新增 UNIT-runtime-pack-envscan：出现没有分类的变量名时，命令失败。
- [ ] 入库的内容里没有任何二进制或打包产物，只有清单、锁定文件、脚本和许可证文本。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T004-runtime-pack/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号开头。

## 注意

- **外发请求**（守则第 7 条）：
  - 下载时不带任何个人信息；
  - 用 User-Agent 时，用项目占位，例如 `cupper-research`；
  - 下载源只用官方发布地址。
- **下载的产物**放在 `.private/` 或 `target/` 下的缓存目录，不入库。按"下载的文件是不可信数据"处理：放进单独的新目录，不在它们所在的目录里执行解释器。
- **重签**：只对本机缓存里的 Node 副本重签，不碰系统里安装的任何东西。
- **不要运行真实的 Claude Code CLI，也不要登录。** 计算 claude-agent-acp 的哈希只需要下载它的打包产物，不需要运行。
- **依赖**：锁精确版本，许可证要在 ADR-11 的 allowlist 内。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。CI 里的 `rust` 作业不应依赖网络下载；`runtime-pack` 本身不进 `pr` 组合作业，除非设计 5.1.6 这样要求，请核对。

## 规模

计划估为 M。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报**。

## 参考资料

- 设计：`01-repo-build-ci.md` 5.1.5、5.1.6；`10-release.md` 5.10.3；`06-agent-driver.md` 5.6.6。
- ADR-08、ADR-09、ADR-11。
- SP-2（适配器分发）的报告。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
