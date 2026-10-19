# T073 汇报

- 执行者：子 agent（Rust 工程师，原生扩展与安全）
- 分支：`feat/T073-desktop-native`
- 完成日期：2026-10-19

## 做了什么

新建 crate `crates/cupper-desktop-native`（napi-rs cdylib，lib 名为 `cupper_native`，产物是 `libcupper_native.dylib`，发布时改名为 `cupper_native.node`）。模块如下：

- **`launch.rs` 拉起宿主**：经 `cupper_sys::spawn` 拉起，用 `new_session()`（`POSIX_SPAWN_SETSID`），环境变量由调用方显式给出（`CUPPER_*` 由 `Env::set` 拒绝）。启动载荷写成一行 JSON 加换行，写完立即关闭 stdin；发布构建不写载荷，stdin 指向 `/dev/null`。宿主由一个独立线程 `waitpid` 回收，然后 resolve JS 侧的 `exit` Promise，不占用 libuv 线程池。
- **`check.rs` 按构建种类校验宿主**（5.8.1 表格中"UI 校验宿主"一列）：
  - 发布构建（不带特性）按签名要求校验。`HOST_REQUIREMENT` 在 T180 之前为空，所以一律拒绝，错误码 `no_requirement`。
  - `dev-auth` 构建（开发、自用、e2e）只要求对端是同一用户，然后固定令牌；宿主的身份由 `host_proof` 证明。
  - 拒绝时，错误码按 5.8.1 的四类错误分别给出：`peer_exited`、`peer_execed`、`code_changed`、`unverifiable`；另有 `not_the_host`、`other_user`、`no_requirement`。
- **`relay.rs` 连接的读写**：主进程拿到的是一对 socketpair 中的一端，用来包 `net.Socket`；宿主的套接字留在扩展里，由两个线程转发。
  - 读宿主一律经 `recv_closing_fds`，每次读完都与固定令牌比对，两项都通过才把数据交给主进程。
  - 收到 fd、令牌变化、EOF 时，两个方向一起断开，原因记录在 `closeReason` 中：`host_closed`、`fds_passed`、`token_changed`、`host_unverified`、`local`。
- **`key.rs`（仅 `dev-auth`）**：`HostKey` 是不透明句柄，32 字节密钥从系统随机源取得，不会离开扩展。启动载荷由扩展拼好：`dev_key` 加上调用方给的 `harness_key`、`test_config`。`auth(nonce)` 用 `c2h` 方向、自己的 pid 计算证明。`verifyHostProof(proof)` 用固定令牌里的 pid 校验，证明不符或缺失时扩展自己断开连接。证明经 `cupper_protocol::proof` 计算。
- **`lib.rs` 的 napi 绑定**：
  - 导出 `buildInfo`、`socketPath(dataDir)`、`spawnHost`、`connectHost`、`HostConnection`（`takeFd`、`hostPid`、`closeReason`、`close`）。
  - 仅 `dev-auth` 构建导出 `HostKey`、`auth`、`verifyHostProof`。
  - 抛给 JS 的错误都带语义 `code`（用 napi 的 `Error<&str>` 实现）。
- **TS 侧**：pnpm 包 `@cupper/desktop-native` 放在 crate 目录里，已加入 `pnpm-workspace.yaml`。
  - `ts/index.ts` 给出模块的类型声明，分 `ReleaseNative` 和 `DevAuthNative` 两种；`DevKeyAuth` 直接取自 `@cupper/protocol` 的 `Auth`，类型由 `tsc --build` 核对。
  - `ts/native.test.ts` 是 Node 侧的 UNIT-native 用例。
- **xtask `rust` 作业**：
  - 对扩展的发布构建（不带特性）单独跑一次 clippy 和测试；
  - 先 `pnpm install --frozen-lockfile`，再运行扩展包的 Node 用例，结果按 Vitest 的 JSON 报告逐条计入。

## 关键取舍

