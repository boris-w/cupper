# T203 笔记

## 为什么光换 HOME 和登录 shell 不够

本机的 Claude 装在 `/opt/homebrew/bin/claude`（Homebrew cask，2.1.296）。这是 `resolve::CLAUDE` 常见位置表里写死的绝对路径，`/usr/local/bin/claude` 也是。测试目录里的 `HOME` 只能移开 `~/` 开头的几处（`~/.local/bin`、`~/.npm-global/bin`、nvm），`test_config.login_env` 只能移开登录 shell 的 PATH。宿主不改的话，夹具能用的只剩在每个会走到发现的用例里，宿主启动前往 `host.db` 写 `agents.claude.cli_path`。共用夹具做不好这件事：很多用例让宿主自己建数据目录，退出码 3 到 6 的用例还故意造坏的数据目录。

所以先向编排者汇报，提议加一个 test-harness 注入点 `test_config.cli_root`：常见位置里的绝对路径改到这个目录下。编排者同意，条件是：只在 test-harness 构建里编译和读取；生产的 `CliLocator` 路径逐字不变；加断言证明非 test-harness 构建不读它；5.9.9 的表由编排者加（"CLI 发现的绝对常见位置 | `test_config.cli_root` 代替 `/` | test-harness | `/`"）。`test_config` 的类型在 `cupper-host` 的 `startup/test_config.rs`，不在 `cupper-protocol`，没有导出的 schema 和生成的 TS，所以没有要更新的生成物。

测试的临时目录都在 `$TMPDIR` 下，而发现不信任临时目录（可信 PATH 和常见位置都按 `Boundaries::trusted` 判，T054）。所以夹具的 `HOME` 和 `cli_root` 放在测试的临时目录里，发现在那里什么也找不到。用例只能经 `cli_path`（设置来源，不过可信判定）或 `test_config.agents` 拿到 CLI。

## 清单：会走到生产 CLI 发现的地方

"发现"指 `launch::production` → `CliLocator::locate` → 对找到的二进制执行 `--version`。配了 `test_config.agents` 或 `in_process` 的宿主不调用它（`Finder::launch_found`）。

### 走到发现、要改的

| 位置 | 怎么拉起宿主 | 修前看得到什么 | 改法 |
|---|---|---|---|
| `tests/common`：`spawn`、`spawn_with`、`spawn_from`（被 lifecycle 的 `it_lifecycle_startup_ends_what_the_last_run_left_running`、`it_lifecycle_read_marks_and_presence_reach_the_index`、`it_lifecycle_host_exit_and_sigterm_exit_in_order`、`it_lifecycle_startup_failures_exit_3_to_6`、`it_project_add_trust_through_the_gateway`、`it_audit_retention_records_land_and_last_90_days`、`it_clock_advance_returns_once_the_host_settled`、`it_seed_writes_sessions_the_normal_way`、`it_gw_blob_put_and_get_through_the_gateway`、`it_idle_exit_once_nobody_is_connected_for_the_idle_time` 用） | 进程级，测试二进制拉起 `cupper-host`，`test_config` 没有 `agents`，`HOME` 是测试进程真实的 | 常见位置的 `/opt/homebrew/bin/claude`，以及真实 `HOME` 下的 `~/.local/bin` 等；`it_gw_blob_put_and_get_through_the_gateway` 的 `test_config` 是 `{}`，还会运行用户真实的登录 shell，PATH 里也有 Homebrew | `spawn_from` 统一隔离：`HOME` 设为数据目录旁边的 `home`，`login_env` 缺省 `{}`，`cli_root` 缺省为数据目录旁边的 `root`（都在临时目录里） |
| lifecycle 的 `bundled_host`（IT-runtime-startup；T201 的 IT-runtime-first-prompt-waits 也用它） | 进程级，`spawn_from` 拉起复制进 `CupperHost.app` 的宿主，没有 `agents`，走运行时分支 | 同上 | 经 `spawn_from` 一并隔离；T201 的用例本来就用 `cli_path` 指向假 agent，隔离后照样找到假 agent |
| `fake_agent_host_from`（lifecycle 的 IT-self-check 用复制的宿主） | 进程级，`spawn_from`，有 `agents` | 不走发现 | 经 `spawn_from` 一并隔离，行为不变 |
| `tests/startup.rs` 的 `unit_release_features_host_does_not_read_stdin` | 进程级，**不带任何特性**的宿主，没有任何注入点，不读 stdin | 运行用户真实的登录 shell，对 `/opt/homebrew/bin/claude` 执行 `--version` | 启动前建好 `host.db`，`agents.claude.cli_path` 设为测试目录里不存在的路径（设置是生产功能，发布构建也读）；宿主环境的 `HOME` 设为测试目录，去掉 `SHELL`（退回固定 PATH，不运行登录 shell） |
| `supervisor` 的 `it_clock_advance_the_host_registers_every_kind_of_actor` | 进程内 `Host::start`，`login_env` 只有 PATH，没有 `agents`；宿主读的是测试进程自己的 `HOME`，没法按用例换 | 对 `/opt/homebrew/bin/claude` 执行 `--version` | `test_config.agents` 指向假 agent。这个用例只看登记的 actor 种类，从不启动适配器 |
| `apps/desktop` 的 `launch.test.ts`（`pnpm test:launch`，手动运行，不进 PR） | 经桌面主进程的 `HostLauncher` 拉起 e2e 构建的 `CupperHost.app`，`test_config.json` 是 `{}` | 运行用户真实的登录 shell，对 `/opt/homebrew/bin/claude` 执行 `--version` | `test_config.json` 写 `login_env`（系统目录）和 `cli_root`；传给宿主的环境里 `HOME` 设为测试目录 |

