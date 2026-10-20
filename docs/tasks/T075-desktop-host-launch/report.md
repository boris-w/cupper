# T075 汇报

- 执行者：子 agent（工程师；T004、T005、T094、T028 的执行者接手）
- 分支：`feat/T075-desktop-host-launch`
- 完成日期：2026-10-20

T075（收窄后的"拉起宿主与 control 连接"）已完成，`cargo xtask ci pr` 全部通过。打包后的 e2e App 能真正拉起宿主并握手。另外实测发现：一个新的宿主二进制第一次执行要等系统检查约 2 秒，超过 500 ms 的重试预算，详见末尾第 1 条，需要你决定怎么处理。

## 做了什么

按补充裁定收窄后的范围，lifecycle 和"重启宿主"归 T007。新代码在 `apps/desktop/src/main/host/`：

1. **`native.ts`**：从 `Resources/native/cupper_native.node` 加载扩展，用 `buildInfo()` 核对它与 App 的 flavor 一致：`release` 要求 `signature`，`local`、`e2e`、`dev` 要求 `key_proof`。不一致就拒绝。
2. **`socket-port.ts`**：主进程的 `HostPort`，包在扩展给的 socketpair 那一端（`net.Socket`）上。出方向整帧写（JSON 帧和 `0x02` 帧），入方向按块回调，每块是独立的 `ArrayBuffer`。
3. **`connection.ts`（hostConnection）**：
   - 在扩展校验过的连接上跑 host-client 的握手。keyed 构建用扩展的 `auth` 和 `verifyHostProof`；发布构建用 `codesign`。
   - 只有发布构建拒绝 `weak_auth`。自用构建按 5.7.9 本来就是退化模式，所以不拒绝。
   - 超时用 `AbortSignal.timeout`，套接字关闭时也会 abort。
   - 任何失败都调用 `conn.close()`；`host_proof` 缺失时扩展不会自己断开（T073 交接），这里补上。失败原因并入 `failed.reason`。
4. **`launcher.ts`（hostLauncher）**：
   - 先连接；扩展报 `connect_failed`（没人监听）才拉起宿主，其他拒绝码直接作为失败原因。
   - keyed 构建每个 launcher 一个 `HostKey`；载荷第一次拉起时读一次，之后只留在内存。宿主被杀后再拉起，传的是同一个对象和同一个 key。
   - 拉起后每 10 ms 重连，总预算 500 ms（时钟可注入）。
   - 宿主退出码 3 改为直接连接正在运行的那个宿主；4、5、6 分别给出 `data_too_new`、`data_dir_unusable`、`migration_failed`；其他退出给 `host_exited`；预算用完给 `host_unreachable`。
   - `installing` 开关打开时不拉起，给 `updating`。
5. **`control.ts`（controlClient）**：把 `HostClient` 挂在 control 连接上；连接断开后重连，宿主没了就由 launcher 重新拉起。
6. **组装与接线**（`host/index.ts`、`main/index.ts`）：
   - 宿主路径是 `Contents/Helpers/CupperHost.app/Contents/MacOS/cupper-host`。
   - 宿主的环境变量取 5.6.6 的固定白名单。
   - e2e 构建读 `CUPPER_E2E_DIR` 下的 `harness_key` 和 `test_config.json`，按设计 5.9.9 的约定组装载荷。
   - 主进程在 App `ready` 之前就开始连接（5.5.1），状态写进 `main.log`，退出时关闭 control 连接。
7. **dev 和 e2e 的打包配置**：
   - `build:dev`、`build:e2e` 先编 `dev-auth` 的扩展和对应种类的 `host-bundle`，再跑 Vite 和 electron-builder。
   - 扩展用 `extraResources` 放在 asar 外；宿主用 `extraFiles` 放进 `Helpers/`，并加 `signIgnore`，不让 electron-builder 重签它。

## 关键取舍

- **分层**：拨号、拉起、重试、退出码、key 和载荷都在 launcher 里；`connection.ts` 只管"一个校验过的连接 → `SocketPort` → 握手"。这样 UNIT-launcher 能用假扩展和虚拟时间测 launcher；真实的套接字和握手由集成用例覆盖。control 客户端直接用 host-client 的 `HostClient`。
- **扩展的位置**：用 `Resources/native/`（`extraResources`）。设计 5.10.1 写的是 `app.asar.unpacked/…`；两者都在 asar 外，前者少一步暂存复制。
- **宿主的环境变量**：桌面不读运行时清单，所以白名单在代码里抄了一份。它与 `runtime/manifest.json` 的 `env.allow` 是同一份名单，注释注明了出处。
- **规模**：新增 732 行、删除 7 行，其中测试 278 行；估计约 925 行，上限 1,000。每完成一个模块统计一次，逐模块明细在 `notes.md`。

## 验证方式

