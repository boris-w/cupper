# T005 汇报

- 执行者：子 agent（工程师，构建与发布工具；T004 的执行者接手）
- 分支：`build/T005-host-bundle`（在原 T004 worktree 里从 main 新开）
- 完成日期：2026-10-20

## 做了什么

1. **`cargo xtask host-bundle --kind <release|self-use|e2e|dev> [--runtime <dir>] [--debug]`**（`crates/xtask/src/host_bundle.rs`）。每次运行按以下顺序：
   1. 按清单核对运行时目录里 Node 的 cdhash。
   2. 按构建种类的特性编译宿主。
   3. 把运行时目录拷进 `CupperHost.app/Contents`，再按清单逐个核对每个文件的 sha-256。多一个、少一个、内容不同都失败；路径里出现 `anthropic` 或 `claude-agent-sdk` 时也失败（ADR-11）。
   4. 放入 `MacOS/cupper-host`，写 `Info.plist`。
   5. 对整个 App 做 ad-hoc 签名，`--verify --strict --deep` 校验，取出宿主的 cdhash；再核对一次 Node 的 cdhash，确认没被重签。
   6. 编译 CLI（自用构建经 `CUPPER_HOST_CDHASH` 传入宿主的 cdhash），单独做 ad-hoc 签名。
   7. 打印两个产物的路径和 cdhash。

   产物在 `target/host-bundle/<kind>/CupperHost.app` 和 `target/host-bundle/<kind>/cupper`。

   - **Info.plist**：`CFBundleExecutable` 为 `cupper-host`，bundle id 按种类，带 `LSUIElement`，`LSMinimumSystemVersion` 为 14.0，不含任何用途说明。
   - **hardened runtime**：发布构建和自用构建开，e2e 和开发构建关；CLI 与同一构建的宿主相同。
2. **签名提成共用模块** `crates/xtask/src/codesign.rs`：
   - 原来的 `runtime_pack::sign` 移过来，参数化 identifier、hardened runtime 和 entitlements；
   - 单独提供读取 cdhash 和读取签名详情的函数；
   - `runtime-pack` 对 Node 的签名结果不变，`--verify` 仍然通过。
3. **IT-host-bundle-layout**（2 条用例，在 `rust` 作业的 `cargo test` 里离线运行）：
   - **自用构建的真实组装**：用假运行时目录（`/usr/bin/true` 复制后 ad-hoc 签名充当 Node，加两个小文件，再配一份对应的清单），用 debug 配置组装。断言：
     - 布局与 5.10.1 一致；
     - 宿主、CLI、Node 的 identifier 和 hardened runtime 标志正确，Node 没有被重签；
     - Info.plist 的键正确，没有用途说明；
     - `cupper --build-info` 的 `host_cdhash` 等于宿主的 cdhash；
     - 改动运行时里的一个文件后，命令失败。
   - **构建种类表**：直接读设计 5.7.9 里"宿主的 bundle id""CLI 的 identifier"两行，与代码里的表逐项比对；另测参数解析。
4. **esbuild-wasm 不精简**：用不到的浏览器版文件和类型声明约 440 KB，只占整包 14.5 MB 的 3%。整包保留后，随包的目录与按 SRI 校验过的 npm 包一一对应，也不用维护保留清单。清单没有改动。

## 关键取舍

- **编译到独立的 target 目录 `target/host-bundle/build/`**：
  - `cupper-host` 的集成测试经 `CARGO_BIN_EXE_cupper-host` 运行 `target/debug/cupper-host`（我核对了测试二进制里写死的路径）。
  - 如果用例在同一个 target 目录里按自用构建的特性重新编译宿主，会把这个文件换掉，后面的宿主集成测试就会拿错二进制。
  - 产物路径从 cargo 的 JSON 消息里取。
  - 代价是多一份 debug 依赖产物，约 1.2 GB。
  - 实测在 `cargo test` 里嵌套调用 `cargo build` 不会卡在锁上。
