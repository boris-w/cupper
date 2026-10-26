# T197 汇报

T197 的两个 PR 都全绿，验收标准逐条满足。PR #4（`ci/T197-baselines`，到 84de78f）可以合并。合并后 PR #2、#3 只点"重跑"仍会失败，要触发一次新的 `pull_request` 事件，见"遗留问题"第 2 条。

- 执行者：子 agent（工程师；T093、T189 的执行者）
- 分支：`ci/T197-github-actions`（PR #1，已合并为 8df33d4）、`ci/T197-baselines`（PR #4）
- 完成日期：2026-10-26

## 做了什么

### 第一轮（PR #1，已合并）

- **`ci.yml`**：一个矩阵作业，覆盖 `pr` 组合的 8 个作业，检查名形如 `ci / rust (macos-15)`。
  - runner 照设计 5.1.6：
    - rust、ui、e2e、compat-framework 用 `macos-15`；
    - protocol、ts、license、testdata-scan 用 `ubuntu-24.04`，4 个都跑通了。
  - 触发：`pull_request` 和推送到 main。没有用 `pull_request_target`，权限是 `contents: read`。
  - 同一个 PR 有新推送时，取消正在跑的那一轮。
- **两个组合 action**：
  - `.github/actions/setup`：
    - 从 `origin/main` 建本地 `main`，checkout 用 `fetch-depth: 0`；
    - Node 照 `.node-version`，pnpm 照 `packageManager`；
    - 四个缓存：rust-cache（按作业分键）、pnpm store、runtime-pack 的下载、git-lfs 的下载。
  - `.github/actions/xtask`：执行 `cargo xtask ci <作业> --in <组合> --record ci-summary`，再上传三类构件：
    - 摘要；
    - 原始日志，保留 7 天；
    - 失败时的 Playwright 结果。
- **xtask `--in <组合>`**：单独跑组合里的一个作业，按路径跳过和 e2e 冒烟子集都照 `ci pr` 判断，workflow 不写 `paths`。
- **`nightly.yml`**：每晚定时运行，也可以手动触发。
  - 作业：differential；security-macos14 在 `macos-14`、`macos-15` 上各跑一次；e2e 全集；compat-framework。
  - `perf` 不上 CI。`release-config`、`codex-drift` 还没实现，没有放进去。
- **其他三个 workflow**：
  - `cupper-sys.yml`：改动 cupper-sys 的 PR，在 `macos-14` 上跑 security-macos14。
  - `e2e-repeat.yml`：`workflow_dispatch`，参数是 spec、用例名、次数，一次 `--repeat-each` 跑完。
  - `ui-baseline.yml`：`workflow_dispatch`，在 runner 上生成截图基线，产出为构件。
- **e2e 的 workers**：取"核数减一，最多 2"，理由写在 `playwright.config.ts`。
- **runner 首次运行暴露的两处问题**：
  - 托管 runner 上，摘要的身份名单为空。原来 runner 的用户名 `runner` 会让步骤名 `test-runner` 被当成个人信息。
  - license 作业在离线的 `cargo deny` 之前，先执行 `cargo fetch --locked`。

### 第二轮（PR #4）

- **ui 基线**：
  - 改用 runner 生成的基线，33 张，和作者机器上的逐字节相同；
  - 元数据检查：Chromium 仍须一致，macOS 不同（不论大小版本）只给提示；
  - `meta.json` 记 macOS 15.7.9。
- **PERF 用例**：PERF-bridge-gate 打 `@perf`。托管 runner 上的 e2e 不跑这些用例，也不计入趋势；作者机器上照旧跑、照旧判。
- **e2e 急修**（main 和各 PR 的 e2e 都因此失败）：
  - 运行时包的 stage，要满足三条才算就绪：清单列出的每个文件都在；哈希一致；没有多余文件。
    - `ci e2e`、`compat --real` 都用这个判断，host-bundle 共用同一个 `mismatch`；
    - 本机和 CI 走同一份代码。
  - 作业结束时，删掉 `target/runtime-pack` 里除 `downloads` 以外的全部内容，不进任何缓存（ADR-11）。
- **摘要的 suite**：加 `--in` 时，结果的 suite 记组合名。nightly 带 `within: nightly`。
- **IT-sys-peer-macos14、IT-sys-peer-macos15**：
  - 摘要按 `facts::os` 归类：macOS 14、15 上的对端校验结果分别记作这两项，其他 macOS 照旧记作 IT-sys-peer-current；
  - `tests/ids.toml` 里两项改为 `"nightly"`（active）。
