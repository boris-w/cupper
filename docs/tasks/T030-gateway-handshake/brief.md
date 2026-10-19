# T030 网关 I：连接、握手与本地认证

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关与安全）
- 分支：`feat/T030-gateway-handshake`
- 依赖：T012、T021、T025（均已合并）。M1a 第 4 波，松弛为 1。T031、T032、T033、T073 等依赖它。

## 背景

客户端（桌面 App、CLI、原生扩展，M3 起还有手机）都经 Unix 套接字连接宿主。网关负责：连接、帧的读写、握手、本地认证，以及每条连接的写循环。本地认证（ADR-05）的组成：
- uid 校验；
- `LOCAL_PEERTOKEN` 加代码签名校验；
- 固定令牌：每次读取后都比对；
- 按构建种类区分的认证方式；
- 绑定信道的双向证明。

已经做好的部分：
- T021：`cupper-sys` 的对端校验、代码签名、`recv_closing_fds`；
- T012：握手方法 `initialize`、`host/hello` 的类型，以及 `PROTOCOL`、`VersionRange::negotiate`；
- T023：启动载荷里的密钥；
- T025：数据目录布局。

方法分发（T031、T032）不在本任务里。

## 范围

**做**（计划 T030 一行；设计 5.2.4、5.8.1、5.3.2）：
1. **迁移 SP-4a 的连接、写队列和线路编解码**（`conn`、`outbox`、`wire`），放进宿主的 `gateway` 模块。
2. **监听套接字**：
   - 套接字路径和权限（5.2.4、5.8.1）；
   - `host.lock`：已有宿主在运行时，以退出码 3 退出，`HostExit::AlreadyRunning`。
3. **每条连接**：
   - uid 校验；
   - 代码签名结论；
   - 固定令牌：**每次读取之后**都与令牌比对；
   - 读套接字一律经 `cupper_sys::recv_closing_fds`；收到 fd 就断开，并记审计。
4. **握手**：
   - 连接建立后，宿主先发 `host/hello`（含 `nonce`）；
   - `initialize` 只接受一次，再来一次回 `forbidden/already_initialized`；
   - 版本协商；
   - 四种认证：`codesign`、`peer`、`dev_key`、`harness`，按构建种类开放，见 5.8.1 的表；
   - 绑定信道的双向证明：客户端的 `proof` 和宿主的 `host_proof`；
   - 确定 principal 和角色。
5. **`dev_key` 的额外限制**（5.8.1）：
   - 只接受拉起宿主的那个进程：比对对端的 `p_uniqueid` 与本进程的 `p_puniqueid`；
   - 证明错误或缺失时，回 `unauthenticated` 并断开；
   - `dev-auth` 构建启动时，如果 ppid 已经是 1，就以退出码 1 退出。
6. **写循环与慢客户端**：每条连接的发送队列有上限，满了就对这条连接重置（5.2.4、5.3.3）。
7. **`ping`**；连接审计：建立、认证结果、拒绝原因、断开。
8. **只运行网关的测试夹具进程**：用 test-harness 门控，T073 会复用。

**不做**：
- 方法分发与策略（T031、T032）；
- 订阅协议（T033）；
- "孤立"状态（T037）。

## 验收标准

- [ ] 新增 IT-handshake：用真实套接字验证以下几点：
  - `host/hello`；
  - `initialize` 只接受一次；
  - 版本协商；
  - 四种认证在各构建种类下是否开放；
  - 错误码。
- [ ] 新增 IT-pinned-token（crate 内，真实套接字）：另一个进程中途接手连接后，下一次读取就被发现，连接断开。
- [ ] 新增 UNIT-proof-binding：双向证明绑定 `nonce` 和信道参数，换掉任何一项，证明都失败。
- [ ] 新增 IT-devkey-launcher-only：拉起夹具的测试进程出示 `dev_key` 被接受；持有同一密钥的其他进程被拒绝。
- [ ] 新增 IT-devkey-orphan-start：`dev-auth` 构建启动时 ppid 为 1，以退出码 1 退出。
- [ ] 收到 `SCM_RIGHTS` 传来的 fd 时断开并记审计，要有用例。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T030-gateway-handshake/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **密钥**：
  - 取 `payload.dev_key`、`payload.harness_key` 的 `SecretKey::expose()`；
  - `dev_key` 只接受拉起宿主的进程（5.8.1，`p_puniqueid`，见 T021）。（T023）
- **读套接字**：
  - 一律用 `cupper_sys::recv_closing_fds`，因为普通读取会装入 `SCM_RIGHTS` 传来的 fd；
  - 网关的写法是 `stream.try_io(Interest::READABLE, || recv_closing_fds(&stream, buf))`；
  - `closed_fds > 0` 时断开连接并记审计（设计 5.8.5 v1.8）。（T021）
