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

## 做的过程中的取舍

- **`--in pr`**：每个 workflow 只调用 `cargo xtask ci <作业> --in pr`。
  - 按路径跳过（ui、compat-framework）和 e2e 的冒烟子集都和本机的 `ci pr` 一样由 xtask 判断，workflow 不另写 `paths`。
  - 推送到 main 时，HEAD 就是 main，没有可比较的改动，xtask 不跳过任何作业；e2e 仍只跑冒烟子集，全集放在每晚。
- **一个 `ci` workflow，检查名只出现一次**（编排者第 1 轮）：
  - 第一版是每个作业一个 workflow、再调用可复用的 `xtask.yml`，检查名重复了三层，例如 `compat-framework / compat-framework / compat-framework (macos-15)`。
  - 现在 `ci.yml` 是一个矩阵作业，检查名形如 `ci / rust (macos-15)`。
  - 准备、运行和上传构件放进组合 action `.github/actions/xtask`，`nightly` 也用它，同样是一个矩阵作业。
  - 改动 cupper-sys 的 PR 另有 `cupper-sys / security-macos14 (macos-14)`。
  - `e2e-repeat`、`ui-baseline` 不经 xtask，但和它共用 `.github/actions/setup`。
- **缓存**：
  - Swatinem/rust-cache 按作业名分键，只保留依赖的编译产物，减少占用仓库 10 GB 的缓存额度。
  - pnpm store 按锁文件分键。
  - runtime-pack 和 git-lfs 的下载按 `runtime/manifest.json` 和 `git_lfs.rs` 分键。`claude-agent-acp.isolated` 里有 Anthropic 的 SDK，不进任何缓存（ADR-11）。
- **security-macos14**：去掉了每晚失败时开 issue 的那一步，因为它要 `issues: write`，而本任务要求权限全部只读。失败会在 Actions 页面和每晚的运行里看到；要不要恢复开 issue，由编排者定。
- **e2e 的 workers**：取"核数减一，最多 2"。runner 是 3 核，得 2；作者的机器得的也是 2，本机行为不变。理由写在 `playwright.config.ts` 里。
- **`workflow_dispatch` 的限制**：GitHub 只允许触发默认分支上已有的 workflow。`nightly`、`e2e-repeat`、`ui-baseline` 要在文件进了 main 之后，才能指定本分支手动触发。

## 计划 1.8 的改写建议（交编排者）

| 行 | 现在 | 建议 |
|---|---|---|
| 标题与引言 | 本机 CI（U-P1：先不建远端） | 改为"CI（GitHub Actions，本机可复现）"：PR 以 GitHub 上 `pr` 组合的各作业为准；本机的 `cargo xtask ci <作业>` 照旧可用，用于复现和合并前的自查 |
| 统一入口 | workflow 照写，等有了远端再启用 | 每个作业一个 workflow，只调用 `cargo xtask ci <作业> --in pr`；跳过与子集由 xtask 判断，workflow 不写 `paths` |
| 环境 | 干净 worktree、不共用 `CARGO_TARGET_DIR` | 加一句：runner 上每个作业一台全新机器，缓存只有 cargo 注册表与 target（rust-cache）、pnpm store 和下载 |
| 机器级锁 | e2e 的 workers 固定为 2（T090） | 改为"核数减一，最多 2"（T197）；runner 上每个作业独占一台机器，锁只在本机起作用 |
| 验收记录 | 入库的只有结构化摘要 | CI 上的摘要和原始日志作为构件（日志保留 7 天），不再提交进仓库；卷宗里记 PR 和运行的链接。本机跑的摘要仍可随任务提交 |
| 每晚作业 | 编排者在 main 上跑 `xtask ci nightly --record docs/ci/nightly/` | 由 `nightly.yml` 定时运行（differential、security-macos14 在 macOS 14 和 15 上、e2e 全集、compat-framework）；`perf` 和它的历史仍在本机跑、由编排者提交 |
| 截图基线 | 基线由专用脚本在本机生成 | 由 `ui-baseline.yml` 在 runner 上生成，产出为构件，经 PR 入库；本机的 macOS 大版本与基线不同时，本机的 `ui` 作业会因元数据不符而失败，见下文"遗留" |
| 本机替代不了的项 | macOS 14、15 的对端校验是缺口 | 由每晚在 `macos-14`、`macos-15` 上运行的 security-macos14 覆盖；把 IT-sys-peer-macos14/15 从"缺口"改为由 CI 覆盖，还要编排者在 `tests/ids.toml` 里改状态，并约定证据记法（运行链接） |
| M1a 退出 | 本机连跑 20 次、运行期间作者不操作机器 | 改在 runner 上运行（D6）：20 次连跑用 `e2e-repeat` 或 nightly 的 e2e 全集，"不操作机器"在 runner 上天然满足 |
| 以后建远端时 | 首次在 CI 上再跑一轮 20 次…… | 删去，已经做完的几项写进"环境"与"验收记录"；R12 的几项（外部贡献者需批准、默认只读、不用自托管 runner）保留为一行"仓库设置" |