- **`notes.md`**：现状核对、取舍、计划 1.8 的改写建议，以及三轮的运行结果和链接。

## 关键取舍

- **`--in pr` 加一个矩阵作业**：
  - 不让每个 workflow 都跑 `ci pr`，否则每台 runner 都要把 8 个作业全跑一遍；
  - 不在 workflow 里写 `paths`，否则跳过规则会写在两个地方；
  - 检查名在编排者第 1 轮意见后定为 `ci / <作业> (<runner>)`，main 的保护规则已经勾了这 8 个名字。
- **托管 runner 上身份名单为空**：只按 `RUNNER_ENVIRONMENT=github-hosted` 判断。runner 的用户名和主机名是每台机器临时生成的；路径、邮箱等通用模式的检查照旧。
- **security-macos14 失败时不再开 issue**：开 issue 要 `issues: write`，和"权限全部只读"冲突。要不要恢复，由编排者定。
- **PERF 不在托管 runner 上判**：runner 上 p99 是 8.65 ms，闸门是 5 ms。设计 5.9.6 规定绝对预算只在参考硬件上判，编排者已认可。
- **ui 元数据**：33 张图在 macOS 15.7.9 和 26.4 上逐字节相同，像素不依赖 macOS 版本。所以 macOS 降为提示，由像素决定，编排者已认可。
- **就绪按清单判断，不调 `runtime-pack --verify`**：
  - `--verify` 会重新下载、解包、重签，还要重新打包 claude-agent-acp；
  - 按清单核对只要 6.5 s，比对的正是 host-bundle 之后要检查的内容。
- **用收尾步骤把 stage 挡在缓存外**：rust-cache 没有排除路径的选项，清理 `target/` 时又会留下非 cargo 目录的骨架，所以在它保存之前删掉这些目录。`downloads` 由自己的缓存先存：post 步骤倒序执行，日志里已核对。
- **对端校验在摘要里按 `os` 归类**：
  - 用例名不带 macOS 版本，按要求由 xtask 按 `facts` 取的版本决定，不靠 workflow 传参；
  - PR 上 `rust (macos-15)` 跑出的对端校验也会如实记作 macos15，不会冒充 current。

## 验证方式

按任务书的验收标准逐条核对：

1. **草稿 PR 上 `pr` 组合的全部作业在 GitHub 上通过**：满足。
   - PR #1：运行 https://github.com/boris-w/cupper/actions/runs/40311848751 ，8 个检查全部通过。
   - PR #4：见下面"PR #4 的结果"。
2. **推送到 main 时同样的作业能运行**：8 个作业都在 main 上运行了。
   - https://github.com/boris-w/cupper/actions/runs/40315183441 （8df33d4）：只有 ui 因基线元数据（macOS 26 对 15）失败，这一处已在 PR #4 修好。
   - 之后 main 上的 e2e 因缓存里的空 stage 失败，也已在 PR #4 修好（"遗留问题"第 1 条）。
   - PR #4 合并后，main 上的 `ci` 由编排者核对。
3. **nightly 至少手动跑通一次，security-macos14 在两个 macOS 版本上都跑过**：满足。
   - https://github.com/boris-w/cupper/actions/runs/40315239100 （main）：除 e2e 全集外都通过；e2e 全集挂在 PERF 上，现已改为 `@perf`。
   - https://github.com/boris-w/cupper/actions/runs/40323977621 （本分支 d5f4adc）：全部通过，见下面"分支 nightly 的结果"。
   - 用这次的摘要跑 trace-ids `status`：IT-sys-peer-macos14、IT-sys-peer-macos15 都是 done（"passed in nightly"），输出见 `notes.md`。
4. **E2E-SEC-real-input 跑 50 次无失败**：满足。
   - https://github.com/boris-w/cupper/actions/runs/40315247047 ：50/50 通过，平均 6.6 s，最长 11.3 s。
   - 这补完了 T189 的连跑。
5. **action 都锁 SHA；没有 `pull_request_target`；权限只读**：满足。
   - 所有外部 action 都锁在提交 SHA 上，注释写了版本号。
   - 5 个 workflow 都是 `contents: read`，grep 确认没有 `pull_request_target`。
   - 每次请求推送前，actionlint 1.7.12 和 prettier 都通过。
