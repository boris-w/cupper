# T073 工作笔记

- 执行者：子 agent（Rust 工程师，原生扩展与安全）

## 开工前的规模估计

行数指 rustfmt、Prettier 展开后的行，含文档注释和空行，不含 `Cargo.lock`、`pnpm-lock.yaml`。

**实现**

| 模块 | 内容 | 估计 |
|---|---|---|
| `Cargo.toml`、`build.rs`、`lib.rs` 的骨架 | napi-rs 锁精确版本；`dev-auth` 特性；cdylib 名为 `cupper_native` | 50 |
| `launch.rs` 拉起宿主 | 经 `cupper-sys::spawn`（`POSIX_SPAWN_SETSID`、显式环境变量、stdio）；写一行启动载荷后立即关闭 stdin；等退出码的线程 | 70 |
| `check.rs` 校验宿主 | 按构建种类：发布构建按签名要求（T180 前为空，一律拒绝），`dev-auth` 构建只要求同一用户、身份留给 `host_proof`；固定令牌；拒绝原因的语义码 | 60 |
| `relay.rs` 连接的读写 | 主进程拿到的是 socketpair 的一端；扩展在两端之间转发，读宿主一律经 `recv_closing_fds`，每次读完比对固定令牌；收到 fd、令牌变化、EOF 时断开并记下原因 | 110 |
| `key.rs` 密钥与证明（`dev-auth`） | 32 字节随机密钥，不出扩展；启动载荷 JSON；`auth`（`c_nonce`、`c2h` 证明，pid 取自己）；校验 `host_proof`（pid 取固定令牌） | 70 |
| `lib.rs` 的 napi 绑定 | `buildInfo`、`socketPath`、`HostKey`、`spawnHost`、`connectHost`、`HostConnection`（`takeFd`、`peerPid`、`closeReason`、`auth`、`verifyHostProof`、`close`）；错误的 `code` | 150 |
| `ts/index.ts` | 给 T075 的模块类型声明 | 70 |
| Node 包骨架 | `package.json`、`tsconfig.json`、`vitest.config.ts`；`pnpm-workspace.yaml` 加一行 | 40 |
| xtask `rust` 作业 | 原生扩展不带特性的 clippy 和测试；Node 用例一步 | 25 |
| **实现小计** | | **645** |

**测试**

| 项 | 内容 | 估计 |
|---|---|---|
| `key.rs` | 经本扩展的路径用 UNIT-proof-binding 的已知答案交叉核对；启动载荷的格式 | 60 |
| `relay.rs` | 双向转发；收到 fd 断开；另一进程写入、令牌变化断开；EOF；本端关闭 | 130 |
| `check.rs` | 发布构建没有签名要求时拒绝；要求不满足时拒绝；`dev-auth` 接受同一用户并固定令牌 | 35 |
| `launch.rs` | 载荷只有一行，之后 stdin 已关闭；新会话；退出码 | 45 |
| Node 的 UNIT-native | 切帧小工具；拉起夹具 → 连接 → `dev_key` 被接受 → `host_proof` 通过 → `ping`；`host_proof` 不符时断开；假宿主那端换了进程写入时断开 | 200 |
| **测试小计** | | **470** |

**合计约 1,115 行**，超出计划的 M（400–900），在约 1,500 行的停止线以内。按 T039 的经验偏差（+13%）推算约 1,260 行，按 +25% 推算约 1,390 行，所以要写紧。每完成一个模块用 `git diff --numstat main...HEAD` 统计一次；推算总数超过约 1,500 行时，停下来汇报。

## 实际行数（逐模块）

| 完成的模块（提交） | 新增 / 删除 | 其中测试 | 估计（实现 + 测试） | 累计新增 |
|---|---|---|---|---|
| 扩展本体：`launch`、`check`、`relay`、`key`、napi 绑定（`feat(native)`） | 953 | 228 | 450 + 270 | 953 |
| 类型声明与 Node 的 UNIT-native（`test(native)`） | 350 | 192 | 110 + 200 | 1,303 |
| xtask `rust` 作业（`ci(xtask)`） | 31 / 17 | — | 25 | 1,334 / 17 |
| `DevKeyAuth` 改取协议的 `Auth`（`refactor(native)`） | 7 / 6 | — | — | **1,341 / 23** |

**合计新增 1,335 行、删除 17 行（`git diff --numstat main...HEAD`，不含两个锁文件和本目录）**，其中实现约 915 行，测试约 420 行（Rust 228、Node 192）。比估计（1,115）多 20%，几乎都在 `lib.rs`：napi 绑定的文档注释，加上 `dev-auth` 与发布构建各一份的 `spawnHost`、`connectHost`（估计 200，实际 311）。第一个模块提交后按 +32% 推算约 1,470 行，接近停止线但在其内；第二个模块后已知总数约 1,300，之后只剩 xtask。

## 实现时的取舍

**主进程拿到的 fd 是 socketpair 的一端，宿主的套接字留在扩展里**

