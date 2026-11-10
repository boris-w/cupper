# T161 汇报

- 执行者：子 agent（工程师，构建与 CI）
- 分支：ci/T161-release-config
- 完成日期：2026-11-09

## 做了什么

1. **共用的扫描 `apps/desktop/src/release-scan.ts`**：都是纯函数，读出来什么就判什么，违规时返回列表。
   - fuses：按 5.7.9 的发布、自用两列核对。Electron 44 的第 9 个 fuse 按名字 `WasmTrapHandlers` 明确列出，期望值是 Electron 出厂的"开"。值不对、读成 undefined、多出 5.7.9 不认识的 fuse，都算违规。
   - asar：自己解析 asar 头取文件清单，不加依赖。只许 `package.json` 和 `dist/{main,preload,renderer,review}/**`，不许有源码映射、测试和夹具，也不许有解包出来的文件。
   - 不许有 `app.asar.unpacked`；原生扩展必须在 `Resources/native/cupper_native.node`。
   - 测试入口：`TEST_ENTRY_POINTS` 里任何一项出现在 asar 里都算违规。T172 的 `dropReplyTo` 挂在 `__cupperE2E` 下面，扫描自然覆盖。
   - Info.plist：必须有 `ElectronAsarIntegrity`；用途说明另外列出，只报告、不判失败。
   - `--build-info` 和原生扩展的 `buildInfo()`：`debug_assertions` 不能为 true，`features` 要等于该构建种类的期望值。
   - `scanApp(app, build)`：在一个构建好的 App 上把以上各项全部跑一遍。
2. **UNIT-release-config-scan**（`src/release-scan.test.ts`，5 条，跑在 PR 的 ts 作业里）：每种检查先喂一份合格的发布配置，确认通过；再喂各种违规，确认每种都会失败。fuse 的情况包括值不对、自用一列、不认识的第 10 个、第 9 个读不出来；每一项测试入口都单独测了一遍。
3. **`release-config` 作业**（`crates/xtask/src/ci/jobs/release_config.rs`，挂进 nightly 矩阵），依次做：
   1. `pnpm install` 和 runtime；
   2. 用 `build:release` 构建，参数 `-c.mac.identity=null -c.mac.target=dir`，环境变量 `CSC_IDENTITY_AUTO_DISCOVERY=false`；
   3. 对 Electron Framework 补一次 ad-hoc 签名（见下面单列的一节）；
   4. 记下产物的 sha256；
   5. `test:release`；
   6. `runtime-pack --verify`。
4. **`apps/desktop/release/config.test.ts`**（`vitest.release.config.ts`），共 3 条：
   - `scanApp(app, "release")` 没有违规，并打印 Info.plist 的用途说明；
   - 宿主、CLI、原生扩展的 `features` 为空，`debug_assertions` 为 false；
   - E2E-SEC-switches：带 `--cupper-unknown-switch` 启动，应以状态 1 立即退出，并打印预期的拒绝信息。
5. **自用构建在 selfuse 作业里扫**：IT-selfuse-build 原来的 fuse 字面量和测试入口检查，换成 `scanApp(app, "self-use")`；补上了 asar 清单、asarUnpack、原生扩展位置；`plist()` 挪进共用模块。
6. **接线**：`package.json` 加 `build:release`、`test:release`；tsconfig 加入新目录；`jobs/mod.rs` 登记作业；`nightly.yml` 矩阵加一行。

## 单列一：ad-hoc 签名为什么非补不可，以及为什么只补 Electron Framework

- 先照你说的，看 electron-builder 写 fuse 时重做的签名能不能启动：不能。
  - `identity: null` 时 electron-builder 跳过签名（日志写着 "skipped macOS code signing: identity explicitly is set to null"）。
  - fuse 线在 `Electron Framework.framework` 里。写 fuse 改了它的内容，它原来那份 linker-signed 的 ad-hoc 签名就失效了，`codesign --verify` 报 "invalid signature (code or signature have been modified)"。
  - arm64 上加载签名无效的代码时，内核会直接杀掉进程。所以带未知开关启动时，进程被 SIGKILL，状态 137，根本到不了开关检查。
  - 主程序 `Contents/MacOS/Cupper` 自己的 linker 签名完好，问题只出在被 fuse 改过的那个 framework。
- 所以作业在打包后，只对 `Contents/Frameworks/Electron Framework.framework` 做一次 `codesign --force --sign -`：
  - 不用身份，不碰钥匙串，不加 hardened runtime；
  - 宿主、CLI、Node 都不动，保持 `host-bundle`、`runtime-pack` 给的签名；
  - 只补被改坏的那一个，产物和"electron-builder 的输出加上 fuse"最接近。
- 补签之后带未知开关启动：状态 1，stderr 是 `cupper: refusing to start with unknown switches: --cupper-unknown-switch`，在 runner 上耗时约 1.1 s；本机核对过，没有新建任何 Cupper 数据目录或日志目录。

## 单列二：Info.plist 的 5 项用途说明（只报告，没有删）

