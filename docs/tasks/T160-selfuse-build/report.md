# T160 汇报（第 4、5 轮，最终）

- 执行者：子 agent（构建与桌面工程师）
- 分支：`build/T160-selfuse-build`（PR #40：https://github.com/boris-w/cupper/pull/40 ）
- 完成日期：2026-11-03

第 1–3 轮的汇报在本目录的 `report-r1.md`、`report-r2.md`；第 3 轮的分析和修正见 `notes.md`。

**状态：完成。** 第 5 轮 selfuse 三个用例全过，PR #40 在 5a490a3 上的 8 个检查全过。本机比远端多一个只改文档的提交 48961e0（安装说明），请推送；推送后 CI 会再跑一遍。

## 运行链接

| 轮次 | 运行 | 提交 | 结果 |
|---|---|---|---|
| 4 | nightly（job=selfuse）https://github.com/boris-w/cupper/actions/runs/41604114489 | 7a4f9ad（已合入 main，含 T225） | IT-selfuse-build、E2E-selfuse-coexist 通过；E2E-selfuse-replace 卡在用例自己的等待上 |
| 5 | nightly（job=selfuse）https://github.com/boris-w/cupper/actions/runs/41606572821 （selfuse 作业 https://github.com/boris-w/cupper/actions/runs/41606572821/job/125817775942 ） | 5a490a3 | **3 个文件 7 例全过**：IT-selfuse-build 5 例，E2E-selfuse-replace 16.7 s，E2E-selfuse-coexist 10.3 s |
| 5 | PR ci https://github.com/boris-w/cupper/actions/runs/41606577120 | 5a490a3 | 8 个检查全过 |

## 第 4 轮：为什么等不到重连结果

证据来自构件 `selfuse-macos-15-logs`，我下载到本机一个新的空目录里查看，看完已删除。

- **日志写法没有变，重连也确实发生了**：
  - 旧宿主以 upgrade 退出后，旧界面在 08:48:45.091 记下 `reconnecting`。
  - 它拉起的宿主因为旧宿主还占着锁，以退出码 3 退出。
  - 08:48:45.597，control 连接停在 `failed: host_unreachable`。
- **等不到的原因在用例**：control 连接失败后，窗口桥不会再去连接，main.log 里就没有 `window connected` 或 `window connection failed`。用例要求 control 和窗口两个结果都出现，所以一直等到超时。第 1、2 轮窗口都去连过，是时序不同。
- **前半段都正常**：
  - 换包前等到了运行时装好（claude-agent-acp 在 43.458 装好）；
  - 新 CLI 以退出码 4 拒绝旧宿主；
  - 旧宿主写下 `host.pending_upgrade`，以 upgrade 有序退出。

## 删掉这段等待的理由

- 这段等待只是为了避开"重连途中退出会卡住"，现在 T225 已修好，并由 E2E-selfuse-quit-during-reconnect……更正：由 **E2E-quit-during-reconnect** 守着。
- 它还假定一定会出现窗口连接的结果，这个假定本身不成立（见上一节）。
- 删掉后，旧宿主一退出就立即退出旧 App，正是用户"忘了先退出就换包、然后按 ⌘Q"的情形。
- 删掉不会掩盖别的问题：
  - 旧 App 的退出仍是硬断言，超时就判失败，再用第二次 SIGTERM 清场；
  - finally 里的退出失败用 `expect.soft` 报告，不会盖住用例本身的失败。

改动在提交 a42b732。

## T225 的修复在自用构建上也生效

- 第 4 轮里，旧 App 停在 `failed: host_unreachable` 时收到 SIGTERM，0.2 s 内就退出了。main.log 依次是 `before-quit` → `before-quit` → `will-quit` → `quit`。修前，这个状态下必定卡住。
- 第 5 轮里，旧 App 在旧宿主退出后马上被退出，这时它可能正在重连；退出正常完成，用例通过。

## codex-acp 组件

main 新增了运行时组件 codex-acp（`runtime::Component::CodexAcp`），也会从 App 包里复制文件。换包前等待的组件清单已加上它：node、esbuild-wasm、claude-agent-acp、codex-acp 都要有 `.installed.json`。改动在提交 a42b732。

## 文档与最终行为是否一致

两份文档都核对过，与最终行为一致：先退出再替换；忘了先退出时，⌘Q 也能正常退出。

- `docs/user/selfuse-install.md`：
  - "手动更新"本来就是先 ⌘Q、再删旧的、再复制新的。
  - "忘了退出就替换了"一段按 runner 上看到的情况改了（提交 48961e0）：
    - 补上一个触发条件：宿主首次运行、还在从包里复制运行时的时候，也会进入待升级；
    - 写明旧界面可能接上新宿主，也可能停在"连不上宿主"；
    - 两种情况下 ⌘Q 都能退出，退出后再打开即可。
- `docs/user/selfuse-notice.md`：没有和退出行为矛盾的地方，不用改。

## 最终规模

`cargo xtask size`：+812 −67。停止线 850（已放宽），硬停止线 1000。