6. **本机的 `cargo xtask ci pr` 照旧能跑，不因本任务而变**：满足。完整的本机 `ci pr` 这一轮没有跑（机器负载约 40），本机受影响的三处逐一核对过：
   - e2e：本机的 stage 判为就绪，耗时 6.5 s；跳过 runtime-pack 时会写明原因。
   - 对端校验：本机跑了 `ci security-macos14 --in nightly --record`，在 macOS 26.4 上照旧记作 IT-sys-peer-current。
   - ui：本机 33 张照旧全过，并提示 macOS 不同。
   - 本机的 `ci pr` 不带 `--in`，suite 照旧是 `pr`。

其他自检都通过：xtask 的 clippy 和 61 个测试、`trace-ids check`（418 个编号）、prettier。

**PR #4 的结果**（d5f4adc，https://github.com/boris-w/cupper/actions/runs/40323986038 ）：8 个检查全绿。
- 第 1 次运行时 ts 挂在一个与本任务无关的偶发失败上（"遗留问题"第 3 条），只重跑 ts 后，第 2 次运行全部通过。
- 前一轮（bad5736，https://github.com/boris-w/cupper/actions/runs/40322065826 ）：
  - e2e 完全命中那份带空 stage 的旧缓存；新的判断让 runtime-pack 重新打包（25.6 s），31 个用例全过。
  - rust 挂在 `it_nfr9_crash_under_load_kill9_again_and_again`，与本任务无关，已记进 T199。

**分支 nightly 的结果**（https://github.com/boris-w/cupper/actions/runs/40323977621 ）：全部通过。
- differential、compat-framework 通过。
- security-macos14 在 macOS 14.8.9 和 15.7.9 上都通过，分别记作 IT-sys-peer-macos14、-macos15，suite 是 `nightly`。
- e2e 全集：48 个用例全过，共 686 s。
  - runtime-pack 重新打包，用时 29.5 s；
  - `@focus` 段 120 s；
  - `perf` 跳过，原因写在摘要里。

**规模**：第一轮 +393 −169，第二轮 +215 −66，合计新增 608 行。越过停止线 600 后报给了编排者，编排者接受，不拆分，上限改为 620/700。

## 提交列表

第一轮（`ci/T197-github-actions`，合并为 8df33d4）：

```
bd5cf2f docs(tasks): T197 notes, the first push and the one ci workflow
c5b920d ci: one `ci` workflow for `ci pr`'s jobs, each check named once
df3e5fb fix(xtask): two jobs that failed on a fresh GitHub runner
35f9320 docs(tasks): T197 notes, choices and the plan 1.8 rewrite
ed8f842 test(e2e): workers by the machine's cores, two at most
13a6ad2 ci: run one e2e test N times, and make the ui baselines, by hand
ee0d3fc ci: the nightly jobs on GitHub Actions
4773c75 ci: GitHub Actions runs each of `ci pr`'s jobs on a runner of its own
9c4a596 feat(xtask): a job of a composite runs on its own as one of its jobs
fe94c2a docs(task): estimate T197
```

第二轮（`ci/T197-baselines`，`git log --oneline main..HEAD`）：

```
84de78f docs(tasks): T197 notes, the nightly that proves IT-sys-peer-macos14/15
d5f4adc feat(xtask): peer checks on macOS 14 and 15 count for IT-sys-peer-macos14/15
8c535f0 fix(xtask): a job run with `--in` records its composite as the suite
bad5736 ci: of target/runtime-pack only the downloads reach a cache
fb7872e fix(xtask): the runtime stage is ready only when it matches the manifest
67708d5 docs(tasks): T197 notes, the merged phase, the 50 runs, baselines and nightly
af4a3f0 ci: PERF tests are left out of the e2e suite on a GitHub-hosted runner
e65c7b2 test(ui): the baselines are the CI runner's, and another macOS only gets a note
```

84de78f 只改了 notes.md，没有单独推送；按编排者的安排随合并一起进 main。

## 遗留问题 / 范围外发现

