# T075 Review

## 第 1 轮（2026-10-20，编排者）

- 结论：通过，合并。
- 过程：
  - 开工前估计约 1,460–1,760 行，并查出三处缺口（扩展缺按 pid 取路径和 cdhash 的接口、App 里还没打包扩展和宿主、主进程缺 `net.Socket` 的 `HostPort`），执行者停下汇报，拆分（计划 v1.3.17）：退出确认与重启宿主归 T007；dev、e2e 的打包配置留在本任务。e2e 启动载荷的文件约定写进了 5.9.9。
  - 合并 main（没有冲突）后在 worktree 里重跑 `cargo xtask ci pr --record`，全部通过，摘要 `ci/pr-2d7467ca48a7.json`。执行者那次的 `pr-14dc7555dd43.json` 一并保留。
- 核对：新增的依赖只有工作区内部的包（`@cupper/desktop-native`、`@cupper/host-client`、`@cupper/protocol`）；没有个人信息。执行者在打包后的 e2e App 上实测两次（`codesign --verify --strict --deep` 通过、宿主 cdhash 与 `host-bundle` 产物相同、`host.lock` 的 pid 是 `cupper-host`），只向自己拉起的进程发信号，真实的 `~/Library/Application Support`、`~/Library/Logs` 下没有新建任何东西；Chromium 改写的 e2e 偏好域已清空（T074 已交接给 T090）。
- 审阅：
  - **分层**：拨号、拉起、重试、退出码、key 和载荷都在 launcher 里，`connection.ts` 只管"校验过的连接 → `SocketPort` → 握手"，UNIT-launcher 用假扩展和虚拟时间测 launcher，真实的套接字和握手由集成用例覆盖。认可。
  - **只有发布构建拒绝 `weak_auth`**：自用构建按 5.7.9 本来就是退化模式。认可。
  - **载荷只留内存、复用同一个 `HostKey`**，宿主被杀后再拉起传同一个对象。
  - **集成用例**（`test:launch`）不进 `pr`：需要先跑 `runtime-pack` 和 `host-bundle`，与任务书"不要求进 `pr` 组合作业"一致。
- 编排者裁定（已写进设计）：
  1. **首次执行超过 500 ms 预算**（实测新复制的宿主第一次执行约 1.9 s）：预算用完时，只要自己拉起的宿主还活着，就继续等它监听或退出，最长 10 s（5.7.2、5.5.1），由 T007 实现。
  2. **扩展放在 `Resources/native/`**（`extraResources`）：认可，5.10.1、5.1.2 和 `release-config` 的断言（`asarUnpack` 为空）已改。
  3. **宿主的环境变量白名单在桌面代码里抄了一份**：由 T007 改为构建时从 `runtime/manifest.json` 的 `env.allow` 读取，不留两份。
- 规模：新增 732 行（测试 278），删除 7 行，在收窄后约 1,000 的上限以内；逐模块统计。
- 转交事项已写进 `handoff-notes.md`：T007、T076、构建前提（`build:dev`、`build:e2e`、`test:launch` 需要先跑过 `runtime-pack`）。