## 第一轮推送（PR #1，ed8f842）

- **通过**：rust（约 20 分钟）、protocol、ts、testdata-scan、ui（没有改到它依赖的路径，跳过）、e2e（冒烟子集，含 `@focus` 段，约 12 分钟）。
  - runner 的图形会话通过了 `setup.ts` 对锁屏和睡眠显示器的检查。
- **失败 1：compat-framework**：
  - 摘要的个人信息检查把 runner 的用户名 `runner` 当成作者的名字，而步骤名 `test-runner` 里正好有这个词，于是拒写摘要，退出码 2。
  - 修法：在 GitHub 托管的 runner 上（`RUNNER_ENVIRONMENT=github-hosted`），用户名和主机名都是每台机器临时生成的，名字清单为空；通用模式的检查照旧。
- **失败 2：license**：
  - 离线的 `cargo deny` 要读每个 crate 的清单，新机器上还没有下载，报 `failed to download`。
  - 修法：检查前先 `cargo fetch --locked`。

## 第二轮：PR #1 通过并合并之后

- **PR #1 的 8 个检查**（运行 40311848751）：全部通过。
  - rust 24m17s，e2e 10m23s（冒烟子集：非 `@focus` 段 66 s，`@focus` 段 76 s），license 2m55s，其余都在 2 分钟以内。
  - ui、compat-framework 因为没改到它们依赖的路径，由 xtask 跳过。
- **E2E-SEC-real-input × 50**（e2e-repeat，main，运行 40315247047）：
  - 50 次全部通过，一次持锁调用跑完；平均 6.6 s，最长 11.3 s，总共 6 分钟。
  - 这补完了 T189 的连跑：本机那次 50 次里有 45 次期间有用户输入，不算数。
- **ui-baseline**（运行 40315243016）：在 macos-15（15.7.9）上生成的 33 张图，和作者 macOS 26.4 上的逐字节相同，不同的只有 `meta.json`。
  - 基线的元数据检查原来要求 macOS 大版本一致（T084），所以 runner 上一跑 ui 就会失败，本机再换成 runner 的元数据又会在作者机器上失败。
  - 改为：Chromium 仍然必须一致；macOS 不同，不论大小版本，都只给提示，由像素决定。`meta.json` 记 runner 的 15.7.9。
  - 本机的 `pnpm test:ui` 照旧 33 个全过，并提示 macOS 不同。
- **nightly**（运行 40315239100）：
  - differential、security-macos14（macos-14 和 macos-15 各一次）、compat-framework 都通过。
  - e2e 全集失败 1 条：PERF-bridge-gate（"窗口在屏幕上"那条），主进程事件循环延迟的 p99 为 8.65 ms，闸门判的是 5 ms 以下。3 核的 runner 上另有一个 worker 同时在跑。
  - runner 不是参考机型（和 `perf` 不上 CI 是同一个理由）。所以 PERF 用例打 `@perf`，托管 runner 上的 e2e 作业不跑它们，也不算 PERF 趋势，跳过的 `perf` 步骤写明原因；作者机器上照旧跑、照旧判。
- **security-macos14 补上的缺口**：macos-14、macos-15 上各跑通一次，摘要在构件里（`security-macos14-macos-14-summary`、`security-macos14-macos-15-summary`）。
  - 当时 `tests/ids.toml` 里 IT-sys-peer-macos14、IT-sys-peer-macos15 的状态还是"缺口"；第三轮由本任务补上（见下）。

## 第三轮：PR #4（ci/T197-baselines）

### e2e 在 main 和各 PR 上都失败（急修，bad5736）

- 现象：`ci / e2e` 在 host-bundle 一步失败，codesign 找不到 `target/runtime-pack/Contents/Helpers/node`（main 的运行 40315862546，以及 PR #2、#3）。
- 根因：rust-cache 2.9.3 存缓存前清理 `target/`。它把里面不是 cargo profile 的目录（`runtime-pack`、`git-lfs` 等）当作嵌套的 target 目录递归处理，删掉文件，但保留目录。
  - main 上 e2e 的缓存因此带着一个空的 `Contents/` 目录骨架。
  - 之后的运行完全命中这份缓存，xtask 只看目录在不在，就跳过了 runtime-pack。
  - 完全命中时 rust-cache 不重新保存，所以这份缓存一直是这样。
