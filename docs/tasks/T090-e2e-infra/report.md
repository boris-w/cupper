# T090 汇报

- 执行者：子 agent（测试工程师）
- 分支：`feat/T090-e2e-infra`
- 完成日期：2026-10-20

## 做了什么

按裁定后收窄的范围（方案一）完成，验收各条都通过。

- **套件骨架**：
  - 新建工作区包 `tools/e2e/`（`@cupper/e2e`），用的是 `@playwright/test` 1.64.1，与截图回归同版本；
  - workers 固定为 2，`@smoke` 用标签标出；
  - 根目录新增 `pnpm test:e2e`；
  - globalSetup 先检查 App、e2e CLI、假 agent 是否都已构建，再清理被杀掉的运行留下的东西；
  - globalTeardown 清理 `dev.cupper.app.e2e` 的 plist：先 `defaults delete`，再删文件（T074 的交接）。
- **夹具**（`fixture.ts`，进程表在 `procs.ts`）：
  - **准备**：
    - 每个用例建一个临时根目录，布局照 `CUPPER_E2E_DIR`：`data/`、`logs/`、`harness_key`（0600）、`test_config.json`；
    - `test_config` 默认把假 agent 配成 `agents.claude`，带 `login_env: {}` 和 `notifier: record`，可以用 `test.use({ testConfig })` 覆盖；
    - 建一个临时 git 仓库，用固定的作者信息，不读本机的 git 配置。
  - **启动**：`_electron.launch` 启动 e2e 构建的 App，启动后立即替换原生确认框。
  - **等待连接**：`connected()` 等窗口的数据连接出现在 `host/status` 的 `clients` 里，并记下宿主的 pid。
  - **结束时**：
    1. 结束 CLI；
    2. 按关机路径退出 App（`powerMonitor` 的 `shutdown`，不弹退出确认）；
    3. 给宿主发 SIGTERM；
    4. 本用例拉起的进程 5 s 后还在的，先 SIGKILL，再让用例失败。
  - **识别进程**：用 pid 加启动时间。宿主要求命令行正好是"e2e 宿主 `--data-dir` 本用例的 data"，只认本用例的那一个。
  - **被杀的运行**：每个根目录记下属主（worker）和本用例拉起的进程。下一次运行时，属主已不在的根目录连同它的进程（含子孙进程）一起清掉。这是照 T008 修复 1、T063 的守卫做法。
- **CLI 包装**（`cli.ts`）：
  - 用 `host-bundle --kind e2e` 构建的 e2e CLI，以 `test_harness` 身份连接；
  - key 从密钥文件经 fd 3 传入；`--json` 的输出按行解析；
  - 有 `run`（返回退出码、输出行、stderr）和 `ok`（必须成功，返回第一行）；
  - 结束时回收所有还在运行的 CLI。
- **原生确认框**（`dialogs.ts`）：
  - 在主进程里替换 `dialog.showMessageBox`，并记录选项（`message`、`detail`、`buttons`、`defaultId`、`cancelId`、`checkboxLabel`）；
  - 每次调用一直挂起，直到用例调 `answer(序号, 按钮, 复选框)`，确认框不会真的弹出来抢焦点；
  - `next(from)` 等下一个确认框出现。
- **用例**：
  - **E2E-single-instance**：
    - 先隐藏窗口，再用同一目录启动第二个实例；
    - 第二个实例以 0 退出，第一个实例的窗口重新显示，`main.log` 里有记录，宿主 pid 不变。
  - **E2E-quit-confirm**，三条：
    - 有运行中的回合（假 agent 的 `wait_cancel`）时退出：弹出确认框，文案与 `@cupper/i18n` 按 App 语言算出的完全一致；取消后 App 和回合都还在，再退出会再问；确认后 App 退出，宿主收到 `host/exit` 后结束；
    - 空闲时退出：不弹框，宿主留着，App 的连接都已断开；
    - 关机路径：有回合也不问，App 什么都不发给宿主。
  - **`@smoke` 夹具自检**：App 拉起宿主，窗口连上，CLI 能看到它们。没有编号，任务书允许。
- **`cargo xtask ci e2e`**（`crates/xtask/src/ci/jobs/e2e.rs`）：
  - 步骤依次为：
    1. `pnpm install`；
    2. 暂存目录不在时先跑 `runtime-pack`，只下载 manifest 钉住哈希的公开物料；
    3. `cargo build -p cupper-fake-agent`；
    4. `pnpm build:e2e`：App、`CupperHost.app` 和 e2e CLI 每次都由当前提交构建；
    5. 把三者的摘要写进摘要的 `artifacts`；
    6. 跑 Playwright。
  - 摘要按路径排序；文件记内容的 sha-256，符号链接记指向，不跟随。
  - 作业持机器锁。`ci pr` 只跑 `@smoke`，并照 `ui` 作业按 `E2E_PATHS` 过滤改动路径；`ci e2e` 和 `ci nightly` 跑全集。
  - `JobCtx` 和 `JobReport` 加了 `artifacts` 字段。

## 关键取舍

