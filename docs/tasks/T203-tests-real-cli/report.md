# T203 汇报

- 执行者：子 agent（Rust 工程师）
- 分支：`test/T203-tests-real-cli`
- 完成日期：2026-10-28

## 做了什么

1. **清单**（全文在 `docs/tasks/T203-tests-real-cli/notes.md`）。会走到生产 CLI 发现的有 6 处：
   - `tests/common` 的 `spawn`、`spawn_with`、`spawn_from`：lifecycle 的 10 个用例在用。其中 `it_gw_blob_put_and_get_through_the_gateway` 的 `test_config` 是 `{}`，还会运行用户真实的登录 shell；
   - lifecycle 的 `bundled_host`（IT-runtime-startup）；
   - `startup.rs` 的发布构建用例 `unit_release_features_host_does_not_read_stdin`：没有注入点，会运行真实登录 shell 和 `claude --version`；
   - supervisor 的进程内用例 `it_clock_advance_the_host_registers_every_kind_of_actor`；
   - `apps/desktop` 的 `launch.test.ts`（`pnpm test:launch`，手动运行），`test_config` 是 `{}`。

   不走发现的也逐处写了原因：配 `agents` 或 `in_process` 的各测试、compat 假模式、e2e 夹具、网关夹具、CLI 测试。compat 真实模式本来就要用真实 CLI，不在范围内。

2. **为什么要动宿主代码，以及加了什么**：本机的 Claude 在 `/opt/homebrew/bin/claude`。这是常见位置表里写死的绝对路径，`/usr/local/bin/claude` 也是。测试目录里的 `HOME` 和 `login_env` 都移不开它们。按任务书的要求先汇报了编排者，经同意后新增了注入点 `test_config.cli_root`：在常见位置的绝对路径里代替 `/`。
   - 它只在 test-harness 构建里编译：`test_config` 整个模块本来就是 test-harness 专用的；`CliLocator` 的 `root` 字段和 `under` 都带 cfg。
   - 生产路径逐字不变。UNIT-cli-resolver-claude 新增 `unit_cli_resolver_claude_common_locations`，在三种特性集下都跑。
   - 发布构建的绊线用例 `unit_release_features_payload_is_not_read` 的载荷里加上了 `cli_root`，仍断言 stdin 没被读。
   - 5.9.9 表的那一行按约定留给你加，我没有改设计文档。
   - `test_config` 在 cupper-host 里，不在 cupper-protocol 里，所以没有 schema 和 TS 生成物要更新。

3. **隔离测试**：
   - `spawn_from` 统一隔离：宿主的 `HOME` 设为数据目录旁的 `home`；没给 `login_env` 时补 `{}`；没给 `cli_root` 时设为数据目录旁的 `root`。这些目录都在 `$TMPDIR` 里，发现本来就不信任临时目录，所以在那里什么也找不到。用例只能经 `test_config.agents` 或 `cli_path` 拿到 CLI。
   - compat 真实模式改为经 `start` 拉起，不隔离，假模式不变。
   - 发布构建用例：预建 `host.db`，`agents.claude.cli_path` 指向不存在的文件，设置测试用的 `HOME`，去掉 `SHELL`。
   - 进程内用例改配 `test_config.agents`。
   - `launch.test.ts` 补上 `login_env`、`cli_root` 和 `HOME`；e2e 夹具补上 `cli_root`。
   - 假 agent 收到 `--version` 时打印 `0.0.0 (cupper-fake-agent)`，用来证明宿主确实对探针执行了 `--version`。

4. **新增 IT-host-tests-no-real-cli**（在 lifecycle.rs）：
   - 共用夹具拉起的宿主，`agents.updated` 里没有 CLI；
   - 把 `cli_root` 设到发现信任的目录（`CARGO_TARGET_TMPDIR`），在其下的 `opt/homebrew/bin/claude` 放一个硬链接到假 agent 的探针。宿主找到的正是这个探针，读到的版本也是探针自己打印的。

## 关键取舍