### 不走发现的（配了 `agents` 或 `in_process`）

| 位置 | 怎么拉起宿主 | 为什么不会走到真实 CLI |
|---|---|---|
| `fake_agent_host`、`fake_agent_config`：driver_k、gateway_sync、crash_concurrency（含 Seatbelt 用例直接调 `start`）、perf、lifecycle 里用假 agent 的用例 | 进程级 | `fake_agent_config` 总是带 `agents.claude`，`login_env` 缺省 `{}`。经 `spawn_from` 的这次也一并隔离 `HOME` |
| interaction_trust 的 `host` | 进程级，`spawn_with` | 自己写 `agents.claude`；经 `spawn_with` 一并隔离 |
| gateway_sync 的 `it_gw_sync_adapter_gets_only_stdio_and_allowed_env` | 进程级，直接调 `start` | `fake_agent_config`；`HOME` 仍是真实的，只影响假 agent 拿到的环境，用例只核对变量名 |
| gold_b | 进程内，`in_process` | 适配器在测试进程里回放，不拉起程序 |
| compat 的假模式（`--fake`，CI 的 compat-framework） | 进程级，`spawn_from` | `fake_agent_config`；隔离后不变 |
| compat 的真实模式（`cargo xtask compat`） | 进程级，真实宿主包 | 真实 CLI 本来就是它的目的（任务书"不做"）。为了让它照旧找到用户的 CLI 和登录态，改为经 `start` 拉起、不隔离；只读核对，没有运行 |
| `tools/e2e` 的夹具（Playwright 经 UI 拉起宿主） | e2e 版主进程读 `CUPPER_E2E_DIR/test_config.json` 组装载荷 | 总是写 `agents.claude`（假 agent），没有用例覆盖它；`login_env` 只有系统目录。原来没有 `cli_root`，这次补上，`agents` 万一被覆盖时也看不到真实的常见位置。`HOME` 是 Electron 进程的真实 `HOME`，宿主沿用它；只在不配 `agents` 时才影响发现，改它会连带改 Electron 自己的 `HOME`，所以不动 |
| `cupper-desktop-native` 的 `native.test.ts`、`tests/gateway.rs` | 拉起 `cupper-gateway-fixture` | 夹具只有网关，不启动 supervisor，没有发现 |
| `tests/startup.rs` 的其他用例 | 进程级 | `--build-info`、`--self-test`、未知参数、坏载荷：都在发现之前退出 |
| `tests/migrations.rs` | 不拉起宿主 | — |
| CLI（`cupper-cli`）的测试 | 不拉起宿主 | 用脚本化的假服务端；lifecycle 里的 `cupper` 用例连的是用假 agent 的宿主 |

