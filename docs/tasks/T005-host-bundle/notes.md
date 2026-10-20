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