- [x] **UNIT-launcher 的拉起部分**（6 条，在 `pr` 里）：
  - 载荷只在内存、只读一次，重拉时同一个对象、同一个 `HostKey`，拨号也一直用这个 key；
  - 发布构建拉起时不带载荷；
  - 拉起后每 10 ms 重试、500 ms 用完时失败：虚拟时间 500 ms，拨号 51 次；`installing` 时不拉起；
  - 退出码 4、5、6、1 各自的失败原因；退出码 3 时继续连接，宿主可连后成功；
  - 只有发布构建拒绝 `weak_auth`（mock 宿主，`release`、`local`、`dev` 三种）；
  - flavor 与扩展不符时拒绝。
- [x] **真实拉起的集成用例**（`pnpm --dir apps/desktop test:launch`，普通的 `pnpm test` 会跳过）：
  - 用 `dev-auth` 扩展和 `cargo xtask host-bundle --kind e2e --debug` 的产物，经 launcher 拉起、握手（`weak_auth` 为 true，`hostPid` 与 `host.lock` 一致）、`ping`；
  - `SIGKILL` 宿主后，用同一个 key 和载荷再拉起、再握手；
  - 连跑 3 次都通过。只向自己拉起的宿主发信号，结束后没有残留进程，临时目录已删。
- [x] **打包后的 e2e App**：`pnpm build:e2e` 成功。
  - 打包后宿主的 cdhash 与 `host-bundle` 的产物相同（`signIgnore` 生效），整个 App 的 `codesign --verify --strict --deep` 通过。
  - 设好 `CUPPER_E2E_DIR` 启动 App 两次，`main.log` 两次都到 `ready`，`host.lock` 的 pid 是 `cupper-host`。
  - 每次都对 App 和那个宿主发了 SIGTERM，没有残留进程。
  - 真实的 `~/Library/Application Support`、`~/Library/Logs` 下没有新建任何东西。Chromium 照例改写了 `~/Library/Preferences/dev.cupper.app.e2e.plist`（T074 已交接给 T090），我用 `defaults delete` 清空了这个域。
- [x] `dialog.showMessageBox` 的约束本任务不涉及（lifecycle 已挪到 T007）。没有新增界面文案，失败只给语义码。
- [x] 测试名以编号开头（`describe("UNIT-launcher")`）；编号已在 `tests/ids.toml` 里。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T075-desktop-host-launch/ci/`，全部通过：
  - rust 1310 / protocol 185 / ts 329 / license 1 / testdata-scan 3；
  - ui、e2e 尚未实现；
  - UNIT-launcher 为 pass；摘要为 `pr-14dc7555dd43.json`，已提交。
  - testdata-scan 的 `sensitive-words` 步骤被跳过：本机没有 `.private/sensitive-words.txt`，与本任务无关。
- [x] grep 过分支的 diff 和提交信息，没有本机路径或用户名。

## 提交列表

```
4acffd8 docs(task): record the T075 pr CI summary
14dc755 docs(task): record T075 sizes, trade-offs and measurements
7af44b8 test(desktop): start the real e2e host, handshake, and start it again
8ec694f feat(desktop): start the control connection and pack the host into dev and e2e apps
455806a feat(desktop): launch the host and connect to it through the native extension
b9318b8 docs(task): estimate the narrowed T075
0e73065 docs(task): estimate T075 and propose a split before starting
```

## 遗留问题 / 范围外发现

1. **首次执行新二进制会超过 500 ms 预算（需要你决定）**：
   - 实测：一份新复制的 `cupper-host`（debug，约 21 MB）第一次执行就用了 1.9 秒，第二次几乎为 0。集成用例第一次运行时因此报 `host_unreachable`，而那个宿主随后仍会起来。用例里已先跑一次 `--build-info` 预热。
   - 影响：用户装好新版本后第一次启动，很可能落进这种情况。control 连接停在 `failed`，宿主却在后台照常起来。
   - 建议的做法：
     - 预算用完时，只要自己拉起的宿主还活着，就继续等它监听或退出，另设一个较长的上限（例如 10 秒）；
     - 或者在 `failed: host_unreachable` 之后由界面或 T007 再连一次。

   这需要改设计 5.5.1、5.7.2 的"总预算 500 ms"，所以我没有改。
2. **T007 要注意**：
   - `ControlConnection.client` 就是 control 连接上的 `HostClient`，`host/status`、`host/exit` 直接用它的 `request`、`command`；
   - 退出确认挂在现有的 `will-quit` 之前；
   - "无法验证宿主"对应的失败码是 `unauthenticated` 或 `host_unverified`。例如新的 UI 实例拿新 key 去连一个还在运行的旧宿主时，就会出现这两种。
3. **T076 要注意**：
   - 数据连接用同一个 launcher 打开：`open({role: "window", features: …})`；
   - `SocketPort.ack` 现在是空操作，按窗口的流控由 T076 做。
4. **构建前提**：`build:dev`、`build:e2e` 和 `test:launch` 都需要本机先跑过一次 `cargo xtask runtime-pack`，`host-bundle` 要用它的暂存目录。
5. **设计核对**：
   - 5.10.1 里扩展的路径可以改写成 `Resources/native/`，按设计选也只差一步暂存复制；
   - 宿主的环境变量白名单在桌面和运行时清单里各有一份，以后如果名单变化，要两处一起改。