| 部分 | 行数 |
|---|---|
| apps/desktop | +668 −34 |
| crates/xtask | +92 −14 |
| crates/cupper-host | +37 −13 |
| .github | +14 −6 |
| package.json | +1 |

## 本轮提交（第 3 轮之后）

```
48961e0 docs(user): what a replaced, still running Cupper Local does, as the runner showed
5a490a3 docs(task): record T160's fourth runner run
a42b732 test(desktop): quit the replaced Cupper Local straight away, now that T225 is in
7a4f9ad Merge branch 'main' into build/T160-selfuse-build（编排者）
c8bbe18 docs(task): record T160's third runner run and the test fixes it led to
f1973f2 test(desktop): replace Cupper Local once its runtime is in, and keep both failures
```

## 设计、计划修改建议（汇总，仍然有效的都在这里）

1. **5.1.5 命令表**：
   - 加 `pnpm build:local`：构建 Cupper Local，前提是先跑 `cargo xtask runtime-pack`，产物在 `apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app`；
   - 加 `pnpm --dir apps/desktop run test:selfuse`：只经 `cargo xtask ci selfuse` 在托管 runner 上运行。
2. **5.1.6 CI 表**：
   - 加 `selfuse` 作业：macos-15，每晚运行，失败开 issue。步骤是构建自用构建、"下一版"自用构建（宿主和 CLI 用 dev profile 编译）和开发构建，再跑三个用例，App 日志进构件。不在托管 runner 上时跳过。
   - `nightly.yml` 有了可选输入 `job`，手动触发时可以只跑一个作业。
3. **5.1.2 目录**：
   - electron-builder 配置现在是 release、local、e2e、dev 四份；
   - `apps/desktop/build/` 下有两份 entitlements；
   - 新增 `apps/desktop/selfuse/`。
4. **5.7.8 Fuses 一行**：
   - Electron 44 有第 9 根 fuse（`WasmTrapHandlers`），@electron/fuses 1.8.0 和 electron-builder 26.17 都设不了，出厂是开启。建议写明"保持出厂"，T161 照此断言。
   - 这一行写着"`asarUnpack` 只有 `cupper_native.node`"，与 5.1.6、5.10.7 的"为空"矛盾。
5. **5.7.9 自用构建一列**：
   - Vite 的 mode 名是 `self-use`，因为 Vite 占用了 `local`；
   - 发布构建和自用构建的包都不带 source map（`!**/*.map`）。原因是主进程的 source map 带着 e2e 专用代码的源码，会让测试钩子扫描误报。
   - Electron 44 不会启动 GPU helper 和 Plugin helper，GPU 进程跑在普通 helper 里。
6. **5.10.1 包结构**：
   - `LSMinimumSystemVersion 14.0` 已过时，应为 15.0；
   - Info.plist 带着 Electron 默认的摄像头、麦克风、蓝牙（两项）、音频采集用途说明。agent 触发这类授权时会显示成 Cupper。是否在打包时删掉，交 T161、T180 或安全工程师决定；"自用前须知"里已写明一律拒绝。
7. **5.10.2 签名表**：可以补上以下事实：
   - 库（framework、dylib、原生扩展）不带 entitlements；
   - `chrome_crashpad_handler`、`app_shim_loader`、`ShipIt` 拿的是 `entitlementsInherit`；
   - helper 的 identifier 是 `<appId>.helper`、`.helper.Renderer`、`.helper.GPU`、`.helper.Plugin`。
8. **5.4.1 与 5.2.11**：worktree 根目录的默认值现在按数据目录推出。e2e 那一行"同一临时目录下"的实际位置是 `<数据目录>/worktrees`。
9. **5.5.7 升级**：在 keyed 构建（自用、开发、e2e）里，宿主因待升级退出后，旧界面会自动从磁盘拉起新宿主，新旧两版混用。建议考虑改为提示重启 App（T160 遗留第 7 条，你已列为另议）。
10. **P-4（T160 之后改表结构要附样例库和迁移用例）还缺**：
    - (a) 按版本存放的样例数据库；
    - (b) 通用迁移用例：把每个样例迁到最新，检查 `schema_version`、`integrity_check`、`foreign_key_check` 和数据保留，再让宿主在上面启动；
    - (c) 防止原地修改 `0001_initial.sql` 的检查，比如固定已发布脚本的哈希。这一条最急：用户装上 Cupper Local 之后，再原地改它会损坏真实数据。
    - (d) `migrate.rs` 里 "Until T160 … changes in place" 的注释要改；
    - (e) 需要一个新编号，例如 IT-migrations-samples。
11. **计划**：
    - T161 可以直接复用 `apps/desktop/src/test-entry-points.ts`，以及 `selfuse/build.test.ts` 里逐个 Mach-O 检查 entitlements、读 fuse 线的做法。
    - `electron-builder.release.yml` 至今没有真正构建过，T161 会是第一次。
12. **其他**：设置页的"安装命令行工具"还没做，安装说明暂时用 `ln -s`。

## 遗留问题 / 范围外发现

只有上面清单里的事项，没有新增。