- **规模**：新增 963 行、删除 15 行（不含 lock 和文档）；上限约 1,000，硬线 1,100。
  - 写到 1,066 行时超过了上限。
  - 我把 CLI 包装的 `repl`、`watch`、以 `cli` 身份连接，以及 `showMessageBoxSync` 的替换都拿掉了。本任务没有用例用到它们，按"没人用的代码不进 main"，留给 T167 跟用到它们的辅助一起加。
  - 这次删减是单独一个 refactor 提交，没有改写历史。
- **测试结束时按关机路径退出 App**：这样回合还在运行时也不会被退出确认挡住；"关机路径不问"本身也有一条用例覆盖。
- **用 e2e CLI**：选的是 `host-bundle` 构建、在宿主旁边的那个 CLI（test-harness，`BUILD` 为 `E2e`，按 harness 证明校验宿主），不是 `target/debug` 下的。harness 有全部 scope，建会话不需要提权。
- **确认框挂起等用例回答**：比预先排好答案更灵活，"先取消、再确认"就是这样做出来的。
- **范围外的小改动**：`@cupper/i18n` 的 8 行 JSON import 加了 `with { type: "json" }`。
  - 原因：用例要在 Node 里直接加载 i18n 算出期望文案，Node 的 ESM 加载器要求 JSON 导入带这个属性。
  - 已验证 Vite、Vitest、`tsc` 都接受：i18n 的单元测试、e2e 构建、ts 作业都通过。
- **新增单元测试编号**：`UNIT-ci-e2e`，已补进 `tests/ids.toml` 的 T090 段。原来"未实现的作业被拒"那条用例拿 `e2e` 举例，改成了 `perf`。

## 验证方式

- **验收各条**：
  - **E2E-single-instance、E2E-quit-confirm**：通过（后者三条）。
  - **每个辅助至少被一条用例调用**：本任务的辅助就是夹具、CLI 包装、确认框替换，全部被上面的用例用到。
  - **`cargo xtask ci e2e`**：通过，3 项检查加 5 条用例。第一次失败是磁盘满（codesign 报 internal error），清出空间后重跑通过。
  - **`ci pr` 里的 `@smoke` 子集**：通过。
  - **连跑 10 次**（workers 为 2）：全绿，每次 3.1–4.3 s。修了清理路径之后又连跑了 10 次，同样全绿。每次跑完都检查过：没有残留进程，`$TMPDIR/cupper-e2e/` 为空，plist 不在。
  - **测试名**：以编号开头，编号都在 T090 段里。
- **被杀的运行能被清理**：用一条临时的、会停住的用例（没有提交），在回合运行中 `kill -9` Playwright 进程。App、宿主、假 agent 和根目录都留了下来；下一次运行把它们全部清掉了。
- **`cargo xtask ci pr --record docs/tasks/T090-e2e-infra/ci/`**：在干净的 worktree 里通过。摘要 `pr-3135501eea51.json` 已提交，内含三个产物的摘要，检查过没有本机路径和用户名。

  | 作业 | 结果 |
  |---|---|
  | rust | 1,856 通过 |
  | protocol | 185 通过 |
  | ts | 436 通过 |
  | license | 1 通过 |
  | testdata-scan | 3 通过 |
  | ui | 16 通过（i18n 有改动，所以跑了） |
  | e2e | 4 通过（`@smoke`） |

## 提交列表

```
dd16d77 docs(task): T090 CI summary
3135501 docs(task): T090 notes: sizes, choices, runs
a763d78 fix(e2e): sweep a killed run's roots by their real path, with descendants
c53f7e9 ci: the e2e job
7cc8086 refactor(e2e): keep the CLI wrapper and the dialog stand-in to what is used
5a01b05 test(e2e): E2E-single-instance and E2E-quit-confirm
70f8489 test(e2e): the suite, its fixture, the CLI and the native dialogs
e88284c docs(task): T090 size estimate and split proposal
```

## 遗留问题 / 范围外发现

1. **交给 T167 的**：CLI 包装的 `repl`、`watch`、以 `cli` 身份连接（提权要用），随用到它们的辅助一起加。
2. **交给 T093 的**：App 还调用 `dialog.showOpenDialog`（添加项目时选文件夹），本任务没有替换。T093 走这条路径时要另外替换。
3. **只在 `ci e2e` 和每晚跑的编号**：E2E-single-instance、E2E-quit-confirm 在 `ids.toml` 里标的是 `nightly`，所以 `ci pr` 只跑夹具自检。
4. **作业保留的构建产物**：`ci e2e` 不删 `apps/desktop/dist/pack/e2e`（约 470 MB）、`target/runtime-pack`（约 190 MB）和 `target/host-bundle`（约 3.7 GB）。这些是有意留下的：下次构建直接覆盖，或者复用 cargo 和下载的缓存。每个用例的临时根目录都会删掉，Playwright 的输出目录每次运行也会自己清空。
5. **磁盘满期间我清理了什么**：只删了本 worktree 的 `target/debug`、`target/shuttle`、`target/aarch64-apple-ios-sim`，没有动别的 worktree。之后的 `ci pr` 已全部重新构建。