1. **主进程拿到的是 socketpair 的一端，不是宿主的 fd。** 5.7.2 写的是"返回 fd，再用 `new net.Socket({fd})` 包装"，但 Node 用普通 `read(2)` 读，会把宿主传来的 fd 装进主进程（T021 实测）。这与 5.8.5 v1.8 和任务书第 4 条直接冲突。
   - 现在的做法：JS 侧照样是标准的 `net.Socket`，流和背压都在，T076 可以直接用 `socket.pause()` 做流控。
   - 代价：每条连接两个线程，多一次拷贝。
   - **建议编排者把 5.7.2 那句改成"返回 socketpair 的一端"。**
2. **密钥不出扩展。** 这样做的后果是 UNIT-proof-binding 的已知答案只能在 Rust 侧，沿本扩展的路径核对（`unit_native_proof_binding_known_answer`，两条答案都一致）。Node 侧对真实网关夹具跑通了双向证明。
3. **自用构建的 UI 没有加 cdhash 校验。** 我按 5.8.1 表格做：UI 在自用构建里用 `dev_key` 的双向证明；交接事项里的"按 cdhash 校验"指的是 CLI。如果要再加一道 cdhash，需要 T005 在编译扩展时传入，请编排者裁定。
4. **`connectHost` 是同步的**，这样错误能带语义码；napi 的 `AsyncTask` 只能用它自己的状态码。dev-auth 构建只取一次令牌；发布构建做签名校验，约 1 ms（5.7.1 的估计）。
5. **`unsafe`**：没有手写任何 `unsafe`，也没有给 `cupper-sys` 加新的系统调用。napi-rs 3.14 的宏展开不触发 `unsafe_code`（工作区的 `deny` 照常生效），所以**没有用任何 `#[expect(unsafe_code)]`**。
6. **依赖**：
   - Rust：`napi =3.14.2`（只开 `napi4`、`dyn-symbols`，这样 `cargo test` 不需要 Node 也能链接）、`napi-derive =3.6.12`、`napi-build =2.6.0`。依赖闭包的许可证是 MIT、ISC、Apache-2.0、Unicode-3.0，`cargo deny` 通过。
   - Node：只有开发依赖，`@types/node 24.19.1`、`vite 8.3.3`、`vitest 5.0.3`，都是精确版本，与现有包一致。

## 验证方式

- **UNIT-native 主路径（Node）**：Node 进程加载 `dev-auth` 构建的扩展，用它拉起 ad-hoc 签名的网关夹具。
  - `socketPath` 与夹具输出的路径一致；
  - 连接成功，`hostPid` 等于夹具的 pid；
  - 以拉起方的身份出示 `dev_key`，得到 `pr_desktop`；
  - `verifyHostProof` 返回 true，`ping` 正常；
  - `close` 后 `closeReason` 为 `local`，`exit` 得到 `{signal: 15}`。
- **拒绝路径**：
  - `host_proof` 不符或缺失时返回 false 并断开，原因为 `host_unverified`（Node）。
  - 令牌变化时断开：Node 侧用假宿主，由 `sh` 在宿主那端写入，主进程收到 0 字节，原因为 `token_changed`；Rust 侧另有 `unit_native_relay_disconnects_when_the_token_changes`。
  - 收到 fd 时断开：只有 Rust 侧的 `unit_native_relay_disconnects_when_fds_come`。Node 没有公开 API 能在普通 Unix 套接字上传 fd。
  - 另外覆盖了：载荷里自带 `dev_key`、环境变量含 `CUPPER_*` 时报 `invalid_argument`；连接失败报 `connect_failed`；第二次调用 `auth` 或 `takeFd` 时报错。