electron-builder 产出的 `Cupper.app/Contents/Info.plist` 带着 Electron 默认的 5 项：

- `NSAudioCaptureUsageDescription`
- `NSBluetoothAlwaysUsageDescription`
- `NSCameraUsageDescription`
- `NSBluetoothPeripheralUsageDescription`
- `NSMicrophoneUsageDescription`

release-config 的测试每晚在输出里打印这一行，runner 上的日志里也能看到；不判失败。删不删、怎么删（需要 `afterPack` 钩子）由你和安全评审定。宿主的 Info.plist 由 `host-bundle` 产出，不带用途说明。

## 单列三：xtask 里改的那条"未实现的作业"用例

`crates/xtask/src/ci/mod.rs` 的 `unimplemented_and_unknown_jobs_are_refused_up_front`，原来拿 `release-config` 当"未实现的作业"，断言它会被拒绝。现在 release-config 实现了，改成拿仍未实现的 `release` 来断言，用例要证明的意思不变。

## 关键取舍

- **扫描写成纯函数**：发布构建、自用构建、UNIT 三处共用一份，UNIT 可以直接喂构造的违规。ts 作业跑在 ubuntu 上，没有 plutil，所以 UNIT 只测纯函数，`scanApp` 由两个 nightly 作业在 macOS 上实测。
- **asar 头自己解析**（约 15 行），没有把 electron-builder 间接依赖的 `@electron/asar` 拉成直接依赖。
- **测试钩子只扫 `TEST_ENTRY_POINTS`**，不扫泛泛的 "harness"：生成的协议类型里本来就有 `test_harness`，会误报。宿主的 `test-harness` 特性由 `--build-info` 的 `features` 管住。
- **自用构建不在 release-config 里重新构建**：selfuse 作业每晚本来就构建 Cupper Local，改用共用扫描后在那里扫。
- **用途说明要能看到**：vitest 默认不显示通过用例的 console 输出，所以在 `vitest.release.config.ts` 里设了 `silent: false`。
- **发布构建上的检查不限托管 runner**：开关检查在建 userData 之前执行，带未知开关启动不会碰数据目录。

## 验证方式

- **本机**：发布构建只构建了一次。
  - release-config 的 3 条用例全部通过：
    - fuse 前 8 个符合发布一列，第 9 个开启；
    - asar 共 571 个文件，全在允许的范围内；
    - 有 ElectronAsarIntegrity，测试入口一个都没有；
    - 宿主、CLI、原生扩展的 features 都为空。
  - `runtime-pack --verify` 通过。
  - desktop 单元测试 108 个通过，含 UNIT-release-config-scan 5 条；xtask 测试 61 个通过。
  - eslint、prettier、tsc、clippy、fmt 都干净；trace-ids check 一致（452 个 ID）。
- **PR #89**：https://github.com/boris-w/cupper/pull/89 ，head 为 71384b21。CI 运行 https://github.com/boris-w/cupper/actions/runs/42595035683 ，8 个检查全部 pass：compat-framework、e2e、license、protocol、rust、testdata-scan、ts、ui。
- **nightly release-config**：https://github.com/boris-w/cupper/actions/runs/42595032953
  - 结果 pass，"8 passed, 0 failed"，耗时 414 s。8 = 5 个步骤（install、runtime-pack、build、sign-ad-hoc、runtime-verify）+ 3 条用例。
  - 摘要里 E2E-SEC-switches 记为 pass。
  - 产物 Cupper.app 的 sha256：ef4220607b0c61e5ea50121043a5e9eb7f38ab83eecc82f2905d34cc25b0e921。
- **nightly selfuse**：https://github.com/boris-w/cupper/actions/runs/42595043294 ，12 passed。IT-selfuse-build 改用共用扫描后通过。
- **验收标准**：
  - [x] release-config 作为 nightly 的一项，在 GitHub runner 上通过；
  - [x] UNIT-release-config-scan 在 PR 里通过，每种违规都有用例；
  - [x] E2E-SEC-switches 通过；
  - [x] PR 的 8 个检查全部通过。
- **规模**：`cargo xtask size` 为 +377 −39，在停止线 450 以内。

## 提交列表

```
71384b21 docs(T161): main's decisions and the ad-hoc route the build took
626380cf ci: release-config builds the release configuration and checks it
b2da5006 test(desktop): the release-config scan, shared with IT-selfuse-build
b629379b docs(T161): what exists, the estimate by check, and four questions
```

## 遗留问题 / 范围外发现

1. Info.plist 的 5 项用途说明待定，见单列二。
2. **设计 5.1.6 的 release-config 一行可以补两点**：
   - 不签名构建要对 Electron Framework 补 ad-hoc 签名，原因是状态 137；
   - 自用构建的扫描在 selfuse 作业里做。

   另外，那一行列了"许可证门禁第二层"，属于 T182，本任务没有做。
3. **T180 的发布流水线**：用 Developer ID 签名时，electron-builder 会给 framework 重新签名，到时候不再需要这一步 ad-hoc 补签。release-config 里的这一步只用于不签名的每晚构建。