- 修法一：新增 `runtime_pack::staged`。清单列出的每个文件都在、sha256 一致，并且没有多出来的文件，才算就绪。
  - 这就是 host-bundle 原有的那项检查，现在两边共用 `mismatch`。
  - `ci e2e` 和 `compat --real` 都改用它，本机的判断也一并修好。
  - 本机的 stage 判为就绪，耗时 6.5 s。
- 修法二：xtask 组合 action 和 e2e-repeat 在最后加一步（`if: always()`），删掉 `target/runtime-pack` 里除 `downloads` 以外的全部内容。
  - 这样 stage 和 claude-agent-acp 的隔离目录都不会进任何缓存，连空目录也不会（ADR-11）。
  - post 步骤倒序执行，`downloads` 由它自己的缓存先存（日志里已核对）。
- 旧缓存不删：里面只有空目录，没有文件。
- 结果（运行 40322065826，[e2e 作业](https://github.com/boris-w/cupper/actions/runs/40322065826/job/121569216689)）：
  - 仍然完全命中旧缓存；
  - stage 对不上清单，于是 runtime-pack 真的跑了（25.6 s）；
  - build 272 s，冒烟子集 31 个用例全过；
  - 清理一步执行了。

### IT-sys-peer-macos14、IT-sys-peer-macos15

- 对端校验用例的名字都是 `it_sys_peer_current_*`，不带 macOS 版本。所以由摘要按机器的 macOS 归类（`summary::on_os`），macOS 版本取自 `facts::os`，不靠 workflow 传参：
  - macOS 14 记作 IT-sys-peer-macos14；
  - macOS 15 记作 IT-sys-peer-macos15；
  - 其他版本（作者本机）照旧记作 IT-sys-peer-current。
- 同时发现：加了 `--in <组合>` 时，摘要里结果的 suite 原来记的是作业名（如 `rust`、`security-macos14`），trace-ids 不认，PR 的摘要也就算不进 `pr`。
  - 改为记组合名：`Run` 新增 `suite`，取 `--in` 的组合，没有 `--in` 时取目标本身。
  - `nightly.yml` 改为带 `within: nightly`。
  - 作业行为不变：只有 `pr` 会让作业跳过或缩小范围。
- `tests/ids.toml`：两项都改为 `"nightly"`（active）。
- 40315239100 的摘要构件推导不出来：它们由旧版 xtask 写出，结果记的是 IT-sys-peer-current，suite 是 `security-macos14`。用 trace-ids 读，只能得出 IT-sys-peer-current "ran only in security-macos14"。
- 模拟核对：把这两份摘要按新格式改写（id 按 `os` 归类，suite 记作 `nightly`），trace-ids `status` 得到：

  ```
  done (2)
    IT-sys-peer-macos14  M1a T021    passed in nightly
    IT-sys-peer-macos15  M1a T021    passed in nightly
  ```

- 核对方法（用新版 xtask 跑出的 nightly 运行）：

  ```
  gh run download <运行号> --repo boris-w/cupper --pattern 'security-macos14-*-summary' --dir nightly
  node tools/trace-ids/src/main.ts status \
    --summary nightly/security-macos14-macos-14-summary/security-macos14-<提交>.json \
    --summary nightly/security-macos14-macos-15-summary/security-macos14-<提交>.json
  ```

  - 两项都应列在 `done` 下，写着 "passed in nightly"。
  - 只给 nightly 的摘要时，UNIT-sys-uniqueid 会因为 "ran only in nightly" 列为 problem：它登记在 `pr`，要和 PR 的摘要一起读。
  - IT-sys-peer 要等 IT-sys-peer-current 在作者本机的 `pr` 运行里通过才算完成。

### 规模（第三轮，按模块）

| 模块 | 草估 | 实际 |
|---|---|---|
| 运行时包就绪的判断（`staged`、`mismatch`，e2e、compat、host-bundle 改用）及测试 | 60 | +68 −16 |
| 缓存清理（组合 action、e2e-repeat、setup 的注释） | 15 | +17 −2 |
| 对端校验按 macOS 归类、suite 记组合名、`within: nightly`、ids.toml 及测试 | 60 | +74 −9 |

`cargo xtask size`：本分支 +215 −66（净 +149）。第一轮（合并 8df33d4）是 +393 −169，两轮合计新增 608 行，越过了停止线 600，没到硬停止线 700。写完映射时才发现这一点，随即报给编排者，由编排者决定是否拆分。