- **耗时**：
  - IT 用 debug 编译：新 worktree 第一次约 25 秒（宿主 17 秒、CLI 8 秒），之后 1–2 秒。
  - 本次 `pr` 运行时缓存已热，`rust` 作业总计 35 秒。
  - 发布配置的完整构建：第一次约 40 秒，之后十几秒。
- **开发构建关闭 hardened runtime**：5.7.9 对开发构建只写了"ad-hoc"，这里按 e2e 的做法处理。
- **ad-hoc 签名的宿主 cdhash 也覆盖 Info.plist 和资源封印**：运行时或 Info.plist 有任何变化，宿主的 cdhash 都会变，自用构建的 CLI 也会随之重新编译。这正是"先签宿主、再编译 CLI"这个顺序的原因。

## 验证方式

- [x] `cargo xtask host-bundle --kind e2e` 和 `--kind self-use`（另外也跑了 release、dev）都在本机产出了签名有效的 App 和 CLI，`codesign --verify --strict`（App 加 `--deep`）都通过：
  - 自用构建：宿主 `dev.cupper.app.local.host`，标志 `adhoc,runtime`；CLI 的 `--build-info` 里 `host_cdhash` 等于宿主的 cdhash。
  - e2e 构建：标志 `adhoc`；宿主特性 `dev-auth`、`test-harness`，CLI 特性 `test-harness`。
- [x] IT-host-bundle-layout 已新增，用例名以 `it_host_bundle_layout_` 开头；在 pr 摘要里结果为 pass。
- [x] 离线运行，不依赖 `runtime-pack` 的下载缓存；命令接受 `--runtime <dir>`，默认仍取 `target/runtime-pack/Contents/`。
- [x] 新增耗时：冷编译约 25 秒，热缓存 1–2 秒；没有新增 CI 作业。
- [x] 在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T005-host-bundle/ci/`，全部通过：
  - rust 1167 / protocol 185 / ts 314 / license 1 / testdata-scan 2；
  - ui、e2e 尚未实现；
  - 摘要为 `pr-0b854172025b.json`，已提交。
- [x] 没有新增依赖（只用系统的 `codesign`、`plutil`）。grep 过分支的 diff 和提交信息，没有本机路径或用户名。
- **规模**：新增 553 行、删除 61 行（`sign.rs` 移走），净增约 490 行，其中测试 152 行；比估计（440）多约 25%，在约 600 行的停止线以内。逐模块统计见 `notes.md`。

## 提交列表

```
4fcd37c docs(task): record the T005 pr CI summary
0b85417 docs(task): record T005 sizes and trade-offs
a733ae5 build(xtask): add `cargo xtask host-bundle`
8a2b1ed refactor(xtask): share ad-hoc signing between runtime-pack and host-bundle
1d7072e docs(task): estimate T005 by module before starting
```

## 遗留问题 / 范围外发现

1. **任务书和 5.10.2 说"两个 Info.plist"不含用途说明**：本任务只产出宿主的 Info.plist，CLI 没有 Info.plist。`Cupper.app` 的那一份归 electron-builder（T075 之后），需要由那个任务断言。
2. **设计 5.7.9 没写开发构建的 hardened runtime**：这里定为关闭。如果编排者另有决定，只需改 `KINDS` 里的一个值。
3. **T075 / T180 要注意**：
   - 宿主的 cdhash 会随 Info.plist、运行时文件和二进制变化，所以自用构建的 CLI 必须与同一次 `host-bundle` 产出的 App 配套分发。
   - Developer ID 签名（T180）之后，Node 和宿主都要按发布身份重签，清单里 Node 的 sha-256 和 cdhash 也要相应重算（见 T004 的遗留问题第 4 条）。
4. **codex-acp 内联依赖的许可证声明还没有**：5.10.1 列在 `codex-acp-2.2.2/` 下，按 T004 的交接归 T006 / license-gate；在那之前，App 里的 codex-acp 目录只有 `index.mjs` 和 `LICENSE`。
5. **磁盘占用**：`target/host-bundle/build/` 每个 worktree 多约 1.2 GB（debug），发布构建另算。
