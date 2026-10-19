# T039 网关 I-b：密钥认证与双向证明

- 派发日期：2026-10-19
- 执行者：子 agent（Rust 工程师，宿主网关与安全）
- 分支：`feat/T039-gateway-key-auth`
- 依赖：T030、T045（均已合并）。本任务由 T030 拆分而来（计划 v1.3.5），在零松弛的链上：T030 → T039 → T073 → T075 → …。T047 也依赖它。

## 背景

T030 做好了网关的连接、准入、读写循环、握手，以及 `peer`、`codesign` 两种认证；`dev_key` 和 `harness` 在那里一律回 `unauthenticated/bad_key`。T045 做好了 CLI 的连接与宿主校验，e2e 分支只查同用户和固定令牌。本任务补上密钥认证和双向证明，宿主和 CLI 两侧都要做（设计 5.8.1）。

## 范围

**做**（计划 T039 一行；设计 5.8.1、5.3.2、5.3.11 末尾 T030 的裁定与握手期限）：
1. **证明的计算**：放进 `cupper-protocol`，写成纯函数，CLI 和原生扩展共用。
   - `proof = HMAC-SHA256(key, "c2h" ‖ h_nonce ‖ c_nonce ‖ pid_c)`；
   - 宿主的 `host_proof` 按 5.8.1 的对称写法；
   - 用常量时间比较；
   - 新依赖 `hmac = "=0.12.1"`，与已有的 `sha2` 同一代，许可证是 MIT OR Apache-2.0；
   - `nonce`、`proof` 字符串的格式校验和转成字节，也在这里。
2. **宿主侧**（接入点见 T030 的 `notes.md`"给 T039 的接入点"）：
   - `dev_key` 认证只在 `dev-auth` 构建里有。它只接受拉起宿主的那个进程：比对对端的 `p_uniqueid` 与本进程的 `p_puniqueid`。证明错误或缺失时，回 `unauthenticated` 并断开，不降级成只读的 cli；
   - `harness` 认证只在 `test-harness` 构建里有；
   - `pid_c` 取固定令牌的 `pid()`；
   - `host_proof` 放进 `initialize` 的结果；
   - `principal` 补上 `pr_test` 那一行。
3. **启动时 ppid 已是 1**：`dev-auth` 构建的宿主这时以退出码 1 退出，不等待任何 `dev_key` 连接。判断用 `process_info(自己).ppid == 1`。
4. **握手期限**（编排者裁定）：发出 `host/hello` 之后 10 s 内没有收到 `initialize`，就断开连接，审计原因记 `handshake_timeout`。期限登记在 `durations.rs`，经 `Clock` 计时。
5. **`cupper-sys`**：增加按 pid 等待退出码的函数，用 kqueue 的 `NOTE_EXIT | NOTE_EXITSTATUS`，能拿到已被 launchd 收养的进程的退出码。写上 `SAFETY:` 注释；IT-devkey-orphan-start 要用它。
6. **CLI 侧**（接入点见 T045 的 `notes.md` 末尾）：
   - `ClientAuth::Harness` 和 `--harness-key-fd`，密钥格式与启动载荷相同，都是 64 位十六进制；
   - 校验 `host_proof`；
   - e2e 分支的 `HostCheck::Harness` 在证明落地后，`is_weak` 改为 false。
7. **只运行网关的测试夹具进程**：
   - 用 `test-harness` 门控，T073 会复用；
   - 夹具把审计记录写到 stderr，供进程级用例断言。

**不做**：
- 审计落盘（T037）；
- UI 侧的 `dev_key` 客户端：TS 和原生扩展部分归 T073、T075。

## 验收标准

