# T030 工作笔记

- 执行者：子 agent（Rust 工程师，宿主网关与安全）
- 实测环境：macOS 26.4（25E246），arm64。

## 开工前的规模估计

按模块估计，行数指 rustfmt 展开后的行，含文档注释和空行。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `gateway/mod.rs` | 配置（各构建开放的认证、密钥、拉起方的唯一 ID、UI 的签名要求）；accept 循环与出错退避；准入：uid、固定令牌、签名结论，记审计 | 150 |
| `gateway/conn.rs` | 读循环：一律 `recv_closing_fds`，收到 fd 就断开，每次读取之后比对固定令牌；握手阶段机；`ping`；其余方法回 `unsupported`（路由归 T033）；先写完错误回复再断开（带宽限）；写循环 | 230 |
| `gateway/handshake.rs` | `host/hello`；`initialize`：nonce、版本协商、四种认证按构建开放、`dev_key` 只认拉起方、角色、principal 与 scope、结果 | 170 |
| `gateway/proof.rs` | 十六进制与 nonce；HMAC-SHA256 证明（方向标签、两个 nonce、pid）与常量时间校验 | 60 |
| `gateway/outbox.rs` | 发送队列：帧不丢；实时事件按字节预算（`max_queue_bytes`），超出返回 `Full`；按流重置（丢掉这条流已排队的实时事件，排一条 `stream/reset{slow_consumer}`） | 120 |
| `gateway/wire.rs` | 响应、错误、通知的帧编码 | 40 |
| `gateway/audit.rs` | 审计记录：`conn.accepted`、`conn.rejected`、`conn.initialized`、`conn.closed` | 70 |
| `gateway/listen.rs` | `host.lock`（被占用时退出码 3）；套接字目录（`run/` 或临时目录下的回退目录）0700 与属主检查；绑定；套接字 0600 | 100 |
| `startup/launcher.rs`、`main.rs` | `dev-auth` 构建启动时 ppid 为 1 就退出（退出码 1）；读自己的 `p_puniqueid` | 45 |
| `bin/gateway_fixture.rs` | 只运行网关的夹具进程（test-harness 门控，T033、T073 复用） | 80 |
| `durations.rs`、`Cargo.toml`、`lib.rs` | 两个时长（关闭宽限、accept 退避）；依赖 `hmac`、`sha2`；夹具的 bin | 20 |
| 小计 | | **1,085** |

**测试与测试辅助**

| 项 | 内容 | 估计 |
|---|---|---|
| `gateway/testkit.rs` | 测试客户端：连接、读帧、握手、算证明；crate 内测试和进程级测试共用（`cfg(any(test, feature = "test-harness"))`） | 110 |
| UNIT-proof-binding | 换掉密钥、方向、`h_nonce`、`c_nonce`、pid 中任何一项，证明都失败 | 50 |
| outbox 单元测试 | 预算、按流重置、关闭 | 60 |
| listen 单元测试 | 锁被占用时退出码 3；目录权限与属主不对时拒绝 | 60 |
| IT-handshake | crate 内、真实套接字、表驱动；同一组用例在四种特性组合下各跑一次，断言哪些认证开放 | 220 |
| IT-pinned-token | 另一进程中途接手（`/bin/cat` 往同一个 fd 写）后下一次读取就断开；收到 `SCM_RIGHTS` 的 fd 时断开并记审计 | 100 |
| IT-devkey-launcher-only | 进程级：测试进程拉起夹具，自己出示 `dev_key` 被接受；`/usr/bin/nc -U` 作为"持同一密钥的另一进程"（测试进程替它按它的 pid 算出正确的证明），被拒 | 80 |
| IT-devkey-orphan-start | 中间进程退出后才 exec 宿主，ppid 为 1 | 50 |
| 小计 | | **730** |

**合计约 1,815 行**，超出约 1,500 的上限约 21%。按任务书"预计会明显超出时，先停下来汇报"，开工前停下，见下一节。

如果 IT-devkey-orphan-start 要在进程级断言退出码（见"待裁定 2"），再加约 80 行，合计约 1,895。

## 汇报与拆分建议

**方案 A：不拆，上限放宽到约 1,900（硬停止线 1,950）。**（建议）

- 理由同计划第 303 行：握手、固定令牌和认证是一条安全路径，拆开要么先合并一个只有一半认证的网关，要么两半互相等待；
- 超出部分主要是测试（730 行，约 40%），实现本身约 1,085 行；
- T030 的松弛只有 1，拆成两个任务会在 T033 之前多一个节点。

**方案 B：拆成两个任务，各自能独立合并。**

- **T030（约 1,250）连接、固定令牌与基础握手**：listen 与 `host.lock`、准入（uid、固定令牌、签名结论）、读循环（`recv_closing_fds`、令牌比对、收到 fd 断开）、写循环与慢客户端、`host/hello`、`initialize` 只接受一次、版本协商、`peer` 和 `codesign` 两种认证、角色与 principal、`ping`、审计、testkit；IT-handshake 的这部分、IT-pinned-token、outbox。合并后 main 上只有同 uid 的只读 cli 和签名校验通过的 UI 能接入，`dev_key`、`harness` 一律 `unauthenticated`，没有可以绕过的入口。
- **T030b（约 600）密钥认证与网关夹具**：`proof`、`dev_key`（只认拉起方）与 `harness`、`host_proof`、ppid 为 1 退出、网关夹具；UNIT-proof-binding、IT-devkey-launcher-only、IT-devkey-orphan-start、IT-handshake 的密钥部分。T033、T073 依赖它。

## 待裁定（开工前一并请编排者定）

