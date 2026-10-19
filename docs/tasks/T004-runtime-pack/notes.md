# T004 工作笔记

- 执行者：子 agent（工程师，构建与发布工具）

## 开工前的规模估计

行数指 rustfmt、Prettier 展开后的行，含文档注释和空行，不含 `Cargo.lock`、锁文件、许可证原文和 `runtime-pack` 写回清单的计算字段（生成物）。清单里手工维护的部分（锁定版本、哈希、环境变量分类）是数据，单列一项。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `runtime_pack/mod.rs` | 参数（`--verify`）；缓存与暂存目录；按组件取文件、重签、算哈希；写回清单或逐项比对；许可证文本写入或核对 | 180 |
| `manifest.rs` | 清单的类型（键有序）、读写、`--verify` 的逐项差异 | 90 |
| `fetch.rs` | curl 下载（不读 `.curlrc`、只走 https、UA `cupper-research`、只允许官方地址）；按 sha-256 或 SRI 校验；缓存复用前重新核对 | 70 |
| `unpack.rs` | tar.gz 读取：只取普通文件，逐段检查路径不越界，去掉首层目录 | 60 |
| `sign.rs` | 本机 Node 副本 ad-hoc 重签（hardened runtime、只有 allow-jit、identifier `dev.cupper.runtime.node`）、校验、取 cdhash | 40 |
| `claude.rs` | 解析锁文件（跳过 optional）、在隔离目录里按 SRI 下载解包、用重签后的 Node 和 esbuild-wasm 跑打包脚本、算哈希、扫描环境变量、删除隔离目录 | 70 |
| `envscan.rs` | 从产物源码里提取环境变量读取点（成员访问、`in`、解构），按白名单、禁用、无害三类归类，未归类即失败 | 110 |
| `runtime/bundle-claude-acp.mjs`、`runtime/node.entitlements.plist` | 本机打包脚本（与 SP-2 的选项一致，产物逐字节可复现）；Node 的 entitlements | 45 |
| `main.rs`、`Cargo.toml`、`.prettierignore` | 子命令入口；`tar`、`flate2`、`base64`、`serde` 依赖；清单和锁文件不过 Prettier | 15 |
| **实现小计** | | **680** |

按计划 1.1 的经验，实现常比草估多出不少；xtask 没有宿主那样字段多的结构体字面量，按 1.3 倍计约 **880**。

**测试**

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-runtime-pack-envscan | 各种读取写法的提取；未归类的名字让命令失败；前缀规则；动态读取计数 | 120 |
| `unpack` | `..` 和绝对路径被拒；去首层目录；指定成员缺失时报错 | 50 |
| `fetch` | sha-256、SRI 不符时失败（本地文件，不联网）；非官方地址被拒 | 30 |
| `manifest` | 读写往返；逐项差异列出变化的键 | 40 |
| `claude` 的锁文件解析 | 跳过 optional、dev、link；非 npm 官方地址被拒 | 30 |
| **测试小计** | | **270** |

**数据**：清单里手工维护的部分，主要是环境变量分类。分类条目数要等第一次扫描出产物后才知道，预估 150–300 行。

**合计**：代码约 1,150 行（实现 880 + 测试 270），加数据约 1,300–1,450 行，接近停止线。每完成一个模块用 `git diff --numstat main...HEAD` 统计一次；推算总数超过约 1,500 行时，停下来汇报。

**范围上的取舍（开工前）**：codex-acp 内联的 22 个依赖的许可证声明（ADR-11）按设计 5.1.2 属于 `tools/license-gate` 的"THIRD_PARTY_NOTICES 生成"，本任务不做，见文末的交接。