1. **e2e 急修的根因**：rust-cache 2.9.3 存缓存前清理 `target/`，会把非 cargo profile 的目录当作嵌套的 target 目录递归处理：删掉文件，保留目录。
   - 缓存因此带着一个空的 `runtime-pack/Contents/` 骨架，xtask 原来只看目录在不在，就跳过了打包。
   - 完全命中时 rust-cache 不会重新保存。
   - 已在 PR #4 修好。
   - 旧的 `v0-rust-e2e-*` 缓存不删（编排者已定），里面只有空目录，没有 Anthropic 的代码。
   - `target/` 下其他非 cargo 目录（`git-lfs`、`host-bundle`）也会留下这种骨架；目前没有别处只凭"目录在"判断，git-lfs 的文件由下载缓存恢复。
2. **PR #2、#3 要触发新的 `pull_request` 事件才会带上修复**：
   - GitHub 重跑时沿用原来的合并提交，所以只点"重跑"仍会失败。
   - 做法：把 main 合进 PR 分支后推送，或者关闭再重新打开 PR。
3. **两处偶发失败，与本任务无关**：
   - `it_nfr9_crash_under_load_kill9_again_and_again`："no load in round 4"，3 核 runner 上第 5 轮在限时内没等到 200 条新 delta。已记进 T199。
   - `UNIT-hostport-contract-bridge`（`apps/desktop/src/main/bridge/contract.test.ts`）：
     - 原因：`bridge/testing.ts` 的 `settle = () => setTimeout(resolve, 10)` 按墙钟只等 10 ms，runner 一慢就先断言了；
     - main 的 40315862546 和 PR #4 的 40323986038（第 1 次运行）各失败一次，挂在不同的用例上；
     - 建议并入 T199，改为等到事件本身发生。
4. **T084 的"macOS 只比对大版本"由本任务改掉**：现在 Chromium 必须一致，macOS 不同只给提示，由像素决定。
   - 设计 5.1.6（`docs/design/01-repo-build-ci.md`）和 5.9（`docs/design/09-testing.md`）里没有这句话。
   - 它在计划里有三处，行号按本分支：`docs/03-plan.md` 1.8 的"截图基线"行（第 169 行）、T118 行（第 411 行）、R9（第 1608 行）。
   - 设计里另有两处相关：
     - 5.1.6 第 125 行"截图基线由专用脚本在本机生成"已经过时，现在由 `ui-baseline` 工作流在 runner 上生成，归入计划 1.8 的改写；
     - 5.9.1 的表（第 19 行）仍然成立，可以补一句"macOS 不同只提示"。
5. **计划 1.8 的改写建议**：见 `notes.md` 的表，逐行写了标题与引言、统一入口、环境、机器级锁、验收记录、每晚作业、截图基线、本机替代不了的项、M1a 退出、以后建远端时。"本机替代不了的项"已由本任务落实。
6. **security-macos14 失败时不再开 issue**：理由见"关键取舍"，要不要恢复由编排者定。
7. **`perf` 的建议**：继续在作者机器上跑，PERF 历史由编排者提交。
   - 理由：托管 runner 不是参考机型，这次 p99 8.65 ms 就是证据；R12 又不允许自托管 runner。
   - T191 的交接（PERF-bridge-gate 和另一个 worker 并行，趋势有噪声）在作者机器上照旧，本任务没有让它单独跑。
8. **T090 的交接"在 CI 上重跑一轮 20 次"没做**：
   - workers 已按核数重新定，runner 和作者机器上都是 2；
   - 20 次连跑属于 T099 的 M1a 退出，可以用 nightly 的 e2e 全集或 `e2e-repeat` 在 runner 上做。
9. **对其他任务的影响**：
   - T192：PR #4 改了 `crates/xtask/src/compat/mod.rs`（`compat --real` 的 stage 判断），T192 没有改这个文件；T192 提交的是本机摘要，suite 不变。
   - T196：没有重叠的文件。
   - T195：和 PR #4 都改了 `tools/e2e/src/gate.spec.ts`，改动区段不重叠。
   - GitHub 上 PR 摘要构件的 suite 从作业名变成了 `pr`；本机的 `ci pr --record` 不受影响。
10. **下载缓存的键由几个作业共用**：只用到 git-lfs 的作业会先存下一份只有 git-lfs 的缓存，PR #2 上就有一份 10 MB 的。runtime-pack 会按清单补下缺的文件，只是多下载一次；按作业分键可以省掉这次下载。
11. **规模**：两轮合计新增 608 行。我写完映射才合计出来，越线时没有及时停下汇报；编排者已接受，上限改为 620/700。