- 5.7.2 写的是"返回 fd，再用 `new net.Socket({fd})` 包装"，而 5.8.5（v1.8）和任务书要求读宿主一律经 `recv_closing_fds`、收到 fd 就断开。Node 的 `net.Socket` 用普通 `read(2)` 读，宿主传来的 fd 会被装进主进程（T021 实测），两条要求直接冲突。
- 做法：扩展建一对 socketpair，主进程拿一端去包 `net.Socket`；扩展用两个线程在另一端与宿主之间转发。读宿主经 `recv_closing_fds`，每次读完比对固定令牌，两项都通过才把数据写给主进程；收到 fd、令牌变化、EOF 时两边一起断开，原因记在 `closeReason`。
- 好处：JS 侧照样是标准的 `net.Socket`（流、背压都在）；主进程不读就不会再读宿主，T076 的"未确认超过 8 MB 暂停读取"可以直接用 `socket.pause()`。代价是每条连接两个线程、多一次拷贝；M1 只有两条连接。
- 建议编排者把 5.7.2 那句改成"返回 socketpair 的一端"。

**密钥不出扩展**

- `HostKey` 是不透明的句柄：密钥在构造时由扩展从系统随机源取，JS 拿不到字节。启动载荷（`dev_key` 加上 e2e 的 `harness_key`、`test_config`）由扩展拼好写进 stdin；`auth()`、`verifyHostProof()` 都在扩展里算。宿主被杀后重新拉起时，T075 再传同一个 `HostKey` 即可（5.7.2"原样再传"）。
- 因此 UNIT-proof-binding 的已知答案只能在 Rust 侧核对：`unit_native_proof_binding_known_answer` 走的是本扩展的路径（`host/hello` 里的十六进制 nonce、`c_nonce`、pid），结果与 T039 用 Python 独立算出的两条答案一致。Node 侧的用例对真实网关夹具跑通双向证明，证明两端互通。

**按构建种类校验宿主**

- 只有一个特性 `dev-auth`：不带特性是发布构建，按签名要求校验（`HOST_REQUIREMENT` 在 T180 前为空，所以一律拒绝，错误码 `no_requirement`）；带 `dev-auth` 是开发、自用、e2e 构建，连接时只要求同一用户并固定令牌，身份由 `host_proof` 证明。这是 5.8.1 表格"UI 校验宿主"一列的做法。
- 任务书交接事项里"自用构建按 cdhash 校验"说的是 CLI（T045）；5.8.1 的表格里 UI 在自用构建用 `dev_key` 的双向证明，这里照表格做，没有加 cdhash。如果要给 UI 的自用构建再加一道 cdhash，需要 T005 在编译扩展时传入宿主的 cdhash，属于范围外。
- 发布构建里没有 `HostKey`、`auth`、`verifyHostProof`，也不写启动载荷；`buildInfo()` 报告特性和校验方式，T075 可以据此核对构建种类。

**同步与异步**

- `connectHost` 是同步的：本机套接字的 connect 立即返回（队列满时 macOS 直接回 `ECONNREFUSED`），`dev-auth` 构建只取一次令牌，发布构建的签名校验约 1 ms（5.7.1 的估计）。这样错误能带语义码（napi 的 `Error<&str>` 会把它设为 JS 错误的 `code`）；napi 的 `AsyncTask` 只能用 napi 自己的状态码。
- 等宿主退出用独立线程加 deferred Promise，不占 libuv 线程池：宿主可能运行好几天。

**`unsafe`**

- 没有手写任何 `unsafe`，也没有往 `cupper-sys` 加新的系统调用。napi-rs 3.14 的宏展开不触发 `unsafe_code`（工作区的 `deny` 照常生效），所以也没有用 `#[expect(unsafe_code)]`。
- `napi` 关掉默认特性，只开 `napi4` 和 `dyn-symbols`：符号在加载时查找，`cargo test` 不需要 Node 也能链接。

**Node 用例放在哪里**

- 扩展的 TS 部分（类型声明 `ts/index.ts` 和 Node 用例）作为 pnpm 包 `@cupper/desktop-native` 放在 crate 目录里，`pnpm-workspace.yaml` 加了这一项。它的 `test` 脚本先 `cargo build` 扩展的 `dev-auth` 构建和网关夹具，再 `tsc --build`、`vitest run`。
- `ts` 作业只扫 `packages/`、`apps/`、`tools/`，设计上它将来跑在 ubuntu 上，编译不了扩展，所以 Node 用例由 `rust` 作业运行（先 `pnpm install --frozen-lockfile`，再按 Vitest 的 JSON 报告逐条计入）。`rust` 作业同时对扩展的发布构建（不带特性）单独跑 clippy 和测试。
- Vitest 用 `forks` 池：加载扩展、拉起夹具、连接夹具的是同一个进程，夹具的"只接受拉起方"才成立。

**与 T071 的 `handshake(port, options)` 对齐**

T071 分支上的接口（`packages/host-client/src/handshake.ts`）：`auth(hello: HostHelloParams): Auth | Promise<Auth>`、`verifyHostProof(proof: string, hello, auth): boolean | Promise<boolean>`，只在 `host_proof` 是字符串时才调用。

- `auth`：扩展是 `conn.auth(nonce: string)`，返回值的类型就是协议 `Auth` 的 `dev_key` 分支。T075 的胶水：`auth: (hello) => conn.auth(hello.nonce)`。
- `verifyHostProof`：扩展是 `conn.verifyHostProof(hostProof?: string | null)`，`c_nonce` 和信道由扩展自己记着，`hello`、`auth` 两个参数用不到。胶水：`verifyHostProof: (proof) => conn.verifyHostProof(proof)`。
- 证明不符时扩展自己断开连接（`host_unverified`），不依赖调用方。但 `host_proof` 缺失时 T071 不调用 `verifyHostProof`，直接报 `host_unverified`，这条路径要由 T075 在握手失败时 `conn.close()`。
- 扩展的两个函数都是同步的，T071 两种都接受。