- **变异核对**：临时去掉令牌比对后，Node 用例失败。Rust 的转发用例原本会因此一直挂住，已给测试套接字加了读超时，改成会失败。
- **稳定性**：Rust 用例连跑 30 次、Node 用例连跑 15 次，全部通过；跑完后没有残留的夹具进程。
- **测试命名**：Node 侧标题为 `describe("UNIT-native")`；Rust 侧共 9 个 `unit_native_*`（发布构建 7 个）。
- **CI**：在干净的 worktree 里运行 `cargo xtask ci pr --record docs/tasks/T073-desktop-native/ci/`，在 `3872125` 上通过：
  - rust 993 项、protocol 176 项、ts 232 项、license 全部通过；
  - testdata-scan、ui、e2e 尚未实现；
  - 摘要 `pr-387212518fee.json` 已提交，其中 UNIT-native 为 pass。
- **规模**：新增 1,335 行、删除 17 行，不含锁文件和任务目录。其中实现约 915 行，测试约 420 行（Rust 228、Node 192），比估计的 1,115 行多 20%，在约 1,500 行的停止线以内。逐模块的行数记在 `notes.md`。

## 提交列表

```
9bbad64 docs(task): T073 CI summary (pr at 387212518fee)
3872125 docs(task): T073 notes: line totals agree
c8e6738 docs(task): T073 notes: estimate, actual lines, trade-offs
cd55bdd refactor(native): take DevKeyAuth from the protocol's Auth type
241662b ci(xtask): native extension's release build and Node tests in the rust job
d8362b9 test(native): UNIT-native from Node, with the module's TypeScript declaration
e420439 feat(native): cupper-desktop-native extension (cupper_native.node)
```

## 遗留问题 / 范围外发现

1. **与 T071 的 `handshake(port, options)` 对不上的地方**（依据 T071 分支上的 `handshake.ts`）：
   - T071 的 `auth(hello)` 接收整个 hello，扩展的是 `conn.auth(nonce)`。T075 的胶水：`(hello) => conn.auth(hello.nonce)`。返回类型就是协议 `Auth` 的 `dev_key` 分支。
   - T071 的 `verifyHostProof(proof, hello, auth)` 有三个参数，扩展只需要 `conn.verifyHostProof(proof)`，信道由扩展自己记着。
   - **`host_proof` 缺失时，T071 不调用 `verifyHostProof`，直接报 `host_unverified`**。这条路径扩展不会自己断开，要由 T075 在握手失败时调用 `conn.close()`。
   - 扩展的两个函数都是同步的，T071 两种都接受。
2. **设计 5.7.2 的措辞**，见关键取舍第 1 条。
3. **给 T075 的交接**：
   - 连接被拒时 `connectHost` 抛出的 `code` 可以并入 `ConnectionState` 的 `failed.reason`。
   - 宿主被杀后重新拉起时，复用同一个 `HostKey`。
   - 用 `buildInfo()` 核对扩展的构建种类与 App 的 flavor 一致。
   - 宿主写完数据后立刻退出时，令牌已经读不到，那批数据会被丢弃，原因记为 `token_changed`（与 CLI 的 `still_pinned` 相同）。例如最后一条 `host/shutdown` 可能收不到；UI 不宜把 `token_changed` 一律当作攻击来提示。
4. **给 T161 和发布流水线**：`release-config` 可以断言扩展发布构建的 `buildInfo().features` 为空。
5. **给 T076**：macOS 上 socketpair 和本机套接字的默认缓冲区只有 8 KB，转发会分成很多小块。如果测量闸门不达标，需要在 `cupper-sys` 里加 `setsockopt(SO_SNDBUF/SO_RCVBUF)`。
6. **依赖整理**：`getrandom 0.4.3` 现在有三个使用方（host、cli、native）。按交接表应提到 `[workspace.dependencies]`，这次没有动别的 crate。
7. **`cargo deny`**：多了一个 `convert_case` 的重复版本告警，来自 napi-derive，只是告警。另外，`napi-build 2.6.0` 的源码包里带一个只用于 wasm 目标的预编译 `.o` 文件，macOS 构建不使用它。