- [ ] 新增 UNIT-proof-binding：双向证明绑定 `h_nonce`、`c_nonce`、`pid_c` 和方向标签，换掉其中任何一项，证明都失败；比较是常量时间的。
- [ ] 新增 IT-devkey-launcher-only：拉起夹具的测试进程出示 `dev_key`，被接受；持有同一密钥的其他进程（例如 `nc -U`）被拒绝，并且确实是被"只接受拉起宿主的进程"这一条拒绝的。
- [ ] 新增 IT-devkey-orphan-start：`dev-auth` 构建启动时 ppid 已是 1，以退出码 1 退出。用 kqueue 在进程级断言退出码。
- [ ] IT-handshake 补上密钥认证的部分：
  - `dev_key`、`harness` 的接受与拒绝；
  - 各构建开放的认证（5.3.11 的表）；
  - `host_proof` 校验；
  - 握手期限。
- [ ] CLI 的 `harness` 认证：有用例，包括 `host_proof` 不符时拒绝连接。
- [ ] 测试要覆盖拒绝的路径。
- [ ] 在干净的 worktree 里，`cargo xtask ci pr --record docs/tasks/T039-gateway-key-auth/ci/` 通过，摘要随任务提交。
- [ ] 测试名以用例编号的 snake_case 开头。

## 交接事项（来自 `docs/tasks/handoff-notes.md`）

- **宿主侧接入点**（T030）：
  - `handshake::authenticate` 的 match；
  - `GatewayConfig` 里的密钥；
  - 传给 `initialize` 的固定令牌；
  - `host_proof` 的返回值；
  - `principal` 里 `pr_test` 那一行。
  - 另外实现握手期限。
- **CLI 侧接入点**（T045）：
  - `ClientAuth` 加 `Harness` 变体；
  - `ClientAuth::credentials(&hello, &host)` 已经拿到 nonce 和固定令牌；
  - 在 `Client::handshake` 里 `check.accept` 的旁边校验 `host_proof`。
- **密钥**（T023）：取 `payload.dev_key`、`payload.harness_key` 的 `SecretKey::expose()`。
- **pid 与父进程**（T021）：
  - `LOCAL_PEERPID` 在最后一次 I/O 的进程退出后，仍返回旧 pid，所以 `pid_c` 取固定令牌的 `pid()`；
  - 判断"启动时父进程已退出"用 `process_info(自己).ppid == 1`，`p_puniqueid` 只用来比对对端。
- **实测手段**（T030 的 `notes.md`）：
  - `nc -U` 可以当"持有同一密钥的另一个进程"：测试进程按它的 pid 替它算出正确的证明，这样能证明是"只接受拉起宿主的进程"这一条把它拒掉的；
  - kqueue 能拿到被收养进程的退出码。
- **惯例**：
  - `unsafe` 只能在 `cupper-sys`，带 `SAFETY:` 注释；
  - 时间只经 `Clock`；
  - 改了协议类型就跑 `pnpm gen` 并提交生成物（T015）。
- **CI 环境**：license 作业偶尔会因为拉不到 advisory 数据库而失败，是网络问题，重跑即可。

## 规模

估为 M，约 600–800 行，上限约 1,500 行。**开工前先在 `notes.md` 里按模块写行数估计，测试单独列一项。每完成一个模块就用 `git diff --numstat` 统计一次**（守则第 8 条）。**预计会超出上限时，必须先停下来汇报，不要继续写**。

## 参考资料

- 设计：
  - `08-security.md` 5.8.1（本地认证、`dev_key`、各构建种类）；
  - `03-protocol-v1.md` 5.3.2、5.3.11 末尾"T030 开工前的裁定"和握手期限；
  - `02-host.md` 5.2.13、5.2.14 末尾的 T045 裁定。
- ADR-05。
- T030、T045 的代码、`notes.md` 和汇报；T021 的 `cupper-sys`。

## 汇报要求

完成后按 `docs/templates/task-report.md` 的结构，把汇报全文作为最终回复返回，由编排者存档为本目录的 `report.md`。不要自己创建 `report.md`。汇报里不要写本机的绝对路径。其他物料由执行者在任务分支上提交。
