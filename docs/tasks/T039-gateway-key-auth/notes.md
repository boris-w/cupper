# T039 工作笔记

- 执行者：子 agent（Rust 工程师，宿主网关与安全）

## 开工前的规模估计

行数指 rustfmt 展开后的行，含文档注释和空行，不含 `Cargo.lock`。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `cupper-protocol` 的 `proof.rs`、`Cargo.toml`、`lib.rs` | 32 字节与 64 位十六进制互转；方向标签；`prove`；常量时间的 `check`；`hmac = "=0.12.1"` | 85 |
| `cupper-sys` 按 pid 等退出码 | kqueue 先登记、后等待（带超时），`SAFETY:` 注释；导出 | 70 |
| 宿主 `handshake.rs` | `dev_key`（只认拉起方）与 `harness` 两个分支、`host_proof`、证明缺失回 `bad_proof`、`pr_test`、nonce 改用 `proof` 的十六进制 | 75 |
| 宿主 `mod.rs`、`conn.rs`、`audit.rs`、`durations.rs` | 配置里的密钥与拉起方唯一 ID；固定令牌传给 `initialize`；握手期限（10 s，经 `Clock`）；`HandshakeTimeout` | 50 |
| 宿主 `startup`、`main.rs` | 读拉起方的唯一 ID，ppid 已是 1 时报错；`dev-auth` 构建的 `main` 据此以退出码 1 退出 | 40 |
| 网关夹具 `bin/gateway_fixture.rs`、`Cargo.toml` | 读启动载荷、加锁绑定、stdout 报套接字路径、审计按 JSON 行写到 stderr；`test-harness` 门控 | 90 |
| CLI：`harness.rs`、`client.rs`、`verify.rs`、`Cargo.toml` | `HarnessKey`（十六进制、按 fd 读）；`ClientAuth::Harness`；`credentials` 算证明；校验 `host_proof`；`is_weak` | 95 |
| **实现小计** | | **505** |

**测试与测试辅助**

| 项 | 内容 | 估计 |
|---|---|---|
| UNIT-proof-binding | 换掉密钥、方向、`h_nonce`、`c_nonce`、pid 任一项都失败；格式错误；常量时间（经 `hmac` 的 `verify_slice`） | 60 |
| `cupper-sys` 等退出码的单元测试 | 被 launchd 收养的进程的退出码；不存在的 pid；超时 | 50 |
| `gateway::testkit` | 按配置起网关（密钥、拉起方、时钟）；算证明的客户端辅助 | 50 |
| IT-handshake 的密钥部分（crate 内） | `dev_key`、`harness` 的接受与拒绝（证明错、缺、没有密钥、不是拉起方）；各构建开放的认证；`host_proof`；握手期限 | 200 |
| 进程级用例 `tests/gateway.rs` | IT-devkey-launcher-only（夹具 + `nc -U`）；IT-devkey-orphan-start（夹具和宿主，kqueue 断言退出码） | 170 |
| CLI 的用例 | `harness` 认证被接受；`host_proof` 不符或缺失时拒绝连接；按 fd 读密钥；`is_weak` | 110 |
| **测试小计** | | **640** |

**合计约 1,145 行。** 按 T030、T045 的经验（多出 25%–30%）推算约 1,430–1,490 行，贴近约 1,500 的上限，所以要写紧。每完成一个模块用 `git diff --numstat main..HEAD` 统计一次；推算总数超过约 1,500 行时，停下来汇报。

## 实际行数（逐模块）

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） | 累计新增 |
|---|---|---|---|---|
