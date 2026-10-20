# T115 汇报

- 执行者：子 agent（工程师；T004、T056 的执行者接手）
- 分支：`feat/T115-runtime-download`
- 完成日期：2026-10-20

## 做了什么

本任务实现了 claude-agent-acp 的按锁文件下载、本机打包和退避重试，修正了 T055 拉起时跑成打包脚本的错误。全部用例离线通过，`cargo xtask ci pr` 通过，摘要已提交。规模新增 1,189 行、删除 265 行，在约 1,250 行的例外上限之内。

**1. 按锁文件下载**（`runtime/npm.rs`）
- **锁文件解析**：
  - 跳过 optional、dev、devOptional、link 四类包，8 个含 Claude Code 二进制的 optional 平台包永远不下载；
  - 安装路径必须在 `node_modules/` 下，而且只由普通路径段组成；
  - `resolved` 必须以 `https://registry.npmjs.org/` 开头，然后换成注入的 registry（结尾的 `/` 会补齐）；
  - 不符合以上任一条，都算 `hash_mismatch`。
- **HTTP 客户端**：
  - `reqwest =0.12.28`，不开默认特性，开 `rustls-tls-native-roots`：加密实现用 `ring`，根证书用系统的；
  - User-Agent 为 `cupper-runtime-installer/<版本>`；
  - 只用传进来的 `HTTPS_PROXY` 作代理，不读本进程环境变量里的代理，也不读系统代理；
  - 不跟随重定向。
- **下载与解包**：
  - 最多 8 个并行；每个包用 `runtime_download`（60 秒）限时，经 `Clock` 的定时器与下载并行等待；
  - 先核对 SRI（sha-512），通过后才解包；
  - 解包只写普通文件，路径越界就算 `hash_mismatch`；
  - 不运行任何包脚本。

**2. claude-agent-acp 组件与本机打包**
- **计划**：`ComponentPlan` 改成两部分。
  - "版本目录里有什么、各自的哈希"：claude-agent-acp 只有 `adapter.mjs`，哈希就是 `bundle_sha256`；
  - "从哪来"：`Bundle` 是从包内复制；`Npm` 是包内的锁文件、打包脚本和入口。
- **核心**：实现 `pure::Core`。
  - claude-agent-acp 要等 node 和 esbuild-wasm 都 `ready` 才开始；依赖失败时随之失败，原因相同；
  - `network` 失败按 `runtime_retry` 退避重试 3 次（2、4、8 秒），其他失败不重试。
- **驱动**：改为异步，`run(&clock, publish)`，用 `TimerTable` 等定时器，文件操作放进 `spawn_blocking`。claude-agent-acp 的安装顺序：
  1. 锁文件和打包脚本经 `Bundle` 读取、核对哈希，放进 `staging/work/`；
  2. 下载 npm 树；
  3. 经 `probe::run::run` 用已装的 Node 跑打包脚本：环境变量只有 `PATH`（Node 所在目录），用 `runtime_bundle`（120 秒）限时；
  4. 把产物移出 `work/`；
  5. 无论成败都删掉 `work/`（里面有 SDK）；
  6. 核对产物的 sha-256，通过后提交。
  - 打包失败、超时、产物不符，都算 `hash_mismatch`。

**3. 两处衔接，含修正 T055 的错误**
- **拉起路径**：`launch::production` 改为从 `runtime::claude_adapter()` 取：
  - `runtime/node/<版本>/node`；
  - `runtime/claude-agent-acp/<版本>/adapter.mjs`，按 `bundle_sha256` 核对。
- **修正**：原来拉起的是打包脚本，核对的也是打包脚本的哈希。`manifest::CLAUDE_BUNDLE` 已删除。
- **核对只留一份**：拉起前用 `runtime::verify`；`launch` 自己的 `verify`、`Mismatch` 和对应测试已删除。

## 关键取舍

均按裁定执行：
- 越界 tarball 和打包失败都算 `hash_mismatch`，不重试。
- 按组件整体重试，每次重下全部包，不做缓存。
- 所有限时都经 `Clock`，不用 reqwest 自带的超时。

另外三点是我自己的选择：
- **只访问 registry**：不跟随重定向，3xx 按网络失败处理。
- **打包环境与 CI 一致**：npm 树和打包输入放在 `staging/work/` 里，产物路径相对 `work`，和 CI 算 `bundle_sha256` 时的调用方式相同，产物逐字节一致。
- **推进时钟只推重试**：测试里只在下一个定时器 8 秒内到期时推进 `ManualClock`。重试的等待最长 8 秒，下载和打包的限时是 60 秒、120 秒，因此只会推进重试的定时器。

