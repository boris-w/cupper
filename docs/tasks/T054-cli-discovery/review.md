# T054 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,620 行，执行者停下汇报，按它的建议拆成两半（计划 v1.3.13）：本任务做"文件系统与规则"，"拉起进程"那一半归 T094（已合并）。
  - 合并 main 时 `driver/mod.rs` 有一处预料中的相邻行冲突，两个模块声明都保留（`locate`、`probe`）。
  - 合并后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过：rust 1249、protocol 185、ts 323、license 1、testdata-scan 3（`sensitive-words` 跳过），摘要 `ci/pr-432d5d015e27.json`。执行者那次的 `pr-24a04390a896.json` 一并保留。`cargo xtask runtime-pack --verify` 在加了 `cli` 段之后仍然通过。
- 核对：没有新依赖；没有运行真实的 CLI，没有读 `~/.claude`、`~/.codex`；样例目录树都是占位名；没有个人信息。
- 审阅：
  - **禁用清单只有一份**：只取内嵌清单的 `env.deny`，用例逐条取内嵌清单的禁用项验证，证明代码里没有另抄。
  - **宿主运行时不读包内文件**：清单经 `include_str!` 编进宿主，符合代码约束；T056 在 `manifest.rs` 上加字段。
  - **解析出的二进制也过位置规则**，npm 包里的平台二进制同样要过，防止可信目录里的符号链接指向工作区；设置里的 `cli_path` 例外（用户经原生确认指定）。认可。
  - **配置目录传原值，不传 realpath**，同时检查路径本身所在的位置（放在项目里、指向外面的符号链接也拒绝）；临时目录允许（兼容套件的隔离配置目录，Q30）。认可。
  - **只认表里列出的位置**：`vendor/` 下的 `rg` 不会被认成 Claude；pnpm 的 `#!/bin/sh` 脚本、纯 JS 的 `cli.js` 判为 `unsupported_install`，与 5.6.6 的裁定一致。
  - 有意保守的两点：常见位置的 `~/.npm-global/bin` 来自 npm 文档、不是实测；nvm 只看默认的 `~/.nvm`、不读 `NVM_DIR`。记录在案，T066 用真实环境跑时再看。
- **规模与流程**：1,166 行（实现约 686、测试约 465、清单数据 15），超出收窄后约 1,100 的上限约 66 行，没过 1,200 的硬停止线。写完 `CliLocator` 时推算已贴线（约 1,120），执行者判断能压回去而没有停下，违反了"推算超线就停下"，执行者在汇报里如实说明。功能完整，编排者不要求为压行数而精简测试，接受。
- 转交事项已写进 `handoff-notes.md`：T055（`Boundaries`、`CliLocator`、`check_config_dir`、`adapter_env` 的调用方式，已验证范围交给 T094 的分类函数）、T060（git 子进程的 PATH 用 `CliLocator::trusted_path()`）、T111（Codex 的布局）。
