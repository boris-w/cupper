# T005 工作笔记

- 执行者：子 agent（工程师，构建与发布工具；T004 的执行者接手）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `host_bundle.rs` 构建种类与参数 | 四种构建的身份、特性、hardened runtime（5.7.9）；`--kind`、`--runtime`、`--debug` | 50 |
| `host_bundle.rs` 编译 | 在独立的 target 目录里按特性编译宿主和 CLI，从 cargo 的 JSON 消息取产物路径；自用构建传 `CUPPER_HOST_CDHASH` | 40 |
| `host_bundle.rs` 组装与核对 | 拷入运行时目录；按清单核对每个文件的 sha-256、Node 拷入前后的 cdhash；路径里不得出现 Anthropic 的 SDK；写 `Info.plist` | 80 |
| `host_bundle.rs` 签名与输出 | 宿主 ad-hoc 签名、取 cdhash；再编译、签名 CLI；打印路径和 cdhash | 40 |
| `codesign.rs` | 把 T004 的 `runtime_pack::sign` 提成共用模块：签名参数（identifier、hardened runtime、entitlements）、单独读取 cdhash | 20 |
| `main.rs`、`runtime_pack` 的可见性 | 子命令入口；清单类型和 `hash_tree` 给本任务用 | 10 |
| **实现小计** | | **240**，按 1.3 倍计约 **310** |

**测试**

| 项 | 内容 | 估计 |
|---|---|---|
| IT-host-bundle-layout | 假运行时目录（ad-hoc 签名的小 Mach-O 充当 Node）和对应的清单；按自用构建真实组装（debug）；布局、identifier、Info.plist 不含用途说明、`--build-info` 的 `host_cdhash` 等于宿主的 cdhash、没有 SDK | 100 |
| 构建种类表 | 四种构建的 bundle id、identifier、特性、hardened runtime 与 5.7.9 一致；`Info.plist` 的键 | 30 |
| **测试小计** | | **130** |

**合计约 440 行**，在停止线（约 600）以内。每完成一个模块用 `git diff --numstat` 统计一次。

**耗时的预估**：测试在独立的 target 目录里用 debug 编译宿主和 CLI，避免覆盖 `target/debug/cupper-host`（`cupper-host` 的集成测试经 `CARGO_BIN_EXE_cupper-host` 用的就是这个文件）。实测冷编译约 25 秒（宿主 17 秒、CLI 8 秒），之后增量编译几秒。

## 实际行数

`git diff --numstat`，不含本目录。

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计 |
|---|---|---|---|
| 签名提成共用模块（`refactor(xtask)`） | 69 + 10 / 4 + 56（`sign.rs` 移走） | — | 20 + 10 |
| `host-bundle` 与 IT-host-bundle-layout（`build(xtask)`） | 466 + 8 / 1 | 152 | 210 + 130 |

**合计新增 553 行、删除 61 行（净增约 490）**，其中测试 152 行。比估计（440）多约 25%，主要是 rustfmt 展开的构建种类表（四个结构体字面量约 40 行）和参数解析。在停止线（约 600）以内。

## 实现时的取舍

**构建种类**：任务书的表与设计 5.7.9、`cupper-host` 和 `cupper-cli` 的 `Cargo.toml` 特性说明一致，CLI 的特性也与 T045 的 `BuildKind` 判定一致（`test-harness` 优先于 `self-use`，`self-use` 优先于 `dev-auth`）。hardened runtime：发布构建和自用构建开，e2e 构建关；开发构建 5.7.9 只写了"ad-hoc"，按 e2e 处理，关闭。CLI 与同一构建的宿主取同样的设置。IT 里另有一条用例直接读 5.7.9 的"宿主的 bundle id""CLI 的 identifier"两行，与代码里的表逐项比对，设计改了而代码没跟上时会失败。

**独立的 target 目录**：`cupper-host` 的集成测试经 `CARGO_BIN_EXE_cupper-host` 运行 `target/debug/cupper-host`（已核对测试二进制里写死的路径）。用例里如果在同一个 target 目录里按自用构建的特性重新编译宿主，就会把这个文件换成别的特性组合，排在后面的宿主集成测试会拿错二进制。所以 `host-bundle` 一律编译到 `target/host-bundle/build/`，产物路径从 cargo 的 JSON 消息里取，不依赖目录约定。代价是多一份依赖的编译产物（debug 约 1.2 GB）。实测在 `cargo test` 里嵌套 `cargo build` 不会卡在锁上：cargo 编译完就释放了构建目录的锁。

**耗时**：IT-host-bundle-layout 用 debug 编译。新 worktree 第一次约 25 秒（宿主 17 秒、CLI 8 秒），之后 1–2 秒。发布配置的完整构建第一次约 40 秒，之后十几秒。

**运行时的核对**：拷入后按清单逐个比对 sha-256，多一个、少一个、内容不同都失败；Node 的 cdhash 在拷入前（运行时目录里）和签完整个 App 之后各核对一次。签 App 不带 `--deep`，`Helpers/node` 的签名不会被改写；校验用 `--verify --strict --deep`，嵌套的 Node 一并校验。路径里出现 `anthropic` 或 `claude-agent-sdk` 时失败（ADR-11）。

**假运行时**：用例把 `/usr/bin/true` 复制一份、ad-hoc 签名后充当 Node，再放两个小文件；清单取入库的那份，只把 `files` 换成假运行时的哈希。不依赖下载缓存，也不联网。

**宿主的 cdhash 会随 Info.plist 和资源变**：ad-hoc 签名的 CodeDirectory 有 Info.plist 和资源封印的特殊槽，所以运行时目录或 Info.plist 有任何变化，宿主的 cdhash 都会变，自用构建的 CLI 也会跟着重新编译。这正是先签宿主、再编译 CLI 的原因。

**esbuild-wasm 不精简**：用不到的浏览器版文件和类型声明共约 440 KB，占整包 14.5 MB 的 3%；整包保留，随包的目录与按 SRI 校验过的 npm 包一一对应，也不用维护保留清单。

**Info.plist**：`CFBundleExecutable` 为 `cupper-host`，另有 `CFBundleIdentifier`、`CFBundleName`（CupperHost）、`CFBundlePackageType`（APPL）、版本号（取工作区版本）、`LSMinimumSystemVersion` 14.0、`LSUIElement`，不写任何用途说明。
