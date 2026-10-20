# T028 工作笔记

- 执行者：子 agent（Rust 工程师，宿主诊断）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。新模块是 `cupper-host` 的 `diag`（设计 5.2 模块表里的名字）。

**实现**

| 模块 | 内容 | 草估 |
|---|---|---|
| `diag/redact.rs` | T050 的结构化脱敏（账号字段、凭据）原样从 `driver/tap/redact.rs` 移来，分接改为调用它 | 10（只算改动） |
| `diag/full.rs` | 完整脱敏：凭据、家目录、用户名、`/var/folders/…`、主机名、40 字符以上的长串；按字段名放行宿主自己的哈希和 ID | 110 |
| `diag/rotate.rs` | 按大小轮转的文件（`host.log` 10 MB × 5，适配器日志 5 MB × 3），目录 0700、文件 0600 | 60 |
| `diag/log.rs` | 自己实现的 tracing `Subscriber`：JSON Lines（时间取自 `Clock`）、级别与 `diagnostics.verbose` 开关、第三方 crate 只记 `warn` 以上、`Content` 字段在 `info` 只记长度和类型、开发构建同时输出到终端、日志目录规则、panic hook | 170 |
| `diag/stderr.rs` | 适配器 stderr 读取器：逐行加实例 ID 和时间、完整脱敏、写进 `adapter-<agent>.log`，每实例保留最后 64 KB | 80 |
| `cupper-sys` 的 `host_name` | 主机名脱敏要用；`unsafe` 只在 sys | 15 |
| `main.rs`、会话 actor、故障点、分接、`Cargo.toml` | 启动顺序里接上日志（第 2 步）；把 `eprintln!` 改写进 `host.log` | 40 |
| **实现小计** | | **485**，按计划 1.1 的经验乘 1.7 约 **825**（按我前几个任务的实际偏差 1.1–1.4 倍计约 630） |

**测试**

| 用例编号 | 内容 | 草估 |
|---|---|---|
| UNIT-redaction | 5.11.2 表里每类规则一个样例，结构化、完整两档分别断言；长串不误伤按字段名放行的哈希和 ID；结构化脱敏不改写 base64 图片和哈希 | 90 |
| UNIT-redact-auth-status | `_auth/status_update` 只保留 `kind` 和套餐名（含多种登录方式、多余字段） | 30 |
| 日志其余 | 轮转（文件数、大小上限）；64 KB 尾部；`info` 不记内容、打开 verbose 后记录；panic hook；JSON 行的字段；日志目录规则 | 185 |
| **测试小计** | | **305** |

**合计约 1,130 行**（实现按 1.7 倍计）或约 935 行（按我的实际偏差计），在约 1,200 的汇报线以内，但贴近，所以这次每完成一个模块就用 `git diff --numstat` 统计一次，推算过线就停下汇报。拆分点（如需要）：日志与轮转、panic hook / 脱敏与 stderr 读取。

## 依赖的取舍（开工前）

- **不引入 `tracing-subscriber`**：完整脱敏要按字段名放行哈希和 ID，用它的 JSON 格式化器就得把写出的行再解析一遍；时间也要取自 `Clock`。自己实现一个只处理事件的 `Subscriber`（不记录 span）约 60 行，只依赖 `tracing` 本身。`tracing` 0.1.44 已经在锁文件里（ACP SDK 依赖它），锁精确版本。
- **不引入轮转的 crate**：按大小轮转约 60 行。
