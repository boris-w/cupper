# T161 notes

## 1. 现状（开工前核对）

- `electron-builder.release.yml` 已有：8 个 fuse 按 5.7.9 发布一列、`!**/*.map`、原生扩展只经 `extraResources`、hardened runtime、`target: [dmg, zip]`。`package.json` 里还没有 `build:release`。
- `cargo xtask host-bundle --kind release` 已有：宿主、CLI 都不带特性，ad-hoc 签名。
- Vite 的 `release` mode 已有，映射到 flavor `release`。
- 未知开关即退出已经实现：`main/security/switches.ts`，在 `index.ts` 里先于建 userData 执行；`hardening.test.ts` 有它的单元用例。E2E-SEC-switches 还没有。
- 测试入口的唯一清单是 `src/test-entry-points.ts` 的 `TEST_ENTRY_POINTS`。`flavors.test.ts` 断言 e2e 构建含有清单里的每一项。
- `dropReplyTo` 在仓库里还不存在：T172 只有任务书。它一旦写进 `TEST_ENTRY_POINTS`，就会被 `flavors.test.ts` 逼着只出现在 e2e 构建里，本任务的扫描也自动覆盖它。现在先写进清单会让 `flavors.test.ts` 的 e2e 一项失败，所以留给 T172。
- IT-selfuse-build（`selfuse/build.test.ts`）已经检查了 Cupper Local 的 fuse、测试入口、`--build-info`、`ElectronAsarIntegrity`；读 fuse 用的是 `getCurrentFuseWire`，第 9 个 fuse 读出来是键 `8`。

## 2. 做法

- 扫描逻辑写成纯函数，放在一个模块里，输入是读出来的东西（fuse 线、Info.plist、asar 文件清单和内容、`--build-info`），输出违规列表：
  - 发布构建的检查和 IT-selfuse-build 共用这一份；
  - UNIT-release-config-scan 直接喂构造的违规输入。
- `release-config` 作业：
  1. 用发布配置构建，不签名：`-c.mac.identity=null`，加 `CSC_IDENTITY_AUTO_DISCOVERY=false`，不碰钥匙串；`-c.mac.target=dir`，只要 `.app`；
  2. 在构建出的 App 上跑扫描，带未知开关启动一次；
  3. 跑 `cargo xtask runtime-pack --verify`。
- 自用构建：由 `selfuse` 作业的 IT-selfuse-build 改用同一份扫描（它本来就每晚构建 Cupper Local），不在 `release-config` 里再构建一次。见第 4 节第 1 问。
- Info.plist 的用途说明：列出 `NS*UsageDescription` 的键，写进作业的输出和汇报，不判失败。

## 3. 按检查项估行数（含测试，不含生成物）

| 项 | 做什么 | 产品/脚本 | 测试 |
|---|---|---|---|
| fuses | 发布、自用两列的期望值（第 9 个 fuse 按名字 `WasmTrapHandlers` 明确列出，期望为 Electron 出厂的开启）；多出、缺少、读不出来（`undefined`）、值不对，都算违规 | 40 | 25 |
| asar 与 `asarUnpack` | 解析 asar 头取文件清单（不加依赖，约 15 行）；清单只许 `package.json` 和 `dist/{main,preload,renderer,review}/**`，不许 `.map`、测试、夹具；不许有 `app.asar.unpacked`；原生扩展在 `Resources/native/cupper_native.node` | 45 | 25 |
| Info.plist | 有 `ElectronAsarIntegrity`；列出用途说明（只报告） | 12 | 10 |
| 测试钩子 | `TEST_ENTRY_POINTS` 在 asar 内容里出现一个就算违规 | 6 | 8 |
| `--build-info` | 宿主、CLI：`debug_assertions` 为 false，`features` 等于该构建种类的期望（发布为空，自用见 IT-selfuse-build）；原生扩展的 `buildInfo().features` | 12 | 10 |
| 发布构建上的整套检查 | 新的 Vitest 文件：在构建出的 App 上逐项跑上面几项，带未知开关启动应以状态 1 立即退出（E2E-SEC-switches） | — | 55 |
| IT-selfuse-build 改用共用扫描 | 把它里面的 fuse、测试入口检查换成共用的函数，补上 asar 清单和 `asarUnpack` | — | 净 +5 |
| `release-config` 作业（xtask） | 构建发布扩展（无特性）→ `host-bundle --kind release` → `vite build --mode release` → electron-builder（不签名、只出 `.app`）→ Vitest → `runtime-pack --verify`；产物的 sha256 记进摘要 | 45 | — |
| 接线 | `package.json` 的 `build:release` 和测试脚本、Vitest 配置、`jobs/mod.rs` 一行、`nightly.yml` 矩阵一行 | 15 | — |
| **小计** | | **约 175** | **约 138** |
| **合计** | | | **约 315** |

在停止线 450 以内，留了约 100 行，给第一次真正构建发布配置时可能冒出来的问题（见第 4 节第 3 问）。

## 4. 要 main 定的

1. **自用构建在哪里扫**：`selfuse` 作业每晚已经构建 Cupper Local，并跑 IT-selfuse-build。
   - 我建议让 IT-selfuse-build 改用共用的扫描，并补上 asar 清单和 `asarUnpack` 两项。
   - 这样"自用构建也扫一遍"由 `selfuse` 作业完成，`release-config` 只构建发布配置，每晚少一次约 5 分钟的构建。
   - 如果要求 `release-config` 自己也构建一次自用版，再加约 15 行、每晚多约 5 分钟。
2. **钩子标记的范围**：只用 `TEST_ENTRY_POINTS`，不扫泛泛的 "harness"。
   - 原因：生成的协议类型里本来就有 `test_harness`（principal 的种类），扫 "harness" 会误报。宿主的 `test-harness` 特性由 `--build-info` 的 `features` 管住。
   - `dropReplyTo` 由 T172 加进 `TEST_ENTRY_POINTS`，见第 1 节。
3. **不签名能不能启动**：arm64 上的可执行文件至少要有 ad-hoc 签名才能运行。
   - electron-builder 写 fuse 后会给主程序重做 ad-hoc 签名，`identity: null` 时不再签别的。我预计带未知开关能启动并立即退出，要实际构建一次才能确认。
   - 如果启动不了，就在打包后对 `.app` 做一次 ad-hoc 签名（`codesign -s -`），不碰钥匙串。这算不算"不签名"请你定。
4. **E2E-SEC-switches 能不能在本机跑**：开关检查在建 userData 之前执行，带未知开关启动不会碰数据目录。
   - 所以发布构建上的检查不限 GitHub 托管的 runner（与 `selfuse` 不同）。
   - 本机仍只构建一次、跑一次。