## 规模估计（开工前）

停止线 200，硬停止线 260（`cargo xtask size`，含测试，不含文档）。

| 模块 | 估计新增 | 内容 |
|---|---|---|
| `startup/test_config.rs` | 15 | `cli_root` 字段（只在 test-harness 里有）；解析用例；非 test-harness 构建没有这个键的断言 |
| `driver/locate/mod.rs` | 30 | 绝对常见位置经 `cli_root` 改根；生产路径逐字不变的用例 |
| `supervisor/mod.rs` | 10 | `Finder` 带上 `cli_root`，`discover` 套上；进程内用例配 `agents` |
| 假 agent | 5 | `--version` 像 `claude --version` 那样打印版本 |
| 测试夹具 | 20 | `spawn_from` 隔离 `HOME`、`login_env`、`cli_root`；compat 真实模式不隔离 |
| `startup.rs` 发布构建用例 | 15 | 预建 `host.db` 和 `cli_path`，`HOME`、`SHELL` |
| IT-host-tests-no-real-cli | 40 | 夹具拉起的宿主找不到 CLI；`cli_root` 下假常见位置里的探针被找到，`--version` 读到的是探针的版本 |
| TS（`launch.test.ts`、e2e 夹具） | 5 | |
| 合计 | 约 140 | |

## 实际行数

`cargo xtask size`：新增 168、删除 21（停止线 200）。比估计多约 30 行，多在 rustfmt 展开的用例代码上：第一版新增 189，把单元用例的期望值改成一个格式串、compat 的分支和 IT 的几处调用收紧后降到 168。

## 修前对照（真实 CLI 的那一次 `--version`）

2026-10-28，修复前（夹具和宿主都没改）只加了 IT-host-tests-no-real-cli 的前半段（夹具拉起的宿主不配 `cli_path`，断言 `agents.updated` 里没有 CLI），单独运行这一个用例：

- 第一次没有走到发现：用例在宿主建好 `host.db` 之前就去读它，当场 panic，`Running` 随即发 SIGTERM。宿主装 SIGTERM 处理器是在发现之后（`Host::start` 末尾），这时还是默认动作，宿主立即结束；`host.db` 都还没建，迁移在发现之前，所以没有执行真实的 CLI。改成先连上宿主（宿主开始监听时发现已经做完）再读。
- 第二次就是对照本身，失败，宿主对本机的 Claude 执行了一次 `--version`：

  ```
  left: "{\"path\":\"/opt/homebrew/Caskroom/claude-code@latest/2.1.296/claude\",\"version\":\"2.1.296\",\"compat\":\"verified\",\"install\":\"native\"}"
  right: "-"
  ```

  找到的是常见位置 `/opt/homebrew/bin/claude`（Homebrew cask 的链接）解析出的原生二进制，版本 2.1.296 来自那次 `--version`。

此后修复，再没有运行会走到真实 CLI 的东西。修后这个用例通过：夹具拉起的宿主没有 CLI；`cli_root` 下假常见位置里的探针被找到，版本是探针自己打印的。

## 取舍