- `cli_root` 只移动绝对位置，`~/` 位置跟着 `HOME`。这样探针放在 `<root>/opt/homebrew/bin/claude` 就能证明改根生效；如果把整张表换掉，测试就看不到表里写的位置了。
- 隔离放在 `spawn_from`，以后新加的用例默认就隔离；只有 compat 真实模式显式不隔离。
- e2e 只补了 `cli_root`、没改 `HOME`：`HOME` 是 Electron 进程自己的，改它会连带影响 Electron；而且 e2e 夹具总是配假 agent，不走发现。
- 期望值没有变的用例。T201 的 IT-runtime-first-prompt-waits 用 `cli_path` 指向假 agent，现在 CLI 版本会读成 `0.0.0` 而不是空，但那个用例不核对版本。

## 验证方式

- **修前对照**（已记进 notes.md）：只加了新用例的前半段，单独运行。
  - 第一次是用例自身的错误：它在宿主建好 `host.db` 前就去读，当场失败。宿主的 SIGTERM 处理器装在发现之后，那时它被默认动作结束，数据库都还没建，没有走到发现，没有执行真实 CLI。
  - 第二次是对照本身，失败，找到 `/opt/homebrew/Caskroom/claude-code@latest/2.1.296/claude`，版本 2.1.296。这就是任务书允许的那唯一一次真实 `--version`。此后没再运行任何会走到真实 CLI 的东西。
- **修后，本机各单份运行一次，全部通过**：
  - test-harness：lib 相关单元用例；lifecycle 18 个（含新用例）、interaction_trust 7 个、driver_k 10 个、gateway_sync 11 个、startup 4 个；
  - 不带特性：startup 5 个，lib 相关单元用例 6 个；
  - `cargo xtask compat --fake`：全部 Pass，`steer` 不适用、两个剧本只限真实模式；
  - `cargo test -p cupper-fake-agent`；`pnpm --dir apps/desktop test:launch`（2 个）；
  - clippy（不带特性、dev-auth、test-harness、all-features，`-D warnings`）、fmt、两个 TS 文件的 eslint、prettier、tsc。
- **没在本机跑**：crash_concurrency（含高负载下的 kill -9）、perf、Playwright e2e，交给 CI。
- **规模**：`cargo xtask size` 为 +168 −21，停止线 200。
- **与 main 合并**：用 `git merge-tree` 只读核对过，与当前 origin/main（已含 T201、T066）合并无冲突；合并后 compat 的 `start_host` 和 T201 的 `bundled_host` 都对得上。
- **CI**：PR https://github.com/boris-w/cupper/pull/18 ，运行 https://github.com/boris-w/cupper/actions/runs/40639547328 （head d2dcf18，pull_request，success），8 个检查全部通过。
- **验收标准**：清单完整，每处都改好或写明了原因 ✔；IT-host-tests-no-real-cli 通过且有修前对照 ✔；本机只单份运行 ✔；CI 8 个检查通过 ✔。

## 提交列表

```
d2dcf18 docs(T203): notes: the inventory, the pre-fix run and the trade-offs
3a954f3 test(host): add IT-host-tests-no-real-cli
218ecfa test(host): keep CLI discovery in tests inside the test
92eca72 test(fake-agent): answer --version as claude does
c9eee98 test(host): add test_config.cli_root for CLI discovery's common locations
```

## 遗留问题 / 范围外发现

1. `launch.test.ts` 的 `test_config` 没有 `runtime.registry_url`。e2e 宿主包是 app，每次启动都会安装运行时，于是向公共 npm registry 发请求（请求里不含个人信息）。e2e 夹具在 T178 已经指向一个关闭的本地端口，这里没有，我没顺手改。
2. e2e 拉起的宿主沿用 Electron 进程真实的 `HOME`，宿主跑 git 时会读用户的全局 git 配置。这和 CLI 发现无关，只记一笔。
3. gateway_sync 的 `it_gw_sync_adapter_gets_only_stdio_and_allowed_env` 直接调 `start`，宿主仍用真实 `HOME`。它配了假 agent，不走发现，只影响假 agent 拿到的 `HOME` 值，用例只核对变量名。
4. compat 真实模式的拉起方式只改了调用点（行为与原来的 `spawn_from` 相同），只读核对过，没有运行：它要用真实 CLI 并消耗额度。
