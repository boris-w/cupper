# T045 工作笔记

## 行数估计（开工前）

### 估算方法

SP-4a 的 `client.rs`（199 行）和 `mirror.rs`（100 行）是起点。本任务在它们之上加：读套接字改走 `recv_closing_fds`、固定令牌、`host/hello` 与版本协商、按构建种类校验宿主、完整的客户端同步状态机（T014 的裁定）。代码按 SP-4a 的密度加 rustfmt 展开估；测试按 T014 的经验（测试比估计多约 70%）单独估，并尽量共用 JSON 构造函数。

### 按模块估计

| 模块 | 内容 | 代码 | 测试 |
|---|---|---|---|
| `Cargo.toml`、`lib.rs` | 包、特性（`dev-auth`、`self-use`、`test-harness`）、`mod` 声明 | 45 | — |
| `build.rs` | 构建种类（编译期常量，测试可注入）、编进来的宿主 cdhash、`--build-info`、默认数据目录与套接字发现 | 110 | 40 |
| `verify.rs` | `HostCheck` 的四个分支（签名要求、cdhash、同目录宿主文件的 cdhash、`harness` 留给 T039）、同 uid、`weak_auth` 规则、固定令牌比对 | 140 | 130（UNIT-cli-host-verify） |
| `client.rs` | 连接与握手（`host/hello`、版本协商、`initialize`、`auth: peer`）、读循环（`recv_closing_fds`、收到 fd 断开、每次读取比对令牌）、请求关联、通知分流、订阅结果与通知保持线上顺序、`command_id` | 260 | 170（UNIT-cli-client，进程内的假宿主） |
| `mirror.rs` | 客户端同步状态机：订阅带游标、快照分页组装与核对、连续性、`ViewError`、`stream/reset`、epoch 改变 | 190 | 260（UNIT-mirror） |
| `main.rs` | bin `cupper`：只有 `--build-info` | 20 | — |
| **合计** | | **约 765** | **约 600** |

**总计约 1,365 行（±10%：1,230–1,500）**，超过计划的 M（400–900 行），在约 1,500 行的上限以内。

**停止线**：每完成一个模块统计一次（`git diff --shortstat main..HEAD -- crates/cupper-cli`）。按已完成模块的实际偏差推算，总数预计超过约 1,500 行时，停下来汇报。到时可以先拿掉的部分：UNIT-cli-client 只留握手和请求关联两例，其余交给 T046 的 IT-cli-commands。

## 实际行数

（每完成一个模块补一行）

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（代码 + 测试） | 累计新增 |
|---|---|---|---|---|
