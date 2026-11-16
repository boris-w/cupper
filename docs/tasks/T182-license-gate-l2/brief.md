# T182 许可证门禁第二层与第三方声明

- 派发日期：2026-11-16
- 执行者：子 agent（前端与 e2e、CI；T103、T155、T262 的执行者）
- 分支：`ci/T182-license-gate-l2`
- 依赖：T161（已合并）。发布组（T180、T181、T183、T184）依赖本任务；R11 记着 T182 不需要 Apple 账号，可以提前做

## 背景

ADR-11 定了 Anthropic 的 SDK 和 Claude Code 二进制绝不进入仓库和产物。许可证门禁第一层（T006，`tools/license-gate`）按依赖清单和许可证放行；第二层（设计 5.10.7）对最终产物按内容指纹扫描，并要求产物里带完整的第三方声明。T161 已经有 nightly 的 release-config 作业，按发布配置打包并扫描。

## 范围

**做**（计划 T182 一行；设计 5.10.7"许可证门禁第二层"、ADR-11）：
1. **内容指纹扫描**：对最终的 `.app`、zip、DMG 扫描：
   - `@anthropic-ai/claude-agent-sdk` 发布文件的哈希和特征串；
   - Claude Code 的 Mach-O 签名 identifier；
   - `locks/` 下的锁文件按哈希放进允许清单（锁文件里出现包名是正常的）。
   指纹清单怎么取、放在哪（不能把 SDK 文件本身放进仓库，只放哈希和特征串），开工前在估算里提出。
2. **负面样例**：构造含特征串、含伪造签名 identifier 的样例产物，扫描必须报出；放行的锁文件被改动后不再放行。
3. **THIRD_PARTY_NOTICES.txt 生成**：包含 Node、codex-acp 及其内联依赖、esbuild-wasm、Electron 与前端依赖、Rust 依赖（cargo-about 生成）；产物里存在且内容齐全由扫描核对。
4. 接进 nightly 的 release-config 作业（或设计规定的位置），PR 上的 `license` 作业按需要跑第二层里不需要打包的部分。
5. 用例：UNIT-license-gate-l2、UNIT-third-party-notices（已在 `tests/ids.toml` 登记）。

**不做**：签名与公证（T180）、发布流水线（T181）、第一层的改动（发现问题记下报给 `main`）。

## 验收标准

- [ ] 两个用例通过；负面样例全部被报出。
- [ ] 分支上的 nightly `job=release-config` 通过，产物里有 THIRD_PARTY_NOTICES.txt 且扫描通过。
- [ ] 新增依赖（如 cargo-about）的许可证在 ADR-11 的允许清单里；不在的话先停下报给 `main`。
- [ ] PR 上 GitHub CI 的 8 个检查都通过，汇报里写上链接。

## 规模

- 停止线：500；硬停止线：600。开工前在 `notes.md` 里按模块估行数（指纹清单的来源单列），报给 `main` 后再写代码。

## 参考资料

- 设计：5.10.7（发布流水线、两层门禁）、5.1.2、ADR-11。
- 卷宗：T006（第一层）、T161（release-config 作业与扫描）、T182 之前的交接事项（如有）。

## 注意

- 仓库会开源：不能下载或提交 SDK、Claude Code 的任何文件，只记哈希和特征串；外发请求不带个人信息。
- 打包和扫描放到 runner 上（nightly `job=release-config`）；本机只跑单元测试。
- `gh` 用量：大家共用每小时 5,000 次的额度，轮询间隔不少于 2 分钟，能等通知就不要轮询。
- 要推送、触发 nightly 时告诉 `main`。

## 工作方式

worktree 是 `.claude/worktrees/T037`，编排者已切到新分支 `ci/T182-license-gate-l2`，从 main 开出（含本任务书）。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。