- **`cli_root` 的形状**：代替 `/`，只改常见位置表里的绝对路径；`~/` 开头的照旧跟 `HOME` 走，由夹具设宿主进程的 `HOME`。这样一个探针放在 `<cli_root>/opt/homebrew/bin/claude` 就能证明改根真的生效。另一种是整张常见位置表可以替换，但那样测试就看不到表里写的位置了。
- **生产路径逐字不变**：`CliLocator` 的 `root` 字段和 `under` 只在 test-harness 里编译，`absolute` 在别的构建里就是 `PathBuf::from(path)`。`test_config` 整个模块本来就只在 test-harness 里编译。UNIT-cli-resolver-claude 补一个用例，三种特性集都跑：默认的定位器看的就是表里写的位置；test-harness 里 `under` 只移动绝对的那几处。非 test-harness 构建不读它：发布构建的绊线用例（`unit_release_features_payload_is_not_read`）的载荷里加上 `cli_root`，仍断言 stdin 没被读。
- **隔离放在 `spawn_from`**：`spawn`、`spawn_with`、`fake_agent_host_from`、`bundled_host` 都经它，以后新加的用例默认就隔离。只有 compat 的真实模式要用用户的 `HOME`、登录 shell 和 CLI，它改为经 `start` 拉起；假模式照样经 `spawn_from`。`HOME` 放在数据目录旁边的 `home`，`cli_root` 是旁边的 `root`（不建，也不存在）。
- **探针的 `--version`**：解析只认 Mach-O，所以探针只能是假 agent。假 agent 以前把 `--version` 当成缺值的参数报错退出，宿主读不到版本，看不出 `--version` 确实执行在探针上。让它像 `claude --version` 那样打印 `<版本> (cupper-fake-agent)`，宿主状态里的版本就是探针打印的。探针用硬链接，不复制，realpath 仍在测试目录下；用例先自己运行一次探针，系统对新文件的首次扫描（T196）落在这里，不落在宿主 5 秒的版本检查里。探针目录放在 `CARGO_TARGET_TMPDIR`：发现不信任 `$TMPDIR`，放在那里找不到。
- **发布构建的用例**没有注入点，只能写设置：`agents.claude.cli_path` 指向测试目录里不存在的文件，它是唯一候选，结果是 `cli_not_found`；去掉 `SHELL`，不运行登录 shell（退回固定 PATH，只用来找 git）。
- **进程内的用例**改不了测试进程的 `HOME`，所以配 `test_config.agents`，不隔离发现；它只核对登记的 actor 种类。
- **e2e**：只补了 `cli_root`。`HOME` 是 Electron 进程自己的，改它会连带影响 Electron；夹具总是配假 agent，不走发现。

## 期望值有变的用例

没有。所有已登记的用例期望值不变；T201 的 IT-runtime-first-prompt-waits 用 `cli_path` 指向假 agent，合并后宿主读到的 CLI 版本从空变成 `0.0.0`，那个用例不核对版本。

## 验证（本机，单份运行）

- `cargo test -p cupper-host --features test-harness --lib -- unit_cli_resolver unit_startup_payload it_clock_advance_the_host_registers`：17 个通过（改名合并后又跑 `unit_startup_payload unit_cli_resolver`，15 个通过）。
- 不带特性：`--test startup`（5 个，含 `unit_release_features_host_does_not_read_stdin`）、`--lib -- unit_release_features unit_cli_resolver`（6 个）通过。
- test-harness：`--test lifecycle`（18 个，含 IT-host-tests-no-real-cli、IT-runtime-startup）、`interaction_trust`（7）、`driver_k`（10）、`gateway_sync`（11）、`startup`（4），各跑一次，全部通过。
- `cargo xtask compat --fake`：除 `steer`（不适用）和两个只限真实模式的剧本外都是 Pass，含经 `start_host` 重启宿主的 `host-restart`。
- `cargo test -p cupper-fake-agent`：通过。
- `pnpm --dir apps/desktop test:launch`：2 个通过。
- `cargo clippy`（`-p cupper-host` 不带特性、`dev-auth`、`test-harness`、`--all-features`，含 fake agent）`-D warnings`、`cargo fmt --check`、两个 TS 文件的 eslint、prettier、tsc：通过。
- 没有在本机跑：`crash_concurrency`（含高负载下的 `kill -9`）、`perf`（每晚）、Playwright e2e：交给 GitHub CI。