- **pid 与父进程**：
  - `LOCAL_PEERPID` 在最后一次 I/O 的进程退出后，仍返回它的旧 pid，这个 pid 可能已被复用，所以 `dev_key` 证明里的 `pid_c` 取固定令牌的 `pid()`；
  - 固定令牌只反映最后一次 I/O，比对要跟在每一次 read 之后；
  - 判断"启动时父进程已退出"用 `process_info(自己).ppid == 1`，`p_puniqueid` 只用来比对对端。（T021）
- **判断对端是否关闭**：以读到 EOF 为信号，不要关闭后立刻查 `getsockopt`，因为并发拉起子进程时内核会短暂复制 fd 表。`peer_token` 已经把两次 `getsockopt` 之间对端关闭的情况归为 `PeerExited`。（T021 修复第 2 轮）
- **`params` 为 `null`**：`RawMessage::classify` 会把缺省的 `params` 变成 `null`。`ping` 这类参数为 `{}` 的方法按结构体解析会失败，所以分发时把 `null` 当作 `{}`。（T012）
- **可以直接用的工具**：`methods::handshake::PROTOCOL`、`VersionRange::negotiate`。`nonce`、`proof` 在协议里是字符串，格式校验和转成字节由本任务做。（T012）
- **惯例**：
  - 时间只经 `Clock`；
  - 拉子进程一律经 `cupper_sys::spawn`；
  - `unsafe` 只能在 `cupper-sys`；
  - 用到 `ManualClock` 等测试设施的集成测试，声明 `required-features = ["test-harness"]`；
  - `lib.rs` 只放 `mod` 声明。

## 规模

计划估为 L，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试和测试辅助单独列一项**（计划 1.1 的执行经验：测试通常占 40%–55%，rustfmt 会把紧凑的写法展开）。动手后**每完成一个模块统计一次**（守则第 8 条）。预计会明显超出时，先停下来汇报，并给出拆分建议，例如"连接、令牌与写循环"和"握手与认证"分开。

## 参考资料

- 设计：
  - `02-host.md` 5.2.4（网关）、5.2.13（启动契约与退出码）；
  - `08-security.md` 5.8.1（本地认证、各构建种类、`dev_key`）、5.8.5；
  - `03-protocol-v1.md` 5.3.2（握手）、5.3.3（慢客户端）。
- ADR-05（含实现时的补充）。
- SP-4a 的 `conn.rs`、`outbox.rs`、`wire.rs`、`auth.rs`；SP-4b 的报告。
- T021 的 `notes.md`；T012 的 `methods/handshake.rs`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料（`notes.md` 等）由执行者在任务分支上提交。

## 补充裁定（2026-10-19，编排者）

执行者开工前按模块估算，总规模约 1,815 行（实现约 1,085 行，测试约 730 行），按守则第 8 条在写代码之前停下汇报。

**裁定：拆分（计划 v1.3.5）。** 本任务收窄为：
- 连接、监听与 `host.lock`；
- 准入（uid、签名结论、固定令牌）；
- 读循环（`recv_closing_fds`，收到 fd 就断开）；
- 写循环与慢客户端；
- `host/hello`、只接受一次的 `initialize`、版本协商；
- `peer` 和 `codesign` 两种认证，以及 principal 与角色；
- `ping`、审计、testkit。

`dev_key` 和 `harness` 在本任务里一律回 `unauthenticated/bad_key`，不留绕过认证的入口。

新任务 T039 接手的部分：
- 证明的计算，放进 `cupper-protocol`，是纯函数，CLI 和原生扩展共用，依赖 `hmac =0.12.1` 和 `sha2`；
- 宿主侧的 `dev_key`（只接受拉起宿主的那个进程）、`harness` 和 `host_proof`；
- ppid 为 1 时退出；
- `cupper-sys` 里按 pid 等待退出码（kqueue）；
- CLI 客户端库的 `harness` 认证；
- 网关夹具；
- UNIT-proof-binding、IT-devkey-launcher-only、IT-devkey-orphan-start，以及 IT-handshake 里密钥认证的部分。

T039 依赖 T030 和 T045。T033 的集成测试用 peer 认证，或者在 crate 内直接构造 principal，所以不依赖 T039，关键路径仍是 33。

执行者 notes.md 里的待裁定事项：
- 第 3–8 条已写进 `03-protocol-v1.md` 5.3.11 末尾的"T030 开工前的裁定"：
  - 审计记录的落盘归 T037；
  - 对端的路径、identifier、Team ID 归 T038；
  - 写循环的范围；
  - 各构建开放的认证；
  - 错误码；
  - 两个时长。
- 第 1、2 条（`hmac`、kqueue）随密钥认证转给 T039。