## 验证方式

**IT-runtime-download**：共 7 个用例，在 `runtime/download_tests.rs`。全部离线：用 tokio 写的最小 HTTP registry、自己造的 tarball，假 Node 是 shell 脚本。

| 用例 | 覆盖 |
|---|---|
| `it_runtime_download_fetches_the_locked_packages_only` | 嵌套包；optional 和 dev 包没有被请求；User-Agent |
| `…_sri_or_tarball_paths_not_accepted_are_hash_mismatch` | SRI 不符；越界的 tarball，越界文件没有写出 |
| `…_unreachable_failing_or_slow_registry_is_network` | 返回 503；端口连不上；不应答时 `ManualClock` 超时 |
| `…_installs_claude_agent_acp_by_its_lockfile` | 经本地 registry 的完整安装；版本目录只剩产物和 `.installed.json`；假 Node 核对环境只有 `PATH` |
| `…_network_failures_are_tried_again_after_2_4_and_8_s` | 一直失败：等 2、4、8 秒，请求 4 次后 `failed{network}`；失败 2 次后成功：等 2、4 秒，请求 3 次后 `ready` |
| `…_bundle_not_matching_the_manifest_is_hash_mismatch` | 产物不符；不重试；组件目录里什么都不剩 |
| `…_waits_for_its_dependencies_and_fails_with_them` | esbuild-wasm 的包内文件被改：两者都 `bundle_changed`，一次下载都没有 |

- **变异检查**：给打包环境多加一个 `HOME`，完整安装用例就失败成 `hash_mismatch`，说明假 Node 的环境核对有效。检查完已还原。
- **其他测试**：锁文件的单元测试；T056 的 IT-runtime-install 改用异步驱动后照常通过；T055 的拉起用例照常通过。
- 没有睡眠，时间只经 `Clock`。没有真实 npm 包的内容，没有访问网络。
- **CI**：`cargo xtask ci pr --record docs/tasks/T115-runtime-download/ci/` 通过，摘要 `pr-4b831744dea8.json` 已提交。

  | 部分 | 通过数 |
  |---|---|
  | rust | 1650 |
  | protocol | 185 |
  | ts | 353 |
  | license | 1 |
  | testdata-scan | 3 |

  第一次运行只有 license 一项失败，原因是 cargo-deny 拉 advisory 数据库时网络出错（`SSL_ERROR_SYSCALL`），重跑就通过了。

**规模**：新增 1,189 行、删除 265 行，净增 924 行，不含 `Cargo.lock`。在第 2 次裁定放宽的约 1,250 行之内。逐模块的数字在 `notes.md`。

## 提交列表

```
d86f04b docs(task): record the T115 tests, final counts and pr CI summary
4b83174 test(host): install claude-agent-acp offline, with retries and failures
c5c75e0 docs(task): record the T115 line counts and stop at the cap
2132702 feat(host): install claude-agent-acp by its lockfile and bundle it here
39de792 feat(host): download claude-agent-acp's npm tree by its lockfile
caf5c6e docs(task): estimate T115 and propose a split before starting
```

## 遗留问题 / 范围外发现

1. **接进宿主启动**（不在范围内）。接入点：
   - 构造 `Installer`：
     - `plans`：`runtime::plans(manifest::embedded())`；
     - `bundle`：`Arc::new(BundleDir::of_this_host()?)`；
     - `dir`：`runtime::runtime_dir(data_dir)`；
     - `in_use`：运行中适配器所用的版本；
     - `registry`：`test_config.runtime.registry_url`，没有时用 `npm::REGISTRY`；
     - `proxy`：登录 shell 的 `HTTPS_PROXY`；
     - `durations`。
   - 在后台调用 `run(&clock, publish)`，状态写进索引流的 `agents`。
   - 要排在宿主的 cdhash 自检（T059）之后。
   - `Failed(BundleChanged)` 时转入待升级。
   - 在接线之前，生产环境的运行时目录是空的，Claude 适配器拉起前的核对会失败，报 `RuntimeFailed`。
2. **`Cargo.lock` 新增约 69 个包**（hyper、rustls、url/idna/icu 等），`cargo deny` 通过。
3. **`cargo clean` 之后本机单独跑 `cargo test`**：`driver::runtime` 的几个用例要先 `cargo build -p cupper-fake-agent`。`cargo xtask ci` 会先构建它，不影响 CI。
4. **CI 不稳定**：license 这一步依赖联网拉 advisory 数据库，网络不稳时会偶发失败，重跑即可。