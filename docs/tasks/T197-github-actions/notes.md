# T197 工作笔记

- 执行者：子 agent（T093、T189 的执行者）

## 开工前核对现状

- 现有 5 个休眠的 workflow（rust、protocol、ts、license、security-macos14）：
  - 只做了 checkout，有的装了 Node 和 pnpm；
  - 没有缓存、没有构件、不和 main 比较；
  - `security-macos14` 失败时用 `issues: write` 开 issue。
- `xtask ci <作业>` 单独跑一个作业时，`ctx.target` 是作业名，不是 `pr`：
  - ui、compat-framework 不会按路径跳过；
  - e2e 跑全集，而不是冒烟子集。
  - 所以要么每个 workflow 都跑 `ci pr`，要么给 xtask 加一个"当作 `pr` 里的一员来跑"的选项。选后者：`cargo xtask ci <作业> --in pr`。
- 和 main 比较用的是本地的 `main` 分支（`git merge-base main HEAD`）。PR 的 checkout 是合并提交、处于游离 HEAD，只有 `origin/main`。所以要 `fetch-depth: 0`，再建一个本地的 `main`。
- ui 的基线是在作者的 macOS 26.4 上生成的，`meta.json` 的大版本对不上 runner 的 macOS 15，比对会直接失败。要先用基线工作流在 runner 上生成。
- 下载缓存：runtime-pack 在 `target/runtime-pack/downloads`，git-lfs 在 `target/git-lfs`。
  - `target/runtime-pack/claude-agent-acp.isolated` 里有 Anthropic 的 SDK，不能进任何缓存（ADR-11）。
- action 的 SHA 都按 `gh api repos/<action>/tags` 查（只读）：
  - checkout v7.0.1、setup-node v7.0.0：照旧；
  - cache v6.1.0、upload-artifact v7.0.2、Swatinem/rust-cache v2.9.3。

## 开工前的规模估计

| 模块 | 内容 | 草估 |
|---|---|---|
| 可复用的 workflow `xtask.yml` | checkout（全历史，建本地 main）、Node、pnpm、Rust 缓存、pnpm store、下载缓存、cargo-deny、`xtask ci`、摘要与日志构件、失败时的 Playwright 结果 | 90 |
| `pr` 组合的 8 个 workflow | 每个只写触发、权限和调用，约 15 行 | 120 |
| 每晚 `nightly.yml` | differential；security-macos14 在 macos-14 和 macos-15 上各一次；e2e 全集；compat-framework | 45 |
| `e2e-repeat.yml` | `workflow_dispatch`：spec、用例名、次数，一次 `--repeat-each` | 45 |
| `ui-baseline.yml` | `workflow_dispatch`：在 runner 上生成基线，产出为构件 | 35 |
| xtask：`--in <组合>` | 单独跑 `pr` 组合里的一个作业，跳过和子集照 `ci pr` | 30 |
| e2e 的 workers | 按核数定 | 10 |
| 测试 | `--in` 的解析与校验 | 25 |
| **合计** | | 约 **400** |

在停止线 600 之内。每写完一个模块跑一次 `cargo xtask size`。
