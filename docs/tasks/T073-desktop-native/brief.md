# T073 cupper-desktop-native

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，原生扩展与安全）
- 分支：`feat/T073-desktop-native`
- 依赖：T020、T039（均已合并）。**零松弛**：T030 → T039 → T073 → T075 → T076 → … 。T075 依赖它。

## 背景

桌面主进程（Electron）经 napi-rs 原生扩展连接宿主（设计 5.1.3、5.7.2、5.8.1）。扩展负责：
- 拉起宿主并写入启动载荷；
- 连接并完整校验宿主的签名，然后把 fd 交给主进程；
- 每次读取后都比对令牌；
- 计算 `dev_key` 的证明，并校验 `host_proof`。

UI 侧的连接逻辑（host-client 的握手，见 T071）通过回调调用扩展计算证明。host-client 不做 HMAC，见 `07-desktop.md` 5.7.6 末尾"T071 开工前的裁定"。

## 范围

**做**（计划 T073 一行；设计 5.1.1、5.1.3、5.7.2、5.8.1、5.8.5）：
1. 新建 crate `crates/cupper-desktop-native`，是 Rust cdylib（napi-rs），产物为 `cupper_native.node`（5.1.1）。
2. **拉起宿主**：经 `cupper-sys::spawn` 拉起宿主，写启动载荷：一行 JSON 加换行，写完立即关闭 stdin。
3. **连接与校验**：
   - 连接套接字；
   - 完整校验宿主的签名，按构建种类区分，见 5.8.1 和 5.7.9；
   - 固定令牌；
   - 校验通过后把 fd 交给主进程。
4. **读取**：每次读取后都与固定令牌比对；读取一律经 `cupper_sys::recv_closing_fds`，收到 fd 就断开。
5. **证明**：
   - `dev_key` 的证明用 `cupper-protocol::proof`（T039）计算；
   - 校验 `host_proof`；
   - 用 T039 的已知答案（UNIT-proof-binding）交叉核对。
6. **给 host-client 的接口**：给 T071 的 `handshake(port, options)` 提供 `auth(hello)` 和 `verifyHostProof` 所需的原生函数，形状由你和 T071 的接口对齐。T071 正在并行进行，接口看它分支上的 `notes.md`；对不上的地方写进汇报。

**不做**：Electron 侧的逻辑（T075），包括 `hostLauncher` 和 control 连接。

## 验收标准

- [ ] 新增 UNIT-native：由加载扩展的 Node 测试进程，对 ad-hoc 签名的网关夹具跑通整条路径：
  - 拉起；
  - 校验；
  - 出示 `dev_key` 被接受；
  - `host_proof` 校验通过。
- [ ] 拒绝路径：
  - `host_proof` 不符时断开；
  - 收到 fd 时断开；
  - 令牌变化时断开。
- [ ] 测试标题和测试名以用例编号开头：Node 侧用 `UNIT-native` 原文，Rust 侧用 snake_case。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T073-desktop-native/ci/` 通过，摘要随任务提交。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **网关夹具**（T039）：
  - 用 `--features dev-auth,test-harness` 构建，必须给 `--data-dir`，stdin 读启动载荷；
  - 监听后在 stdout 输出 `{"socket": ...}`，审计按 JSON 行写到 stderr；
  - `dev_key` 只接受拉起夹具的那个进程，也就是 Node 测试进程；
  - `host_proof` 按夹具的 pid 校验；
  - 证明的已知答案见 UNIT-proof-binding（`cupper-protocol::proof`）。
- **读取**（T021）：
  - 读 cupper 的套接字一律用 `cupper_sys::recv_closing_fds`，普通读取会装入 `SCM_RIGHTS` 传来的 fd；
  - 判断对端已关闭，以读到 EOF 为准。
- **pid**（T021）：`LOCAL_PEERPID` 在最后一次 I/O 的进程退出后，仍会返回旧 pid，所以证明里的 pid 取固定令牌的 `pid()`；固定令牌只反映最后一次 I/O，所以每次 read 之后都要比对。
- **启动载荷**（T023）：
  - 一行 JSON 加换行，写完立即关闭 stdin；
  - 密钥是 64 位十六进制；
  - 出现未知的键时，宿主以退出码 1 退出。
- **宿主侧的对应**（T030、T045）：
  - 签名要求在 T180 之前为空，所以发布构建的签名校验暂时不通过任何宿主；
  - 自用构建按 cdhash 校验（T045 的 CLI 也是这样做的），可以参考 `crates/cupper-cli/src/verify.rs`。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 注意

- **`unsafe`**：只能出现在 `cupper-sys`，工作区设了 `unsafe_code = "deny"`。
  - 不要手写任何 `unsafe`；需要新的系统调用时，加到 `cupper-sys` 里，带 `SAFETY:` 注释。
  - 如果 napi-rs 的宏展开触发了 `unsafe_code`，用范围最小的 `#[expect(unsafe_code, reason = "...")]`，写明是宏生成的代码，并在汇报里单独列出，由编排者裁定。
- **依赖**：napi-rs 及其依赖锁精确版本；许可证要在 ADR-11 的 allowlist 内；Node 侧依赖也锁精确版本。
- **产物**：放在 asar 之外（`asarUnpack`），签名归 T160；本任务只负责构建和测试。

## 规模

计划估为 M（400–900 行）。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出约 1,500 行时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `01-repo-build-ci.md` 5.1.1、5.1.3；
  - `07-desktop.md` 5.7.2、5.7.6（T071 的裁定）、5.7.9；
  - `08-security.md` 5.8.1、5.8.5。
- T039 的 `proof.rs`、网关夹具和 `notes.md`；T045 的 `verify.rs`；T021 的 `cupper-sys`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