1. **新依赖 `hmac = "=0.12.1"`**（RustCrypto，MIT OR Apache-2.0，与现有的 `sha2 0.10.9`、`digest 0.10.7` 同代），用它算证明，并用它的 `verify_slice` 做常量时间比较；`sha2` 以 workspace 依赖加进 `cupper-host`。另一个做法是用 `sha2` 手写 HMAC（约 15 行），不建议。
2. **IT-devkey-orphan-start 的退出码**：父进程退出后，宿主由 launchd 收养，测试进程拿不到它的退出码（`waitpid` 只对子进程有效）。实测（scratchpad 里的一个 C 小程序）：同一用户的进程对一个已被 launchd 收养的进程注册 `EVFILT_PROC`、`NOTE_EXIT | NOTE_EXITSTATUS`，能收到它的退出码（XNU 对"能向它发信号"的进程放行）。两个做法：
   - (a) 在 `cupper-sys` 里加一个按 pid 等待退出状态的函数（kqueue，带 `SAFETY:` 注释，约 80 行含单元测试），用例在进程级断言退出码 1；
   - (b) 用例在进程级断言 stderr 的提示、进程随即退出、没有建套接字，退出码只在函数级断言（`HostExit::Error`，即 1）。
   我倾向 (a)，验收标准写的就是退出码。
3. **审计的落盘**：网关把审计记录发到一个无界 channel，连接任务不等待。写进 `audit` 表和 `host.log` 的接线放在宿主组装（T037），夹具把记录按 JSON 行写到 stderr，供进程级用例断言。是否要求本任务就写出"记录 → `audit` 行"的插入函数（约 40 行含测试）？
4. **审计里对端的可执行文件路径、签名 identifier、Team ID**（5.8.6）：`cupper-sys` 目前没有对应的接口（`proc_pidpath`、`SecCodeCopySigningInformation`）。本任务只记 pid、pid version、签名结论或拒绝类别；路径等字段建议交给 T038（提权确认框同样需要路径和 cdhash）。
5. **写循环的范围**：实时事件以编好的 `stream/events` 帧进入发送队列，预算和按流重置在本任务完成；按 `coalesce_ms` 的发送时合并和 `Replay` 项依赖 T026 的 `Published` 和读路径，留给 T033。`initialize` 照常协商并保存 `coalesce_ms`。
6. **各构建开放的认证**（据 5.7.9、5.8.1 推定）：

   | 认证 | 发布（无特性） | `dev-auth` | `test-harness` | 两者都开（e2e） |
   |---|---|---|---|---|
   | `codesign` | 开放 | 不开放 | 开放 | 不开放 |
   | `peer` | 开放 | 开放 | 开放 | 开放 |
   | `dev_key` | 不开放 | 开放 | 不开放 | 开放 |
   | `harness` | 不开放 | 不开放 | 开放 | 开放 |

   不开放的认证一律回 `unauthenticated` 并断开。UI 的签名要求要等 T180 带上 Team ID；在此之前配置里为空，`codesign` 认证全部失败（`signature_invalid`）。`weak_auth` 等于是否开了 `dev-auth`。
7. **错误码的取法**：nonce 不符 `unauthenticated/bad_nonce`；签名结论不是 UI `unauthenticated/signature_invalid`；本构建没有这种认证、没有载入密钥、`dev_key` 不是拉起方 `unauthenticated/bad_key`；证明格式错或不符 `unauthenticated/bad_proof`；未知认证类型 `unauthenticated`（不带 reason）；非 UI 声明角色、UI 不声明角色 `forbidden/role`。握手失败一律先回错误、再断开。
8. **新时长**：关闭宽限（先写完错误回复再断开，设计没有给值，取 2 s）、accept 出错退避（沿用 SP-4a 的 50 ms），登记到 `durations.rs`。

## 裁定之后：收窄后的估计（2026-10-19）

编排者裁定按方案 B 拆分（任务书末尾的"补充裁定"、设计 5.3.11 末尾的"T030 开工前的裁定"）。`proof`、`dev_key`、`harness`、`host_proof`、ppid 为 1 退出、kqueue、网关夹具归 T039；本任务里 `dev_key`、`harness` 一律回 `unauthenticated/bad_key`。

| 模块 | 内容 | 估计 |
|---|---|---|
| `gateway/listen.rs` | `host.lock`、套接字目录检查、绑定、0600 | 100 |
| `gateway/wire.rs` | 响应、错误、通知的帧编码；过长的错误信息截断 | 45 |
| `gateway/outbox.rs` | 帧、按字节预算的实时帧、按流重置、`finish` 与 `close` | 110 |
| `gateway/audit.rs` | 审计记录 | 70 |
| `gateway/handshake.rs` | `initialize`：nonce、版本、认证分派（`peer`、`codesign`）、角色、principal、结果 | 130 |
| `gateway/conn.rs` | 准入、读循环、阶段机、`ping`、关闭宽限、写循环 | 220 |
| `gateway/mod.rs` | 配置、`Gateway`、accept 循环 | 110 |
| `cupper-sys` | 测试用的 `send_with_fds`（从 peer 的测试里挪出，`test-util` 特性门控） | 15 |
| `durations.rs`、`Cargo.toml`、`lib.rs` | | 20 |
| 实现小计 | | **820** |
| `gateway/testkit.rs` | 测试客户端、起一个网关、临时目录 | 130 |
| listen、outbox、handshake 的单元测试 | | 160 |
| IT-handshake | | 200 |
| IT-pinned-token（含收到 fd） | | 90 |
| 测试小计 | | **580** |
| **合计** | | **约 1,400** |
